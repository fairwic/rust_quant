use super::market_velocity_backfill::fetch_okx_history_candles;
use super::okx_historical_universe::{
    load_current_live_contract_values, load_official_archive_urls, HistoricalUniverseManifest,
};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, TimeZone, Utc};
use okx::dto::market_dto::CandleOkxRespDto;
use reqwest::Client;
use rust_quant_market::models::CandlesModel;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Cursor};
use std::path::PathBuf;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::sleep;
use zip::ZipArchive;

const MINUTE_MS: i64 = 60 * 1_000;
const CANDLE_15M_MS: i64 = 15 * MINUTE_MS;
const OKX_ARCHIVE_UTC_OFFSET_MS: i64 = 8 * 60 * 60 * 1_000;
const DEFAULT_OKX_BASE: &str = "https://www.okx.com";
const OKX_ARCHIVE_CDN_BASE: &str =
    "https://static.okx.com/cdn/okex/traderecords/candlesticks/monthly";

/// 历史 15m 补数命令；默认 dry-run，只有显式 `--write` 才写本地 quant_core。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Historical15mBackfillArgs {
    /// 已通过完整性审计的当前 live 币池 manifest。
    pub manifest: PathBuf,
    /// 同时下载的官方月包数量。
    pub download_concurrency: usize,
    /// 每批写入本地 K 线表的行数。
    pub batch_size: usize,
    /// 是否写入本地 quant_core；false 时只下载、聚合和校验。
    pub write: bool,
    /// OKX 官方站点基地址；本地协议测试可替换。
    pub okx_base: String,
}

/// 补数结果只报告本地研究写入，不表示生产数据或策略已晋级。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Historical15mBackfillReport {
    /// manifest 中需要补数的唯一当前 live 币种数。
    pub symbols: usize,
    /// 实际下载并完成整月校验的官方归档文件数。
    pub archive_files: usize,
    /// 从官方分钟包严格聚合得到的 15m K 线总数。
    pub candles_15m: usize,
    /// 月包链接缺失或内容不完整时，由官方 history-candles 补齐并通过整月校验的文件数。
    pub rest_fallback_files: usize,
    /// 官方源文件存在缺口但仍保留了完整 15m 桶的文件数；缺失桶不会被填充。
    pub partial_files: usize,
    /// 仅用于月末持仓结算的尾月无法完整取得时，受影响文件数；对应交易必须标记不完整。
    pub optional_outcome_files_unavailable: usize,
    /// 本地 quant_core 实际插入或内容发生变化的行数。
    pub rows_upserted: u64,
    /// true 表示只校验和聚合，没有写本地数据库。
    pub dry_run: bool,
}

/// 唯一归档请求以 `symbol x month` 标识，URL 只作为当次官方下载地址。
#[derive(Debug, Clone, PartialEq)]
struct ArchiveRequest {
    symbol: String,
    month: String,
    url: Option<String>,
    required_full_month: bool,
    contract_value: f64,
}

/// 解析最小 CLI 参数，未知参数直接失败，避免误把 dry-run 当成真实写入。
pub fn parse_historical_15m_backfill_args<I>(values: I) -> Result<Historical15mBackfillArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut manifest = None;
    let mut download_concurrency = 8usize;
    let mut batch_size = 500usize;
    let mut write = false;
    let mut okx_base = DEFAULT_OKX_BASE.to_owned();
    while let Some(arg) = values.next() {
        let value = |values: &mut I::IntoIter| {
            values
                .next()
                .ok_or_else(|| anyhow!("{arg} requires a value"))
        };
        match arg.as_str() {
            "--manifest" => manifest = Some(PathBuf::from(value(&mut values)?)),
            "--download-concurrency" => {
                download_concurrency = value(&mut values)?
                    .parse()
                    .context("parse --download-concurrency")?
            }
            "--batch-size" => {
                batch_size = value(&mut values)?.parse().context("parse --batch-size")?
            }
            "--write" => write = true,
            "--dry-run" => write = false,
            "--okx-base" => okx_base = value(&mut values)?.trim_end_matches('/').to_owned(),
            "--help" | "-h" => bail!(historical_15m_backfill_usage()),
            _ => bail!(
                "unknown argument: {arg}\n{}",
                historical_15m_backfill_usage()
            ),
        }
    }
    if download_concurrency == 0 || download_concurrency > 16 {
        bail!("--download-concurrency must be between 1 and 16");
    }
    if batch_size == 0 || batch_size > 2_000 {
        bail!("--batch-size must be between 1 and 2000");
    }
    Ok(Historical15mBackfillArgs {
        manifest: manifest.context("--manifest is required")?,
        download_concurrency,
        batch_size,
        write,
        okx_base,
    })
}

/// 返回只面向本地研究库的命令用法。
pub fn historical_15m_backfill_usage() -> &'static str {
    "Usage: okx_historical_15m_backfill --manifest PATH [--download-concurrency 8] [--batch-size 500] [--dry-run|--write]"
}

/// 下载 manifest 覆盖月、两天预热所在月和 48h outcome 所在月，并聚合写入 15m。
pub async fn run_historical_15m_backfill(
    args: &Historical15mBackfillArgs,
) -> Result<Historical15mBackfillReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read manifest {}", args.manifest.display()))?,
    )
    .context("decode historical universe manifest")?;
    validate_manifest(&manifest)?;
    let requests = archive_requests(&manifest, &args.okx_base).await?;
    let symbols = requests
        .iter()
        .map(|request| request.symbol.clone())
        .collect::<BTreeSet<_>>();
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("build OKX historical 15m HTTP client")?;
    let model = CandlesModel::new();
    if args.write {
        for symbol in &symbols {
            model.create_table(symbol, "15m").await?;
        }
    }
    let mut report = Historical15mBackfillReport {
        symbols: symbols.len(),
        archive_files: requests.len(),
        dry_run: !args.write,
        ..Default::default()
    };
    for chunk in requests.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for request in chunk.iter().cloned() {
            let client = client.clone();
            let okx_base = args.okx_base.clone();
            tasks.spawn(async move {
                let archive_result = async {
                    let url = request.url.as_deref().context("official archive link is absent")?;
                    let bytes = download_archive_with_retry(&client, url).await?;
                    match aggregate_archive_to_15m(
                        &bytes,
                        &request.symbol,
                        &request.month,
                        request.contract_value,
                    ) {
                        Ok(candles) => Ok((candles, false)),
                        Err(_) => aggregate_available_archive_to_15m(
                            &bytes,
                            &request.symbol,
                            &request.month,
                            request.contract_value,
                        )
                        .map(|candles| (candles, true)),
                    }
                }
                .await;
                match archive_result {
                    Ok((candles, partial)) => {
                        Ok::<_, anyhow::Error>((request, Some(candles), false, partial))
                    }
                    Err(archive_error) => {
                        match load_rest_fallback_month(&client, &okx_base, &request).await {
                            Ok((candles, partial)) => {
                                Ok((request, Some(candles), true, partial))
                            }
                            Err(_) if !request.required_full_month => {
                                Ok((request, None, false, false))
                            }
                            Err(rest_error) => Err(rest_error).with_context(|| {
                                format!(
                                    "official archive failed ({archive_error}); REST fallback failed for {} {}",
                                    request.symbol, request.month
                                )
                            }),
                        }
                    }
                }
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let (request, candles, used_rest_fallback, partial) =
                joined.context("join historical 15m archive task")??;
            let Some(candles) = candles else {
                report.optional_outcome_files_unavailable += 1;
                continue;
            };
            report.candles_15m += candles.len();
            report.rest_fallback_files += usize::from(used_rest_fallback);
            report.partial_files += usize::from(partial);
            if args.write {
                for batch in candles.chunks(args.batch_size) {
                    report.rows_upserted += model
                        .upsert_batch(batch.to_vec(), &request.symbol, "15m")
                        .await
                        .with_context(|| {
                            format!("upsert {} {} 15m", request.symbol, request.month)
                        })?;
                }
            }
        }
    }
    Ok(report)
}

/// 月包不可用时读取官方 15m REST；完整月优先，缺口只保留实际返回的已确认 15m。
async fn load_rest_fallback_month(
    client: &Client,
    okx_base: &str,
    request: &ArchiveRequest,
) -> Result<(Vec<CandleOkxRespDto>, bool)> {
    let (month_start, month_end) = archive_month_bounds(&request.month)?;
    let candles = fetch_okx_history_candles(
        client,
        okx_base,
        &request.symbol,
        "15m",
        month_start,
        month_end.saturating_sub(CANDLE_15M_MS),
        300,
        120,
    )
    .await?;
    if validate_complete_rest_month(&candles, &request.symbol, &request.month).is_ok() {
        Ok((candles, false))
    } else {
        validate_available_rest_month(&candles, &request.symbol, &request.month)?;
        Ok((candles, true))
    }
}

/// 接受 REST 返回的已确认、UTC 对齐 15m 行，保留缺口但拒绝错月数据。
fn validate_available_rest_month(
    candles: &[CandleOkxRespDto],
    symbol: &str,
    month: &str,
) -> Result<()> {
    let (month_start, month_end) = archive_month_bounds(month)?;
    if candles.is_empty() {
        bail!("REST fallback returned no candles for {symbol} {month}");
    }
    let mut previous = None;
    for candle in candles {
        let ts = candle
            .ts
            .parse::<i64>()
            .context("parse REST candle timestamp")?;
        if ts < month_start
            || ts >= month_end
            || ts.rem_euclid(CANDLE_15M_MS) != 0
            || candle.confirm != "1"
            || previous.is_some_and(|value| ts <= value)
        {
            bail!("REST fallback contains invalid available candle for {symbol} {month}");
        }
        previous = Some(ts);
    }
    Ok(())
}

/// 要求 REST 回退完整覆盖目标月；仅用于必须完整的研究边界。
fn validate_complete_rest_month(
    candles: &[CandleOkxRespDto],
    symbol: &str,
    month: &str,
) -> Result<()> {
    let (month_start, month_end) = archive_month_bounds(month)?;
    let expected = usize::try_from((month_end - month_start) / CANDLE_15M_MS)?;
    if candles.len() != expected {
        bail!("REST fallback month is incomplete for {symbol} {month}");
    }
    for (index, candle) in candles.iter().enumerate() {
        let ts = candle
            .ts
            .parse::<i64>()
            .context("parse REST fallback timestamp")?;
        if ts != month_start + index as i64 * CANDLE_15M_MS || candle.confirm != "1" {
            bail!("REST fallback month is non-contiguous for {symbol} {month}");
        }
    }
    Ok(())
}

/// 拒绝非 OKX、非 15m 或未明确限制当前 live 合约的 manifest。
fn validate_manifest(manifest: &HistoricalUniverseManifest) -> Result<()> {
    if manifest.exchange != "okx"
        || manifest.market_type != "perpetual_swap"
        || manifest.timeframe != "15m"
    {
        bail!("manifest must describe OKX perpetual_swap 15m research data");
    }
    if !manifest
        .selection_rule
        .starts_with("current-live OKX USDT swaps only")
    {
        bail!("manifest is not restricted to the current-live OKX universe");
    }
    if manifest.months.is_empty() || manifest.months.iter().any(|month| month.members.is_empty()) {
        bail!("manifest must contain non-empty effective months");
    }
    Ok(())
}

/// 展开每个有效月的排名预热月、生效月和 outcome 尾月，并按币种月份去重。
async fn archive_requests(
    manifest: &HistoricalUniverseManifest,
    okx_base: &str,
) -> Result<Vec<ArchiveRequest>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .context("build OKX contract-value client")?;
    let contract_values = load_current_live_contract_values(&client, okx_base).await?;
    let mut requests = BTreeMap::<(String, String), (Option<String>, bool, f64)>::new();
    for effective in &manifest.months {
        let families = effective
            .members
            .iter()
            .map(|member| {
                member
                    .symbol
                    .strip_suffix("-SWAP")
                    .map(str::to_owned)
                    .with_context(|| format!("invalid swap symbol {}", member.symbol))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut months = BTreeMap::from([(effective.ranking_source_month.clone(), true)]);
        months.insert(month_label(effective.effective_from_ms)?, true);
        // effective_to 所在月只用于月末交易结算；若同币下月仍是成员，后续循环会提升为必需月。
        months
            .entry(month_label(effective.effective_to_ms)?)
            .or_insert(false);
        for (month, required_full_month) in months {
            let urls = load_official_archive_urls(okx_base, &families, &month).await?;
            for member in &effective.members {
                let family = member.symbol.strip_suffix("-SWAP").unwrap_or_default();
                let contract_value = *contract_values
                    .get(family)
                    .with_context(|| format!("missing current ctVal for {}", member.symbol))?;
                let url = urls
                    .get(family)
                    .cloned()
                    .or_else(|| direct_archive_url(okx_base, &member.symbol, &month));
                let key = (member.symbol.clone(), month.clone());
                if let Some((existing_url, existing_required, existing_contract_value)) =
                    requests.get_mut(&key)
                {
                    if *existing_url != url {
                        bail!("conflicting official archive URLs for {} {}", key.0, key.1);
                    }
                    if *existing_contract_value != contract_value {
                        bail!("conflicting current ctVal for {}", key.0);
                    }
                    *existing_required |= required_full_month;
                } else {
                    requests.insert(key, (url, required_full_month, contract_value));
                }
            }
        }
    }
    Ok(requests
        .into_iter()
        .map(
            |((symbol, month), (url, required_full_month, contract_value))| ArchiveRequest {
                symbol,
                month,
                url,
                required_full_month,
                contract_value,
            },
        )
        .collect())
}

/// 为缺失下载链接的当前 live 合约构造官方月包地址。
fn direct_archive_url(base: &str, symbol: &str, month: &str) -> Option<String> {
    (base == DEFAULT_OKX_BASE).then(|| {
        format!(
            "{}/{}/{}-candlesticks-{}.zip",
            OKX_ARCHIVE_CDN_BASE,
            month.replace('-', ""),
            symbol,
            month
        )
    })
}

/// 把 manifest 的 UTC 生效边界转换为归档月标签。
fn month_label(timestamp_ms: i64) -> Result<String> {
    let datetime = Utc
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .context("manifest month timestamp outside supported range")?;
    Ok(format!("{:04}-{:02}", datetime.year(), datetime.month()))
}

/// 对官方临时下载地址执行有界重试，不在本地静默跳过缺失归档。
async fn download_archive_with_retry(client: &Client, url: &str) -> Result<Vec<u8>> {
    let mut last_error = None;
    for attempt in 0..4u64 {
        let response = async {
            client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await
        }
        .await;
        match response {
            Ok(bytes) => return Ok(bytes.to_vec()),
            Err(error) => last_error = Some(error),
        }
        sleep(Duration::from_millis(250 * (attempt + 1))).await;
    }
    Err(last_error.context("archive retry loop produced no error")?)
        .with_context(|| format!("download {url} failed after retries"))
}

/// 严格校验整月分钟覆盖后按 UTC 15m 桶聚合；1m 行只作为原始事实，不产生信号。
pub fn aggregate_archive_to_15m(
    bytes: &[u8],
    expected_symbol: &str,
    month: &str,
    contract_value: f64,
) -> Result<Vec<CandleOkxRespDto>> {
    let (month_start, month_end) = archive_month_bounds(month)?;
    let rows = parse_archive_rows(bytes, expected_symbol, month)?;
    let expected_minutes = usize::try_from((month_end - month_start) / MINUTE_MS)?;
    if rows.len() != expected_minutes
        || rows.first_key_value().map(|(ts, _)| *ts) != Some(month_start)
        || rows.last_key_value().map(|(ts, _)| *ts) != Some(month_end - MINUTE_MS)
    {
        bail!("incomplete archive month for {expected_symbol} {month}");
    }
    let ordered = rows.into_iter().collect::<Vec<_>>();
    for pair in ordered.windows(2) {
        if pair[1].0 != pair[0].0 + MINUTE_MS {
            bail!("minute gap for {expected_symbol} {month}");
        }
    }
    ordered
        .chunks(15)
        .map(|chunk| aggregate_15m_chunk(chunk, expected_symbol, month, contract_value))
        .collect()
}

/// 月包不完整时只保留自身 15 根分钟线完整的 UTC 15m 桶，缺口不做任何填补。
fn aggregate_available_archive_to_15m(
    bytes: &[u8],
    expected_symbol: &str,
    month: &str,
    contract_value: f64,
) -> Result<Vec<CandleOkxRespDto>> {
    let rows = parse_archive_rows(bytes, expected_symbol, month)?;
    let mut buckets = BTreeMap::<i64, Vec<(i64, String)>>::new();
    for (ts, line) in rows {
        let bucket = ts.div_euclid(CANDLE_15M_MS) * CANDLE_15M_MS;
        buckets.entry(bucket).or_default().push((ts, line));
    }
    let candles = buckets
        .into_values()
        .filter(|chunk| chunk.len() == 15)
        .map(|chunk| aggregate_15m_chunk(&chunk, expected_symbol, month, contract_value))
        .collect::<Result<Vec<_>>>()?;
    if candles.is_empty() {
        bail!("archive has no complete 15m bucket for {expected_symbol} {month}");
    }
    Ok(candles)
}

/// 解析官方分钟 CSV，并重建早期缺失的合约计价成交量字段。
fn parse_archive_rows(
    bytes: &[u8],
    expected_symbol: &str,
    month: &str,
) -> Result<BTreeMap<i64, String>> {
    let (month_start, month_end) = archive_month_bounds(month)?;
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("open candlestick ZIP")?;
    if archive.len() != 1 {
        bail!("{expected_symbol} {month} ZIP must contain exactly one CSV");
    }
    let csv = archive.by_index(0).context("open candlestick CSV")?;
    let mut reader = BufReader::new(csv);
    let mut header = String::new();
    reader.read_line(&mut header)?;
    if header.trim_end()
        != "instrument_name,open,high,low,close,vol,vol_ccy,vol_quote,open_time,confirm"
    {
        bail!("unexpected OKX candlestick header for {expected_symbol} {month}");
    }
    let mut rows = BTreeMap::<i64, String>::new();
    for line in reader.lines() {
        let line = line?;
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let columns = line.split(',').collect::<Vec<_>>();
        if columns.len() != 10 || columns[0] != expected_symbol {
            bail!("unexpected archive row for {expected_symbol} {month}");
        }
        if !matches!(columns[9], "0" | "1") {
            bail!("invalid archive confirm flag for {expected_symbol} {month}");
        }
        let ts = columns[8]
            .parse::<i64>()
            .context("parse archive timestamp")?;
        if ts < month_start || ts >= month_end {
            bail!("archive row outside {month} for {expected_symbol}");
        }
        if let Some(existing) = rows.insert(ts, line.to_owned()) {
            if existing != line {
                bail!("conflicting duplicate minute for {expected_symbol} at {ts}");
            }
        }
    }
    Ok(rows)
}

/// 按固定 15 根连续分钟线聚合一根 UTC 对齐的 15m K 线。
fn aggregate_15m_chunk(
    chunk: &[(i64, String)],
    symbol: &str,
    month: &str,
    contract_value: f64,
) -> Result<CandleOkxRespDto> {
    if chunk.len() != 15 || chunk[0].0.rem_euclid(CANDLE_15M_MS) != 0 {
        bail!("unaligned 15m chunk for {symbol} {month}");
    }
    let mut high = (f64::NEG_INFINITY, String::new());
    let mut low = (f64::INFINITY, String::new());
    let mut volume = 0.0;
    let mut volume_ccy = 0.0;
    let mut volume_quote = 0.0;
    let mut open = None::<f64>;
    let mut close = None::<f64>;
    for (index, (ts, line)) in chunk.iter().enumerate() {
        if *ts != chunk[0].0 + index as i64 * MINUTE_MS {
            bail!("non-contiguous 15m chunk for {symbol} {month}");
        }
        let columns = line.split(',').collect::<Vec<_>>();
        let open_value = columns[1].parse::<f64>().context("parse archive open")?;
        let close_value = columns[4].parse::<f64>().context("parse archive close")?;
        open.get_or_insert(open_value);
        close = Some(close_value);
        let high_value = columns[2].parse::<f64>().context("parse archive high")?;
        let low_value = columns[3].parse::<f64>().context("parse archive low")?;
        if high_value > high.0 {
            high = (high_value, String::new());
        }
        if low_value < low.0 {
            low = (low_value, String::new());
        }
        volume += columns[5].parse::<f64>().context("parse archive volume")?;
        let minute_volume = columns[5].parse::<f64>().context("parse archive volume")?;
        volume_ccy += columns[6]
            .parse::<f64>()
            .unwrap_or(minute_volume * contract_value);
        volume_quote += columns[7]
            .parse::<f64>()
            .unwrap_or(minute_volume * contract_value * columns[4].parse::<f64>()?);
    }
    if !high.0.is_finite()
        || !low.0.is_finite()
        || !volume.is_finite()
        || !volume_ccy.is_finite()
        || !volume_quote.is_finite()
    {
        bail!("non-finite 15m aggregate for {symbol} {month}");
    }
    Ok(CandleOkxRespDto {
        ts: chunk[0].0.to_string(),
        o: storage_number(open.context("missing 15m open")?)?,
        h: storage_number(high.0)?,
        l: storage_number(low.0)?,
        c: storage_number(close.context("missing 15m close")?)?,
        v: storage_number(volume)?,
        vol_ccy: storage_number(volume_ccy)?,
        vol_ccy_quote: storage_number(volume_quote)?,
        confirm: "1".to_owned(),
    })
}

/// 将浮点数压缩为适合历史 varchar 列且可无损回读的十进制文本。
fn storage_number(value: f64) -> Result<String> {
    if !value.is_finite() {
        bail!("cannot store non-finite candle number");
    }
    let plain = value.to_string();
    Ok(if plain.len() <= 20 {
        plain
    } else {
        format!("{value:.12e}")
    })
}

/// 返回 OKX 官方 UTC+8 月包在 Unix 毫秒轴上的半开区间。
fn archive_month_bounds(month: &str) -> Result<(i64, i64)> {
    let date = chrono::NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d")
        .with_context(|| format!("parse archive month {month}"))?;
    let next = if date.month() == 12 {
        chrono::NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
    }
    .context("archive month outside supported range")?;
    let start = Utc
        .with_ymd_and_hms(date.year(), date.month(), 1, 0, 0, 0)
        .single()
        .context("archive month start outside supported range")?
        .timestamp_millis()
        - OKX_ARCHIVE_UTC_OFFSET_MS;
    let end = Utc
        .with_ymd_and_hms(next.year(), next.month(), 1, 0, 0, 0)
        .single()
        .context("archive month end outside supported range")?
        .timestamp_millis()
        - OKX_ARCHIVE_UTC_OFFSET_MS;
    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::FileOptions;

    #[test]
    fn full_month_archive_aggregates_exactly_to_utc_15m() {
        let (start, end) = archive_month_bounds("2025-02").unwrap();
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        zip.start_file("fixture.csv", FileOptions::default())
            .unwrap();
        writeln!(
            zip,
            "instrument_name,open,high,low,close,vol,vol_ccy,vol_quote,open_time,confirm"
        )
        .unwrap();
        let midpoint = start + (end - start) / 2;
        for (index, ts) in (start..end).step_by(MINUTE_MS as usize).enumerate() {
            let confirm = if ts < midpoint { "0" } else { "1" };
            let row = format!(
                "BTC-USDT-SWAP,1,{},0.5,1.5,2,3,4,{},{}",
                2.0 + (index % 15) as f64,
                ts,
                confirm
            );
            writeln!(zip, "{row}").unwrap();
            if index == 10 {
                writeln!(zip, "{row}").unwrap();
            }
        }
        let bytes = zip.finish().unwrap().into_inner();
        let candles = aggregate_archive_to_15m(&bytes, "BTC-USDT-SWAP", "2025-02", 1.0).unwrap();
        assert_eq!(candles.len(), 28 * 24 * 4);
        assert_eq!(candles[0].ts, start.to_string());
        assert_eq!(candles[0].o, "1");
        assert_eq!(candles[0].h, "16");
        assert_eq!(candles[0].l, "0.5");
        assert_eq!(candles[0].c, "1.5");
        assert_eq!(candles[0].v, "30");
        assert_eq!(candles[0].confirm, "1");
    }

    #[test]
    fn partial_archive_keeps_only_complete_15m_buckets_without_filling_gap() {
        let (start, _) = archive_month_bounds("2025-02").unwrap();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("fixture.csv", FileOptions::default())
            .unwrap();
        writeln!(
            zip,
            "instrument_name,open,high,low,close,vol,vol_ccy,vol_quote,open_time,confirm"
        )
        .unwrap();
        for offset in (0..15).chain(30..45) {
            writeln!(
                zip,
                "BTC-USDT-SWAP,1,2,0.5,1.5,2,3,4,{},1",
                start + offset * MINUTE_MS
            )
            .unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();

        assert!(aggregate_archive_to_15m(&bytes, "BTC-USDT-SWAP", "2025-02", 1.0).is_err());
        let candles =
            aggregate_available_archive_to_15m(&bytes, "BTC-USDT-SWAP", "2025-02", 1.0).unwrap();

        assert_eq!(candles.len(), 2);
        assert_eq!(candles[0].ts, start.to_string());
        assert_eq!(candles[1].ts, (start + 30 * MINUTE_MS).to_string());
    }

    #[test]
    fn early_archive_missing_volume_columns_uses_contract_notional() {
        let (start, _) = archive_month_bounds("2022-10").unwrap();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("fixture.csv", FileOptions::default())
            .unwrap();
        writeln!(
            zip,
            "instrument_name,open,high,low,close,vol,vol_ccy,vol_quote,open_time,confirm"
        )
        .unwrap();
        for offset in 0..15 {
            writeln!(
                zip,
                "BTC-USDT-SWAP,2,2,2,2,100,None,None,{},1",
                start + offset * MINUTE_MS
            )
            .unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();

        let candles =
            aggregate_available_archive_to_15m(&bytes, "BTC-USDT-SWAP", "2022-10", 0.01).unwrap();

        assert_eq!(candles[0].vol_ccy, "15");
        assert_eq!(candles[0].vol_ccy_quote, "30");
    }

    #[test]
    fn legacy_table_numbers_fit_existing_varchar_twenty_contract() {
        for value in [
            19_757.400000000001,
            123_456_789_012_345_678_901.0,
            0.000000000000123456789,
        ] {
            let stored = storage_number(value).unwrap();
            assert!(stored.len() <= 20, "{stored}");
            assert!(stored.parse::<f64>().unwrap().is_finite());
        }
    }

    #[test]
    fn backfill_is_dry_run_unless_write_is_explicit() {
        let dry = parse_historical_15m_backfill_args(
            ["--manifest", "/tmp/manifest.json"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();
        assert!(!dry.write);
        let write = parse_historical_15m_backfill_args(
            ["--manifest", "/tmp/manifest.json", "--write"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();
        assert!(write.write);
    }

    #[test]
    fn missing_download_link_has_deterministic_official_cdn_fallback() {
        assert_eq!(
            direct_archive_url(DEFAULT_OKX_BASE, "KITE-USDT-SWAP", "2025-12").as_deref(),
            Some("https://static.okx.com/cdn/okex/traderecords/candlesticks/monthly/202512/KITE-USDT-SWAP-candlesticks-2025-12.zip")
        );
        assert!(direct_archive_url("http://127.0.0.1:1234", "KITE-USDT-SWAP", "2025-12").is_none());
    }

    #[test]
    fn rest_fallback_requires_a_confirmed_contiguous_full_month() {
        let (start, end) = archive_month_bounds("2025-02").unwrap();
        let mut candles = (start..end)
            .step_by(CANDLE_15M_MS as usize)
            .map(|ts| CandleOkxRespDto {
                ts: ts.to_string(),
                o: "1".to_owned(),
                h: "1".to_owned(),
                l: "1".to_owned(),
                c: "1".to_owned(),
                v: "1".to_owned(),
                vol_ccy: "1".to_owned(),
                vol_ccy_quote: "1".to_owned(),
                confirm: "1".to_owned(),
            })
            .collect::<Vec<_>>();

        validate_complete_rest_month(&candles, "BTC-USDT-SWAP", "2025-02").unwrap();
        candles.remove(10);
        assert!(validate_complete_rest_month(&candles, "BTC-USDT-SWAP", "2025-02").is_err());
        validate_available_rest_month(&candles, "BTC-USDT-SWAP", "2025-02").unwrap();
    }
}
