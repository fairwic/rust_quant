use super::binance_klines::write_atomic;
use super::large_trade_absorption::LargeTradePanelArgs;
use anyhow::{bail, Context, Result};
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Cursor};
use std::path::Path;
use std::time::Duration;
use tokio::task::JoinSet;
use zip::ZipArchive;

const MS_6H: i64 = 6 * 60 * 60 * 1_000;

/// Binance aggTrades 月包与逐笔解析审计。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BinanceAggTradesAudit {
    /// BTC/ETH 共十三个月的请求文件数。
    pub requested_files: usize,
    /// checksum、ZIP、CSV 和顺序合同有效的文件数。
    pub available_files: usize,
    /// 官方明确缺失的文件数。
    pub missing_files: usize,
    /// 文件存在但内容合同失败的文件数。
    pub invalid_files: usize,
    /// 解析的 aggregate trade 行数。
    pub parsed_rows: usize,
    /// 最多十个无效文件及其首个合同错误。
    pub invalid_examples: Vec<String>,
}

/// 一个 6h 桶内按成交额平方加权的主动流累积量。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct TailFlowBucket {
    /// 主动买为正、主动卖为负的成交额平方和。
    pub signed_square_notional: f64,
    /// 不分方向的成交额平方和。
    pub total_square_notional: f64,
}

impl TailFlowBucket {
    /// 返回 `[-1,1]` 尾部主动流压力；无成交或非有限时阻塞。
    pub fn pressure(self) -> Option<f64> {
        (self.total_square_notional.is_finite() && self.total_square_notional > 0.0)
            .then_some(self.signed_square_notional / self.total_square_notional)
            .filter(|value| value.is_finite() && (-1.0..=1.0).contains(value))
    }
}

/// BTC/ETH 到 UTC 对齐 6h 尾部主动流桶。
pub(super) type BinanceAggTradesData = BTreeMap<&'static str, BTreeMap<i64, TailFlowBucket>>;

/// 唯一定位一个 USD-M aggTrades 月包。
#[derive(Debug, Clone, PartialEq, Eq)]
struct AggTradesFileKey {
    /// `BTC` 或 `ETH`。
    asset: &'static str,
    /// Binance USD-M symbol。
    symbol: &'static str,
    /// UTC 年。
    year: i32,
    /// UTC 月。
    month: u32,
}

/// 一个有效月包的流桶与行数。
struct ParsedAggTradesMonth {
    /// 当月 UTC 6h 桶。
    buckets: BTreeMap<i64, TailFlowBucket>,
    /// aggregate trade 行数。
    parsed_rows: usize,
}

/// 区分有效、明确缺失和格式无效的官方月包。
enum AggTradesFileLoad {
    /// 完整校验后的月聚合。
    Available(ParsedAggTradesMonth),
    /// 官方 ZIP 或 checksum 404。
    Missing,
    /// 文件存在但 checksum 或 CSV 合同失败。
    Invalid(String),
}

/// 下载并缓存 BTC/ETH aggTrades 月包，直接聚合为 6h 而不生成 1m。
pub(super) async fn load_binance_aggtrades(
    args: &LargeTradePanelArgs,
    first_month: NaiveDate,
    last_month: NaiveDate,
) -> Result<(BinanceAggTradesData, BinanceAggTradesAudit)> {
    let cache_dir = args.cache_dir.join("aggtrades_monthly");
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("create aggTrades cache {}", cache_dir.display()))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(900))
        .build()
        .context("build Binance aggTrades monthly client")?;
    let requested = requested_files(first_month, last_month)?;
    let mut data = BinanceAggTradesData::new();
    let mut audit = BinanceAggTradesAudit {
        requested_files: requested.len(),
        ..BinanceAggTradesAudit::default()
    };
    for chunk in requested.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for key in chunk.iter().cloned() {
            let client = client.clone();
            let data_base = args.binance_data_base.clone();
            let cache_dir = cache_dir.clone();
            tasks.spawn(async move {
                let loaded = load_aggtrades_file(&client, &data_base, &cache_dir, &key).await?;
                Ok::<_, anyhow::Error>((key, loaded))
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let (key, loaded) = joined.context("join Binance aggTrades monthly task")??;
            match loaded {
                AggTradesFileLoad::Available(parsed) => {
                    audit.available_files += 1;
                    audit.parsed_rows += parsed.parsed_rows;
                    let asset_buckets = data.entry(key.asset).or_default();
                    for (ts, bucket) in parsed.buckets {
                        let merged = asset_buckets.entry(ts).or_default();
                        merged.signed_square_notional += bucket.signed_square_notional;
                        merged.total_square_notional += bucket.total_square_notional;
                    }
                }
                AggTradesFileLoad::Missing => audit.missing_files += 1,
                AggTradesFileLoad::Invalid(error) => {
                    audit.invalid_files += 1;
                    if audit.invalid_examples.len() < 10 {
                        audit.invalid_examples.push(error);
                    }
                }
            }
        }
    }
    Ok((data, audit))
}

/// 为 BTC/ETH 生成包含首尾月份的确定性请求。
fn requested_files(first_month: NaiveDate, last_month: NaiveDate) -> Result<Vec<AggTradesFileKey>> {
    if first_month > last_month || first_month.day() != 1 || last_month.day() != 1 {
        bail!("aggTrades month boundaries must be ordered month starts");
    }
    let mut files = Vec::new();
    for (asset, symbol) in [("BTC", "BTCUSDT"), ("ETH", "ETHUSDT")] {
        let mut month = first_month;
        while month <= last_month {
            files.push(AggTradesFileKey {
                asset,
                symbol,
                year: month.year(),
                month: month.month(),
            });
            month = next_month(month)?;
        }
    }
    Ok(files)
}

/// 计算下一个 UTC 月初。
fn next_month(month: NaiveDate) -> Result<NaiveDate> {
    let (year, next) = if month.month() == 12 {
        (month.year() + 1, 1)
    } else {
        (month.year(), month.month() + 1)
    };
    NaiveDate::from_ymd_opt(year, next, 1).context("aggTrades month overflow")
}

/// 优先复用本地校验缓存，否则下载大体积官方 ZIP 与 checksum。
async fn load_aggtrades_file(
    client: &Client,
    data_base: &str,
    cache_dir: &Path,
    key: &AggTradesFileKey,
) -> Result<AggTradesFileLoad> {
    let filename = format!(
        "{}-aggTrades-{:04}-{:02}.zip",
        key.symbol, key.year, key.month
    );
    let symbol_dir = cache_dir.join(key.symbol);
    std::fs::create_dir_all(&symbol_dir)
        .with_context(|| format!("create aggTrades symbol cache {}", symbol_dir.display()))?;
    let zip_path = symbol_dir.join(&filename);
    let checksum_path = symbol_dir.join(format!("{filename}.CHECKSUM"));
    if zip_path.exists() && checksum_path.exists() {
        if let Ok(parsed) = parse_verified_file(&zip_path, &checksum_path, key) {
            return Ok(AggTradesFileLoad::Available(parsed));
        }
    }
    let url = format!(
        "{data_base}/data/futures/um/monthly/aggTrades/{}/{}",
        key.symbol, filename
    );
    let checksum_url = format!("{url}.CHECKSUM");
    let (Some(zip_bytes), Some(checksum_bytes)) = (
        download_optional(client, &url).await?,
        download_optional(client, &checksum_url).await?,
    ) else {
        return Ok(AggTradesFileLoad::Missing);
    };
    let parsed = match parse_verified_bytes(&zip_bytes, &checksum_bytes, key, &filename) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Ok(AggTradesFileLoad::Invalid(format!(
                "{} {:04}-{:02}: {error:#}",
                key.symbol, key.year, key.month
            )))
        }
    };
    write_atomic(&zip_path, &zip_bytes)?;
    write_atomic(&checksum_path, &checksum_bytes)?;
    Ok(AggTradesFileLoad::Available(parsed))
}

/// 下载公开文件；404 表示明确缺失，其他错误保留上下文。
async fn download_optional(client: &Client, url: &str) -> Result<Option<Vec<u8>>> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("download {url}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response
        .error_for_status()
        .with_context(|| format!("download status {url}"))?;
    Ok(Some(
        response
            .bytes()
            .await
            .with_context(|| format!("read download body {url}"))?
            .to_vec(),
    ))
}

/// 校验缓存后解析 aggTrades 月包。
fn parse_verified_file(
    zip_path: &Path,
    checksum_path: &Path,
    key: &AggTradesFileKey,
) -> Result<ParsedAggTradesMonth> {
    let zip_bytes = std::fs::read(zip_path)
        .with_context(|| format!("read cached aggTrades zip {}", zip_path.display()))?;
    let checksum_bytes = std::fs::read(checksum_path)
        .with_context(|| format!("read aggTrades checksum {}", checksum_path.display()))?;
    let filename = zip_path
        .file_name()
        .and_then(|value| value.to_str())
        .context("aggTrades cache filename is not UTF-8")?;
    parse_verified_bytes(&zip_bytes, &checksum_bytes, key, filename)
}

/// 校验官方 SHA-256 后按真实成交时间直接聚合到 6h。
fn parse_verified_bytes(
    zip_bytes: &[u8],
    checksum_bytes: &[u8],
    key: &AggTradesFileKey,
    filename: &str,
) -> Result<ParsedAggTradesMonth> {
    let checksum =
        std::str::from_utf8(checksum_bytes).context("aggTrades checksum is not UTF-8")?;
    let fields = checksum.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 2
        || fields[1].trim_start_matches('*') != filename
        || fields[0].len() != 64
        || !fields[0].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("invalid Binance aggTrades checksum format");
    }
    let actual = format!("{:x}", Sha256::digest(zip_bytes));
    if !actual.eq_ignore_ascii_case(fields[0]) {
        bail!("Binance aggTrades checksum mismatch");
    }
    let mut archive = ZipArchive::new(Cursor::new(zip_bytes)).context("open aggTrades ZIP")?;
    if archive.len() != 1 {
        bail!("Binance aggTrades ZIP must contain exactly one CSV");
    }
    let file = archive.by_index(0).context("open aggTrades CSV")?;
    let reader = BufReader::new(file);
    let mut buckets = BTreeMap::<i64, TailFlowBucket>::new();
    let mut parsed_rows = 0usize;
    let mut previous_id = None;
    let mut previous_time = None;
    for (line_number, line) in reader.lines().enumerate() {
        let line = line.context("read aggTrades CSV row")?;
        if line_number == 0 && line.starts_with("agg_trade_id,") {
            continue;
        }
        let fields = line.trim_end_matches('\r').split(',').collect::<Vec<_>>();
        if fields.len() != 7 {
            bail!("aggTrades row has unexpected field count");
        }
        let aggregate_id = fields[0].parse::<u64>().context("parse agg_trade_id")?;
        let price = parse_positive(fields[1], "price")?;
        let quantity = parse_positive(fields[2], "quantity")?;
        let transact_time = fields[5]
            .parse::<i64>()
            .context("parse aggTrades transact_time")?;
        let buyer_is_maker = fields[6]
            .parse::<bool>()
            .context("parse aggTrades is_buyer_maker")?;
        let timestamp = Utc
            .timestamp_millis_opt(transact_time)
            .single()
            .context("invalid aggTrades timestamp")?;
        if timestamp.year() != key.year || timestamp.month() != key.month {
            bail!("aggTrades row is outside requested UTC month");
        }
        if previous_id.is_some_and(|prior| aggregate_id <= prior)
            || previous_time.is_some_and(|prior| transact_time < prior)
        {
            bail!("aggTrades ids or timestamps are not strictly ordered");
        }
        previous_id = Some(aggregate_id);
        previous_time = Some(transact_time);
        let notional = price * quantity;
        let square = notional * notional;
        if !square.is_finite() || square <= 0.0 {
            bail!("aggTrades squared notional must be finite and positive");
        }
        let bucket_ts = transact_time - transact_time.rem_euclid(MS_6H);
        let bucket = buckets.entry(bucket_ts).or_default();
        bucket.total_square_notional += square;
        bucket.signed_square_notional += if buyer_is_maker { -square } else { square };
        if !bucket.total_square_notional.is_finite() || !bucket.signed_square_notional.is_finite() {
            bail!("aggTrades bucket accumulation overflow");
        }
        parsed_rows += 1;
    }
    if parsed_rows == 0 {
        bail!("aggTrades monthly CSV is empty");
    }
    Ok(ParsedAggTradesMonth {
        buckets,
        parsed_rows,
    })
}

/// 解析严格正且有限的逐笔成交字段。
fn parse_positive(value: &str, field: &str) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse aggTrades {field}"))?;
    if !parsed.is_finite() || parsed <= 0.0 {
        bail!("aggTrades {field} must be finite and positive");
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_preserves_large_trade_direction() {
        let bucket = TailFlowBucket {
            signed_square_notional: -75.0,
            total_square_notional: 125.0,
        };
        assert_eq!(bucket.pressure(), Some(-0.6));
    }

    #[test]
    fn requested_months_cover_both_assets_and_boundaries() {
        let first = NaiveDate::from_ymd_opt(2024, 6, 1).unwrap();
        let last = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap();
        let files = requested_files(first, last).unwrap();
        assert_eq!(files.len(), 4);
        assert_eq!(files[0].symbol, "BTCUSDT");
        assert_eq!(files[2].symbol, "ETHUSDT");
    }
}
