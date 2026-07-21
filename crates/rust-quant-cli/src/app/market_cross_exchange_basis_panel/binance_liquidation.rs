use super::binance_klines::{download_optional, write_atomic};
use super::liquidation_relative::LiquidationRelativePanelArgs;
use super::MS_15M;
use anyhow::{bail, Context, Result};
use chrono::{Duration as ChronoDuration, NaiveDate};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Cursor};
use std::path::Path;
use std::time::Duration;
use tokio::task::JoinSet;
use zip::ZipArchive;

/// Binance COIN-M 强平文件、去重订单和 15m 桶审计。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BinanceLiquidationAudit {
    /// BTC/ETH 在请求区间内的 symbol-day 文件数。
    pub requested_files: usize,
    /// checksum、ZIP 和 CSV 合同有效的文件数。
    pub available_files: usize,
    /// 官方明确缺失的文件数。
    pub missing_files: usize,
    /// 文件存在但内容合同失败的文件数。
    pub invalid_files: usize,
    /// BTC 有效 UTC 日数。
    pub btc_valid_days: usize,
    /// ETH 有效 UTC 日数。
    pub eth_valid_days: usize,
    /// CSV 中包含的原始 snapshot 行数。
    pub raw_rows: usize,
    /// 完全重复与同语义更新归并后的强平订单数。
    pub unique_orders: usize,
    /// 最多十个无效文件及其首个合同错误。
    pub invalid_examples: Vec<String>,
}

/// 每个资产按 15m 聚合后的净强平卖压与有效日期。
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct BinanceLiquidationData {
    /// `BTC`/`ETH` 到 `15m bucket -> SELL_USD-BUY_USD`。
    pub buckets: BTreeMap<&'static str, BTreeMap<i64, f64>>,
    /// 只有官方文件完整有效的 UTC 日期才进入集合。
    pub valid_days: BTreeMap<&'static str, BTreeSet<NaiveDate>>,
}

/// 唯一定位 BTC 或 ETH 的一个官方 COIN-M 强平日包。
#[derive(Debug, Clone, PartialEq, Eq)]
struct LiquidationFileKey {
    /// `BTC` 或 `ETH`。
    asset: &'static str,
    /// Binance COIN-M perpetual symbol。
    symbol: &'static str,
    /// 稳定的美元合约面值。
    contract_size_usd: u64,
    /// UTC 日期。
    day: NaiveDate,
}

/// 同一 snapshot 语义键只保留最大累计成交，避免官方重复行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct LiquidationOrderKey {
    /// 官方事件时间。
    time: i64,
    /// `SELL=true`，`BUY=false`。
    forced_sell: bool,
    /// 原始数量位模式。
    original_quantity_bits: u64,
    /// 委托价格位模式。
    price_bits: u64,
    /// 平均成交价位模式。
    average_price_bits: u64,
}

/// 一个有效日包的聚合结果。
struct ParsedLiquidationDay {
    /// 当日非零 15m 净强平桶。
    buckets: BTreeMap<i64, f64>,
    /// CSV 原始行数。
    raw_rows: usize,
    /// 去重后订单数。
    unique_orders: usize,
}

/// 区分有效、明确缺失和格式无效的官方日包。
enum LiquidationFileLoad {
    /// 完整校验后的日聚合。
    Available(ParsedLiquidationDay),
    /// 官方 ZIP 或 checksum 404。
    Missing,
    /// 文件存在但 checksum 或 CSV 合同失败。
    Invalid(String),
}

/// 下载并缓存 BTC/ETH COIN-M 强平日包，输出严格 15m 聚合。
pub(super) async fn load_binance_liquidations(
    args: &LiquidationRelativePanelArgs,
    first_day: NaiveDate,
    last_day: NaiveDate,
) -> Result<(BinanceLiquidationData, BinanceLiquidationAudit)> {
    let cache_dir = args.cache_dir.join("coinm_liquidation_snapshot");
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("create Binance liquidation cache {}", cache_dir.display()))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .context("build Binance liquidation daily client")?;
    let requested = requested_files(first_day, last_day)?;
    let mut data = BinanceLiquidationData::default();
    let mut audit = BinanceLiquidationAudit {
        requested_files: requested.len(),
        ..BinanceLiquidationAudit::default()
    };
    for chunk in requested.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for key in chunk.iter().cloned() {
            let client = client.clone();
            let data_base = args.binance_data_base.clone();
            let cache_dir = cache_dir.clone();
            tasks.spawn(async move {
                let loaded = load_liquidation_file(&client, &data_base, &cache_dir, &key).await?;
                Ok::<_, anyhow::Error>((key, loaded))
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let (key, loaded) = joined.context("join Binance liquidation daily task")??;
            match loaded {
                LiquidationFileLoad::Available(parsed) => {
                    audit.available_files += 1;
                    audit.raw_rows += parsed.raw_rows;
                    audit.unique_orders += parsed.unique_orders;
                    data.valid_days
                        .entry(key.asset)
                        .or_default()
                        .insert(key.day);
                    let asset_buckets = data.buckets.entry(key.asset).or_default();
                    for (ts, value) in parsed.buckets {
                        *asset_buckets.entry(ts).or_default() += value;
                    }
                }
                LiquidationFileLoad::Missing => audit.missing_files += 1,
                LiquidationFileLoad::Invalid(error) => {
                    audit.invalid_files += 1;
                    if audit.invalid_examples.len() < 10 {
                        audit.invalid_examples.push(error);
                    }
                }
            }
        }
    }
    audit.btc_valid_days = data.valid_days.get("BTC").map_or(0, BTreeSet::len);
    audit.eth_valid_days = data.valid_days.get("ETH").map_or(0, BTreeSet::len);
    Ok((data, audit))
}

/// 为 BTC 100USD 与 ETH 10USD 合约生成闭区间日包请求。
fn requested_files(first_day: NaiveDate, last_day: NaiveDate) -> Result<Vec<LiquidationFileKey>> {
    if first_day > last_day {
        bail!("liquidation first day must not exceed last day");
    }
    let mut files = Vec::new();
    for (asset, symbol, contract_size_usd) in
        [("BTC", "BTCUSD_PERP", 100), ("ETH", "ETHUSD_PERP", 10)]
    {
        let mut day = first_day;
        while day <= last_day {
            files.push(LiquidationFileKey {
                asset,
                symbol,
                contract_size_usd,
                day,
            });
            day = day
                .checked_add_signed(ChronoDuration::days(1))
                .context("liquidation day overflow")?;
        }
    }
    Ok(files)
}

/// 优先复用本地校验缓存，否则下载官方 ZIP 与 checksum。
async fn load_liquidation_file(
    client: &Client,
    data_base: &str,
    cache_dir: &Path,
    key: &LiquidationFileKey,
) -> Result<LiquidationFileLoad> {
    let filename = format!(
        "{}-liquidationSnapshot-{}.zip",
        key.symbol,
        key.day.format("%Y-%m-%d")
    );
    let symbol_dir = cache_dir.join(key.symbol);
    std::fs::create_dir_all(&symbol_dir)
        .with_context(|| format!("create liquidation cache {}", symbol_dir.display()))?;
    let zip_path = symbol_dir.join(&filename);
    let checksum_path = symbol_dir.join(format!("{filename}.CHECKSUM"));
    if zip_path.exists() && checksum_path.exists() {
        if let Ok(parsed) = parse_verified_file(&zip_path, &checksum_path, key) {
            return Ok(LiquidationFileLoad::Available(parsed));
        }
    }
    let url = format!(
        "{data_base}/data/futures/cm/daily/liquidationSnapshot/{}/{}",
        key.symbol, filename
    );
    let checksum_url = format!("{url}.CHECKSUM");
    let (Some(zip_bytes), Some(checksum_bytes)) = (
        download_optional(client, &url).await?,
        download_optional(client, &checksum_url).await?,
    ) else {
        return Ok(LiquidationFileLoad::Missing);
    };
    let parsed = match parse_verified_bytes(&zip_bytes, &checksum_bytes, key, &filename) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Ok(LiquidationFileLoad::Invalid(format!(
                "{} {}: {error:#}",
                key.symbol, key.day
            )))
        }
    };
    write_atomic(&zip_path, &zip_bytes)?;
    write_atomic(&checksum_path, &checksum_bytes)?;
    Ok(LiquidationFileLoad::Available(parsed))
}

/// 校验缓存后解析强平日包。
fn parse_verified_file(
    zip_path: &Path,
    checksum_path: &Path,
    key: &LiquidationFileKey,
) -> Result<ParsedLiquidationDay> {
    let zip_bytes = std::fs::read(zip_path)
        .with_context(|| format!("read cached liquidation zip {}", zip_path.display()))?;
    let checksum_bytes = std::fs::read(checksum_path).with_context(|| {
        format!(
            "read cached liquidation checksum {}",
            checksum_path.display()
        )
    })?;
    let filename = zip_path
        .file_name()
        .and_then(|value| value.to_str())
        .context("liquidation cache filename is not UTF-8")?;
    parse_verified_bytes(&zip_bytes, &checksum_bytes, key, filename)
}

/// 校验官方 SHA-256 后，把去重订单按真实事件时间聚合到 15m。
fn parse_verified_bytes(
    zip_bytes: &[u8],
    checksum_bytes: &[u8],
    key: &LiquidationFileKey,
    filename: &str,
) -> Result<ParsedLiquidationDay> {
    let checksum =
        std::str::from_utf8(checksum_bytes).context("liquidation checksum is not UTF-8")?;
    let fields = checksum.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 2
        || fields[1].trim_start_matches('*') != filename
        || fields[0].len() != 64
        || !fields[0].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("invalid Binance liquidation checksum format");
    }
    let actual = format!("{:x}", Sha256::digest(zip_bytes));
    if !actual.eq_ignore_ascii_case(fields[0]) {
        bail!("Binance liquidation checksum mismatch");
    }
    let mut archive = ZipArchive::new(Cursor::new(zip_bytes)).context("open liquidation ZIP")?;
    if archive.len() != 1 {
        bail!("Binance liquidation ZIP must contain exactly one CSV");
    }
    let file = archive.by_index(0).context("open liquidation CSV")?;
    let mut lines = BufReader::new(file).lines();
    let header = lines.next().context("missing liquidation header")??;
    if header.trim_end_matches('\r')
        != "time,side,order_type,time_in_force,original_quantity,price,average_price,order_status,last_fill_quantity,accumulated_fill_quantity"
    {
        bail!("unexpected Binance liquidation header");
    }
    let day_start = key
        .day
        .and_hms_opt(0, 0, 0)
        .context("build liquidation day start")?
        .and_utc()
        .timestamp_millis();
    let day_end = day_start.saturating_add(24 * 60 * 60 * 1_000);
    let mut raw_rows = 0usize;
    let mut orders = BTreeMap::<LiquidationOrderKey, f64>::new();
    for line in lines {
        let line = line.context("read liquidation CSV row")?;
        let fields = line.trim_end_matches('\r').split(',').collect::<Vec<_>>();
        if fields.len() != 10 {
            bail!("liquidation row has unexpected field count");
        }
        raw_rows += 1;
        let time = fields[0].parse::<i64>().context("parse liquidation time")?;
        if time < day_start || time >= day_end {
            bail!("liquidation row is outside requested UTC day");
        }
        let forced_sell = match fields[1] {
            "SELL" => true,
            "BUY" => false,
            _ => bail!("liquidation side must be BUY or SELL"),
        };
        if !matches!(fields[7], "FILLED" | "PARTIALLY_FILLED") {
            continue;
        }
        let original_quantity = parse_positive(fields[4], "original_quantity")?;
        let price = parse_positive(fields[5], "price")?;
        let average_price = parse_positive(fields[6], "average_price")?;
        let accumulated = parse_positive(fields[9], "accumulated_fill_quantity")?;
        let order_key = LiquidationOrderKey {
            time,
            forced_sell,
            original_quantity_bits: original_quantity.to_bits(),
            price_bits: price.to_bits(),
            average_price_bits: average_price.to_bits(),
        };
        orders
            .entry(order_key)
            .and_modify(|current| *current = current.max(accumulated))
            .or_insert(accumulated);
    }
    let unique_orders = orders.len();
    let mut buckets = BTreeMap::<i64, f64>::new();
    for (order, accumulated) in orders {
        let bucket = order.time - order.time.rem_euclid(MS_15M);
        let notional = accumulated * key.contract_size_usd as f64;
        let signed = if order.forced_sell {
            notional
        } else {
            -notional
        };
        *buckets.entry(bucket).or_default() += signed;
    }
    Ok(ParsedLiquidationDay {
        buckets,
        raw_rows,
        unique_orders,
    })
}

/// 解析严格正且有限的强平字段。
fn parse_positive(value: &str, field: &str) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse liquidation {field}"))?;
    if !parsed.is_finite() || parsed <= 0.0 {
        bail!("liquidation {field} must be finite and positive");
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_days_preserve_official_contract_sizes() {
        let day = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap();
        let files = requested_files(day, day).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].contract_size_usd, 100);
        assert_eq!(files[1].contract_size_usd, 10);
    }

    #[test]
    fn positive_parser_rejects_zero_and_nan() {
        assert_eq!(parse_positive("10", "quantity").unwrap(), 10.0);
        assert!(parse_positive("0", "quantity").is_err());
        assert!(parse_positive("NaN", "quantity").is_err());
    }
}
