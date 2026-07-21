use super::{FlowFlipResearchArgs, UniverseSchedule};
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

const FIVE_MINUTES_MS: i64 = 5 * 60 * 1_000;
const FOUR_HOURS_MS: i64 = 4 * 60 * 60 * 1_000;
const POINTS_PER_DAY: usize = 24 * 12;
const FLOW_WINDOW_POINTS: usize = 12;
const FLOW_TOTAL_POINTS: usize = FLOW_WINDOW_POINTS * 2;
const CACHE_RULE_VERSION: &str = "binance_metrics_5m_oi4h_taker_median_1h_flip_v1";

/// 记录跨交易所指标映射、下载完整性和有效行数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetricsAudit {
    pub mapped_symbols: usize,
    pub mapping_blocked_symbols: usize,
    pub requested_files: usize,
    pub available_files: usize,
    pub missing_files: usize,
    pub invalid_files: usize,
    pub rows: usize,
}

/// 信号时点可见的 OI 与主动买卖流证据。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FlowEvidence {
    pub oi_change_4h: f64,
    pub prior_taker_median: f64,
    pub current_taker_median: f64,
    pub top_account_ratio: Option<f64>,
    pub top_position_ratio: Option<f64>,
}

/// 杠杆冲量延续规则使用的 OI 扩张与同向主动流证据。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ContinuationEvidence {
    pub oi_change_4h: f64,
    pub taker_median_1h: f64,
    pub top_account_ratio: Option<f64>,
    pub top_position_ratio: Option<f64>,
}

/// 按 Binance 合约和五分钟时间戳索引的指标快照。
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct MetricsStore {
    points: BTreeMap<String, Vec<MetricPoint>>,
}

/// 缓存中的单个五分钟指标点；可选字段仅用于诊断。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
struct MetricPoint {
    ts: i64,
    oi_value: f64,
    top_account_ratio: Option<f64>,
    top_position_ratio: Option<f64>,
    taker_ratio: f64,
}

/// 唯一标识一个合约、日期和指标类型的官方文件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct MetricFileKey {
    okx_symbol: String,
    binance_symbol: String,
    day: NaiveDate,
}

/// 可复用但必须重新校验请求集合的本地指标缓存。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct MetricsCache {
    schema_version: u32,
    rule_version: String,
    universe_version: String,
    generated_at_ms: i64,
    rest_base: String,
    data_base: String,
    mapping: BTreeMap<String, String>,
    requested_files: Vec<MetricFileKey>,
    missing_files: Vec<MetricFileKey>,
    invalid_files: Vec<MetricFileKey>,
    points: BTreeMap<String, Vec<MetricPoint>>,
}

/// 区分官方文件可用、明确缺失与内容无效三种结果。
enum MetricFileLoad {
    Available(Vec<MetricPoint>),
    Missing,
    Invalid,
}

/// Binance exchangeInfo 响应的最小外层结构。
#[derive(Debug, Deserialize)]
struct ExchangeInfo {
    symbols: Vec<ExchangeSymbol>,
}

/// 当前 Binance 合约快照中用于映射的最小字段。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeSymbol {
    symbol: String,
    status: String,
    contract_type: String,
    quote_asset: String,
    underlying_type: String,
}

impl MetricsStore {
    /// 只使用至少延迟 5m 的点，要求 2h 主动流连续且存在精确 4h OI 对照点。
    pub(super) fn evidence_at(&self, symbol: &str, decision_ts: i64) -> Option<FlowEvidence> {
        let points = self.points.get(symbol)?;
        let cutoff = decision_ts.checked_sub(FIVE_MINUTES_MS)?;
        let latest_index = points
            .partition_point(|point| point.ts <= cutoff)
            .checked_sub(1)?;
        let latest = points[latest_index];
        if decision_ts - latest.ts > 2 * FIVE_MINUTES_MS || latest_index + 1 < FLOW_TOTAL_POINTS {
            return None;
        }
        let flow = &points[latest_index + 1 - FLOW_TOTAL_POINTS..=latest_index];
        if flow
            .windows(2)
            .any(|pair| pair[1].ts != pair[0].ts + FIVE_MINUTES_MS)
        {
            return None;
        }
        let prior_taker_median = median(
            flow[..FLOW_WINDOW_POINTS]
                .iter()
                .map(|point| point.taker_ratio)
                .collect(),
        )?;
        let current_taker_median = median(
            flow[FLOW_WINDOW_POINTS..]
                .iter()
                .map(|point| point.taker_ratio)
                .collect(),
        )?;
        let oi_target_ts = latest.ts.checked_sub(FOUR_HOURS_MS)?;
        let oi_index = points
            .binary_search_by_key(&oi_target_ts, |point| point.ts)
            .ok()?;
        let prior_oi = points[oi_index].oi_value;
        if prior_oi <= 0.0
            || latest.oi_value >= prior_oi
            || prior_taker_median >= 1.0
            || current_taker_median <= 1.0
        {
            return None;
        }
        Some(FlowEvidence {
            oi_change_4h: latest.oi_value / prior_oi - 1.0,
            prior_taker_median,
            current_taker_median,
            top_account_ratio: latest.top_account_ratio,
            top_position_ratio: latest.top_position_ratio,
        })
    }

    /// 只使用至少延迟 5m 的连续点，验证 4h OI 扩张和最近 1h 主动流同向。
    pub(super) fn continuation_evidence_at(
        &self,
        symbol: &str,
        decision_ts: i64,
        long: bool,
    ) -> Option<ContinuationEvidence> {
        let points = self.points.get(symbol)?;
        let cutoff = decision_ts.checked_sub(FIVE_MINUTES_MS)?;
        let latest_index = points
            .partition_point(|point| point.ts <= cutoff)
            .checked_sub(1)?;
        let latest = points[latest_index];
        if decision_ts - latest.ts > 2 * FIVE_MINUTES_MS || latest_index + 1 < FLOW_WINDOW_POINTS {
            return None;
        }
        let flow = &points[latest_index + 1 - FLOW_WINDOW_POINTS..=latest_index];
        if flow
            .windows(2)
            .any(|pair| pair[1].ts != pair[0].ts + FIVE_MINUTES_MS)
        {
            return None;
        }
        let taker_median_1h = median(flow.iter().map(|point| point.taker_ratio).collect())?;
        let oi_target_ts = latest.ts.checked_sub(FOUR_HOURS_MS)?;
        let oi_index = points
            .binary_search_by_key(&oi_target_ts, |point| point.ts)
            .ok()?;
        let prior_oi = points[oi_index].oi_value;
        let aligned_flow = if long {
            taker_median_1h > 1.0
        } else {
            taker_median_1h < 1.0
        };
        if prior_oi <= 0.0 || latest.oi_value <= prior_oi || !aligned_flow {
            return None;
        }
        Some(ContinuationEvidence {
            oi_change_4h: latest.oi_value / prior_oi - 1.0,
            taker_median_1h,
            top_account_ratio: latest.top_account_ratio,
            top_position_ratio: latest.top_position_ratio,
        })
    }
}

/// 根据只用当时价格生成的候选日，读取缓存或下载并校验最小 Binance metrics 文件集。
pub(super) async fn load_metrics(
    args: &FlowFlipResearchArgs,
    schedule: &UniverseSchedule,
    candidate_times: &BTreeMap<String, Vec<i64>>,
) -> Result<(MetricsStore, MetricsAudit)> {
    let cache = if args.metrics_cache.exists() {
        let cache: MetricsCache = serde_json::from_slice(
            &std::fs::read(&args.metrics_cache)
                .with_context(|| format!("read metrics cache {}", args.metrics_cache.display()))?,
        )
        .context("decode Binance metrics cache")?;
        validate_cache(&cache, args, schedule, candidate_times)?;
        cache
    } else {
        let cache = fetch_cache(args, schedule, candidate_times).await?;
        write_cache_atomic(&args.metrics_cache, &cache)?;
        cache
    };
    let missing = cache.missing_files.iter().cloned().collect::<BTreeSet<_>>();
    let invalid = cache.invalid_files.iter().cloned().collect::<BTreeSet<_>>();
    let audit = MetricsAudit {
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
            .saturating_sub(missing.len() + invalid.len()),
        missing_files: missing.len(),
        invalid_files: invalid.len(),
        rows: cache.points.values().map(Vec::len).sum(),
    };
    Ok((
        MetricsStore {
            points: cache.points,
        },
        audit,
    ))
}

/// 校验缓存版本、币池版本和请求文件集合完全一致。
fn validate_cache(
    cache: &MetricsCache,
    args: &FlowFlipResearchArgs,
    schedule: &UniverseSchedule,
    candidate_times: &BTreeMap<String, Vec<i64>>,
) -> Result<()> {
    if cache.schema_version != 3
        || cache.rule_version != CACHE_RULE_VERSION
        || cache.universe_version != schedule.version
        || cache.rest_base != args.binance_rest_base
        || cache.data_base != args.binance_data_base
    {
        bail!("Binance metrics cache does not match the frozen V2 sources and universe");
    }
    let expected = requested_files(candidate_times, &cache.mapping)?;
    if expected != cache.requested_files {
        bail!("Binance metrics cache candidate-day set does not match frozen price candidates");
    }
    Ok(())
}

/// 并发下载候选日所需文件并构造确定性缓存。
async fn fetch_cache(
    args: &FlowFlipResearchArgs,
    schedule: &UniverseSchedule,
    candidate_times: &BTreeMap<String, Vec<i64>>,
) -> Result<MetricsCache> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build Binance public-data client")?;
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
    let mut missing = Vec::new();
    let mut invalid = Vec::new();
    let mut unique = BTreeMap::<(String, i64), (MetricPoint, NaiveDate)>::new();
    for chunk in requested.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for key in chunk.iter().cloned() {
            let client = client.clone();
            let data_base = args.binance_data_base.clone();
            tasks.spawn(async move {
                let points = load_metric_file(&client, &data_base, &key).await?;
                Ok::<_, anyhow::Error>((key, points))
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let (key, points) = joined.context("join Binance metrics file task")??;
            let points = match points {
                MetricFileLoad::Available(points) => points,
                MetricFileLoad::Missing => {
                    missing.push(key);
                    continue;
                }
                MetricFileLoad::Invalid => {
                    invalid.push(key);
                    continue;
                }
            };
            for point in points {
                merge_metric_point(&mut unique, &key.okx_symbol, key.day, point)?;
            }
        }
    }
    let mut points = BTreeMap::<String, Vec<MetricPoint>>::new();
    for ((symbol, _), (point, _)) in unique {
        points.entry(symbol).or_default().push(point);
    }
    for values in points.values_mut() {
        values.sort_by_key(|point| point.ts);
    }
    missing.sort();
    invalid.sort();
    Ok(MetricsCache {
        schema_version: 3,
        rule_version: CACHE_RULE_VERSION.to_owned(),
        universe_version: schedule.version.clone(),
        generated_at_ms: Utc::now().timestamp_millis(),
        rest_base: args.binance_rest_base.clone(),
        data_base: args.binance_data_base.clone(),
        mapping,
        requested_files: requested,
        missing_files: missing,
        invalid_files: invalid,
        points,
    })
}

/// 按指标类型合并同一时间点，并按时间戳归属日解决跨日重叠。
fn merge_metric_point(
    unique: &mut BTreeMap<(String, i64), (MetricPoint, NaiveDate)>,
    symbol: &str,
    source_day: NaiveDate,
    point: MetricPoint,
) -> Result<()> {
    let key = (symbol.to_owned(), point.ts);
    let Some((existing, existing_day)) = unique.get_mut(&key) else {
        unique.insert(key, (point, source_day));
        return Ok(());
    };
    if *existing == point {
        return Ok(());
    }
    let timestamp_day = Utc
        .timestamp_millis_opt(point.ts)
        .single()
        .context("Binance metric timestamp outside supported range")?
        .date_naive();
    match (*existing_day == timestamp_day, source_day == timestamp_day) {
        (false, true) => {
            *existing = point;
            *existing_day = source_day;
            Ok(())
        }
        (true, false) => Ok(()),
        _ => bail!("conflicting Binance metrics for {} at {}", symbol, point.ts),
    }
}

/// 读取当前仍为交易中状态的 Binance 加密永续用于符号映射。
async fn load_current_live_crypto_perpetuals(
    client: &Client,
    base: &str,
) -> Result<BTreeSet<String>> {
    let url = format!("{base}/fapi/v1/exchangeInfo");
    let info = client
        .get(&url)
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

/// 从价格候选时点生成最小且去重的官方下载文件集合。
fn requested_files(
    candidate_times: &BTreeMap<String, Vec<i64>>,
    mapping: &BTreeMap<String, String>,
) -> Result<Vec<MetricFileKey>> {
    let mut files = BTreeSet::new();
    for (okx_symbol, timestamps) in candidate_times {
        let Some(binance_symbol) = mapping.get(okx_symbol) else {
            continue;
        };
        for timestamp in timestamps {
            let day = Utc
                .timestamp_millis_opt(*timestamp)
                .single()
                .context("candidate timestamp outside supported range")?
                .date_naive();
            let prior = day
                .pred_opt()
                .context("candidate prior day outside supported range")?;
            files.insert(MetricFileKey {
                okx_symbol: okx_symbol.clone(),
                binance_symbol: binance_symbol.clone(),
                day: prior,
            });
            files.insert(MetricFileKey {
                okx_symbol: okx_symbol.clone(),
                binance_symbol: binance_symbol.clone(),
                day,
            });
        }
    }
    Ok(files.into_iter().collect())
}

/// 下载、校验 checksum 并解析一个官方日度指标压缩包。
async fn load_metric_file(
    client: &Client,
    base: &str,
    key: &MetricFileKey,
) -> Result<MetricFileLoad> {
    let filename = format!(
        "{}-metrics-{}.zip",
        key.binance_symbol,
        key.day.format("%Y-%m-%d")
    );
    let url = format!(
        "{base}/data/futures/um/daily/metrics/{}/{filename}",
        key.binance_symbol
    );
    let Some(bytes) = download_optional(client, &url).await? else {
        return Ok(MetricFileLoad::Missing);
    };
    let checksum_bytes = download_required(client, &format!("{url}.CHECKSUM")).await?;
    verify_checksum(&bytes, &checksum_bytes, &filename)?;
    Ok(
        match parse_metric_archive(&bytes, &key.binance_symbol, key.day)? {
            Some(points) => MetricFileLoad::Available(points),
            None => MetricFileLoad::Invalid,
        },
    )
}

/// 下载允许 404 的官方文件，其他错误仍立即失败。
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

/// 把必需文件的缺失转换为明确错误。
async fn download_required(client: &Client, url: &str) -> Result<Vec<u8>> {
    download_optional(client, url)
        .await?
        .with_context(|| format!("required Binance public data is missing: {url}"))
}

/// 校验官方 checksum 中声明的 SHA-256 指纹。
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

/// 解析指标 CSV，并拒绝不对齐、重复或关键字段非法的行。
fn parse_metric_archive(
    bytes: &[u8],
    expected_symbol: &str,
    expected_day: NaiveDate,
) -> Result<Option<Vec<MetricPoint>>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("open Binance metrics ZIP")?;
    if archive.len() != 1 {
        bail!("Binance metrics ZIP must contain exactly one CSV");
    }
    let file = archive.by_index(0).context("open Binance metrics CSV")?;
    let mut lines = BufReader::new(file).lines();
    let header = lines.next().context("missing Binance metrics header")??;
    if header.trim_end_matches('\r')
        != "create_time,symbol,sum_open_interest,sum_open_interest_value,count_toptrader_long_short_ratio,sum_toptrader_long_short_ratio,count_long_short_ratio,sum_taker_long_short_vol_ratio"
    {
        bail!("unexpected Binance metrics header");
    }
    let day_start = expected_day
        .and_hms_opt(0, 0, 0)
        .context("build Binance metrics day start")?
        .and_utc()
        .timestamp_millis();
    let mut points = Vec::new();
    for line in lines {
        let line = line.context("read Binance metrics row")?;
        let fields = line.trim_end_matches('\r').split(',').collect::<Vec<_>>();
        if fields.len() != 8 || fields[1] != expected_symbol {
            return Ok(None);
        }
        let Ok(timestamp) = NaiveDateTime::parse_from_str(fields[0], "%Y-%m-%d %H:%M:%S") else {
            return Ok(None);
        };
        let Ok(oi_value) = parse_nonnegative(fields[3], "sum_open_interest_value") else {
            return Ok(None);
        };
        let Ok(top_account_ratio) = parse_optional_positive(fields[4], "top account ratio") else {
            return Ok(None);
        };
        let Ok(top_position_ratio) = parse_optional_positive(fields[5], "top position ratio")
        else {
            return Ok(None);
        };
        let Ok(taker_ratio) = parse_nonnegative(fields[7], "taker ratio") else {
            return Ok(None);
        };
        points.push(MetricPoint {
            ts: timestamp.and_utc().timestamp_millis(),
            oi_value,
            top_account_ratio,
            top_position_ratio,
            taker_ratio,
        });
    }
    if points.len() != POINTS_PER_DAY {
        return Ok(None);
    }
    let first_ts = points[0].ts;
    if first_ts != day_start && first_ts != day_start + FIVE_MINUTES_MS {
        return Ok(None);
    }
    for (index, point) in points.iter().enumerate() {
        if point.ts != first_ts + index as i64 * FIVE_MINUTES_MS {
            return Ok(None);
        }
    }
    Ok(Some(points))
}

/// 将规范 OKX USDT 永续标识映射为 Binance 合约标识。
fn map_okx_symbol(symbol: &str) -> Option<String> {
    let base = symbol.strip_suffix("-USDT-SWAP")?;
    let mapped = match base {
        "BONK" | "FLOKI" | "PEPE" | "SATS" | "SHIB" => format!("1000{base}USDT"),
        "LUNA" => "LUNA2USDT".to_owned(),
        _ => format!("{base}USDT"),
    };
    Some(mapped)
}

/// 返回有限样本的中位数。
fn median(mut values: Vec<f64>) -> Option<f64> {
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

/// 解析必须非负的关键指标值。
fn parse_nonnegative(value: &str, label: &str) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse {label}"))?;
    if !parsed.is_finite() || parsed < 0.0 {
        bail!("{label} must be finite and nonnegative");
    }
    Ok(parsed)
}

/// 解析可选且必须为正的诊断指标值。
fn parse_optional_positive(value: &str, label: &str) -> Result<Option<f64>> {
    if value.is_empty() {
        return Ok(None);
    }
    let parsed = parse_nonnegative(value, label)?;
    Ok((parsed > 0.0).then_some(parsed))
}

/// 通过临时文件和 rename 原子更新指标缓存。
fn write_cache_atomic(path: &Path, cache: &MetricsCache) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "create Binance metrics cache directory {}",
            parent.display()
        )
    })?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec(cache).context("encode Binance metrics cache")?,
    )
    .with_context(|| format!("write Binance metrics cache {}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .with_context(|| format!("publish Binance metrics cache {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::FileOptions;

    #[test]
    fn official_metric_archive_requires_full_contiguous_utc_day() {
        let day = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("BTCUSDT-metrics-2024-07-01.csv", FileOptions::default())
            .unwrap();
        writeln!(
            writer,
            "create_time,symbol,sum_open_interest,sum_open_interest_value,count_toptrader_long_short_ratio,sum_toptrader_long_short_ratio,count_long_short_ratio,sum_taker_long_short_vol_ratio"
        )
        .unwrap();
        for index in 1..=POINTS_PER_DAY {
            let timestamp = day.and_hms_opt(0, 0, 0).unwrap().and_utc()
                + chrono::Duration::minutes(index as i64 * 5);
            writeln!(
                writer,
                "{},BTCUSDT,1,100,1.2,1.3,1.1,0.9",
                timestamp.format("%Y-%m-%d %H:%M:%S")
            )
            .unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();

        let points = parse_metric_archive(&bytes, "BTCUSDT", day)
            .unwrap()
            .unwrap();

        assert_eq!(points.len(), POINTS_PER_DAY);
        assert_eq!(points[0].oi_value, 100.0);
    }

    #[test]
    fn archive_with_empty_metric_is_invalid_instead_of_filled() {
        let day = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("BTCUSDT-metrics-2024-07-01.csv", FileOptions::default())
            .unwrap();
        writeln!(
            writer,
            "create_time,symbol,sum_open_interest,sum_open_interest_value,count_toptrader_long_short_ratio,sum_toptrader_long_short_ratio,count_long_short_ratio,sum_taker_long_short_vol_ratio"
        )
        .unwrap();
        for index in 0..POINTS_PER_DAY {
            let timestamp = day.and_hms_opt(0, 0, 0).unwrap().and_utc()
                + chrono::Duration::minutes(index as i64 * 5);
            writeln!(
                writer,
                "{},BTCUSDT,1,100,1.2,1.3,1.1,{}",
                timestamp.format("%Y-%m-%d %H:%M:%S"),
                if index == 10 { "" } else { "0.9" }
            )
            .unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();

        assert!(parse_metric_archive(&bytes, "BTCUSDT", day)
            .unwrap()
            .is_none());
    }

    #[test]
    fn checksum_and_multiplier_symbol_mapping_are_deterministic() {
        let bytes = b"fixture";
        let checksum = format!("{}  fixture.zip", hex::encode(Sha256::digest(bytes)));

        verify_checksum(bytes, checksum.as_bytes(), "fixture.zip").unwrap();
        assert_eq!(
            map_okx_symbol("PEPE-USDT-SWAP").as_deref(),
            Some("1000PEPEUSDT")
        );
        assert_eq!(
            map_okx_symbol("LUNA-USDT-SWAP").as_deref(),
            Some("LUNA2USDT")
        );
    }

    #[test]
    fn evidence_waits_five_minutes_and_requires_oi_drop_plus_flow_flip() {
        let mut points = Vec::new();
        for index in 0..=48 {
            points.push(MetricPoint {
                ts: index * FIVE_MINUTES_MS,
                oi_value: 100.0 - index as f64 * 0.1,
                top_account_ratio: Some(1.2),
                top_position_ratio: Some(1.3),
                taker_ratio: if index < 37 { 0.8 } else { 1.2 },
            });
        }
        let mut store = MetricsStore::default();
        store.points.insert("BTC-USDT-SWAP".to_owned(), points);
        let latest_ts = 48 * FIVE_MINUTES_MS;

        assert!(store.evidence_at("BTC-USDT-SWAP", latest_ts).is_none());
        let evidence = store
            .evidence_at("BTC-USDT-SWAP", latest_ts + FIVE_MINUTES_MS)
            .unwrap();
        assert!(evidence.oi_change_4h < 0.0);
        assert!(evidence.prior_taker_median < 1.0);
        assert!(evidence.current_taker_median > 1.0);
    }

    #[test]
    fn continuation_evidence_requires_oi_expansion_and_directional_taker_flow() {
        let mut points = Vec::new();
        for index in 0..=48 {
            points.push(MetricPoint {
                ts: index * FIVE_MINUTES_MS,
                oi_value: 100.0 + index as f64 * 0.1,
                top_account_ratio: Some(1.2),
                top_position_ratio: Some(1.3),
                taker_ratio: 1.2,
            });
        }
        let mut store = MetricsStore::default();
        store.points.insert("BTC-USDT-SWAP".to_owned(), points);
        let decision_ts = 49 * FIVE_MINUTES_MS;

        let evidence = store
            .continuation_evidence_at("BTC-USDT-SWAP", decision_ts, true)
            .unwrap();
        assert!(evidence.oi_change_4h > 0.0);
        assert!(evidence.taker_median_1h > 1.0);
        assert!(store
            .continuation_evidence_at("BTC-USDT-SWAP", decision_ts, false)
            .is_none());
    }

    #[test]
    fn duplicate_midnight_prefers_file_for_timestamp_day() {
        let timestamp_day = NaiveDate::from_ymd_opt(2024, 4, 4).unwrap();
        let prior_day = timestamp_day.pred_opt().unwrap();
        let ts = timestamp_day
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let prior_tail = MetricPoint {
            ts,
            oi_value: 90.0,
            top_account_ratio: Some(1.1),
            top_position_ratio: Some(1.2),
            taker_ratio: 0.8,
        };
        let current_start = MetricPoint {
            ts,
            oi_value: 100.0,
            top_account_ratio: Some(1.3),
            top_position_ratio: Some(1.4),
            taker_ratio: 1.2,
        };
        let mut unique = BTreeMap::new();

        merge_metric_point(&mut unique, "BTC-USDT-SWAP", prior_day, prior_tail).unwrap();
        merge_metric_point(&mut unique, "BTC-USDT-SWAP", timestamp_day, current_start).unwrap();

        assert_eq!(
            unique.get(&("BTC-USDT-SWAP".to_owned(), ts)),
            Some(&(current_start, timestamp_day))
        );
    }
}
