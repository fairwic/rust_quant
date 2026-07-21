use super::{OrderbookDepthPanelArgs, UniverseSchedule, MS_15M};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Cursor};
use std::path::Path;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::sleep;
use zip::ZipArchive;

const FACTOR_WINDOW_MS: i64 = MS_15M;
const MAX_SNAPSHOT_GAP_MS: i64 = 90 * 1_000;
const MIN_SNAPSHOTS: usize = 20;
const BID_PERCENTAGE: f64 = -1.0;
const ASK_PERCENTAGE: f64 = 1.0;
const CACHE_SCHEMA_VERSION: u32 = 1;
const CACHE_RULE_VERSION: &str = "binance_bookdepth_1pct_median_and_thirds_15m_v2";

/// 订单簿映射、官方文件和候选窗口完整性审计。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookDepthAudit {
    pub mapped_symbols: usize,
    pub mapping_blocked_symbols: usize,
    pub requested_files: usize,
    pub available_files: usize,
    pub missing_files: usize,
    pub invalid_files: usize,
    pub incomplete_windows: usize,
}

/// Binance 当前合约列表的最小响应。
#[derive(Debug, Deserialize)]
struct ExchangeInfo {
    symbols: Vec<ExchangeSymbol>,
}

/// 当前 Binance 合约映射需要的字段。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeSymbol {
    symbol: String,
    status: String,
    contract_type: String,
    quote_asset: String,
    underlying_type: String,
}

/// 唯一标识一个 OKX 映射合约和 Binance UTC 日包。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct BookDepthFileKey {
    okx_symbol: String,
    binance_symbol: String,
    day: NaiveDate,
}

/// 缓存中一个信号时点的冻结 1% 深度失衡。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
struct DepthRecord {
    decision_ts: i64,
    imbalance: f64,
    bid_change: f64,
    ask_change: f64,
}

/// 一个完整窗口同时保留静态失衡和前后 5m 深度变化。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DepthFactor {
    pub imbalance: f64,
    pub bid_change: f64,
    pub ask_change: f64,
}

/// 可复用缓存绑定币池、候选文件集、数据源和全部完整观测。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct BookDepthCache {
    schema_version: u32,
    rule_version: String,
    universe_version: String,
    generated_at_ms: i64,
    rest_base: String,
    data_base: String,
    mapping: BTreeMap<String, String>,
    requested_files: Vec<BookDepthFileKey>,
    missing_files: Vec<BookDepthFileKey>,
    invalid_files: Vec<BookDepthFileKey>,
    records: BTreeMap<String, Vec<DepthRecord>>,
}

/// 区分官方文件完整、明确缺失与 CSV 无效。
enum FileLoad {
    Available(Vec<DepthSnapshot>),
    Missing,
    Invalid,
}

/// 同一 snapshot 的 1% bid/ask notional。
#[derive(Debug, Clone, Copy, PartialEq)]
struct DepthSnapshot {
    ts: i64,
    bid_notional: f64,
    ask_notional: f64,
}

/// 读取或构造订单簿缓存，并返回按 OKX symbol x decision_ts 索引的因子。
pub(super) async fn load_book_depth(
    args: &OrderbookDepthPanelArgs,
    schedule: &UniverseSchedule,
    candidate_times: &BTreeMap<String, Vec<i64>>,
) -> Result<(BTreeMap<(String, i64), DepthFactor>, BookDepthAudit)> {
    let cache = if args.cache.exists() {
        let cache: BookDepthCache = serde_json::from_slice(
            &std::fs::read(&args.cache)
                .with_context(|| format!("read bookDepth cache {}", args.cache.display()))?,
        )
        .context("decode bookDepth cache")?;
        validate_cache(&cache, args, schedule, candidate_times)?;
        cache
    } else {
        let cache = fetch_cache(args, schedule, candidate_times).await?;
        write_cache_atomic(&args.cache, &cache)?;
        cache
    };
    let mut values = BTreeMap::new();
    for (symbol, records) in &cache.records {
        for record in records {
            if values
                .insert(
                    (symbol.clone(), record.decision_ts),
                    DepthFactor {
                        imbalance: record.imbalance,
                        bid_change: record.bid_change,
                        ask_change: record.ask_change,
                    },
                )
                .is_some()
            {
                bail!(
                    "duplicate cached bookDepth factor for {symbol} at {}",
                    record.decision_ts
                );
            }
        }
    }
    let requested_windows = candidate_times
        .iter()
        .filter(|(symbol, _)| cache.mapping.contains_key(*symbol))
        .map(|(_, points)| points.len())
        .sum::<usize>();
    let audit = BookDepthAudit {
        mapped_symbols: cache.mapping.len(),
        mapping_blocked_symbols: schedule
            .union_symbols()
            .into_iter()
            .filter(|symbol| !cache.mapping.contains_key(symbol))
            .count(),
        requested_files: cache.requested_files.len(),
        available_files: cache
            .requested_files
            .len()
            .saturating_sub(cache.missing_files.len() + cache.invalid_files.len()),
        missing_files: cache.missing_files.len(),
        invalid_files: cache.invalid_files.len(),
        incomplete_windows: requested_windows.saturating_sub(values.len()),
    };
    Ok((values, audit))
}

/// 校验缓存与冻结规则、币池、数据源和候选文件集合完全一致。
fn validate_cache(
    cache: &BookDepthCache,
    args: &OrderbookDepthPanelArgs,
    schedule: &UniverseSchedule,
    candidate_times: &BTreeMap<String, Vec<i64>>,
) -> Result<()> {
    if cache.schema_version != CACHE_SCHEMA_VERSION
        || cache.rule_version != CACHE_RULE_VERSION
        || cache.universe_version != schedule.version
        || cache.rest_base != args.binance_rest_base
        || cache.data_base != args.binance_data_base
    {
        bail!("bookDepth cache does not match frozen panel sources and universe");
    }
    let expected = requested_files(candidate_times, &cache.mapping)?;
    if expected != cache.requested_files {
        bail!("bookDepth cache candidate-day set does not match frozen candidates");
    }
    Ok(())
}

/// 下载最小候选日集合，并只缓存满足 15m 完整性门禁的因子记录。
async fn fetch_cache(
    args: &OrderbookDepthPanelArgs,
    schedule: &UniverseSchedule,
    candidate_times: &BTreeMap<String, Vec<i64>>,
) -> Result<BookDepthCache> {
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("build Binance bookDepth client")?;
    let live = load_current_live_crypto_perpetuals(&client, &args.binance_rest_base).await?;
    let mapping = schedule
        .union_symbols()
        .into_iter()
        .filter_map(|okx_symbol| {
            let binance_symbol = map_okx_symbol(&okx_symbol)?;
            live.contains(&binance_symbol)
                .then_some((okx_symbol, binance_symbol))
        })
        .collect::<BTreeMap<_, _>>();
    let requested = requested_files(candidate_times, &mapping)?;
    let decision_times = decision_times_by_file(candidate_times, &mapping)?;
    let mut missing = Vec::new();
    let mut invalid = Vec::new();
    let mut records = BTreeMap::<String, Vec<DepthRecord>>::new();
    for chunk in requested.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for key in chunk.iter().cloned() {
            let client = client.clone();
            let data_base = args.binance_data_base.clone();
            tasks.spawn(async move {
                let loaded = load_file(&client, &data_base, &key).await?;
                Ok::<_, anyhow::Error>((key, loaded))
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let (key, loaded) = joined.context("join Binance bookDepth file task")??;
            let snapshots = match loaded {
                FileLoad::Available(snapshots) => snapshots,
                FileLoad::Missing => {
                    missing.push(key);
                    continue;
                }
                FileLoad::Invalid => {
                    invalid.push(key);
                    continue;
                }
            };
            let times = decision_times
                .get(&key)
                .context("missing candidate times for requested bookDepth file")?;
            for decision_ts in times {
                if let Some(factor) = factor_at(&snapshots, *decision_ts) {
                    records
                        .entry(key.okx_symbol.clone())
                        .or_default()
                        .push(DepthRecord {
                            decision_ts: *decision_ts,
                            imbalance: factor.imbalance,
                            bid_change: factor.bid_change,
                            ask_change: factor.ask_change,
                        });
                }
            }
        }
    }
    for values in records.values_mut() {
        values.sort_by_key(|record| record.decision_ts);
        if values
            .windows(2)
            .any(|pair| pair[0].decision_ts == pair[1].decision_ts)
        {
            bail!("duplicate bookDepth record after file merge");
        }
    }
    missing.sort();
    invalid.sort();
    Ok(BookDepthCache {
        schema_version: CACHE_SCHEMA_VERSION,
        rule_version: CACHE_RULE_VERSION.to_owned(),
        universe_version: schedule.version.clone(),
        generated_at_ms: Utc::now().timestamp_millis(),
        rest_base: args.binance_rest_base.clone(),
        data_base: args.binance_data_base.clone(),
        mapping,
        requested_files: requested,
        missing_files: missing,
        invalid_files: invalid,
        records,
    })
}

/// 从候选信号生成去重的 Binance 日包请求。
fn requested_files(
    candidate_times: &BTreeMap<String, Vec<i64>>,
    mapping: &BTreeMap<String, String>,
) -> Result<Vec<BookDepthFileKey>> {
    Ok(decision_times_by_file(candidate_times, mapping)?
        .into_keys()
        .collect())
}

/// 将决策前一毫秒所属 UTC 日映射到唯一日包。
fn decision_times_by_file(
    candidate_times: &BTreeMap<String, Vec<i64>>,
    mapping: &BTreeMap<String, String>,
) -> Result<BTreeMap<BookDepthFileKey, Vec<i64>>> {
    let mut files = BTreeMap::<BookDepthFileKey, Vec<i64>>::new();
    for (okx_symbol, times) in candidate_times {
        let Some(binance_symbol) = mapping.get(okx_symbol) else {
            continue;
        };
        for decision_ts in times {
            let day = Utc
                .timestamp_millis_opt(decision_ts.saturating_sub(1))
                .single()
                .context("bookDepth candidate timestamp outside supported range")?
                .date_naive();
            files
                .entry(BookDepthFileKey {
                    okx_symbol: okx_symbol.clone(),
                    binance_symbol: binance_symbol.clone(),
                    day,
                })
                .or_default()
                .push(*decision_ts);
        }
    }
    for times in files.values_mut() {
        times.sort_unstable();
        times.dedup();
    }
    Ok(files)
}

/// 下载、checksum 校验并解析一个官方 bookDepth 日包。
async fn load_file(client: &Client, base: &str, key: &BookDepthFileKey) -> Result<FileLoad> {
    let filename = format!(
        "{}-bookDepth-{}.zip",
        key.binance_symbol,
        key.day.format("%Y-%m-%d")
    );
    let url = format!(
        "{base}/data/futures/um/daily/bookDepth/{}/{filename}",
        key.binance_symbol
    );
    let Some(bytes) = download_optional(client, &url).await? else {
        return Ok(FileLoad::Missing);
    };
    let checksum = download_required(client, &format!("{url}.CHECKSUM")).await?;
    verify_checksum(&bytes, &checksum, &filename)?;
    Ok(match parse_archive(&bytes, key.day) {
        Ok(snapshots) if !snapshots.is_empty() => FileLoad::Available(snapshots),
        Ok(_) | Err(_) => FileLoad::Invalid,
    })
}

/// 解析 CSV 并为每个 timestamp 配对 1% bid/ask 深度。
fn parse_archive(bytes: &[u8], expected_day: NaiveDate) -> Result<Vec<DepthSnapshot>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("open bookDepth ZIP")?;
    if archive.len() != 1 {
        bail!("bookDepth ZIP must contain exactly one CSV");
    }
    let file = archive.by_index(0).context("open bookDepth CSV")?;
    let mut lines = BufReader::new(file).lines();
    let header = lines.next().context("missing bookDepth header")??;
    if header.trim_end_matches('\r') != "timestamp,percentage,depth,notional" {
        bail!("unexpected bookDepth header");
    }
    let mut grouped = BTreeMap::<i64, (Option<f64>, Option<f64>)>::new();
    for line in lines {
        let line = line.context("read bookDepth row")?;
        let fields = line.trim_end_matches('\r').split(',').collect::<Vec<_>>();
        if fields.len() != 4 {
            bail!("invalid bookDepth row width");
        }
        let percentage = fields[1]
            .parse::<f64>()
            .context("parse bookDepth percentage")?;
        if !percentage.is_finite() || (percentage != BID_PERCENTAGE && percentage != ASK_PERCENTAGE)
        {
            continue;
        }
        let timestamp = NaiveDateTime::parse_from_str(fields[0], "%Y-%m-%d %H:%M:%S")
            .context("parse bookDepth timestamp")?;
        if timestamp.date() != expected_day {
            bail!("bookDepth timestamp outside expected UTC day");
        }
        let notional = fields[3]
            .parse::<f64>()
            .context("parse bookDepth notional")?;
        if !notional.is_finite() || notional < 0.0 {
            bail!("bookDepth notional must be finite and nonnegative");
        }
        let slot = grouped
            .entry(timestamp.and_utc().timestamp_millis())
            .or_default();
        let target = if percentage == BID_PERCENTAGE {
            &mut slot.0
        } else {
            &mut slot.1
        };
        if target.replace(notional).is_some() {
            bail!("duplicate 1% bookDepth side at one timestamp");
        }
    }
    Ok(grouped
        .into_iter()
        .filter_map(|(ts, (bid, ask))| {
            Some(DepthSnapshot {
                ts,
                bid_notional: bid?,
                ask_notional: ask?,
            })
        })
        .collect())
}

/// 在 `[T-15m,T)` 内检查完整性，并返回静态失衡与前后段深度变化。
fn factor_at(snapshots: &[DepthSnapshot], decision_ts: i64) -> Option<DepthFactor> {
    let start = decision_ts.checked_sub(FACTOR_WINDOW_MS)?;
    let first = snapshots.partition_point(|snapshot| snapshot.ts < start);
    let last = snapshots.partition_point(|snapshot| snapshot.ts < decision_ts);
    let window = snapshots.get(first..last)?;
    if window.len() < MIN_SNAPSHOTS
        || decision_ts - window.last()?.ts > MAX_SNAPSHOT_GAP_MS
        || window
            .windows(2)
            .any(|pair| pair[1].ts - pair[0].ts > MAX_SNAPSHOT_GAP_MS)
    {
        return None;
    }
    let mut values = window
        .iter()
        .filter_map(|snapshot| {
            let total = snapshot.bid_notional + snapshot.ask_notional;
            (total > 0.0).then_some((snapshot.bid_notional - snapshot.ask_notional) / total)
        })
        .collect::<Vec<_>>();
    let imbalance = median(&mut values)?;
    let first_end = decision_ts.checked_sub(10 * 60 * 1_000)?;
    let last_start = decision_ts.checked_sub(5 * 60 * 1_000)?;
    let first = window
        .iter()
        .filter(|snapshot| snapshot.ts < first_end)
        .collect::<Vec<_>>();
    let last = window
        .iter()
        .filter(|snapshot| snapshot.ts >= last_start)
        .collect::<Vec<_>>();
    if first.len() < 6 || last.len() < 6 {
        return None;
    }
    let mut first_bid = first
        .iter()
        .map(|snapshot| snapshot.bid_notional)
        .collect::<Vec<_>>();
    let mut last_bid = last
        .iter()
        .map(|snapshot| snapshot.bid_notional)
        .collect::<Vec<_>>();
    let mut first_ask = first
        .iter()
        .map(|snapshot| snapshot.ask_notional)
        .collect::<Vec<_>>();
    let mut last_ask = last
        .iter()
        .map(|snapshot| snapshot.ask_notional)
        .collect::<Vec<_>>();
    let first_bid = median(&mut first_bid)?;
    let last_bid = median(&mut last_bid)?;
    let first_ask = median(&mut first_ask)?;
    let last_ask = median(&mut last_ask)?;
    if first_bid <= 0.0 || first_ask <= 0.0 {
        return None;
    }
    Some(DepthFactor {
        imbalance,
        bid_change: last_bid / first_bid - 1.0,
        ask_change: last_ask / first_ask - 1.0,
    })
}

/// 返回有限值样本的确定性中位数。
fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    })
}

/// 读取当前 Binance 加密 USDT 永续用于规范符号映射。
async fn load_current_live_crypto_perpetuals(
    client: &Client,
    base: &str,
) -> Result<BTreeSet<String>> {
    let info = client
        .get(format!("{base}/fapi/v1/exchangeInfo"))
        .send()
        .await?
        .error_for_status()?
        .json::<ExchangeInfo>()
        .await
        .context("decode Binance exchangeInfo")?;
    let live = info
        .symbols
        .into_iter()
        .filter(|symbol| {
            symbol.status == "TRADING"
                && symbol.contract_type == "PERPETUAL"
                && symbol.quote_asset == "USDT"
                && symbol.underlying_type == "COIN"
        })
        .map(|symbol| symbol.symbol)
        .collect::<BTreeSet<_>>();
    if live.is_empty() {
        bail!("Binance exchangeInfo returned no current live crypto USDT perpetuals");
    }
    Ok(live)
}

/// 将规范 OKX USDT 永续映射为 Binance USD-M 合约。
fn map_okx_symbol(symbol: &str) -> Option<String> {
    let base = symbol.strip_suffix("-USDT-SWAP")?;
    Some(match base {
        "BONK" | "FLOKI" | "PEPE" | "SATS" | "SHIB" => format!("1000{base}USDT"),
        "LUNA" => "LUNA2USDT".to_owned(),
        _ => format!("{base}USDT"),
    })
}

/// 下载允许 404 的官方文件，其他错误有界重试。
async fn download_optional(client: &Client, url: &str) -> Result<Option<Vec<u8>>> {
    let mut last_error = None;
    for attempt in 0..4u64 {
        match client.get(url).send().await {
            Ok(response) if response.status() == StatusCode::NOT_FOUND => return Ok(None),
            Ok(response) => match response.error_for_status() {
                Ok(response) => match response.bytes().await {
                    Ok(bytes) => return Ok(Some(bytes.to_vec())),
                    Err(error) => last_error = Some(anyhow::Error::from(error)),
                },
                Err(error) => last_error = Some(anyhow::Error::from(error)),
            },
            Err(error) => last_error = Some(anyhow::Error::from(error)),
        }
        sleep(Duration::from_millis(250 * (attempt + 1))).await;
    }
    Err(last_error.unwrap_or_else(|| anyhow!("download retry loop produced no error")))
        .with_context(|| format!("download Binance public data {url}"))
}

/// 下载必须存在的 checksum 文件。
async fn download_required(client: &Client, url: &str) -> Result<Vec<u8>> {
    download_optional(client, url)
        .await?
        .with_context(|| format!("required Binance public data is missing: {url}"))
}

/// 校验官方 checksum 声明的 SHA-256。
fn verify_checksum(bytes: &[u8], checksum_bytes: &[u8], filename: &str) -> Result<()> {
    let checksum = std::str::from_utf8(checksum_bytes)
        .context("Binance checksum is not UTF-8")?
        .split_whitespace()
        .next()
        .context("Binance checksum is empty")?;
    let actual = hex::encode(Sha256::digest(bytes));
    if !checksum.eq_ignore_ascii_case(&actual) {
        bail!("Binance checksum mismatch for {filename}");
    }
    Ok(())
}

/// 原子写入缓存，避免中断留下可被误读的半文件。
fn write_cache_atomic(path: &Path, cache: &BookDepthCache) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create bookDepth cache directory {}", parent.display()))?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec(cache).context("encode bookDepth cache")?,
    )
    .with_context(|| format!("write bookDepth cache {}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .with_context(|| format!("publish bookDepth cache {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::FileOptions;

    #[test]
    fn archive_pairs_only_one_percent_bid_and_ask_depth() {
        let day = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("BTCUSDT-bookDepth-2024-07-01.csv", FileOptions::default())
            .unwrap();
        writeln!(writer, "timestamp,percentage,depth,notional").unwrap();
        writeln!(writer, "2024-07-01 00:00:00,-1.00,1,120").unwrap();
        writeln!(writer, "2024-07-01 00:00:00,1.00,1,80").unwrap();
        writeln!(writer, "2024-07-01 00:00:00,-2,2,999").unwrap();
        let bytes = writer.finish().unwrap().into_inner();

        let snapshots = parse_archive(&bytes, day).unwrap();

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].bid_notional, 120.0);
        assert_eq!(snapshots[0].ask_notional, 80.0);
    }

    #[test]
    fn factor_requires_twenty_fresh_contiguous_snapshots_before_decision() {
        let decision_ts = 30 * 60 * 1_000;
        let snapshots = (0..20)
            .map(|index| DepthSnapshot {
                ts: decision_ts - FACTOR_WINDOW_MS + index * 45 * 1_000,
                bid_notional: 120.0,
                ask_notional: 80.0,
            })
            .collect::<Vec<_>>();

        let factor = factor_at(&snapshots, decision_ts).unwrap();
        assert!((factor.imbalance - 0.2).abs() < 1e-12);
        assert_eq!(factor.bid_change, 0.0);
        assert_eq!(factor.ask_change, 0.0);
        assert!(factor_at(&snapshots[..19], decision_ts).is_none());
    }

    #[test]
    fn midnight_decision_uses_prior_utc_day_file() {
        let decision = NaiveDate::from_ymd_opt(2024, 7, 2)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let mapping = BTreeMap::from([("BTC-USDT-SWAP".to_owned(), "BTCUSDT".to_owned())]);
        let candidates = BTreeMap::from([("BTC-USDT-SWAP".to_owned(), vec![decision])]);

        let files = requested_files(&candidates, &mapping).unwrap();

        assert_eq!(files[0].day, NaiveDate::from_ymd_opt(2024, 7, 1).unwrap());
    }
}
