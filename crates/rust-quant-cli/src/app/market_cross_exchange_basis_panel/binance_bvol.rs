use super::binance_klines::{download_optional, write_atomic};
use super::bvol_relative::BvolRelativePanelArgs;
use anyhow::{bail, Context, Result};
use chrono::{Duration as ChronoDuration, NaiveDate, TimeZone, Timelike, Utc};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Cursor};
use std::path::Path;
use std::time::Duration;
use tokio::task::JoinSet;
use zip::ZipArchive;

const ONE_SECOND_MS: i64 = 1_000;
const RETAINED_POINTS_PER_DAY: usize = 4;

/// Binance BVOL 日包的文件完整性和保留决策点审计。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BinanceBvolAudit {
    /// BTC 与 ETH 在请求区间内的 symbol-day 文件数。
    pub requested_files: usize,
    /// checksum、ZIP、CSV 和四个决策前点均有效的文件数。
    pub available_files: usize,
    /// 官方明确缺失的文件数。
    pub missing_files: usize,
    /// 文件存在但内容合同失败的文件数。
    pub invalid_files: usize,
    /// 合并去重后保留的决策前一秒点数。
    pub retained_points: usize,
    /// 最多十个无效文件及其首个合同错误，便于区分解析缺陷与真实缺档。
    pub invalid_examples: Vec<String>,
}

/// 决策前已经发布的单秒 BVOL 指数值。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BinanceBvolPoint {
    /// 归一到整秒的 UTC Unix 毫秒。
    pub ts: i64,
    /// 严格正且有限的 BVOL 指数值。
    pub value: f64,
}

/// 唯一定位 BTC 或 ETH 的一个官方 BVOL 日包。
#[derive(Debug, Clone, PartialEq, Eq)]
struct BvolFileKey {
    /// `BTC` 或 `ETH`，用于面板映射。
    asset: &'static str,
    /// Binance option BVOL symbol。
    symbol: &'static str,
    /// UTC 日期。
    day: NaiveDate,
}

/// 区分有效、明确缺失和格式无效的官方日包。
enum BvolFileLoad {
    /// 完整校验后的四个决策前点。
    Available(Vec<BinanceBvolPoint>),
    /// 官方 ZIP 或 checksum 返回 404。
    Missing,
    /// 文件存在但 checksum、CSV 或时点合同失败。
    Invalid(String),
}

/// 下载并缓存 BTC/ETH 官方 BVOL 日包，只保留每个 6h 决策前一秒。
pub(super) async fn load_binance_bvol(
    args: &BvolRelativePanelArgs,
    first_day: NaiveDate,
    last_day: NaiveDate,
) -> Result<(
    BTreeMap<&'static str, Vec<BinanceBvolPoint>>,
    BinanceBvolAudit,
)> {
    let cache_dir = args.cache_dir.join("bvol_index");
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("create Binance BVOL cache {}", cache_dir.display()))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .context("build Binance BVOL daily client")?;
    let requested = requested_files(first_day, last_day)?;
    let mut available_files = 0usize;
    let mut missing_files = 0usize;
    let mut invalid_files = 0usize;
    let mut invalid_examples = Vec::new();
    let mut rows = BTreeMap::<&'static str, Vec<BinanceBvolPoint>>::new();
    for chunk in requested.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for key in chunk.iter().cloned() {
            let client = client.clone();
            let data_base = args.binance_data_base.clone();
            let cache_dir = cache_dir.clone();
            tasks.spawn(async move {
                let loaded = load_bvol_file(&client, &data_base, &cache_dir, &key).await?;
                Ok::<_, anyhow::Error>((key, loaded))
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let (key, loaded) = joined.context("join Binance BVOL daily task")??;
            match loaded {
                BvolFileLoad::Available(mut points) => {
                    available_files += 1;
                    rows.entry(key.asset).or_default().append(&mut points);
                }
                BvolFileLoad::Missing => missing_files += 1,
                BvolFileLoad::Invalid(error) => {
                    invalid_files += 1;
                    if invalid_examples.len() < 10 {
                        invalid_examples.push(error);
                    }
                }
            }
        }
    }
    let mut retained_points = 0usize;
    for (asset, points) in &mut rows {
        points.sort_by_key(|point| point.ts);
        if points.windows(2).any(|pair| {
            pair[0].ts == pair[1].ts && pair[0].value.to_bits() != pair[1].value.to_bits()
        }) {
            bail!("conflicting duplicate Binance BVOL points for {asset}");
        }
        points.dedup_by_key(|point| point.ts);
        if points.windows(2).any(|pair| pair[0].ts >= pair[1].ts) {
            bail!("duplicate or unsorted Binance BVOL points for {asset}");
        }
        retained_points += points.len();
    }
    Ok((
        rows,
        BinanceBvolAudit {
            requested_files: requested.len(),
            available_files,
            missing_files,
            invalid_files,
            retained_points,
            invalid_examples,
        },
    ))
}

/// 为两个 BVOL symbol 生成闭区间内的确定性日包请求。
fn requested_files(first_day: NaiveDate, last_day: NaiveDate) -> Result<Vec<BvolFileKey>> {
    if first_day > last_day {
        bail!("BVOL first day must not exceed last day");
    }
    let mut files = Vec::new();
    for (asset, symbol) in [("BTC", "BTCBVOLUSDT"), ("ETH", "ETHBVOLUSDT")] {
        let mut day = first_day;
        while day <= last_day {
            files.push(BvolFileKey { asset, symbol, day });
            day = day
                .checked_add_signed(ChronoDuration::days(1))
                .context("BVOL day overflow")?;
        }
    }
    Ok(files)
}

/// 优先复用本地校验缓存，否则下载官方 ZIP 与 checksum。
async fn load_bvol_file(
    client: &Client,
    data_base: &str,
    cache_dir: &Path,
    key: &BvolFileKey,
) -> Result<BvolFileLoad> {
    let filename = format!(
        "{}-BVOLIndex-{}.zip",
        key.symbol,
        key.day.format("%Y-%m-%d")
    );
    let symbol_dir = cache_dir.join(key.symbol);
    std::fs::create_dir_all(&symbol_dir)
        .with_context(|| format!("create BVOL symbol cache {}", symbol_dir.display()))?;
    let zip_path = symbol_dir.join(&filename);
    let checksum_path = symbol_dir.join(format!("{filename}.CHECKSUM"));
    if zip_path.exists() && checksum_path.exists() {
        if let Ok(points) = parse_verified_file(&zip_path, &checksum_path, key) {
            return Ok(BvolFileLoad::Available(points));
        }
    }
    let url = format!(
        "{data_base}/data/option/daily/BVOLIndex/{}/{}",
        key.symbol, filename
    );
    let checksum_url = format!("{url}.CHECKSUM");
    let (Some(zip_bytes), Some(checksum_bytes)) = (
        download_optional(client, &url).await?,
        download_optional(client, &checksum_url).await?,
    ) else {
        return Ok(BvolFileLoad::Missing);
    };
    let points = match parse_verified_bytes(&zip_bytes, &checksum_bytes, key, &filename) {
        Ok(points) => points,
        Err(error) => {
            return Ok(BvolFileLoad::Invalid(format!(
                "{} {}: {error:#}",
                key.symbol, key.day
            )))
        }
    };
    write_atomic(&zip_path, &zip_bytes)?;
    write_atomic(&checksum_path, &checksum_bytes)?;
    Ok(BvolFileLoad::Available(points))
}

/// 校验缓存后解析 BVOL 日包。
fn parse_verified_file(
    zip_path: &Path,
    checksum_path: &Path,
    key: &BvolFileKey,
) -> Result<Vec<BinanceBvolPoint>> {
    let zip_bytes = std::fs::read(zip_path)
        .with_context(|| format!("read cached BVOL zip {}", zip_path.display()))?;
    let checksum_bytes = std::fs::read(checksum_path)
        .with_context(|| format!("read cached BVOL checksum {}", checksum_path.display()))?;
    let filename = zip_path
        .file_name()
        .and_then(|value| value.to_str())
        .context("BVOL cache filename is not UTF-8")?;
    parse_verified_bytes(&zip_bytes, &checksum_bytes, key, filename)
}

/// 校验官方 SHA-256、CSV 合同和四个决策前一秒点。
fn parse_verified_bytes(
    zip_bytes: &[u8],
    checksum_bytes: &[u8],
    key: &BvolFileKey,
    filename: &str,
) -> Result<Vec<BinanceBvolPoint>> {
    let checksum = std::str::from_utf8(checksum_bytes).context("BVOL checksum is not UTF-8")?;
    let fields = checksum.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 2
        || fields[1].trim_start_matches('*') != filename
        || fields[0].len() != 64
        || !fields[0].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("invalid Binance BVOL checksum format");
    }
    let actual = format!("{:x}", Sha256::digest(zip_bytes));
    if !actual.eq_ignore_ascii_case(fields[0]) {
        bail!("Binance BVOL checksum mismatch");
    }
    let mut archive = ZipArchive::new(Cursor::new(zip_bytes)).context("open BVOL ZIP")?;
    if archive.len() != 1 {
        bail!("Binance BVOL ZIP must contain exactly one CSV");
    }
    let file = archive.by_index(0).context("open BVOL CSV")?;
    let mut lines = BufReader::new(file).lines();
    let header = lines.next().context("missing BVOL header")??;
    if header.trim_end_matches('\r') != "calc_time,symbol,base_asset,quote_asset,index_value" {
        bail!("unexpected Binance BVOL header");
    }
    let day_start = key
        .day
        .and_hms_opt(0, 0, 0)
        .context("build BVOL day start")?
        .and_utc()
        .timestamp_millis();
    let day_end = day_start.saturating_add(24 * 60 * 60 * 1_000);
    let mut retained = Vec::new();
    let mut previous_ts = None;
    for line in lines {
        let line = line.context("read BVOL CSV row")?;
        let fields = line.trim_end_matches('\r').split(',').collect::<Vec<_>>();
        if fields.len() != 5 || fields[1] != key.symbol {
            bail!("BVOL row violates symbol or field count");
        }
        let raw_ts = fields[0].parse::<i64>().context("parse BVOL calc_time")?;
        let ts = normalize_second_timestamp(raw_ts);
        if ts < day_start || ts >= day_end || previous_ts.is_some_and(|prior| ts <= prior) {
            bail!("BVOL row is outside UTC day or not strictly increasing");
        }
        previous_ts = Some(ts);
        let value = fields[4].parse::<f64>().context("parse BVOL index_value")?;
        if !value.is_finite() || value <= 0.0 {
            bail!("BVOL index_value must be finite and positive");
        }
        let time = Utc
            .timestamp_millis_opt(ts)
            .single()
            .context("invalid normalized BVOL timestamp")?;
        if time.minute() == 59 && time.second() == 59 && matches!(time.hour(), 5 | 11 | 17 | 23) {
            retained.push(BinanceBvolPoint { ts, value });
        }
    }
    if retained.len() != RETAINED_POINTS_PER_DAY {
        bail!("BVOL day does not contain all four decision-prior points");
    }
    Ok(retained)
}

/// 把官方毫秒时间戳归一到其所属整秒，不跨秒借用其他点。
fn normalize_second_timestamp(raw_ts: i64) -> i64 {
    raw_ts - raw_ts.rem_euclid(ONE_SECOND_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_normalization_keeps_the_reported_second() {
        let nominal = 1_727_740_800_000;
        assert_eq!(normalize_second_timestamp(nominal + 1), nominal);
        assert_eq!(normalize_second_timestamp(nominal + 999), nominal);
        assert_eq!(normalize_second_timestamp(nominal + 1_000), nominal + 1_000);
    }

    #[test]
    fn requested_days_cover_two_assets_and_both_boundaries() {
        let first = NaiveDate::from_ymd_opt(2024, 10, 1).unwrap();
        let last = NaiveDate::from_ymd_opt(2024, 10, 2).unwrap();
        let files = requested_files(first, last).unwrap();
        assert_eq!(files.len(), 4);
        assert_eq!(files[0].symbol, "BTCBVOLUSDT");
        assert_eq!(files[2].symbol, "ETHBVOLUSDT");
    }
}
