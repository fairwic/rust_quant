use super::{CrossExchangeBasisPanelArgs, UniverseSchedule, DAY_MS, MS_15M};
use anyhow::{bail, Context, Result};
use chrono::{Datelike, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::sleep;
use zip::ZipArchive;

const INTERVAL: &str = "15m";

/// Binance 当前映射、官方月包与解析行数审计。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BinanceKlineAudit {
    /// 同时满足当前 live crypto perpetual 合同的 OKX 映射数。
    pub mapped_symbols: usize,
    /// 当前无法映射到 Binance live crypto perpetual 的 OKX 币种数。
    pub mapping_blocked_symbols: usize,
    /// 按映射合约和覆盖月份请求的官方月包数。
    pub requested_files: usize,
    /// 校验和、ZIP 和 CSV 均有效的月包数。
    pub available_files: usize,
    /// 官方明确返回 404 的月包数。
    pub missing_files: usize,
    /// 下载存在但校验和、ZIP、CSV 或连续性无效的月包数。
    pub invalid_files: usize,
    /// 所有有效月包解析出的去重 15m 行数。
    pub parsed_rows: usize,
}

/// Binance 可执行腿所需的最小 15m 开盘和收盘。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BinanceCandle {
    /// 15m 开盘时间，Unix 毫秒。
    pub ts: i64,
    /// 合约开盘价。
    pub open: f64,
    /// 合约收盘价。
    pub close: f64,
    /// 该原生 15m 棒的总 quote asset volume。
    pub quote_volume: f64,
    /// 该原生 15m 棒由 taker buy 成交贡献的 quote asset volume。
    pub taker_buy_quote_volume: f64,
}

/// Binance premium index 因子只需要 15m 时间与可正可负的收盘值。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BinancePremiumCandle {
    /// 15m 开盘时间，Unix 毫秒。
    pub ts: i64,
    /// 永续相对现货指数的 premium index 收盘值。
    pub close: f64,
}

/// Binance 当前合约列表的最小响应。
#[derive(Debug, Deserialize)]
struct ExchangeInfo {
    /// 当前 USD-M 合约元数据。
    symbols: Vec<ExchangeSymbol>,
}

/// 当前 Binance 合约映射需要的字段。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeSymbol {
    /// Binance 合约标识。
    symbol: String,
    /// 只接受 `TRADING`。
    status: String,
    /// 只接受 `PERPETUAL`。
    contract_type: String,
    /// 只接受 `USDT`。
    quote_asset: String,
    /// 只接受 `COIN`，排除股票永续等非加密类别。
    underlying_type: String,
}

/// 唯一标识一个 OKX 映射合约和 Binance UTC 月包。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MonthlyFileKey {
    /// OKX 标准合约标识。
    okx_symbol: String,
    /// Binance USD-M 合约标识。
    binance_symbol: String,
    /// UTC 年。
    year: i32,
    /// UTC 月，范围 1 到 12。
    month: u32,
}

/// 区分官方月包有效、明确缺失与内容无效。
enum FileLoad {
    /// 校验和与连续性均有效的 15m 行。
    Available(Vec<BinanceCandle>),
    /// ZIP 或 checksum 官方明确 404。
    Missing,
    /// 文件存在但无法满足冻结格式与完整性合同。
    Invalid,
}

/// 区分 premium index 官方月包有效、缺失与内容无效。
enum PremiumFileLoad {
    /// 校验和与连续性均有效的 premium 15m 行。
    Available(Vec<BinancePremiumCandle>),
    /// ZIP 或 checksum 官方明确 404。
    Missing,
    /// 文件存在但无法满足冻结格式与完整性合同。
    Invalid,
}

/// 读取或下载官方月包，并按 OKX symbol 返回合并的 Binance 15m 序列。
pub(super) async fn load_binance_klines(
    args: &CrossExchangeBasisPanelArgs,
    schedule: &UniverseSchedule,
) -> Result<(BTreeMap<String, Vec<BinanceCandle>>, BinanceKlineAudit)> {
    std::fs::create_dir_all(&args.cache_dir)
        .with_context(|| format!("create Binance kline cache {}", args.cache_dir.display()))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .http1_only()
        .build()
        .context("build Binance monthly kline client")?;
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
    let mut rows = BTreeMap::<String, Vec<BinanceCandle>>::new();
    for chunk in requested.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for key in chunk.iter().cloned() {
            let client = client.clone();
            let cache_dir = args.cache_dir.clone();
            let data_base = args.binance_data_base.clone();
            tasks.spawn(async move {
                let loaded = load_file(&client, &data_base, &cache_dir, &key).await?;
                Ok::<_, anyhow::Error>((key, loaded))
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let (key, loaded) = joined.context("join Binance monthly kline task")??;
            match loaded {
                FileLoad::Available(mut candles) => {
                    available_files += 1;
                    rows.entry(key.okx_symbol).or_default().append(&mut candles);
                }
                FileLoad::Missing => missing_files += 1,
                FileLoad::Invalid => invalid_files += 1,
            }
        }
    }
    let mut parsed_rows = 0usize;
    for (symbol, candles) in &mut rows {
        candles.sort_by_key(|candle| candle.ts);
        if candles.windows(2).any(|pair| pair[0].ts >= pair[1].ts) {
            bail!("duplicate or unsorted Binance 15m rows after month merge for {symbol}");
        }
        parsed_rows += candles.len();
    }
    let audit = BinanceKlineAudit {
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

/// 读取或下载 Binance premium index 官方月包，并按 OKX symbol 合并。
pub(super) async fn load_binance_premium_index(
    args: &CrossExchangeBasisPanelArgs,
    schedule: &UniverseSchedule,
) -> Result<(
    BTreeMap<String, Vec<BinancePremiumCandle>>,
    BinanceKlineAudit,
)> {
    std::fs::create_dir_all(&args.cache_dir).with_context(|| {
        format!(
            "create Binance premium index cache {}",
            args.cache_dir.display()
        )
    })?;
    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .http1_only()
        .build()
        .context("build Binance premium index client")?;
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
    let mut rows = BTreeMap::<String, Vec<BinancePremiumCandle>>::new();
    for chunk in requested.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for key in chunk.iter().cloned() {
            let client = client.clone();
            let cache_dir = args.cache_dir.clone();
            let data_base = args.binance_data_base.clone();
            tasks.spawn(async move {
                let loaded = load_premium_file(&client, &data_base, &cache_dir, &key).await?;
                Ok::<_, anyhow::Error>((key, loaded))
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let (key, loaded) = joined.context("join Binance premium index task")??;
            match loaded {
                PremiumFileLoad::Available(mut candles) => {
                    available_files += 1;
                    rows.entry(key.okx_symbol).or_default().append(&mut candles);
                }
                PremiumFileLoad::Missing => missing_files += 1,
                PremiumFileLoad::Invalid => invalid_files += 1,
            }
        }
    }
    let mut parsed_rows = 0usize;
    for (symbol, candles) in &mut rows {
        candles.sort_by_key(|candle| candle.ts);
        if candles.windows(2).any(|pair| pair[0].ts >= pair[1].ts) {
            bail!("duplicate or unsorted Binance premium rows after month merge for {symbol}");
        }
        parsed_rows += candles.len();
    }
    let audit = BinanceKlineAudit {
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

/// 读取当前 Binance 加密 USDT 永续集合用于严格 live 映射。
pub(super) async fn load_current_live_crypto_perpetuals(
    client: &Client,
    base: &str,
) -> Result<BTreeSet<String>> {
    let url = format!("{base}/fapi/v1/exchangeInfo");
    let mut info = None;
    let mut last_error = None;
    for attempt in 0..4u64 {
        match client.get(&url).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(response) => match response.json::<ExchangeInfo>().await {
                    Ok(value) => {
                        info = Some(value);
                        break;
                    }
                    Err(error) => last_error = Some(error.into()),
                },
                Err(error) => last_error = Some(error.into()),
            },
            Err(error) => last_error = Some(error.into()),
        }
        sleep(Duration::from_millis(250 * (attempt + 1))).await;
    }
    let info = info.ok_or_else(|| {
        last_error.unwrap_or_else(|| anyhow::anyhow!("Binance exchangeInfo request failed"))
    })?;
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

/// 构造覆盖 7 日前缀和 24h outcome 尾部的全部映射月包键。
fn requested_files(
    schedule: &UniverseSchedule,
    mapping: &BTreeMap<String, String>,
) -> Result<Vec<MonthlyFileKey>> {
    let first = schedule
        .windows
        .first()
        .context("missing first universe month")?;
    let last = schedule
        .windows
        .last()
        .context("missing last universe month")?;
    let start = Utc
        .timestamp_millis_opt(first.from_ms.saturating_sub(8 * DAY_MS))
        .single()
        .context("invalid Binance monthly kline start")?;
    let end = Utc
        .timestamp_millis_opt(last.to_ms.saturating_add(2 * DAY_MS))
        .single()
        .context("invalid Binance monthly kline end")?;
    let mut months = Vec::<(i32, u32)>::new();
    let (mut year, mut month) = (start.year(), start.month());
    loop {
        months.push((year, month));
        if year == end.year() && month == end.month() {
            break;
        }
        (year, month) = next_month(year, month);
        if months.len() > 24 {
            bail!("unexpectedly wide Binance monthly kline request range");
        }
    }
    Ok(mapping
        .iter()
        .flat_map(|(okx_symbol, binance_symbol)| {
            months.iter().map(move |(year, month)| MonthlyFileKey {
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

/// 优先复用已校验本地月包；缓存无效时重新下载官方 ZIP 和 checksum。
async fn load_file(
    client: &Client,
    data_base: &str,
    cache_dir: &Path,
    key: &MonthlyFileKey,
) -> Result<FileLoad> {
    let filename = format!(
        "{}-{}-{:04}-{:02}.zip",
        key.binance_symbol, INTERVAL, key.year, key.month
    );
    let symbol_dir = cache_dir.join(&key.binance_symbol);
    std::fs::create_dir_all(&symbol_dir)
        .with_context(|| format!("create Binance symbol cache {}", symbol_dir.display()))?;
    let zip_path = symbol_dir.join(&filename);
    let checksum_path = symbol_dir.join(format!("{filename}.CHECKSUM"));
    if zip_path.exists() && checksum_path.exists() {
        if let Ok(candles) = parse_verified_file(&zip_path, &checksum_path, key) {
            return Ok(FileLoad::Available(candles));
        }
    }
    let url = format!(
        "{data_base}/data/futures/um/monthly/klines/{}/{}/{}",
        key.binance_symbol, INTERVAL, filename
    );
    let checksum_url = format!("{url}.CHECKSUM");
    let (Some(zip_bytes), Some(checksum_bytes)) = (
        download_optional(client, &url).await?,
        download_optional(client, &checksum_url).await?,
    ) else {
        return Ok(FileLoad::Missing);
    };
    let candles = match parse_verified_bytes(&zip_bytes, &checksum_bytes, key, &filename) {
        Ok(candles) => candles,
        Err(_) => return Ok(FileLoad::Invalid),
    };
    write_atomic(&zip_path, &zip_bytes)?;
    write_atomic(&checksum_path, &checksum_bytes)?;
    Ok(FileLoad::Available(candles))
}

/// 优先复用已校验 premium 缓存，否则下载官方 ZIP 与 checksum。
async fn load_premium_file(
    client: &Client,
    data_base: &str,
    cache_dir: &Path,
    key: &MonthlyFileKey,
) -> Result<PremiumFileLoad> {
    let filename = format!(
        "{}-{}-{:04}-{:02}.zip",
        key.binance_symbol, INTERVAL, key.year, key.month
    );
    let symbol_dir = cache_dir.join(&key.binance_symbol);
    std::fs::create_dir_all(&symbol_dir)
        .with_context(|| format!("create Binance premium cache {}", symbol_dir.display()))?;
    let zip_path = symbol_dir.join(&filename);
    let checksum_path = symbol_dir.join(format!("{filename}.CHECKSUM"));
    if zip_path.exists() && checksum_path.exists() {
        if let Ok(candles) = parse_verified_premium_file(&zip_path, &checksum_path, key) {
            return Ok(PremiumFileLoad::Available(candles));
        }
    }
    let url = format!(
        "{data_base}/data/futures/um/monthly/premiumIndexKlines/{}/{}/{}",
        key.binance_symbol, INTERVAL, filename
    );
    let checksum_url = format!("{url}.CHECKSUM");
    let (Some(zip_bytes), Some(checksum_bytes)) = (
        download_optional(client, &url).await?,
        download_optional(client, &checksum_url).await?,
    ) else {
        return Ok(PremiumFileLoad::Missing);
    };
    let candles = match parse_verified_premium_bytes(&zip_bytes, &checksum_bytes, key, &filename) {
        Ok(candles) => candles,
        Err(_) => return Ok(PremiumFileLoad::Invalid),
    };
    write_atomic(&zip_path, &zip_bytes)?;
    write_atomic(&checksum_path, &checksum_bytes)?;
    Ok(PremiumFileLoad::Available(candles))
}

/// 校验已缓存文件的官方 SHA-256 并解析冻结月包格式。
fn parse_verified_file(
    zip_path: &Path,
    checksum_path: &Path,
    key: &MonthlyFileKey,
) -> Result<Vec<BinanceCandle>> {
    let zip_bytes = std::fs::read(zip_path)
        .with_context(|| format!("read cached Binance zip {}", zip_path.display()))?;
    let checksum_bytes = std::fs::read(checksum_path)
        .with_context(|| format!("read cached Binance checksum {}", checksum_path.display()))?;
    let filename = zip_path
        .file_name()
        .and_then(|value| value.to_str())
        .context("cached Binance zip filename is not UTF-8")?;
    parse_verified_bytes(&zip_bytes, &checksum_bytes, key, filename)
}

/// 校验已缓存 premium 文件的 SHA-256 并解析冻结格式。
fn parse_verified_premium_file(
    zip_path: &Path,
    checksum_path: &Path,
    key: &MonthlyFileKey,
) -> Result<Vec<BinancePremiumCandle>> {
    let zip_bytes = std::fs::read(zip_path)
        .with_context(|| format!("read cached Binance premium zip {}", zip_path.display()))?;
    let checksum_bytes = std::fs::read(checksum_path).with_context(|| {
        format!(
            "read cached Binance premium checksum {}",
            checksum_path.display()
        )
    })?;
    let filename = zip_path
        .file_name()
        .and_then(|value| value.to_str())
        .context("cached Binance premium filename is not UTF-8")?;
    parse_verified_premium_bytes(&zip_bytes, &checksum_bytes, key, filename)
}

/// 用官方 checksum 校验内存 ZIP 后解析严格 15m CSV。
fn parse_verified_bytes(
    zip_bytes: &[u8],
    checksum_bytes: &[u8],
    key: &MonthlyFileKey,
    filename: &str,
) -> Result<Vec<BinanceCandle>> {
    let checksum_text = std::str::from_utf8(checksum_bytes).context("checksum is not UTF-8")?;
    let mut fields = checksum_text.split_whitespace();
    let expected = fields.next().context("missing checksum hash")?;
    let expected_filename = fields.next().context("missing checksum filename")?;
    if expected.len() != 64
        || !expected.bytes().all(|byte| byte.is_ascii_hexdigit())
        || expected_filename.trim_start_matches('*') != filename
    {
        bail!("invalid Binance checksum contract for {filename}");
    }
    let actual = format!("{:x}", Sha256::digest(zip_bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("Binance checksum mismatch for {filename}");
    }
    parse_zip(zip_bytes, key)
}

/// 用官方 checksum 校验 premium ZIP 后解析可正可负的 close。
fn parse_verified_premium_bytes(
    zip_bytes: &[u8],
    checksum_bytes: &[u8],
    key: &MonthlyFileKey,
    filename: &str,
) -> Result<Vec<BinancePremiumCandle>> {
    let checksum_text = std::str::from_utf8(checksum_bytes).context("checksum is not UTF-8")?;
    let mut fields = checksum_text.split_whitespace();
    let expected = fields.next().context("missing checksum hash")?;
    let expected_filename = fields.next().context("missing checksum filename")?;
    if expected.len() != 64
        || !expected.bytes().all(|byte| byte.is_ascii_hexdigit())
        || expected_filename.trim_start_matches('*') != filename
    {
        bail!("invalid Binance premium checksum contract for {filename}");
    }
    let actual = format!("{:x}", Sha256::digest(zip_bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("Binance premium checksum mismatch for {filename}");
    }
    parse_premium_zip(zip_bytes, key)
}

/// 解析单 CSV ZIP，并拒绝跨月、非 15m、重复或中间缺口数据。
fn parse_zip(bytes: &[u8], key: &MonthlyFileKey) -> Result<Vec<BinanceCandle>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("open Binance kline zip")?;
    if archive.len() != 1 {
        bail!("Binance monthly kline zip must contain exactly one CSV");
    }
    let file = archive.by_index(0).context("read Binance kline CSV")?;
    let reader = BufReader::new(file);
    let mut candles = Vec::new();
    for (line_number, line) in reader.lines().enumerate() {
        let line = line.context("read Binance kline CSV line")?;
        if line_number == 0 && line.starts_with("open_time,") {
            continue;
        }
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() < 11 {
            bail!("Binance kline row has fewer than eleven fields");
        }
        let ts = fields[0]
            .parse::<i64>()
            .context("parse Binance open_time")?;
        let open = parse_positive(fields[1], "open")?;
        let close = parse_positive(fields[4], "close")?;
        let quote_volume = parse_non_negative(fields[7], "quote_volume")?;
        let taker_buy_quote_volume = parse_non_negative(fields[10], "taker_buy_quote_volume")?;
        if taker_buy_quote_volume > quote_volume * (1.0 + 1e-9) {
            bail!("Binance taker buy quote volume exceeds total quote volume");
        }
        let timestamp = Utc
            .timestamp_millis_opt(ts)
            .single()
            .context("invalid Binance kline timestamp")?;
        if timestamp.year() != key.year
            || timestamp.month() != key.month
            || ts.rem_euclid(MS_15M) != 0
        {
            bail!("Binance kline row is outside requested UTC month or 15m grid");
        }
        candles.push(BinanceCandle {
            ts,
            open,
            close,
            quote_volume,
            taker_buy_quote_volume,
        });
    }
    if candles.is_empty()
        || candles
            .windows(2)
            .any(|pair| pair[0].ts.saturating_add(MS_15M) != pair[1].ts)
    {
        bail!("Binance monthly kline CSV is empty, duplicated or internally gapped");
    }
    Ok(candles)
}

/// 解析 premium 单 CSV ZIP，并拒绝跨月、重复或内部 15m 缺口。
fn parse_premium_zip(bytes: &[u8], key: &MonthlyFileKey) -> Result<Vec<BinancePremiumCandle>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("open Binance premium zip")?;
    if archive.len() != 1 {
        bail!("Binance premium monthly zip must contain exactly one CSV");
    }
    let file = archive.by_index(0).context("read Binance premium CSV")?;
    let reader = BufReader::new(file);
    let mut candles = Vec::new();
    for (line_number, line) in reader.lines().enumerate() {
        let line = line.context("read Binance premium CSV line")?;
        if line_number == 0 && line.starts_with("open_time,") {
            continue;
        }
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() < 5 {
            bail!("Binance premium row has fewer than five fields");
        }
        let ts = fields[0]
            .parse::<i64>()
            .context("parse Binance premium open_time")?;
        let close = parse_finite(fields[4], "premium close")?;
        let timestamp = Utc
            .timestamp_millis_opt(ts)
            .single()
            .context("invalid Binance premium timestamp")?;
        if timestamp.year() != key.year
            || timestamp.month() != key.month
            || ts.rem_euclid(MS_15M) != 0
        {
            bail!("Binance premium row is outside requested UTC month or 15m grid");
        }
        candles.push(BinancePremiumCandle { ts, close });
    }
    if candles.is_empty()
        || candles
            .windows(2)
            .any(|pair| pair[0].ts.saturating_add(MS_15M) != pair[1].ts)
    {
        bail!("Binance premium CSV is empty, duplicated or internally gapped");
    }
    Ok(candles)
}

/// 解析严格正且有限的 Binance 价格字段。
fn parse_positive(value: &str, field: &str) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse Binance kline {field}"))?;
    if !parsed.is_finite() || parsed <= 0.0 {
        bail!("Binance kline {field} must be finite and positive");
    }
    Ok(parsed)
}

/// 解析允许为负或零、但必须有限的 premium 数值。
fn parse_finite(value: &str, field: &str) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse Binance {field}"))?;
    if !parsed.is_finite() {
        bail!("Binance {field} must be finite");
    }
    Ok(parsed)
}

/// 解析允许为零但禁止负数的 Binance 成交量字段。
fn parse_non_negative(value: &str, field: &str) -> Result<f64> {
    let parsed = parse_finite(value, field)?;
    if parsed < 0.0 {
        bail!("Binance {field} must be non-negative");
    }
    Ok(parsed)
}

/// 将验证后的缓存字节原子写入唯一目标文件。
pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = temporary_path(path)?;
    std::fs::write(&temporary, bytes)
        .with_context(|| format!("write temporary Binance cache {}", temporary.display()))?;
    std::fs::rename(&temporary, path).with_context(|| {
        format!(
            "publish Binance cache {} -> {}",
            temporary.display(),
            path.display()
        )
    })?;
    Ok(())
}

/// 为唯一月包构造同目录临时文件名，避免部分下载被复用。
fn temporary_path(path: &Path) -> Result<PathBuf> {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("Binance cache filename is not UTF-8")?;
    Ok(path.with_file_name(format!(".{filename}.{}.part", std::process::id())))
}

/// 下载允许 404 的官方文件，其他瞬时错误执行有界重试。
pub(super) async fn download_optional(client: &Client, url: &str) -> Result<Option<Vec<u8>>> {
    let mut last_error = None;
    for attempt in 0..4u64 {
        match client.get(url).send().await {
            Ok(response) if response.status() == StatusCode::NOT_FOUND => return Ok(None),
            Ok(response) if response.status().is_success() => match response.bytes().await {
                Ok(bytes) => return Ok(Some(bytes.to_vec())),
                Err(error) => last_error = Some(error.into()),
            },
            Ok(response) => last_error = Some(anyhow::anyhow!("HTTP {}", response.status())),
            Err(error) => last_error = Some(error.into()),
        }
        sleep(Duration::from_millis(250 * (attempt + 1))).await;
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("download failed")))
        .with_context(|| format!("download Binance official file {url}"))
}

/// 将规范 OKX USDT 永续映射为 Binance USD-M 合约。
pub(super) fn map_okx_symbol(symbol: &str) -> Option<String> {
    let base = symbol.strip_suffix("-USDT-SWAP")?;
    Some(match base {
        "BONK" | "FLOKI" | "PEPE" | "SATS" | "SHIB" => format!("1000{base}USDT"),
        "LUNA" => "LUNA2USDT".to_owned(),
        _ => format!("{base}USDT"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_mapping_preserves_known_contract_multipliers() {
        assert_eq!(map_okx_symbol("BTC-USDT-SWAP").as_deref(), Some("BTCUSDT"));
        assert_eq!(
            map_okx_symbol("PEPE-USDT-SWAP").as_deref(),
            Some("1000PEPEUSDT")
        );
        assert_eq!(
            map_okx_symbol("LUNA-USDT-SWAP").as_deref(),
            Some("LUNA2USDT")
        );
        assert!(map_okx_symbol("BTC-USDT").is_none());
    }

    #[test]
    fn month_iteration_crosses_year_boundary() {
        assert_eq!(next_month(2022, 11), (2022, 12));
        assert_eq!(next_month(2022, 12), (2023, 1));
    }

    #[test]
    fn positive_parser_rejects_zero_and_non_finite_values() {
        assert_eq!(parse_positive("12.5", "open").unwrap(), 12.5);
        assert!(parse_positive("0", "open").is_err());
        assert!(parse_positive("NaN", "open").is_err());
    }

    #[test]
    fn non_negative_parser_accepts_zero_and_rejects_negative_values() {
        assert_eq!(parse_non_negative("0", "quote_volume").unwrap(), 0.0);
        assert!(parse_non_negative("-1", "quote_volume").is_err());
        assert!(parse_non_negative("NaN", "quote_volume").is_err());
    }
}
