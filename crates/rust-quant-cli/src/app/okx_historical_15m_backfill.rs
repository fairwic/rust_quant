use super::market_velocity_backfill::fetch_okx_history_candles;
use super::okx_historical_universe::{
    load_current_live_contract_values, load_official_archive_urls, HistoricalUniverseManifest,
};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, TimeZone, Utc};
use okx::dto::market_dto::CandleOkxRespDto;
use reqwest::{Client, Proxy};
use rust_quant_market::models::{
    get_quant_core_postgres_pool, quote_legacy_table_name, CandlesModel,
};
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Cursor};
use std::path::PathBuf;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::sleep;
use zip::ZipArchive;

const MINUTE_MS: i64 = 60 * 1_000;
const CANDLE_15M_MS: i64 = 15 * MINUTE_MS;
const OKX_HISTORY_PAGE_LIMIT: usize = 100;
const OKX_ARCHIVE_UTC_OFFSET_MS: i64 = 8 * 60 * 60 * 1_000;
const DEFAULT_OKX_BASE: &str = "https://www.okx.com";
const OKX_ARCHIVE_CDN_BASE: &str =
    "https://static.okx.com/cdn/okex/traderecords/candlesticks/monthly";

/// 需要写入并在 Postgres 中逐根验收的 UTC 15m 半开窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Historical15mCoverageWindow {
    /// 首根预热 K 线时间，Unix 毫秒时间戳，包含在窗口内。
    pub warmup_start_ms: i64,
    /// 评估窗口结束边界，Unix 毫秒时间戳，不包含该时刻。
    pub evaluation_end_exclusive_ms: i64,
}

impl Historical15mCoverageWindow {
    /// 创建严格 15m 对齐的半开窗口，拒绝空窗口和无法精确计数的边界。
    pub fn new(warmup_start_ms: i64, evaluation_end_exclusive_ms: i64) -> Result<Self> {
        if warmup_start_ms >= evaluation_end_exclusive_ms {
            bail!("warmup start must be earlier than evaluation end exclusive");
        }
        if warmup_start_ms.rem_euclid(CANDLE_15M_MS) != 0
            || evaluation_end_exclusive_ms.rem_euclid(CANDLE_15M_MS) != 0
        {
            bail!("historical 15m coverage window must align to 900000ms boundaries");
        }
        Ok(Self {
            warmup_start_ms,
            evaluation_end_exclusive_ms,
        })
    }

    fn expected_candles(self) -> Result<usize> {
        usize::try_from((self.evaluation_end_exclusive_ms - self.warmup_start_ms) / CANDLE_15M_MS)
            .context("historical 15m expected candle count overflow")
    }
}

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
    /// true 时 required month 不允许使用任何 partial source，并要求显式冻结覆盖窗口。
    pub strict: bool,
    /// 显式冻结的预热到评估结束半开窗口；None 保留旧 manifest 月份行为。
    pub coverage_window: Option<Historical15mCoverageWindow>,
    /// OKX 官方站点基地址；本地协议测试可替换。
    pub okx_base: String,
    /// 显式 HTTP/SOCKS 代理；None 强制直连且不读取系统代理。
    pub proxy_url: Option<String>,
}

/// 冻结成员直接补数所需的执行参数，不包含也不读取旧 universe manifest。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Historical15mFrozenSymbolsBackfillArgs {
    /// 同时下载的官方月包数量。
    pub download_concurrency: usize,
    /// 每批写入本地 K 线表的行数。
    pub batch_size: usize,
    /// 是否写入本地 quant_core；false 时只下载、聚合和校验。
    pub write: bool,
    /// true 时 required 交集只接受同源完整数据。
    pub strict: bool,
    /// OKX 官方站点基地址；本地协议测试可替换。
    pub okx_base: String,
    /// 显式 HTTP/SOCKS 代理；None 强制直连且不读取系统代理。
    pub proxy_url: Option<String>,
}

impl From<&Historical15mBackfillArgs> for Historical15mFrozenSymbolsBackfillArgs {
    fn from(args: &Historical15mBackfillArgs) -> Self {
        Self {
            download_concurrency: args.download_concurrency,
            batch_size: args.batch_size,
            write: args.write,
            strict: args.strict,
            okx_base: args.okx_base.clone(),
            proxy_url: args.proxy_url.clone(),
        }
    }
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
    /// 写入前已通过 required 交集审计、因而无需重复下载的 symbol×month 数。
    pub already_complete_files: usize,
    /// 写入后通过边界、确认状态、根数和 15m 连续性审计的币种数。
    pub coverage_audited_symbols: usize,
    /// true 表示只校验和聚合，没有写本地数据库。
    pub dry_run: bool,
}

/// 单个币种在显式半开窗口内的 Postgres 覆盖证据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Historical15mCoverageAudit {
    /// 被审计的 OKX 永续合约标识。
    pub symbol: String,
    /// 期望首根 K 线时间，Unix 毫秒时间戳。
    pub expected_first_ts: i64,
    /// 期望末根 K 线时间，Unix 毫秒时间戳。
    pub expected_last_ts: i64,
    /// 半开窗口按 15m 精确换算出的期望根数。
    pub expected_candles: usize,
    /// Postgres 中实际首根 K 线；None 表示窗口内没有数据。
    pub actual_first_ts: Option<i64>,
    /// Postgres 中实际末根 K 线；None 表示窗口内没有数据。
    pub actual_last_ts: Option<i64>,
    /// Postgres 中落在半开窗口内的实际根数。
    pub actual_candles: usize,
    /// `confirm != 1` 的实际行数；通过审计时必须为零。
    pub unconfirmed_candles: usize,
    /// 相邻时间差不等于 900000ms 的实际次数；通过审计时必须为零。
    pub discontinuities: usize,
    /// 缺失或不是非负有限十进制的 `vol_ccy` 行数；通过审计时必须为零。
    pub invalid_volume_ccy: usize,
}

/// 单次索引窗口查询得到的原始覆盖统计，只在生成审计结论前短暂存在。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Historical15mCoverageStats {
    /// 窗口内最早时间戳。
    first_ts: Option<i64>,
    /// 窗口内最晚时间戳。
    last_ts: Option<i64>,
    /// 窗口内实际行数。
    actual_candles: usize,
    /// 未确认行数。
    unconfirmed_candles: usize,
    /// 相邻行不满足 900000ms 的次数。
    discontinuities: usize,
    /// 不符合官方 `vol_ccy` 存储格式的行数。
    invalid_volume_ccy: usize,
}

/// 唯一归档请求以 `symbol x month` 标识，URL 只作为当次官方下载地址。
#[derive(Debug, Clone, PartialEq)]
struct ArchiveRequest {
    symbol: String,
    month: String,
    url: Option<String>,
    required: bool,
    required_window: Historical15mCoverageWindow,
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
    let mut strict = false;
    let mut warmup_start_ms = None;
    let mut evaluation_end_exclusive_ms = None;
    let mut okx_base = DEFAULT_OKX_BASE.to_owned();
    let mut proxy_url = None;
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
            "--strict" => strict = true,
            "--warmup-start-ms" => {
                warmup_start_ms = Some(
                    value(&mut values)?
                        .parse()
                        .context("parse --warmup-start-ms")?,
                )
            }
            "--evaluation-end-exclusive-ms" => {
                evaluation_end_exclusive_ms = Some(
                    value(&mut values)?
                        .parse()
                        .context("parse --evaluation-end-exclusive-ms")?,
                )
            }
            "--okx-base" => okx_base = value(&mut values)?.trim_end_matches('/').to_owned(),
            "--proxy-url" => proxy_url = Some(value(&mut values)?),
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
    let coverage_window = match (warmup_start_ms, evaluation_end_exclusive_ms) {
        (Some(start), Some(end)) => Some(Historical15mCoverageWindow::new(start, end)?),
        (None, None) => None,
        _ => bail!("--warmup-start-ms and --evaluation-end-exclusive-ms must be provided together"),
    };
    if strict && coverage_window.is_none() {
        bail!("--strict requires an explicit historical 15m coverage window");
    }
    Ok(Historical15mBackfillArgs {
        manifest: manifest.context("--manifest is required")?,
        download_concurrency,
        batch_size,
        write,
        strict,
        coverage_window,
        okx_base,
        proxy_url,
    })
}

/// 返回只面向本地研究库的命令用法。
pub fn historical_15m_backfill_usage() -> &'static str {
    "Usage: okx_historical_15m_backfill --manifest PATH [--download-concurrency 8] [--batch-size 500] [--warmup-start-ms UNIX_MS --evaluation-end-exclusive-ms UNIX_MS] [--strict] [--proxy-url URL] [--dry-run|--write]"
}

/// 构造明确直连或显式代理的 Research 客户端，不继承这台 Mac 的系统代理状态。
pub fn build_historical_15m_http_client(
    proxy_url: Option<&str>,
    timeout: Duration,
) -> Result<Client> {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .no_proxy();
    if let Some(proxy_url) = proxy_url.map(str::trim).filter(|value| !value.is_empty()) {
        builder = builder
            .proxy(Proxy::all(proxy_url).context("configure historical 15m explicit proxy")?);
    }
    builder
        .build()
        .context("build OKX historical 15m HTTP client")
}

/// 下载 manifest 月份或显式冻结窗口覆盖的全部归档月，并聚合写入 15m。
pub async fn run_historical_15m_backfill(
    args: &Historical15mBackfillArgs,
) -> Result<Historical15mBackfillReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read manifest {}", args.manifest.display()))?,
    )
    .context("decode historical universe manifest")?;
    validate_manifest(&manifest)?;
    let execution_args = Historical15mFrozenSymbolsBackfillArgs::from(args);
    if let Some(window) = args.coverage_window {
        let symbols = manifest
            .months
            .iter()
            .flat_map(|month| month.members.iter())
            .map(|member| member.symbol.clone())
            .collect::<Vec<_>>();
        return run_historical_15m_backfill_for_symbols(&execution_args, &symbols, window).await;
    }
    let requests = archive_requests(&manifest, &args.okx_base, args.proxy_url.as_deref()).await?;
    execute_historical_15m_requests(&execution_args, requests, None).await
}

/// 对调用方已冻结的 symbol 集合回补显式窗口，不读取旧 manifest，也不选择替补成员。
pub async fn run_historical_15m_backfill_for_symbols(
    args: &Historical15mFrozenSymbolsBackfillArgs,
    symbols: &[String],
    window: Historical15mCoverageWindow,
) -> Result<Historical15mBackfillReport> {
    if args.download_concurrency == 0 || args.download_concurrency > 16 {
        bail!("download concurrency must be between 1 and 16");
    }
    if args.batch_size == 0 || args.batch_size > 2_000 {
        bail!("batch size must be between 1 and 2000");
    }
    if symbols.is_empty() {
        bail!("historical 15m backfill requires at least one frozen symbol");
    }
    let mut unique_symbols = BTreeSet::new();
    for symbol in symbols {
        if !unique_symbols.insert(symbol.clone()) {
            bail!("frozen symbol list contains duplicate member {symbol}");
        }
    }
    let requests = archive_requests_for_symbols(
        &unique_symbols,
        &args.okx_base,
        args.proxy_url.as_deref(),
        window,
    )
    .await?;
    execute_historical_15m_requests(args, requests, Some(window)).await
}

/// 执行已冻结的 symbol×month 请求；成员选择必须在进入本函数前完成。
async fn execute_historical_15m_requests(
    args: &Historical15mFrozenSymbolsBackfillArgs,
    requests: Vec<ArchiveRequest>,
    coverage_window: Option<Historical15mCoverageWindow>,
) -> Result<Historical15mBackfillReport> {
    let symbols = requests
        .iter()
        .map(|request| request.symbol.clone())
        .collect::<BTreeSet<_>>();
    let client =
        build_historical_15m_http_client(args.proxy_url.as_deref(), Duration::from_secs(60))?;
    let model = CandlesModel::new();
    if args.write {
        for symbol in &symbols {
            model.create_table(symbol, "15m").await?;
        }
    }
    let mut pending_requests = Vec::with_capacity(requests.len());
    let mut already_complete_files = 0usize;
    for request in requests {
        if args.write {
            let stats =
                load_historical_15m_coverage_stats(&request.symbol, request.required_window)
                    .await?;
            if coverage_audit_from_stats(
                &request.symbol,
                request.required_window,
                stats.first_ts,
                stats.last_ts,
                stats.actual_candles,
                stats.unconfirmed_candles,
                stats.discontinuities,
                stats.invalid_volume_ccy,
            )
            .is_ok()
            {
                already_complete_files += 1;
                continue;
            }
        }
        pending_requests.push(request);
    }
    let mut report = Historical15mBackfillReport {
        symbols: symbols.len(),
        archive_files: pending_requests.len(),
        already_complete_files,
        dry_run: !args.write,
        ..Default::default()
    };
    for chunk in pending_requests.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for request in chunk.iter().cloned() {
            let client = client.clone();
            let okx_base = args.okx_base.clone();
            let strict = args.strict;
            tasks.spawn(async move {
                let archive_result: Result<(Vec<CandleOkxRespDto>, bool)> = async {
                    let url = request.url.as_deref().context("official archive link is absent")?;
                    let bytes = download_archive_with_retry(&client, url).await?;
                    if strict && request.required {
                        validate_archive_native_vol_ccy_window(
                            &bytes,
                            &request.symbol,
                            &request.month,
                            request.required_window,
                        )?;
                    }
                    match aggregate_archive_to_15m(
                        &bytes,
                        &request.symbol,
                        &request.month,
                        request.contract_value,
                    ) {
                        Ok(candles) => Ok((candles, false)),
                        Err(full_month_error) => {
                            let candles = aggregate_available_archive_to_15m(
                                &bytes,
                                &request.symbol,
                                &request.month,
                                request.contract_value,
                            )?;
                            if strict && request.required {
                                // 首尾月允许窗口外缺失，但 required 交集本身必须逐根完整；
                                // 交集不完整时转 REST，而不是把可用桶当成正式数据。
                                validate_complete_15m_window(
                                    &candles,
                                    &request.symbol,
                                    request.required_window,
                                )
                                .with_context(|| {
                                    format!(
                                        "full archive failed ({full_month_error}); required intersection is incomplete"
                                    )
                                })?;
                                Ok((candles, false))
                            } else {
                                Ok((candles, true))
                            }
                        }
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
                            Err(_) if !request.required => {
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
            reject_strict_required_partial(args.strict, &request, partial)?;
            let Some(mut candles) = candles else {
                report.optional_outcome_files_unavailable += 1;
                continue;
            };
            if let Some(window) = coverage_window {
                candles = candles_in_window(candles, window)?;
                if candles.is_empty() && request.required {
                    bail!(
                        "required archive has no candles inside explicit window for {} {}",
                        request.symbol,
                        request.month
                    );
                }
            }
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
    if args.write {
        if let Some(window) = coverage_window {
            let symbols = symbols.into_iter().collect::<Vec<_>>();
            report.coverage_audited_symbols =
                audit_historical_15m_coverage(&symbols, window).await?.len();
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
        OKX_HISTORY_PAGE_LIMIT,
        120,
    )
    .await?;
    if validate_complete_rest_month(&candles, &request.symbol, &request.month).is_ok() {
        Ok((candles, false))
    } else {
        validate_available_rest_month(&candles, &request.symbol, &request.month)?;
        let required_intersection_complete = request.required
            && validate_complete_15m_window(&candles, &request.symbol, request.required_window)
                .is_ok();
        Ok((candles, !required_intersection_complete))
    }
}

/// 正式 strict 模式把 required month 的 partial source 视为硬失败。
fn reject_strict_required_partial(
    strict: bool,
    request: &ArchiveRequest,
    partial: bool,
) -> Result<()> {
    if strict && request.required && partial {
        bail!(
            "strict historical backfill rejects partial required month for {} {}",
            request.symbol,
            request.month
        );
    }
    Ok(())
}

/// 只保留显式半开窗口内的 K 线，避免边界归档月把额外行情写入冻结数据集。
fn candles_in_window(
    candles: Vec<CandleOkxRespDto>,
    window: Historical15mCoverageWindow,
) -> Result<Vec<CandleOkxRespDto>> {
    candles
        .into_iter()
        .filter_map(|candle| {
            let ts = match candle.ts.parse::<i64>() {
                Ok(ts) => ts,
                Err(error) => return Some(Err(error).context("parse backfill candle timestamp")),
            };
            (ts >= window.warmup_start_ms && ts < window.evaluation_end_exclusive_ms)
                .then_some(Ok(candle))
        })
        .collect()
}

/// 对已写入的币种执行严格 Postgres 覆盖审计；任一币种不完整即整体失败。
///
/// 查询只扫描带 `ts` 唯一索引的冻结半开窗口，验证首尾边界、确认状态、
/// 精确根数和相邻 900000ms 连续性，不用全表 COUNT 代替时序证据。
pub async fn audit_historical_15m_coverage(
    symbols: &[String],
    window: Historical15mCoverageWindow,
) -> Result<Vec<Historical15mCoverageAudit>> {
    let mut audits = Vec::with_capacity(symbols.len());
    let mut failures = Vec::new();
    for symbol in symbols {
        match audit_historical_15m_symbol_coverage(symbol, window).await {
            Ok(audit) => audits.push(audit),
            Err(error) => failures.push(error.to_string()),
        }
    }
    if !failures.is_empty() {
        bail!(
            "historical 15m coverage audit failed closed: {}",
            failures.join("; ")
        );
    }
    Ok(audits)
}

/// 审计一个 symbol 的冻结半开窗口；查询或覆盖不完整均返回错误。
pub async fn audit_historical_15m_symbol_coverage(
    symbol: &str,
    window: Historical15mCoverageWindow,
) -> Result<Historical15mCoverageAudit> {
    let stats = load_historical_15m_coverage_stats(symbol, window).await?;
    coverage_audit_from_stats(
        symbol,
        window,
        stats.first_ts,
        stats.last_ts,
        stats.actual_candles,
        stats.unconfirmed_candles,
        stats.discontinuities,
        stats.invalid_volume_ccy,
    )
}

/// 从带 `ts` 唯一索引的 legacy 分表聚合指定半开窗口覆盖统计。
async fn load_historical_15m_coverage_stats(
    symbol: &str,
    window: Historical15mCoverageWindow,
) -> Result<Historical15mCoverageStats> {
    let pool = get_quant_core_postgres_pool()?;
    let table_name = CandlesModel::get_table_name(symbol, "15m");
    let quoted_table_name = quote_legacy_table_name(&table_name)?;
    let row = sqlx::query(&format!(
        "WITH ordered AS (
            SELECT ts, confirm, vol_ccy, LAG(ts) OVER (ORDER BY ts) AS previous_ts
            FROM {quoted_table_name}
            WHERE ts >= $1 AND ts < $2
         )
         SELECT MIN(ts) AS first_ts,
                MAX(ts) AS last_ts,
                COUNT(*) AS actual_candles,
                COUNT(*) FILTER (WHERE confirm IS DISTINCT FROM '1') AS unconfirmed_candles,
                COUNT(*) FILTER (
                    WHERE previous_ts IS NOT NULL AND ts - previous_ts <> $3
                ) AS discontinuities,
                COUNT(*) FILTER (
                    WHERE vol_ccy IS NULL
                       OR btrim(vol_ccy) !~ '^(0|[0-9]+([.][0-9]*)?|[.][0-9]+)([eE][+-]?[0-9]+)?$'
                ) AS invalid_volume_ccy
         FROM ordered"
    ))
    .bind(window.warmup_start_ms)
    .bind(window.evaluation_end_exclusive_ms)
    .bind(CANDLE_15M_MS)
    .fetch_one(pool)
    .await
    .with_context(|| format!("audit historical 15m coverage for {symbol}"))?;
    Ok(Historical15mCoverageStats {
        first_ts: row.try_get("first_ts")?,
        last_ts: row.try_get("last_ts")?,
        actual_candles: usize::try_from(row.try_get::<i64, _>("actual_candles")?)?,
        unconfirmed_candles: usize::try_from(row.try_get::<i64, _>("unconfirmed_candles")?)?,
        discontinuities: usize::try_from(row.try_get::<i64, _>("discontinuities")?)?,
        invalid_volume_ccy: usize::try_from(row.try_get::<i64, _>("invalid_volume_ccy")?)?,
    })
}

/// 把数据库聚合结果转换为覆盖证据，集中锁定 fail-closed 判定。
fn coverage_audit_from_stats(
    symbol: &str,
    window: Historical15mCoverageWindow,
    actual_first_ts: Option<i64>,
    actual_last_ts: Option<i64>,
    actual_candles: usize,
    unconfirmed_candles: usize,
    discontinuities: usize,
    invalid_volume_ccy: usize,
) -> Result<Historical15mCoverageAudit> {
    let expected_candles = window.expected_candles()?;
    let expected_last_ts = window
        .evaluation_end_exclusive_ms
        .checked_sub(CANDLE_15M_MS)
        .context("historical 15m expected last timestamp overflow")?;
    if actual_first_ts != Some(window.warmup_start_ms)
        || actual_last_ts != Some(expected_last_ts)
        || actual_candles != expected_candles
        || unconfirmed_candles != 0
        || discontinuities != 0
        || invalid_volume_ccy != 0
    {
        bail!(
            "{symbol}: first={actual_first_ts:?}/{}, last={actual_last_ts:?}/{expected_last_ts}, candles={actual_candles}/{expected_candles}, unconfirmed={unconfirmed_candles}, discontinuities={discontinuities}, invalid_vol_ccy={invalid_volume_ccy}",
            window.warmup_start_ms
        );
    }
    Ok(Historical15mCoverageAudit {
        symbol: symbol.to_owned(),
        expected_first_ts: window.warmup_start_ms,
        expected_last_ts,
        expected_candles,
        actual_first_ts,
        actual_last_ts,
        actual_candles,
        unconfirmed_candles,
        discontinuities,
        invalid_volume_ccy,
    })
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

/// 验证任意 UTC 15m 半开交集的首尾、根数、confirm 和原始 `vol_ccy`。
///
/// 窗口外 K 线不会参与判断，供首尾归档月在不要求整月的情况下严格验收。
pub fn validate_complete_15m_window(
    candles: &[CandleOkxRespDto],
    symbol: &str,
    window: Historical15mCoverageWindow,
) -> Result<()> {
    let expected = window.expected_candles()?;
    let mut actual = 0usize;
    for candle in candles {
        let ts = candle
            .ts
            .parse::<i64>()
            .context("parse required 15m candle timestamp")?;
        if ts < window.warmup_start_ms || ts >= window.evaluation_end_exclusive_ms {
            continue;
        }
        let expected_ts = window
            .warmup_start_ms
            .checked_add(i64::try_from(actual)? * CANDLE_15M_MS)
            .context("required 15m timestamp overflow")?;
        let volume_ccy = candle
            .vol_ccy
            .parse::<f64>()
            .with_context(|| format!("missing native vol_ccy for {symbol} at {ts}"))?;
        if ts != expected_ts || candle.confirm != "1" || !volume_ccy.is_finite() || volume_ccy < 0.0
        {
            bail!("required 15m window contains invalid candle for {symbol} at {ts}");
        }
        actual += 1;
    }
    if actual != expected {
        bail!(
            "required 15m window is incomplete for {symbol}: actual={actual}, expected={expected}"
        );
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
    validate_complete_15m_window(
        candles,
        symbol,
        Historical15mCoverageWindow::new(month_start, month_end)?,
    )
    .with_context(|| format!("REST fallback month is incomplete for {symbol} {month}"))
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

/// 按旧 manifest 规则展开排名月、生效月和可选 outcome 月。
async fn archive_requests(
    manifest: &HistoricalUniverseManifest,
    okx_base: &str,
    proxy_url: Option<&str>,
) -> Result<Vec<ArchiveRequest>> {
    let mut month_symbols = BTreeMap::<String, BTreeMap<String, bool>>::new();
    for effective in &manifest.months {
        let mut months = BTreeMap::from([(effective.ranking_source_month.clone(), true)]);
        months.insert(month_label(effective.effective_from_ms)?, true);
        // 旧入口的 effective_to 月只用于月末持仓结算；后续成员月仍会提升为 required。
        months
            .entry(month_label(effective.effective_to_ms)?)
            .or_insert(false);
        for (month, required) in months {
            let members = month_symbols.entry(month).or_default();
            for member in &effective.members {
                members
                    .entry(member.symbol.clone())
                    .and_modify(|existing| *existing |= required)
                    .or_insert(required);
            }
        }
    }
    build_archive_requests(okx_base, proxy_url, month_symbols, None).await
}

/// 展开调用方冻结 symbol 在显式半开窗口内需要的全部 OKX 月包。
async fn archive_requests_for_symbols(
    symbols: &BTreeSet<String>,
    okx_base: &str,
    proxy_url: Option<&str>,
    window: Historical15mCoverageWindow,
) -> Result<Vec<ArchiveRequest>> {
    let mut month_symbols = BTreeMap::<String, BTreeMap<String, bool>>::new();
    for month in archive_month_labels_for_window(window)? {
        month_symbols.insert(
            month,
            symbols
                .iter()
                .map(|symbol| (symbol.clone(), true))
                .collect(),
        );
    }
    build_archive_requests(okx_base, proxy_url, month_symbols, Some(window)).await
}

/// 从冻结的 month×symbol 集合解析官方 URL、ctVal 和 required 交集。
async fn build_archive_requests(
    okx_base: &str,
    proxy_url: Option<&str>,
    month_symbols: BTreeMap<String, BTreeMap<String, bool>>,
    explicit_window: Option<Historical15mCoverageWindow>,
) -> Result<Vec<ArchiveRequest>> {
    let client = build_historical_15m_http_client(proxy_url, Duration::from_secs(45))?;
    let contract_values = load_current_live_contract_values(&client, okx_base).await?;
    let mut requests = Vec::new();
    for (month, symbols) in month_symbols {
        let families = symbols
            .keys()
            .map(|symbol| {
                symbol
                    .strip_suffix("-SWAP")
                    .map(str::to_owned)
                    .with_context(|| format!("invalid swap symbol {symbol}"))
            })
            .collect::<Result<Vec<_>>>()?;
        // 正式 OKX 路径直接使用可验证的 CDN 规则，确保下载仍经过本命令显式代理；
        // 自定义 base 只用于本地协议 fixture，继续复用其 download-link endpoint。
        let urls = if okx_base == DEFAULT_OKX_BASE {
            BTreeMap::new()
        } else {
            load_official_archive_urls(okx_base, &families, &month).await?
        };
        let full_month = {
            let (start, end) = archive_month_bounds(&month)?;
            Historical15mCoverageWindow::new(start, end)?
        };
        let required_window = explicit_window
            .map(|window| archive_required_window(window, &month))
            .transpose()?
            .unwrap_or(full_month);
        for (symbol, required) in symbols {
            let family = symbol.strip_suffix("-SWAP").unwrap_or_default();
            let contract_value = *contract_values
                .get(family)
                .with_context(|| format!("missing current ctVal for {symbol}"))?;
            let url = urls
                .get(family)
                .cloned()
                .or_else(|| direct_archive_url(okx_base, &symbol, &month));
            requests.push(ArchiveRequest {
                symbol,
                month: month.clone(),
                url,
                required,
                required_window,
                contract_value,
            });
        }
    }
    Ok(requests)
}

/// 返回与 UTC 半开窗口相交的全部 OKX UTC+8 月包标签。
///
/// OKX 文件按 UTC+8 换月；仅枚举 UTC 日历月会漏掉月末 16:00Z 后的八小时，
/// 因此先把边界平移到归档时区，再枚举月份。
pub fn archive_month_labels_for_window(window: Historical15mCoverageWindow) -> Result<Vec<String>> {
    let first = Utc
        .timestamp_millis_opt(
            window
                .warmup_start_ms
                .checked_add(OKX_ARCHIVE_UTC_OFFSET_MS)
                .context("archive month start overflow")?,
        )
        .single()
        .context("archive month start outside supported range")?;
    let last = Utc
        .timestamp_millis_opt(
            window
                .evaluation_end_exclusive_ms
                .checked_sub(1)
                .and_then(|value| value.checked_add(OKX_ARCHIVE_UTC_OFFSET_MS))
                .context("archive month end overflow")?,
        )
        .single()
        .context("archive month end outside supported range")?;
    let mut year = first.year();
    let mut month = first.month();
    let mut labels = Vec::new();
    loop {
        labels.push(format!("{year:04}-{month:02}"));
        if year == last.year() && month == last.month() {
            break;
        }
        if month == 12 {
            year = year.checked_add(1).context("archive month year overflow")?;
            month = 1;
        } else {
            month += 1;
        }
    }
    Ok(labels)
}

/// 计算显式 UTC 窗口与单个 OKX UTC+8 月包的严格半开交集。
pub fn archive_required_window(
    window: Historical15mCoverageWindow,
    month: &str,
) -> Result<Historical15mCoverageWindow> {
    let (month_start, month_end) = archive_month_bounds(month)?;
    Historical15mCoverageWindow::new(
        window.warmup_start_ms.max(month_start),
        window.evaluation_end_exclusive_ms.min(month_end),
    )
    .with_context(|| format!("archive month {month} does not intersect explicit window"))
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

/// strict 同源模式要求 required 交集的每根分钟线都有官方原生 `vol_ccy`。
fn validate_archive_native_vol_ccy_window(
    bytes: &[u8],
    symbol: &str,
    month: &str,
    window: Historical15mCoverageWindow,
) -> Result<()> {
    let rows = parse_archive_rows(bytes, symbol, month)?;
    let expected =
        usize::try_from((window.evaluation_end_exclusive_ms - window.warmup_start_ms) / MINUTE_MS)?;
    let mut actual = 0usize;
    for (ts, line) in rows.range(window.warmup_start_ms..window.evaluation_end_exclusive_ms) {
        let expected_ts = window
            .warmup_start_ms
            .checked_add(i64::try_from(actual)? * MINUTE_MS)
            .context("required native vol_ccy timestamp overflow")?;
        let columns = line.split(',').collect::<Vec<_>>();
        let volume_ccy = columns[6]
            .parse::<f64>()
            .with_context(|| format!("missing native vol_ccy for {symbol} at {ts}"))?;
        if *ts != expected_ts || columns[9] != "1" || !volume_ccy.is_finite() || volume_ccy < 0.0 {
            bail!("invalid native vol_ccy source for {symbol} at {ts}");
        }
        actual += 1;
    }
    if actual != expected {
        bail!(
            "native vol_ccy source window is incomplete for {symbol}: actual={actual}, expected={expected}"
        );
    }
    Ok(())
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

        assert!(validate_archive_native_vol_ccy_window(
            &bytes,
            "BTC-USDT-SWAP",
            "2022-10",
            Historical15mCoverageWindow::new(start, start + CANDLE_15M_MS).unwrap(),
        )
        .is_err());
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
        assert!(!write.strict);
        assert_eq!(write.coverage_window, None);
    }

    #[test]
    fn strict_window_is_paired_aligned_and_half_open() {
        let args = parse_historical_15m_backfill_args(
            [
                "--manifest",
                "/tmp/manifest.json",
                "--strict",
                "--warmup-start-ms",
                "0",
                "--evaluation-end-exclusive-ms",
                "1800000",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap();
        assert!(args.strict);
        assert_eq!(
            args.coverage_window,
            Some(Historical15mCoverageWindow {
                warmup_start_ms: 0,
                evaluation_end_exclusive_ms: 1_800_000,
            })
        );
        for invalid in [
            vec!["--manifest", "/tmp/manifest.json", "--warmup-start-ms", "0"],
            vec![
                "--manifest",
                "/tmp/manifest.json",
                "--warmup-start-ms",
                "1",
                "--evaluation-end-exclusive-ms",
                "900000",
            ],
            vec!["--manifest", "/tmp/manifest.json", "--strict"],
        ] {
            assert!(
                parse_historical_15m_backfill_args(invalid.into_iter().map(str::to_owned)).is_err()
            );
        }
    }

    #[test]
    fn explicit_proxy_is_parsed_and_invalid_proxy_is_rejected() {
        let args = parse_historical_15m_backfill_args(
            [
                "--manifest",
                "/tmp/manifest.json",
                "--proxy-url",
                "http://127.0.0.1:7890",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(args.proxy_url.as_deref(), Some("http://127.0.0.1:7890"));
        build_historical_15m_http_client(None, Duration::from_secs(1)).unwrap();
        assert!(
            build_historical_15m_http_client(Some("://invalid"), Duration::from_secs(1)).is_err()
        );
    }

    #[test]
    fn explicit_window_expands_every_intersecting_archive_month() {
        let window = Historical15mCoverageWindow::new(
            Utc.with_ymd_and_hms(2025, 5, 2, 0, 0, 0)
                .single()
                .unwrap()
                .timestamp_millis(),
            Utc.with_ymd_and_hms(2025, 7, 20, 0, 0, 0)
                .single()
                .unwrap()
                .timestamp_millis(),
        )
        .unwrap();
        assert_eq!(
            archive_month_labels_for_window(window).unwrap(),
            ["2025-05", "2025-06", "2025-07"]
        );
    }

    #[test]
    fn archive_month_intersections_honor_okx_utc_plus_eight_boundaries() {
        let april_end = archive_month_bounds("2025-04").unwrap().1;
        let window =
            Historical15mCoverageWindow::new(april_end - CANDLE_15M_MS, april_end + CANDLE_15M_MS)
                .unwrap();

        assert_eq!(
            archive_month_labels_for_window(window).unwrap(),
            ["2025-04", "2025-05"]
        );
        assert_eq!(
            archive_required_window(window, "2025-04").unwrap(),
            Historical15mCoverageWindow::new(april_end - CANDLE_15M_MS, april_end).unwrap()
        );
        assert_eq!(
            archive_required_window(window, "2025-05").unwrap(),
            Historical15mCoverageWindow::new(april_end, april_end + CANDLE_15M_MS).unwrap()
        );
    }

    #[test]
    fn required_window_rejects_missing_unconfirmed_or_invalid_native_volume_rows() {
        let window = Historical15mCoverageWindow::new(0, 2 * CANDLE_15M_MS).unwrap();
        let candle = |ts: i64, confirm: &str, vol_ccy: &str| CandleOkxRespDto {
            ts: ts.to_string(),
            o: "1".to_owned(),
            h: "1".to_owned(),
            l: "1".to_owned(),
            c: "1".to_owned(),
            v: "1".to_owned(),
            vol_ccy: vol_ccy.to_owned(),
            vol_ccy_quote: "1".to_owned(),
            confirm: confirm.to_owned(),
        };
        let valid = vec![
            candle(-CANDLE_15M_MS, "0", "None"),
            candle(0, "1", "1"),
            candle(CANDLE_15M_MS, "1", "2"),
            candle(2 * CANDLE_15M_MS, "0", "None"),
        ];
        validate_complete_15m_window(&valid, "BTC-USDT-SWAP", window).unwrap();

        let mut missing = valid.clone();
        missing.remove(2);
        assert!(validate_complete_15m_window(&missing, "BTC-USDT-SWAP", window).is_err());
        let mut unconfirmed = valid.clone();
        unconfirmed[2].confirm = "0".to_owned();
        assert!(validate_complete_15m_window(&unconfirmed, "BTC-USDT-SWAP", window).is_err());
        let mut invalid_volume = valid;
        invalid_volume[2].vol_ccy = "None".to_owned();
        assert!(validate_complete_15m_window(&invalid_volume, "BTC-USDT-SWAP", window).is_err());
    }

    #[test]
    fn strict_mode_rejects_partial_required_month_only() {
        let mut request = ArchiveRequest {
            symbol: "BTC-USDT-SWAP".to_owned(),
            month: "2025-05".to_owned(),
            url: None,
            required: true,
            required_window: Historical15mCoverageWindow::new(0, CANDLE_15M_MS).unwrap(),
            contract_value: 0.01,
        };
        assert!(reject_strict_required_partial(true, &request, true).is_err());
        assert!(reject_strict_required_partial(false, &request, true).is_ok());
        request.required = false;
        assert!(reject_strict_required_partial(true, &request, true).is_ok());
    }

    #[test]
    fn coverage_audit_requires_exact_confirmed_900000ms_sequence() {
        let window = Historical15mCoverageWindow::new(0, 3 * CANDLE_15M_MS).unwrap();
        let valid = coverage_audit_from_stats(
            "BTC-USDT-SWAP",
            window,
            Some(0),
            Some(2 * CANDLE_15M_MS),
            3,
            0,
            0,
            0,
        )
        .unwrap();
        assert_eq!(valid.expected_candles, 3);
        assert!(coverage_audit_from_stats(
            "BTC-USDT-SWAP",
            window,
            Some(0),
            Some(2 * CANDLE_15M_MS),
            2,
            0,
            1,
            0,
        )
        .is_err());
        assert!(coverage_audit_from_stats(
            "BTC-USDT-SWAP",
            window,
            Some(0),
            Some(2 * CANDLE_15M_MS),
            3,
            1,
            0,
            0,
        )
        .is_err());
        assert!(coverage_audit_from_stats(
            "BTC-USDT-SWAP",
            window,
            Some(0),
            Some(2 * CANDLE_15M_MS),
            3,
            0,
            0,
            1,
        )
        .is_err());
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
