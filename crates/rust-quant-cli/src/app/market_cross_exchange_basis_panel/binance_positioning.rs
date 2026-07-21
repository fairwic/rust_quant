use super::binance_klines::{
    download_optional, load_current_live_crypto_perpetuals, map_okx_symbol, write_atomic,
};
use super::{CrossExchangeBasisPanelArgs, UniverseSchedule};
use anyhow::{bail, Context, Result};
use chrono::{Duration as ChronoDuration, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Cursor};
use std::path::Path;
use std::time::Duration;
use tokio::task::JoinSet;
use zip::ZipArchive;

const FIVE_MINUTES_MS: i64 = 5 * 60 * 1_000;
const POINTS_PER_DAY: usize = 24 * 12;

/// Binance top-trader 日包的映射、文件和保留决策点审计。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BinancePositioningAudit {
    /// 当前 Binance live crypto perpetual 的 OKX 映射数。
    pub mapped_symbols: usize,
    /// 当前无法映射的 OKX 历史币池成员数。
    pub mapping_blocked_symbols: usize,
    /// 按月度成员关系请求的 symbol-day 文件数。
    pub requested_files: usize,
    /// checksum、ZIP 和 CSV 均有效的文件数。
    pub available_files: usize,
    /// 官方明确缺失的文件数。
    pub missing_files: usize,
    /// 文件存在但内容合同失败的文件数。
    pub invalid_files: usize,
    /// 在 UTC 07:55/15:55/23:55 保留且双比率有效的点数。
    pub retained_points: usize,
}

/// 决策前五分钟可见的 top-trader 账户与持仓方向事实。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BinancePositioningPoint {
    /// 官方 5m 创建时间，Unix 毫秒。
    pub ts: i64,
    /// 头部交易者账户数量 long/short ratio。
    pub account_ratio: f64,
    /// 头部交易者持仓金额 long/short ratio。
    pub position_ratio: f64,
    /// 全市场账户数量 long/short ratio；缺失时仅阻塞 crowd 因子。
    pub global_account_ratio: Option<f64>,
}

/// 唯一定位一个 Binance metrics 日包。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PositioningFileKey {
    /// OKX 标准 symbol。
    okx_symbol: String,
    /// Binance USD-M symbol。
    binance_symbol: String,
    /// UTC 日期。
    day: NaiveDate,
}

/// 区分有效、明确缺失和格式无效的官方日包。
enum PositioningFileLoad {
    /// 完整校验后保留的三个决策前点。
    Available(Vec<BinancePositioningPoint>),
    /// 官方 ZIP 或 checksum 404。
    Missing,
    /// 文件存在但 checksum、CSV 或连续性失败。
    Invalid,
}

/// 下载并缓存全年完整 point-in-time top-trader 指标日包。
pub(super) async fn load_binance_positioning(
    args: &CrossExchangeBasisPanelArgs,
    schedule: &UniverseSchedule,
) -> Result<(
    BTreeMap<String, Vec<BinancePositioningPoint>>,
    BinancePositioningAudit,
)> {
    let cache_dir = args.cache_dir.join("top_trader_metrics");
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("create Binance positioning cache {}", cache_dir.display()))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .context("build Binance positioning daily client")?;
    let live = load_current_live_crypto_perpetuals(&client, &args.binance_rest_base).await?;
    let all_symbols = schedule.union_symbols();
    let mapping = all_symbols
        .iter()
        .filter_map(|okx_symbol| {
            let binance_symbol = map_okx_symbol(okx_symbol)?;
            live.contains(&binance_symbol)
                .then_some((okx_symbol.clone(), binance_symbol))
        })
        .collect::<BTreeMap<_, _>>();
    let requested = requested_files(schedule, &mapping)?;
    let mut available_files = 0usize;
    let mut missing_files = 0usize;
    let mut invalid_files = 0usize;
    let mut rows = BTreeMap::<String, Vec<BinancePositioningPoint>>::new();
    for chunk in requested.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for key in chunk.iter().cloned() {
            let client = client.clone();
            let data_base = args.binance_data_base.clone();
            let cache_dir = cache_dir.clone();
            tasks.spawn(async move {
                let loaded = load_positioning_file(&client, &data_base, &cache_dir, &key).await?;
                Ok::<_, anyhow::Error>((key, loaded))
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let (key, loaded) = joined.context("join Binance positioning daily task")??;
            match loaded {
                PositioningFileLoad::Available(mut points) => {
                    available_files += 1;
                    rows.entry(key.okx_symbol).or_default().append(&mut points);
                }
                PositioningFileLoad::Missing => missing_files += 1,
                PositioningFileLoad::Invalid => invalid_files += 1,
            }
        }
    }
    let mut retained_points = 0usize;
    for (symbol, points) in &mut rows {
        points.sort_by_key(|point| point.ts);
        if points.windows(2).any(|pair| {
            pair[0].ts == pair[1].ts
                && (pair[0].account_ratio != pair[1].account_ratio
                    || pair[0].position_ratio != pair[1].position_ratio
                    || pair[0].global_account_ratio != pair[1].global_account_ratio)
        }) {
            bail!("conflicting duplicate Binance positioning points for {symbol}");
        }
        points.dedup_by_key(|point| point.ts);
        if points.windows(2).any(|pair| pair[0].ts >= pair[1].ts) {
            bail!("duplicate or unsorted Binance positioning points for {symbol}");
        }
        retained_points += points.len();
    }
    let audit = BinancePositioningAudit {
        mapped_symbols: mapping.len(),
        mapping_blocked_symbols: all_symbols.len().saturating_sub(mapping.len()),
        requested_files: requested.len(),
        available_files,
        missing_files,
        invalid_files,
        retained_points,
    };
    Ok((rows, audit))
}

/// 依据每月成员关系请求该月全部 UTC 日，并为月初决策补前一日。
fn requested_files(
    schedule: &UniverseSchedule,
    mapping: &BTreeMap<String, String>,
) -> Result<Vec<PositioningFileKey>> {
    let mut files = BTreeSet::new();
    for window in &schedule.windows {
        let start = Utc
            .timestamp_millis_opt(window.from_ms)
            .single()
            .context("invalid positioning window start")?
            .date_naive()
            .pred_opt()
            .context("positioning prior day outside supported range")?;
        let end = Utc
            .timestamp_millis_opt(window.to_ms.saturating_sub(1))
            .single()
            .context("invalid positioning window end")?
            .date_naive();
        for okx_symbol in &window.members {
            let Some(binance_symbol) = mapping.get(okx_symbol) else {
                continue;
            };
            let mut day = start;
            while day <= end {
                files.insert(PositioningFileKey {
                    okx_symbol: okx_symbol.clone(),
                    binance_symbol: binance_symbol.clone(),
                    day,
                });
                day = day
                    .checked_add_signed(ChronoDuration::days(1))
                    .context("positioning day overflow")?;
            }
        }
    }
    Ok(files.into_iter().collect())
}

/// 优先复用本地校验缓存，否则下载官方日包和 checksum。
async fn load_positioning_file(
    client: &Client,
    data_base: &str,
    cache_dir: &Path,
    key: &PositioningFileKey,
) -> Result<PositioningFileLoad> {
    let filename = format!(
        "{}-metrics-{}.zip",
        key.binance_symbol,
        key.day.format("%Y-%m-%d")
    );
    let symbol_dir = cache_dir.join(&key.binance_symbol);
    std::fs::create_dir_all(&symbol_dir).with_context(|| {
        format!(
            "create Binance positioning symbol cache {}",
            symbol_dir.display()
        )
    })?;
    let zip_path = symbol_dir.join(&filename);
    let checksum_path = symbol_dir.join(format!("{filename}.CHECKSUM"));
    if zip_path.exists() && checksum_path.exists() {
        if let Ok(points) = parse_verified_file(&zip_path, &checksum_path, key) {
            return Ok(PositioningFileLoad::Available(points));
        }
    }
    let url = format!(
        "{data_base}/data/futures/um/daily/metrics/{}/{}",
        key.binance_symbol, filename
    );
    let checksum_url = format!("{url}.CHECKSUM");
    let (Some(zip_bytes), Some(checksum_bytes)) = (
        download_optional(client, &url).await?,
        download_optional(client, &checksum_url).await?,
    ) else {
        return Ok(PositioningFileLoad::Missing);
    };
    let points = match parse_verified_bytes(&zip_bytes, &checksum_bytes, key, &filename) {
        Ok(points) => points,
        Err(_) => return Ok(PositioningFileLoad::Invalid),
    };
    write_atomic(&zip_path, &zip_bytes)?;
    write_atomic(&checksum_path, &checksum_bytes)?;
    Ok(PositioningFileLoad::Available(points))
}

/// 校验本地 ZIP 和 checksum 后解析指标日包。
fn parse_verified_file(
    zip_path: &Path,
    checksum_path: &Path,
    key: &PositioningFileKey,
) -> Result<Vec<BinancePositioningPoint>> {
    let zip_bytes = std::fs::read(zip_path)
        .with_context(|| format!("read cached positioning zip {}", zip_path.display()))?;
    let checksum_bytes = std::fs::read(checksum_path).with_context(|| {
        format!(
            "read cached positioning checksum {}",
            checksum_path.display()
        )
    })?;
    let filename = zip_path
        .file_name()
        .and_then(|value| value.to_str())
        .context("positioning cache filename is not UTF-8")?;
    parse_verified_bytes(&zip_bytes, &checksum_bytes, key, filename)
}

/// 校验官方 SHA-256、288 个连续 5m 行，并只保留决策前五分钟点。
fn parse_verified_bytes(
    zip_bytes: &[u8],
    checksum_bytes: &[u8],
    key: &PositioningFileKey,
    filename: &str,
) -> Result<Vec<BinancePositioningPoint>> {
    let checksum =
        std::str::from_utf8(checksum_bytes).context("Binance positioning checksum is not UTF-8")?;
    let fields = checksum.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 2 || fields[1] != filename || fields[0].len() != 64 {
        bail!("invalid Binance positioning checksum format");
    }
    let actual = format!("{:x}", Sha256::digest(zip_bytes));
    if !actual.eq_ignore_ascii_case(fields[0]) {
        bail!("Binance positioning checksum mismatch");
    }
    let mut archive = ZipArchive::new(Cursor::new(zip_bytes)).context("open positioning ZIP")?;
    if archive.len() != 1 {
        bail!("Binance positioning ZIP must contain exactly one CSV");
    }
    let file = archive.by_index(0).context("open positioning CSV")?;
    let mut lines = BufReader::new(file).lines();
    let header = lines.next().context("missing positioning header")??;
    if header.trim_end_matches('\r')
        != "create_time,symbol,sum_open_interest,sum_open_interest_value,count_toptrader_long_short_ratio,sum_toptrader_long_short_ratio,count_long_short_ratio,sum_taker_long_short_vol_ratio"
    {
        bail!("unexpected Binance positioning header");
    }
    let mut raw = Vec::<(i64, Option<(f64, f64)>, Option<f64>)>::new();
    for line in lines {
        let line = line.context("read positioning CSV row")?;
        let fields = line.trim_end_matches('\r').split(',').collect::<Vec<_>>();
        if fields.len() != 8 || fields[1] != key.binance_symbol {
            bail!("positioning row violates symbol or field count");
        }
        let raw_timestamp = NaiveDateTime::parse_from_str(fields[0], "%Y-%m-%d %H:%M:%S")
            .context("parse positioning create_time")?
            .and_utc()
            .timestamp_millis();
        let timestamp = normalize_metric_timestamp(raw_timestamp)?;
        let account = parse_optional_positive(fields[4])?;
        let position = parse_optional_positive(fields[5])?;
        let global_account = parse_optional_positive(fields[6])?;
        raw.push((timestamp, account.zip(position), global_account));
    }
    if raw.len() != POINTS_PER_DAY
        || raw
            .windows(2)
            .any(|pair| pair[1].0 - pair[0].0 != FIVE_MINUTES_MS)
    {
        bail!("positioning CSV is not one complete continuous 5m day");
    }
    let expected_start = key
        .day
        .and_hms_opt(0, 0, 0)
        .context("build positioning day start")?
        .and_utc()
        .timestamp_millis();
    if raw[0].0 != expected_start && raw[0].0 != expected_start + FIVE_MINUTES_MS {
        bail!("positioning CSV starts outside expected UTC day boundary");
    }
    Ok(raw
        .into_iter()
        .filter_map(|(ts, ratios, global_account_ratio)| {
            let time = Utc.timestamp_millis_opt(ts).single()?;
            let keep = time.minute() == 55 && matches!(time.hour(), 7 | 15 | 23);
            let (account_ratio, position_ratio) = ratios?;
            keep.then_some(BinancePositioningPoint {
                ts,
                account_ratio,
                position_ratio,
                global_account_ratio,
            })
        })
        .collect())
}

/// 把官方名义 5m 槽后最多 60 秒的发布抖动归一，较大延迟直接拒绝。
fn normalize_metric_timestamp(raw_ts: i64) -> Result<i64> {
    let nominal = raw_ts - raw_ts.rem_euclid(FIVE_MINUTES_MS);
    if raw_ts - nominal > 60_000 {
        bail!("Binance positioning timestamp exceeds 60-second publication tolerance");
    }
    Ok(nominal)
}

/// 解析允许为空、否则必须严格正且有限的 ratio。
fn parse_optional_positive(value: &str) -> Result<Option<f64>> {
    if value.is_empty() || value.eq_ignore_ascii_case("null") {
        return Ok(None);
    }
    let parsed = value.parse::<f64>().context("parse top-trader ratio")?;
    if !parsed.is_finite() || parsed <= 0.0 {
        bail!("top-trader ratio must be finite and positive");
    }
    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_ratio_rejects_nonpositive_values() {
        assert_eq!(parse_optional_positive("").unwrap(), None);
        assert_eq!(parse_optional_positive("1.25").unwrap(), Some(1.25));
        assert!(parse_optional_positive("0").is_err());
        assert!(parse_optional_positive("NaN").is_err());
    }

    #[test]
    fn requested_days_include_prior_day_for_month_open() {
        let from_ms = 1_719_792_000_000;
        let schedule = UniverseSchedule {
            version: "v".to_owned(),
            windows: vec![super::super::UniverseWindow {
                from_ms,
                to_ms: from_ms + 24 * 60 * 60 * 1_000,
                members: BTreeSet::from(["BTC-USDT-SWAP".to_owned()]),
            }],
        };
        let mapping = BTreeMap::from([("BTC-USDT-SWAP".to_owned(), "BTCUSDT".to_owned())]);
        let files = requested_files(&schedule, &mapping).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].day.to_string(), "2024-06-30");
        assert_eq!(files[1].day.to_string(), "2024-07-01");
    }

    #[test]
    fn timestamp_normalization_accepts_small_publication_drift_only() {
        let nominal = 1_719_792_000_000;
        assert_eq!(
            normalize_metric_timestamp(nominal + 6_000).unwrap(),
            nominal
        );
        assert!(normalize_metric_timestamp(nominal + 60_001).is_err());
    }
}
