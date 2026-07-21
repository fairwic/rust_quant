use super::binance_klines::{
    download_optional, load_current_live_crypto_perpetuals, map_okx_symbol, write_atomic,
};
use super::{CrossExchangeBasisPanelArgs, UniverseSchedule, MS_15M};
use anyhow::{bail, Context, Result};
use chrono::{Datelike, TimeZone, Utc};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Cursor};
use std::path::Path;
use std::time::Duration;
use tokio::task::JoinSet;
use zip::ZipArchive;

/// Binance 官方 funding 月包的映射、覆盖和内容审计。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BinanceFundingAudit {
    /// 同时满足当前 Binance crypto perpetual 合同的 OKX 映射数。
    pub mapped_symbols: usize,
    /// 当前无法映射到 Binance 的 OKX 币种数。
    pub mapping_blocked_symbols: usize,
    /// 请求的合约月包数。
    pub requested_files: usize,
    /// checksum、ZIP 和 CSV 均有效的月包数。
    pub available_files: usize,
    /// 官方明确不存在的月包数。
    pub missing_files: usize,
    /// 文件存在但校验或内容无效的月包数。
    pub invalid_files: usize,
    /// 合并去重后的 funding 行数。
    pub parsed_rows: usize,
}

/// 已按最多一秒偏移归一到 15m 边界的 Binance funding 事实。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BinanceFundingPoint {
    /// Funding 结算时间，Unix 毫秒。
    pub ts: i64,
    /// 官方声明的结算间隔小时数。
    pub interval_hours: i64,
    /// 该时点实际结算的费率。
    pub rate: f64,
}

/// 唯一定位一个 Binance funding 合约月包。
#[derive(Debug, Clone, PartialEq, Eq)]
struct FundingFileKey {
    /// OKX 标准 symbol，用于与历史币池连接。
    okx_symbol: String,
    /// Binance USD-M symbol，用于官方路径。
    binance_symbol: String,
    /// UTC 年。
    year: i32,
    /// UTC 月。
    month: u32,
}

/// 区分有效、明确缺失和内容无效的官方 funding 月包。
enum FundingFileLoad {
    /// 完整校验后的 funding 行。
    Available(Vec<BinanceFundingPoint>),
    /// 官方 ZIP 或 checksum 返回 404。
    Missing,
    /// 文件存在但 checksum、ZIP 或 CSV 合同失败。
    Invalid,
}

/// 下载并校验当前 live 映射成员的 Binance 官方 funding 月包。
pub(super) async fn load_binance_funding(
    args: &CrossExchangeBasisPanelArgs,
    schedule: &UniverseSchedule,
) -> Result<(
    BTreeMap<String, Vec<BinanceFundingPoint>>,
    BinanceFundingAudit,
)> {
    let funding_cache = args.cache_dir.join("funding_rate");
    std::fs::create_dir_all(&funding_cache)
        .with_context(|| format!("create Binance funding cache {}", funding_cache.display()))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .context("build Binance funding monthly client")?;
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
    let requested = requested_funding_files(schedule, &mapping)?;
    let mut available_files = 0usize;
    let mut missing_files = 0usize;
    let mut invalid_files = 0usize;
    let mut rows = BTreeMap::<String, Vec<BinanceFundingPoint>>::new();
    for chunk in requested.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for key in chunk.iter().cloned() {
            let client = client.clone();
            let cache_dir = funding_cache.clone();
            let data_base = args.binance_data_base.clone();
            tasks.spawn(async move {
                let loaded = load_funding_file(&client, &data_base, &cache_dir, &key).await?;
                Ok::<_, anyhow::Error>((key, loaded))
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let (key, loaded) = joined.context("join Binance funding monthly task")??;
            match loaded {
                FundingFileLoad::Available(mut points) => {
                    available_files += 1;
                    rows.entry(key.okx_symbol).or_default().append(&mut points);
                }
                FundingFileLoad::Missing => missing_files += 1,
                FundingFileLoad::Invalid => invalid_files += 1,
            }
        }
    }
    let mut parsed_rows = 0usize;
    for (symbol, points) in &mut rows {
        points.sort_by_key(|point| point.ts);
        if points.windows(2).any(|pair| pair[0].ts >= pair[1].ts) {
            bail!("duplicate or unsorted Binance funding rows after merge for {symbol}");
        }
        parsed_rows += points.len();
    }
    let audit = BinanceFundingAudit {
        mapped_symbols: mapping.len(),
        mapping_blocked_symbols: all_symbols.len().saturating_sub(mapping.len()),
        requested_files: requested.len(),
        available_files,
        missing_files,
        invalid_files,
        parsed_rows,
    };
    Ok((rows, audit))
}

/// 构造研究窗口及尾部下一结算所需的全部 UTC 月份。
fn requested_funding_files(
    schedule: &UniverseSchedule,
    mapping: &BTreeMap<String, String>,
) -> Result<Vec<FundingFileKey>> {
    let first = schedule
        .windows
        .first()
        .context("missing first universe month")?;
    let last = schedule
        .windows
        .last()
        .context("missing last universe month")?;
    let start = Utc
        .timestamp_millis_opt(first.from_ms)
        .single()
        .context("invalid Binance funding start")?;
    let end = Utc
        .timestamp_millis_opt(last.to_ms.saturating_add(24 * 60 * 60 * 1_000))
        .single()
        .context("invalid Binance funding end")?;
    let mut months = Vec::<(i32, u32)>::new();
    let (mut year, mut month) = (start.year(), start.month());
    loop {
        months.push((year, month));
        if year == end.year() && month == end.month() {
            break;
        }
        (year, month) = next_month(year, month);
        if months.len() > 18 {
            bail!("unexpectedly wide Binance funding request range");
        }
    }
    Ok(mapping
        .iter()
        .flat_map(|(okx_symbol, binance_symbol)| {
            months.iter().map(move |(year, month)| FundingFileKey {
                okx_symbol: okx_symbol.clone(),
                binance_symbol: binance_symbol.clone(),
                year: *year,
                month: *month,
            })
        })
        .collect())
}

/// 返回给定年月的下一个 UTC 月份。
fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

/// 优先复用校验缓存，否则下载官方 ZIP 和 checksum。
async fn load_funding_file(
    client: &Client,
    data_base: &str,
    cache_dir: &Path,
    key: &FundingFileKey,
) -> Result<FundingFileLoad> {
    let filename = format!(
        "{}-fundingRate-{:04}-{:02}.zip",
        key.binance_symbol, key.year, key.month
    );
    let symbol_dir = cache_dir.join(&key.binance_symbol);
    std::fs::create_dir_all(&symbol_dir).with_context(|| {
        format!(
            "create Binance funding symbol cache {}",
            symbol_dir.display()
        )
    })?;
    let zip_path = symbol_dir.join(&filename);
    let checksum_path = symbol_dir.join(format!("{filename}.CHECKSUM"));
    if zip_path.exists() && checksum_path.exists() {
        if let Ok(points) = parse_verified_file(&zip_path, &checksum_path, key) {
            return Ok(FundingFileLoad::Available(points));
        }
    }
    let url = format!(
        "{data_base}/data/futures/um/monthly/fundingRate/{}/{}",
        key.binance_symbol, filename
    );
    let checksum_url = format!("{url}.CHECKSUM");
    let (Some(zip_bytes), Some(checksum_bytes)) = (
        download_optional(client, &url).await?,
        download_optional(client, &checksum_url).await?,
    ) else {
        return Ok(FundingFileLoad::Missing);
    };
    let points = match parse_verified_bytes(&zip_bytes, &checksum_bytes, key, &filename) {
        Ok(points) => points,
        Err(_) => return Ok(FundingFileLoad::Invalid),
    };
    write_atomic(&zip_path, &zip_bytes)?;
    write_atomic(&checksum_path, &checksum_bytes)?;
    Ok(FundingFileLoad::Available(points))
}

/// 校验本地缓存的官方 checksum 后解析 funding CSV。
fn parse_verified_file(
    zip_path: &Path,
    checksum_path: &Path,
    key: &FundingFileKey,
) -> Result<Vec<BinanceFundingPoint>> {
    let zip_bytes = std::fs::read(zip_path)
        .with_context(|| format!("read cached Binance funding zip {}", zip_path.display()))?;
    let checksum_bytes = std::fs::read(checksum_path).with_context(|| {
        format!(
            "read cached Binance funding checksum {}",
            checksum_path.display()
        )
    })?;
    let filename = zip_path
        .file_name()
        .and_then(|value| value.to_str())
        .context("cached Binance funding filename is not UTF-8")?;
    parse_verified_bytes(&zip_bytes, &checksum_bytes, key, filename)
}

/// 校验 SHA-256、唯一 CSV、表头、月份和 funding 数值合同。
fn parse_verified_bytes(
    zip_bytes: &[u8],
    checksum_bytes: &[u8],
    key: &FundingFileKey,
    filename: &str,
) -> Result<Vec<BinanceFundingPoint>> {
    let checksum =
        std::str::from_utf8(checksum_bytes).context("Binance funding checksum is not UTF-8")?;
    let fields = checksum.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 2 || fields[1] != filename || fields[0].len() != 64 {
        bail!("invalid Binance funding checksum format");
    }
    let actual = format!("{:x}", Sha256::digest(zip_bytes));
    if !actual.eq_ignore_ascii_case(fields[0]) {
        bail!("Binance funding checksum mismatch");
    }
    let mut archive =
        ZipArchive::new(Cursor::new(zip_bytes)).context("open Binance funding ZIP")?;
    if archive.len() != 1 {
        bail!("Binance funding ZIP must contain exactly one CSV");
    }
    let file = archive.by_index(0).context("read Binance funding CSV")?;
    let mut lines = BufReader::new(file).lines();
    let header = lines
        .next()
        .context("missing Binance funding CSV header")??;
    if header.trim_end_matches('\r') != "calc_time,funding_interval_hours,last_funding_rate" {
        bail!("unexpected Binance funding CSV header");
    }
    let mut points = Vec::new();
    for line in lines {
        let line = line.context("read Binance funding CSV row")?;
        let fields = line.trim_end_matches('\r').split(',').collect::<Vec<_>>();
        if fields.len() != 3 {
            bail!("Binance funding row must contain exactly three fields");
        }
        let raw_ts = fields[0]
            .parse::<i64>()
            .context("parse funding calc_time")?;
        let interval_hours = fields[1]
            .parse::<i64>()
            .context("parse funding interval hours")?;
        let rate = fields[2]
            .parse::<f64>()
            .context("parse Binance funding rate")?;
        let timestamp = Utc
            .timestamp_millis_opt(raw_ts)
            .single()
            .context("invalid Binance funding timestamp")?;
        let normalized_ts = raw_ts - raw_ts.rem_euclid(MS_15M);
        if timestamp.year() != key.year
            || timestamp.month() != key.month
            || raw_ts - normalized_ts > 1_000
            || !(1..=8).contains(&interval_hours)
            || !rate.is_finite()
            || rate.abs() > 0.1
        {
            bail!("Binance funding row violates frozen timestamp or value contract");
        }
        points.push(BinanceFundingPoint {
            ts: normalized_ts,
            interval_hours,
            rate,
        });
    }
    if points.is_empty() || points.windows(2).any(|pair| pair[0].ts >= pair[1].ts) {
        bail!("Binance funding CSV is empty, duplicated or unsorted");
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造官方格式 ZIP 与 checksum，覆盖毫秒归一和严格表头。
    fn archive(csv: &str, filename: &str) -> (Vec<u8>, Vec<u8>) {
        use std::io::Write;
        use zip::write::FileOptions;
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut bytes);
            writer
                .start_file("funding.csv", FileOptions::default())
                .unwrap();
            writer.write_all(csv.as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        let zip = bytes.into_inner();
        let checksum = format!("{:x}  {filename}\n", Sha256::digest(&zip)).into_bytes();
        (zip, checksum)
    }

    #[test]
    fn parser_normalizes_one_millisecond_and_rejects_larger_drift() {
        let filename = "BTCUSDT-fundingRate-2024-07.zip";
        let key = FundingFileKey {
            okx_symbol: "BTC-USDT-SWAP".to_owned(),
            binance_symbol: "BTCUSDT".to_owned(),
            year: 2024,
            month: 7,
        };
        let csv = "calc_time,funding_interval_hours,last_funding_rate\n1719792000001,8,-0.0001\n";
        let (zip, checksum) = archive(csv, filename);
        let points = parse_verified_bytes(&zip, &checksum, &key, filename).unwrap();
        assert_eq!(points[0].ts, 1_719_792_000_000);
        assert_eq!(points[0].interval_hours, 8);
        assert_eq!(points[0].rate, -0.0001);

        let invalid =
            "calc_time,funding_interval_hours,last_funding_rate\n1719792001001,8,-0.0001\n";
        let (zip, checksum) = archive(invalid, filename);
        assert!(parse_verified_bytes(&zip, &checksum, &key, filename).is_err());
    }
}
