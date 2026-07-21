use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::sleep;
use zip::ZipArchive;

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const MINUTE_MS: i64 = 60 * 1_000;
const OKX_ARCHIVE_UTC_OFFSET_MS: i64 = 8 * 60 * 60 * 1_000;
const ARCHIVE_BATCH_SIZE: usize = 20;
const MIN_RANKING_MINUTE_COVERAGE: f64 = 0.999;
const DEFAULT_ARCHIVE_BASE: &str = "https://www.okx.com";
const STOCK_PERPETUAL_FIRST_LIVE_MS: i64 = 1_772_006_400_000;

/// 生成历史币种池所需的命令参数；月份使用 UTC `YYYY-MM`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalUniverseArgs {
    /// 第一个生效月份；该月成员只使用上一个完整 UTC 月的数据排名。
    pub start_month: String,
    /// 最后一个生效月份，包含该月。
    pub end_month: String,
    /// 每月按历史成交额选出的最大成员数。
    pub top_n: usize,
    /// 同时下载的官方月度归档数量，避免无界并发冲击公共 CDN。
    pub download_concurrency: usize,
    /// 生成的不可变研究 manifest 路径。
    pub output: PathBuf,
    /// OKX 官方站点基地址；仅用于本地协议测试时替换。
    pub okx_base: String,
}

/// 可审计的历史币种池；每个成员只由生效日前完整可见的官方归档决定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoricalUniverseManifest {
    pub schema_version: u32,
    pub universe_version: String,
    pub generated_at_ms: i64,
    pub exchange: String,
    pub market_type: String,
    pub quote_currency: String,
    pub timeframe: String,
    pub selection_rule: String,
    pub source: HistoricalUniverseSource,
    pub months: Vec<HistoricalUniverseMonth>,
}

/// 官方历史数据来源与边界，防止 archive 文件被误认为交易所当前 instruments 快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoricalUniverseSource {
    pub instruments_endpoint: String,
    pub download_link_endpoint: String,
    pub candlestick_archive_format: String,
    pub stock_perpetual_first_live_ms: i64,
    pub classification_boundary: String,
}

/// 单个生效月份的 point-in-time 成员和选择证据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoricalUniverseMonth {
    pub effective_from_ms: i64,
    pub effective_to_ms: i64,
    pub ranking_source_month: String,
    pub archive_candidate_families: usize,
    pub archive_files_available: usize,
    pub complete_candidates: usize,
    pub members: Vec<HistoricalUniverseMember>,
}

/// 单个成员的历史成交额和源文件指纹。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoricalUniverseMember {
    pub symbol: String,
    pub median_daily_quote_volume: f64,
    pub source_url: String,
    pub source_sha256: String,
    pub source_rows: usize,
    pub source_first_ts: i64,
    pub source_last_ts: i64,
}

/// OKX 公共接口统一响应的最小外层结构。
#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    code: String,
    msg: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
/// 当前公开 instruments 快照中的最小字段，只用于排除已退市合约。
struct PublicSwapInstrument {
    inst_id: String,
    inst_family: String,
    inst_category: String,
    settle_ccy: String,
    state: String,
    ct_val: String,
    ct_val_ccy: String,
}

/// 官方历史数据下载链接请求体。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadLinkRequest<'a> {
    module: &'static str,
    inst_type: &'static str,
    inst_query_param: DownloadInstrumentQuery<'a>,
    date_query: DownloadDateQuery,
}

/// 下载请求中限定 SWAP 与当前成员集合的查询条件。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadInstrumentQuery<'a> {
    inst_family_list: &'a [String],
}

/// 下载请求的 UTC+8 月份边界。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadDateQuery {
    date_aggr_type: &'static str,
    begin: String,
    end: String,
}

/// OKX 下载链接响应中的分组列表。
#[derive(Debug, Deserialize)]
struct DownloadLinkData {
    details: Vec<DownloadGroup>,
}

/// 单个产品分组下的官方归档文件列表。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadGroup {
    inst_family: String,
    group_details: Vec<DownloadFile>,
}

/// 官方归档文件的名称和临时下载地址。
#[derive(Debug, Clone, Deserialize)]
struct DownloadFile {
    filename: String,
    url: String,
}

/// 通过月份完整性审计后用于排名的合约候选。
#[derive(Debug)]
struct CompleteArchiveCandidate {
    family: String,
    median_daily_quote_volume: f64,
    source_url: String,
    source_sha256: String,
    source_rows: usize,
    source_first_ts: i64,
    source_last_ts: i64,
}

/// 解析 CLI 参数；未知参数直接失败，避免研究命令因拼写错误静默改变数据集。
pub fn parse_historical_universe_args<I>(values: I) -> Result<HistoricalUniverseArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut start_month = None;
    let mut end_month = None;
    let mut output = None;
    let mut top_n = 60usize;
    let mut download_concurrency = 6usize;
    let mut okx_base = DEFAULT_ARCHIVE_BASE.to_owned();
    while let Some(arg) = values.next() {
        let value = |values: &mut I::IntoIter| {
            values
                .next()
                .ok_or_else(|| anyhow!("{arg} requires a value"))
        };
        match arg.as_str() {
            "--start-month" => start_month = Some(value(&mut values)?),
            "--end-month" => end_month = Some(value(&mut values)?),
            "--top-n" => top_n = value(&mut values)?.parse().context("parse --top-n")?,
            "--download-concurrency" => {
                download_concurrency = value(&mut values)?
                    .parse()
                    .context("parse --download-concurrency")?
            }
            "--output" => output = Some(PathBuf::from(value(&mut values)?)),
            "--okx-base" => okx_base = value(&mut values)?.trim_end_matches('/').to_owned(),
            "--help" | "-h" => bail!(historical_universe_usage()),
            _ => bail!("unknown argument: {arg}\n{}", historical_universe_usage()),
        }
    }
    let args = HistoricalUniverseArgs {
        start_month: start_month.context("--start-month is required")?,
        end_month: end_month.context("--end-month is required")?,
        top_n,
        download_concurrency,
        output: output.context("--output is required")?,
        okx_base,
    };
    validate_args(&args)?;
    Ok(args)
}

/// 返回研究命令的最小用法说明。
pub fn historical_universe_usage() -> &'static str {
    "Usage: okx_historical_universe_manifest --start-month YYYY-MM --end-month YYYY-MM --output PATH [--top-n 60] [--download-concurrency 6]"
}

/// 为指定当前 live 合约查询某个 OKX UTC+8 月包 URL；缺失文件由调用方决定是否失败。
pub async fn load_official_archive_urls(
    okx_base: &str,
    families: &[String],
    month: &str,
) -> Result<BTreeMap<String, String>> {
    let source_month = parse_month(month)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .context("build OKX archive link HTTP client")?;
    let files = load_month_archive_files(&client, okx_base, families, source_month).await?;
    let mut urls = BTreeMap::new();
    for (family, file) in files {
        if urls.insert(family.clone(), file.url).is_some() {
            bail!("OKX archive returned duplicate {month} file for {family}");
        }
    }
    Ok(urls)
}

/// 从 OKX 官方归档生成 manifest，并以漂亮 JSON 原子写入用户指定路径。
pub async fn generate_historical_universe_manifest(
    args: &HistoricalUniverseArgs,
) -> Result<HistoricalUniverseManifest> {
    validate_args(args)?;
    let effective_months = inclusive_months(&args.start_month, &args.end_month)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .context("build OKX archive HTTP client")?;
    let contract_values = load_current_live_contract_values(&client, &args.okx_base).await?;
    let families = contract_values.keys().cloned().collect::<Vec<_>>();
    let mut months = Vec::with_capacity(effective_months.len());
    for effective_month in effective_months {
        let source_month = previous_month(effective_month)?;
        let files = load_month_archive_files(&client, &args.okx_base, &families, source_month)
            .await
            .with_context(|| format!("load archive links for {}", format_month(source_month)))?;
        let file_count = files.len();
        let mut complete = audit_month_files(
            &client,
            files,
            source_month,
            args.download_concurrency,
            &contract_values,
        )
        .await?;
        complete.sort_by(|left, right| {
            right
                .median_daily_quote_volume
                .total_cmp(&left.median_daily_quote_volume)
                .then_with(|| left.family.cmp(&right.family))
        });
        let complete_candidates = complete.len();
        if complete_candidates < args.top_n {
            bail!(
                "{} has only {complete_candidates} complete current-live candidates; top {} cannot be built",
                format_month(source_month),
                args.top_n
            );
        }
        complete.truncate(args.top_n);
        let effective_from_ms = month_start_ms(effective_month)?;
        let effective_to_ms = month_start_ms(next_month(effective_month)?)?;
        months.push(HistoricalUniverseMonth {
            effective_from_ms,
            effective_to_ms,
            ranking_source_month: format_month(source_month),
            archive_candidate_families: families.len(),
            archive_files_available: file_count,
            complete_candidates,
            members: complete.into_iter().map(member_from_candidate).collect(),
        });
    }
    let manifest = HistoricalUniverseManifest {
        schema_version: 1,
        universe_version: format!(
            "okx_current_live_usdt_swap_prior_month_median_quote_volume_top{}_{}_{}",
            args.top_n,
            args.start_month.replace('-', ""),
            args.end_month.replace('-', "")
        ),
        generated_at_ms: Utc::now().timestamp_millis(),
        exchange: "okx".to_owned(),
        market_type: "perpetual_swap".to_owned(),
        quote_currency: "USDT".to_owned(),
        timeframe: "15m".to_owned(),
        selection_rule: format!(
            "current-live OKX USDT swaps only; each effective UTC month uses top {} median daily quote volume from the prior OKX UTC+8 calendar month with at least 99.9% minute coverage and every calendar day present; lower-coverage or not-yet-listed archives are excluded",
            args.top_n
        ),
        source: HistoricalUniverseSource {
            instruments_endpoint: format!(
                "{}/api/v5/public/instruments?instType=SWAP",
                args.okx_base
            ),
            download_link_endpoint: format!(
                "{}/priapi/v5/broker/public/trade-data/download-link",
                args.okx_base
            ),
            candlestick_archive_format:
                "official UTC+8 calendar-month ZIP containing historical 1m candlesticks; row-level confirm flags may vary, identical duplicate rows are deduplicated, ranking requires both month boundaries, every day and at least 99.9% minute coverage without filling gaps, missing historical vol_quote is reconstructed as vol*close*current ctVal, and SHA-256 is calculated before aggregation"
                    .to_owned(),
            stock_perpetual_first_live_ms: STOCK_PERPETUAL_FIRST_LIVE_MS,
            classification_boundary:
                "current-live public instruments must have instCategory=1 (crypto); instCategory=3 stock perpetuals and symbols without a complete source-month crypto archive are excluded"
                    .to_owned(),
        },
        months,
    };
    write_manifest_atomic(&args.output, &manifest)?;
    Ok(manifest)
}

/// 校验月份、规模、输出路径和并发参数的基本约束。
fn validate_args(args: &HistoricalUniverseArgs) -> Result<()> {
    if args.top_n == 0 {
        bail!("--top-n must be greater than zero");
    }
    if args.download_concurrency == 0 || args.download_concurrency > 16 {
        bail!("--download-concurrency must be between 1 and 16");
    }
    let start = parse_month(&args.start_month)?;
    let end = parse_month(&args.end_month)?;
    if start > end {
        bail!("--start-month must not be later than --end-month");
    }
    if args.output.as_os_str().is_empty() {
        bail!("--output must not be empty");
    }
    Ok(())
}

/// 研究池只接受当前仍为 live 的 USDT 永续，不再下载或回放已退市合约。
pub async fn load_current_live_contract_values(
    client: &Client,
    base: &str,
) -> Result<BTreeMap<String, f64>> {
    let endpoint = format!("{base}/api/v5/public/instruments?instType=SWAP");
    let envelope: ApiEnvelope<Vec<PublicSwapInstrument>> =
        get_json_with_retry(client, &endpoint).await?;
    let data = api_data(envelope, "load current live instruments")?;
    let contract_values = data
        .into_iter()
        .filter(is_current_live_crypto_usdt_swap)
        .map(|instrument| {
            let base_ccy = instrument
                .inst_id
                .strip_suffix("-USDT-SWAP")
                .context("current swap symbol has no USDT suffix")?;
            let contract_value = instrument
                .ct_val
                .parse::<f64>()
                .context("parse current swap ctVal")?;
            if instrument.ct_val_ccy != base_ccy
                || !contract_value.is_finite()
                || contract_value <= 0.0
            {
                bail!("current crypto swap has unsupported contract value currency");
            }
            Ok((instrument.inst_family, contract_value))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    if contract_values.is_empty() {
        bail!("OKX public instruments returned no current-live USDT swap families");
    }
    Ok(contract_values)
}

/// 只保留 OKX 明确标为加密货币的当前 USDT 永续，避免股票永续污染币种池。
fn is_current_live_crypto_usdt_swap(instrument: &PublicSwapInstrument) -> bool {
    instrument.state == "live"
        && instrument.inst_category == "1"
        && instrument.settle_ccy == "USDT"
        && instrument.inst_id.ends_with("-USDT-SWAP")
}

/// 获取指定月份、限定当前 live 合约的官方月包元数据。
async fn load_month_archive_files(
    client: &Client,
    base: &str,
    families: &[String],
    source_month: NaiveDate,
) -> Result<Vec<(String, DownloadFile)>> {
    let endpoint = format!("{base}/priapi/v5/broker/public/trade-data/download-link");
    let source_start = month_start_ms(source_month)?;
    let source_end = month_start_ms(next_month(source_month)?)?;
    // 向月内各收一日，避免下载接口把相邻月边界一并解释为选中月份。
    let date_query = DownloadDateQuery {
        date_aggr_type: "monthly",
        begin: source_start.saturating_add(DAY_MS).to_string(),
        end: source_end.saturating_sub(DAY_MS).to_string(),
    };
    let expected_suffix = format!("-candlesticks-{}.zip", format_month(source_month));
    let mut files = Vec::new();
    for batch in families.chunks(ARCHIVE_BATCH_SIZE) {
        let request = DownloadLinkRequest {
            module: "2",
            inst_type: "SWAP",
            inst_query_param: DownloadInstrumentQuery {
                inst_family_list: batch,
            },
            date_query: DownloadDateQuery {
                date_aggr_type: date_query.date_aggr_type,
                begin: date_query.begin.clone(),
                end: date_query.end.clone(),
            },
        };
        let envelope: ApiEnvelope<DownloadLinkData> =
            post_json_with_retry(client, &endpoint, &request).await?;
        let data = api_data(envelope, "load archive download links")?;
        for group in data.details {
            for file in group.group_details {
                if file.filename.ends_with(&expected_suffix) {
                    files.push((group.inst_family.clone(), file));
                }
            }
        }
        sleep(Duration::from_millis(100)).await;
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

/// 并发审计一个月份的全部候选归档并收集完整成员。
async fn audit_month_files(
    client: &Client,
    files: Vec<(String, DownloadFile)>,
    source_month: NaiveDate,
    concurrency: usize,
    contract_values: &BTreeMap<String, f64>,
) -> Result<Vec<CompleteArchiveCandidate>> {
    let mut complete = Vec::new();
    for chunk in files.chunks(concurrency) {
        let mut tasks = JoinSet::new();
        for (family, file) in chunk.iter().cloned() {
            let client = client.clone();
            let contract_value = *contract_values
                .get(&family)
                .with_context(|| format!("missing current ctVal for {family}"))?;
            tasks.spawn(async move {
                let bytes = get_bytes_with_retry(&client, &file.url).await?;
                audit_archive_bytes(
                    &family,
                    &file.url,
                    bytes.as_ref(),
                    source_month,
                    contract_value,
                )
            });
        }
        while let Some(joined) = tasks.join_next().await {
            match joined.context("join archive audit task")?? {
                Some(candidate) => complete.push(candidate),
                None => {}
            }
        }
    }
    Ok(complete)
}

/// 校验官方分钟归档的边界、方向、覆盖率并计算月度成交额。
fn audit_archive_bytes(
    family: &str,
    source_url: &str,
    bytes: &[u8],
    source_month: NaiveDate,
    contract_value: f64,
) -> Result<Option<CompleteArchiveCandidate>> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let source_sha256 = hex::encode(hasher.finalize());
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("open candlestick ZIP")?;
    if archive.len() != 1 {
        bail!("candlestick ZIP for {family} must contain exactly one CSV");
    }
    let csv = archive.by_index(0).context("open candlestick CSV")?;
    let mut reader = BufReader::new(csv);
    let mut header = String::new();
    reader.read_line(&mut header)?;
    if header.trim_end()
        != "instrument_name,open,high,low,close,vol,vol_ccy,vol_quote,open_time,confirm"
    {
        bail!("unexpected OKX candlestick header for {family}");
    }
    let month_start = okx_archive_month_start_ms(source_month)?;
    let month_end = okx_archive_month_start_ms(next_month(source_month)?)?;
    let expected_rows = usize::try_from((month_end - month_start) / MINUTE_MS)?;
    let mut row_count = 0usize;
    let mut first_ts: Option<i64> = None;
    let mut last_ts: Option<i64> = None;
    let mut prior_ts: Option<i64> = None;
    let mut prior_line: Option<String> = None;
    let mut timestamp_direction: Option<i64> = None;
    let mut daily_quote_volume = BTreeMap::<i64, f64>::new();
    for line in reader.lines() {
        let line = line?;
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let columns = line.split(',').collect::<Vec<_>>();
        if columns.len() != 10 {
            bail!("unexpected candlestick column count for {family}");
        }
        let Ok(ts) = columns[8].parse::<i64>() else {
            return Ok(None);
        };
        if !matches!(columns[9], "0" | "1") {
            return Ok(None);
        }
        if let Some(prior) = prior_ts {
            if ts == prior {
                if prior_line.as_deref() != Some(line) {
                    return Ok(None);
                }
                continue;
            }
            // 官方月包可能按正序或倒序写行；小缺口不填，只要求方向一致且落在分钟网格。
            let step = ts - prior;
            if step == 0 || step.abs().rem_euclid(MINUTE_MS) != 0 {
                return Ok(None);
            }
            let direction = step.signum();
            if timestamp_direction.is_some_and(|expected| direction != expected) {
                return Ok(None);
            }
            timestamp_direction.get_or_insert(direction);
        }
        let quote_volume = match columns[7].parse::<f64>() {
            Ok(value) if value.is_finite() && value >= 0.0 => value,
            _ => {
                let (Ok(volume), Ok(close)) =
                    (columns[5].parse::<f64>(), columns[4].parse::<f64>())
                else {
                    return Ok(None);
                };
                volume * close * contract_value
            }
        };
        if !quote_volume.is_finite() || quote_volume < 0.0 {
            return Ok(None);
        }
        *daily_quote_volume
            .entry((ts + OKX_ARCHIVE_UTC_OFFSET_MS).div_euclid(DAY_MS))
            .or_default() += quote_volume;
        first_ts = Some(first_ts.map_or(ts, |value: i64| value.min(ts)));
        last_ts = Some(last_ts.map_or(ts, |value: i64| value.max(ts)));
        prior_ts = Some(ts);
        prior_line = Some(line.to_owned());
        row_count += 1;
    }
    let minimum_rows = (expected_rows as f64 * MIN_RANKING_MINUTE_COVERAGE).ceil() as usize;
    if row_count < minimum_rows
        || first_ts != Some(month_start)
        || last_ts != Some(month_end - MINUTE_MS)
    {
        return Ok(None);
    }
    let expected_days = usize::try_from((month_end - month_start) / DAY_MS)?;
    if daily_quote_volume.len() != expected_days {
        return Ok(None);
    }
    let median_daily_quote_volume = median(daily_quote_volume.into_values().collect())?;
    Ok(Some(CompleteArchiveCandidate {
        family: family.to_owned(),
        median_daily_quote_volume,
        source_url: source_url.to_owned(),
        source_sha256,
        source_rows: row_count,
        source_first_ts: first_ts.context("missing first archive timestamp")?,
        source_last_ts: last_ts.context("missing last archive timestamp")?,
    }))
}

/// 将通过审计的内部候选转换为可序列化 manifest 成员。
fn member_from_candidate(candidate: CompleteArchiveCandidate) -> HistoricalUniverseMember {
    HistoricalUniverseMember {
        symbol: format!("{}-SWAP", candidate.family),
        median_daily_quote_volume: candidate.median_daily_quote_volume,
        source_url: candidate.source_url,
        source_sha256: candidate.source_sha256,
        source_rows: candidate.source_rows,
        source_first_ts: candidate.source_first_ts,
        source_last_ts: candidate.source_last_ts,
    }
}

/// 计算非空有限样本的中位数。
fn median(mut values: Vec<f64>) -> Result<f64> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        bail!("median requires non-empty finite values");
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Ok(if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    })
}

/// 解析严格 `YYYY-MM` 月份。
fn parse_month(value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d")
        .with_context(|| format!("parse month {value} as YYYY-MM"))
}

/// 将月份规范化为 `YYYY-MM`。
fn format_month(month: NaiveDate) -> String {
    month.format("%Y-%m").to_string()
}

/// 返回 UTC 自然月起点的 Unix 毫秒值。
fn month_start_ms(month: NaiveDate) -> Result<i64> {
    Utc.with_ymd_and_hms(month.year(), month.month(), 1, 0, 0, 0)
        .single()
        .map(|value| value.timestamp_millis())
        .context("month is outside supported UTC range")
}

/// OKX 官方月包按 UTC+8 自然月切分，转换为 CSV 中实际保存的 UTC 毫秒边界。
fn okx_archive_month_start_ms(month: NaiveDate) -> Result<i64> {
    Ok(month_start_ms(month)? - OKX_ARCHIVE_UTC_OFFSET_MS)
}

/// 返回下一个自然月的第一天。
fn next_month(month: NaiveDate) -> Result<NaiveDate> {
    let (year, month_number) = if month.month() == 12 {
        (month.year() + 1, 1)
    } else {
        (month.year(), month.month() + 1)
    };
    NaiveDate::from_ymd_opt(year, month_number, 1).context("next month is outside supported range")
}

/// 返回上一个自然月的第一天。
fn previous_month(month: NaiveDate) -> Result<NaiveDate> {
    let (year, month_number) = if month.month() == 1 {
        (month.year() - 1, 12)
    } else {
        (month.year(), month.month() - 1)
    };
    NaiveDate::from_ymd_opt(year, month_number, 1)
        .context("previous month is outside supported range")
}

/// 展开包含首尾的连续月份列表。
fn inclusive_months(start: &str, end: &str) -> Result<Vec<NaiveDate>> {
    let start = parse_month(start)?;
    let end = parse_month(end)?;
    let mut months = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        months.push(cursor);
        cursor = next_month(cursor)?;
    }
    Ok(months)
}

/// 通过同目录临时文件和 rename 原子写入 manifest。
fn write_manifest_atomic(path: &Path, manifest: &HistoricalUniverseManifest) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create manifest directory {}", parent.display()))?;
    let temporary = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(manifest).context("serialize historical universe")?;
    std::fs::write(&temporary, json)
        .with_context(|| format!("write temporary manifest {}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .with_context(|| format!("publish manifest {}", path.display()))?;
    Ok(())
}

/// 校验 OKX 业务码后提取响应数据。
fn api_data<T>(envelope: ApiEnvelope<T>, action: &str) -> Result<T> {
    if envelope.code != "0" {
        bail!(
            "{action} failed: code={} msg={}",
            envelope.code,
            envelope.msg
        );
    }
    envelope
        .data
        .with_context(|| format!("{action} returned no data"))
}

/// 对公共 GET JSON 请求执行有界退避重试。
async fn get_json_with_retry<T>(client: &Client, url: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let mut last_error = None;
    for attempt in 0..4u64 {
        let response = async {
            client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .json::<T>()
                .await
        }
        .await;
        match response {
            Ok(value) => return Ok(value),
            Err(error) => last_error = Some(error),
        }
        sleep(Duration::from_millis(250 * (attempt + 1))).await;
    }
    Err(last_error.context("GET retry loop produced no error")?)
        .with_context(|| format!("GET {url} failed after retries"))
}

/// 对公共 POST JSON 请求执行有界退避重试。
async fn post_json_with_retry<T, B>(client: &Client, url: &str, body: &B) -> Result<T>
where
    T: serde::de::DeserializeOwned,
    B: Serialize + ?Sized,
{
    let mut last_error = None;
    for attempt in 0..4u64 {
        let response = async {
            client
                .post(url)
                .json(body)
                .send()
                .await?
                .error_for_status()?
                .json::<T>()
                .await
        }
        .await;
        match response {
            Ok(value) => return Ok(value),
            Err(error) => last_error = Some(error),
        }
        sleep(Duration::from_millis(250 * (attempt + 1))).await;
    }
    Err(last_error.context("POST retry loop produced no error")?)
        .with_context(|| format!("POST {url} failed after retries"))
}

/// 对官方归档下载执行有界退避重试。
async fn get_bytes_with_retry(client: &Client, url: &str) -> Result<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::FileOptions;

    #[test]
    fn args_accept_months_after_stock_perpetual_launch() {
        let args = parse_historical_universe_args(
            [
                "--start-month",
                "2026-02",
                "--end-month",
                "2026-02",
                "--output",
                "/tmp/out.json",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(args.start_month, "2026-02");
        assert_eq!(args.end_month, "2026-02");
    }

    #[test]
    fn current_live_pool_accepts_crypto_category_and_rejects_stock_category() {
        let crypto = PublicSwapInstrument {
            inst_id: "BTC-USDT-SWAP".to_owned(),
            inst_family: "BTC-USDT".to_owned(),
            inst_category: "1".to_owned(),
            settle_ccy: "USDT".to_owned(),
            state: "live".to_owned(),
            ct_val: "0.01".to_owned(),
            ct_val_ccy: "BTC".to_owned(),
        };
        let stock = PublicSwapInstrument {
            inst_id: "TSLA-USDT-SWAP".to_owned(),
            inst_family: "TSLA-USDT".to_owned(),
            inst_category: "3".to_owned(),
            settle_ccy: "USDT".to_owned(),
            state: "live".to_owned(),
            ct_val: "1".to_owned(),
            ct_val_ccy: "TSLA".to_owned(),
        };

        assert!(is_current_live_crypto_usdt_swap(&crypto));
        assert!(!is_current_live_crypto_usdt_swap(&stock));
    }

    #[test]
    fn ranking_allows_tiny_gap_but_still_requires_month_boundaries() {
        let month = parse_month("2025-02").unwrap();
        let complete = archive_fixture(month, None, false, None, None);
        let ranked = audit_archive_bytes("BTC-USDT", "fixture", &complete, month, 1.0)
            .unwrap()
            .expect("complete month should be eligible");
        assert_eq!(ranked.source_rows, 28 * 24 * 60);
        assert!(ranked.median_daily_quote_volume > 0.0);

        let missing = archive_fixture(
            month,
            Some(okx_archive_month_start_ms(month).unwrap() + 60_000),
            false,
            None,
            None,
        );
        assert!(
            audit_archive_bytes("BTC-USDT", "fixture", &missing, month, 1.0)
                .unwrap()
                .is_some()
        );

        let missing_boundary = archive_fixture(
            month,
            Some(okx_archive_month_start_ms(month).unwrap()),
            false,
            None,
            None,
        );
        assert!(
            audit_archive_bytes("BTC-USDT", "fixture", &missing_boundary, month, 1.0)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn official_reverse_chronological_archive_is_accepted_without_changing_time_bounds() {
        let month = parse_month("2025-02").unwrap();
        let archive = archive_fixture(month, None, true, None, None);
        let ranked = audit_archive_bytes("BTC-USDT", "fixture", &archive, month, 1.0)
            .unwrap()
            .expect("reverse chronological official archive should remain complete");
        assert_eq!(
            ranked.source_first_ts,
            okx_archive_month_start_ms(month).unwrap()
        );
        assert_eq!(
            ranked.source_last_ts,
            okx_archive_month_start_ms(next_month(month).unwrap()).unwrap() - MINUTE_MS
        );
    }

    #[test]
    fn identical_archive_correction_rows_are_deduplicated() {
        let month = parse_month("2025-02").unwrap();
        let duplicate_ts = okx_archive_month_start_ms(month).unwrap() + 10 * MINUTE_MS;
        let archive = archive_fixture(month, None, false, Some(duplicate_ts), None);
        let ranked = audit_archive_bytes("BTC-USDT", "fixture", &archive, month, 1.0)
            .unwrap()
            .expect("identical duplicate should preserve complete-month eligibility");
        assert_eq!(ranked.source_rows, 28 * 24 * 60);
    }

    #[test]
    fn manifest_months_use_prior_calendar_month_without_future_data() {
        let months = inclusive_months("2024-07", "2024-09").unwrap();
        assert_eq!(months.len(), 3);
        assert_eq!(format_month(previous_month(months[0]).unwrap()), "2024-06");
        assert_eq!(month_start_ms(months[0]).unwrap(), 1_719_792_000_000);
    }

    #[test]
    fn archive_with_missing_quote_volume_uses_contract_notional_fallback() {
        let month = parse_month("2025-02").unwrap();
        let invalid_ts = okx_archive_month_start_ms(month).unwrap() + 10 * MINUTE_MS;
        let archive = archive_fixture(month, None, false, None, Some(invalid_ts));

        assert!(
            audit_archive_bytes("BTC-USDT", "fixture", &archive, month, 1.0)
                .unwrap()
                .is_some()
        );
    }

    fn archive_fixture(
        month: NaiveDate,
        missing_ts: Option<i64>,
        descending: bool,
        duplicate_ts: Option<i64>,
        invalid_quote_ts: Option<i64>,
    ) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        zip.start_file("fixture.csv", FileOptions::default())
            .unwrap();
        writeln!(
            zip,
            "instrument_name,open,high,low,close,vol,vol_ccy,vol_quote,open_time,confirm"
        )
        .unwrap();
        let start = okx_archive_month_start_ms(month).unwrap();
        let end = okx_archive_month_start_ms(next_month(month).unwrap()).unwrap();
        let midpoint = start + (end - start) / 2;
        let mut timestamps = (start..end).step_by(MINUTE_MS as usize).collect::<Vec<_>>();
        if descending {
            timestamps.reverse();
        }
        for ts in timestamps {
            if Some(ts) == missing_ts {
                continue;
            }
            let quote_volume = if Some(ts) == invalid_quote_ts {
                "invalid".to_owned()
            } else {
                ((ts / DAY_MS + 1) as f64).to_string()
            };
            let row = format!(
                "BTC-USDT-SWAP,1,1,1,1,1,1,{},{},{}",
                quote_volume,
                ts,
                if ts < midpoint { "0" } else { "1" }
            );
            writeln!(zip, "{row}").unwrap();
            if Some(ts) == duplicate_ts {
                writeln!(zip, "{row}").unwrap();
            }
        }
        zip.finish().unwrap().into_inner()
    }
}
