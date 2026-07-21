use super::env_parse::first_non_empty_env;
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use okx::dto::market_dto::CandleOkxRespDto;
use reqwest::{Client, Proxy, StatusCode, Url};
use rust_quant_market::models::{quote_legacy_table_name, CandlesModel};
use serde::Deserialize;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};
const DEFAULT_OKX_REST_BASE: &str = "https://www.okx.com";
const DEFAULT_TIMEFRAME: &str = "15m";
const DEFAULT_DAYS: u64 = 60;
const DEFAULT_PAGE_LIMIT: usize = 100;
const DEFAULT_BATCH_SIZE: usize = 500;
const DEFAULT_REQUEST_SLEEP_MS: u64 = 500;
const OKX_RATE_LIMIT_BACKOFF_MS: u64 = 2_000;
const OKX_RATE_LIMIT_MAX_RETRIES: usize = 3;
const OKX_MISSING_INSTRUMENT_CODE: &str = "51001";
const CANDLE_1M_MS: i64 = 60 * 1_000;
const CANDLE_5M_MS: i64 = 5 * 60 * 1_000;
const CANDLE_15M_MS: i64 = 15 * 60 * 1_000;
const CANDLE_1H_MS: i64 = 60 * 60 * 1_000;
const CANDLE_4H_MS: i64 = 4 * 60 * 60 * 1_000;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketVelocityBackfillConfig {
    /// databaseURL，用于配置运行参数。
    pub database_url: String,
    /// okxrest基础，用于配置运行参数。
    pub okx_rest_base: String,
    /// proxyURL；为空时使用默认值或表示不限制。
    pub proxy_url: Option<String>,
    /// 列表数据。
    pub symbols: Vec<String>,
    /// require4h，用于配置运行参数。
    pub require_4h: bool,
    /// 是否按已启用的生产策略配置加载本周期交易对，而不是读取短周期雷达候选。
    pub enabled_strategy_symbols: bool,
    /// 天数。
    pub days: u64,
    /// 周期。
    pub timeframe: String,
    /// pagelimit，用于配置运行参数。
    pub page_limit: usize,
    /// 数量数值。
    pub batch_size: usize,
    /// Dry-runrun，用于配置运行参数。
    pub dry_run: bool,
    /// 最大交易对数量；为空时不限制数量。
    pub max_symbols: Option<usize>,
    /// 毫秒级时间戳或时长。
    pub request_sleep_ms: u64,
    /// 错误信息。
    pub continue_on_error: bool,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarketVelocityBackfillCliArgs {
    /// 列表数据。
    pub symbols: Option<Vec<String>>,
    /// 天数。
    pub days: Option<u64>,
    /// 时间周期；为空时使用默认周期。
    pub timeframe: Option<String>,
    /// 调度器需要顺序补齐的多个周期；为空时保持单周期兼容。
    pub timeframes: Option<Vec<String>>,
    /// proxyURL；为空时使用默认值或表示不限制。
    pub proxy_url: Option<Option<String>>,
    /// 是否要求 4 小时级别数据；为空时使用默认策略。
    pub require_4h: Option<bool>,
    /// 是否从已启用策略配置加载本周期交易对；为空时保持雷达候选逻辑。
    pub enabled_strategy_symbols: Option<bool>,
    /// 页码限制；为空时使用默认值或表示不限制。
    pub page_limit: Option<usize>,
    /// 数量数值。
    pub batch_size: Option<usize>,
    /// 是否仅做 dry-run；为空时使用默认运行模式。
    pub dry_run: Option<bool>,
    /// 最大交易对数量；为空时不限制数量。
    pub max_symbols: Option<Option<usize>>,
    /// 毫秒级时间戳或时长。
    pub request_sleep_ms: Option<u64>,
    /// 错误信息。
    pub continue_on_error: Option<bool>,
    /// 秒级时长。
    pub loop_interval_seconds: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketVelocityBackfillReport {
    /// 交易对总数。
    pub symbols_total: usize,
    /// symbolsattempted，用于展示或持久化查询结果。
    pub symbols_attempted: usize,
    /// K 线fetched，用于展示或持久化查询结果。
    pub candles_fetched: usize,
    /// 数据行upserted，用于展示或持久化查询结果。
    pub rows_upserted: u64,
    /// 本轮通过 bounds/count 判定缺失的 K 线数量。
    pub missing_candles_detected: i64,
    /// 本轮触发 gap 修复的交易对数量。
    pub gap_repair_symbols: usize,
    /// Dry-runrun，用于展示或持久化查询结果。
    pub dry_run: bool,
    /// 列表数据。
    pub failed_symbols: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolBackfillReport {
    /// 交易对或资产符号。
    pub symbol: String,
    /// fetched，用于展示或持久化查询结果。
    pub fetched: usize,
    /// upserted，用于展示或持久化查询结果。
    pub upserted: u64,
    /// 本地连续性检查发现的缺失 K 线数量。
    pub missing_candles_detected: i64,
    /// 本次 OKX 拉取窗口的起点，Unix 毫秒时间戳。
    pub fetch_start_ms: i64,
    /// 说明本次拉取是完整补数、增量尾部更新还是 gap 修复。
    pub fetch_reason: BackfillWindowReason,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackfillWindowReason {
    /// 本地表不存在或窗口内没有 K 线，只能按配置窗口完整补数。
    EmptyOrMissingTable,
    /// 本地窗口内 bounds/count 不匹配，需要从最早断点附近重新拉取。
    GapRepair,
    /// 本地窗口连续，只需要从最新 K 线附近做尾部增量刷新。
    IncrementalTail,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandleContinuityStatus {
    /// 目标窗口内最早一根本地 K 线时间，Unix 毫秒时间戳。
    pub earliest_ts: Option<i64>,
    /// 目标窗口内最新一根本地 K 线时间，Unix 毫秒时间戳。
    pub latest_ts: Option<i64>,
    /// 目标窗口内实际存在的 K 线数量。
    pub actual_count: i64,
    /// 根据 earliest/latest 与周期长度计算出的理论 K 线数量。
    pub expected_count: i64,
    /// 确认缺失后用于缩小修复范围的最早断点后一根 K 线时间。
    pub earliest_gap_start_ts: Option<i64>,
}
impl CandleContinuityStatus {
    /// 使用 bounds/count 作为连续性事实源；断点位置只用于决定最小修复窗口。
    pub fn has_missing_candles(&self) -> bool {
        self.expected_count > self.actual_count
    }

    fn missing_candle_count(&self) -> i64 {
        self.expected_count.saturating_sub(self.actual_count)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncrementalBackfillWindow {
    /// 本轮实际请求 OKX 的起始时间，Unix 毫秒时间戳。
    pub fetch_start_ms: i64,
    /// 本轮窗口选择原因，用于日志和运行态诊断。
    pub reason: BackfillWindowReason,
}
#[derive(Debug, Deserialize)]
struct OkxHistoryCandlesResponse {
    /// 代码。
    code: String,
    #[serde(default)]
    /// msg，用于返回接口响应。
    msg: String,
    #[serde(default)]
    /// 列表数据。
    data: Vec<Vec<String>>,
}
/// 封装当前函数，减少行情数据调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
pub fn parse_cli_args_from<I, S>(args: I) -> Result<MarketVelocityBackfillCliArgs>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut parsed = MarketVelocityBackfillCliArgs::default();
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--symbols" => parsed.symbols = Some(parse_symbol_list(&next_arg(&mut args, &arg)?)),
            "--days" => parsed.days = Some(parse_next(&mut args, &arg)?),
            "--timeframe" => parsed.timeframe = Some(next_arg(&mut args, &arg)?),
            "--timeframes" => {
                parsed.timeframes = Some(parse_timeframe_list(&next_arg(&mut args, &arg)?))
            }
            "--proxy-url" => parsed.proxy_url = Some(Some(next_arg(&mut args, &arg)?)),
            "--no-proxy" => parsed.proxy_url = Some(None),
            "--require-4h" => parsed.require_4h = Some(true),
            "--all-radar-symbols" => parsed.require_4h = Some(false),
            "--enabled-strategy-symbols" => parsed.enabled_strategy_symbols = Some(true),
            "--limit" => parsed.page_limit = Some(parse_next(&mut args, &arg)?),
            "--batch-size" => parsed.batch_size = Some(parse_next(&mut args, &arg)?),
            "--dry-run" => parsed.dry_run = Some(true),
            "--write" => parsed.dry_run = Some(false),
            "--max-symbols" => parsed.max_symbols = Some(Some(parse_next(&mut args, &arg)?)),
            "--no-max-symbols" => parsed.max_symbols = Some(None),
            "--request-sleep-ms" => parsed.request_sleep_ms = Some(parse_next(&mut args, &arg)?),
            "--continue-on-error" => parsed.continue_on_error = Some(true),
            "--fail-fast" => parsed.continue_on_error = Some(false),
            "--loop-interval-seconds" => {
                parsed.loop_interval_seconds = Some(parse_positive_u64(&mut args, &arg)?);
            }
            other if other.starts_with("--loop-interval-seconds=") => {
                parsed.loop_interval_seconds = Some(parse_positive_u64_value(
                    other
                        .split_once('=')
                        .map(|(_, value)| value)
                        .unwrap_or_default(),
                    "--loop-interval-seconds",
                )?);
            }
            "--help" | "-h" => {
                print_market_velocity_backfill_usage();
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    Ok(parsed)
}
/// 执行输出市场动量backfillusage步骤，串起行情数据需要的状态推进和错误处理。
pub fn print_market_velocity_backfill_usage() {
    println!(
        "Usage: market_velocity_candle_backfill [--symbols BTC-USDT-SWAP,ETH-USDT-SWAP] [--days 60] [--timeframe 1m|5m|15m|1h|4h] [--timeframes 1m,5m,15m] [--enabled-strategy-symbols] [--proxy-url http://127.0.0.1:7897] [--dry-run|--write] [--all-radar-symbols] [--loop-interval-seconds 300]"
    );
}
/// 把单进程调度器参数展开成多个周期配置，避免为每个小周期新增一个容器。
pub fn configs_from_env_and_args(
    cli_args: MarketVelocityBackfillCliArgs,
) -> Result<Vec<MarketVelocityBackfillConfig>> {
    let timeframes = resolve_backfill_timeframes(&cli_args)?;
    let mut configs = Vec::with_capacity(timeframes.len());
    for timeframe in timeframes {
        let mut scoped_args = cli_args.clone();
        scoped_args.timeframe = Some(timeframe);
        scoped_args.timeframes = None;
        configs.push(config_from_env_and_args(scoped_args)?);
    }
    Ok(configs)
}

/// 提供配置from环境变量andargs的集中实现，避免行情数据调用方重复处理相同细节。
pub fn config_from_env_and_args(
    cli_args: MarketVelocityBackfillCliArgs,
) -> Result<MarketVelocityBackfillConfig> {
    let database_url = first_non_empty_env(&[
        "QUANT_CORE_DATABASE_URL",
        "POSTGRES_QUANT_CORE_DATABASE_URL",
    ])
    .context("market velocity candle backfill requires QUANT_CORE_DATABASE_URL")?;
    let okx_rest_base = env_or_default(
        "MARKET_VELOCITY_BACKFILL_OKX_REST_BASE",
        DEFAULT_OKX_REST_BASE,
    );
    let proxy_url = match cli_args.proxy_url {
        Some(value) => value,
        None => first_non_empty_env(&[
            "MARKET_VELOCITY_BACKFILL_PROXY_URL",
            "HTTPS_PROXY",
            "HTTP_PROXY",
        ])
        .filter(|value| value.starts_with("http://") || value.starts_with("https://")),
    };
    let symbols = cli_args.symbols.unwrap_or_else(|| {
        parse_symbol_list(&env_or_default("MARKET_VELOCITY_BACKFILL_SYMBOLS", ""))
    });
    let require_4h = cli_args.require_4h.unwrap_or_else(|| {
        !parse_env_bool("MARKET_VELOCITY_BACKFILL_ALL_RADAR_SYMBOLS", false)
            && parse_env_bool("MARKET_VELOCITY_BACKFILL_REQUIRE_4H", true)
    });
    let enabled_strategy_symbols = cli_args.enabled_strategy_symbols.unwrap_or(false);
    if enabled_strategy_symbols && !symbols.is_empty() {
        bail!("use only one of --symbols or --enabled-strategy-symbols");
    }
    let days = cli_args
        .days
        .unwrap_or_else(|| parse_env_u64("MARKET_VELOCITY_BACKFILL_DAYS", DEFAULT_DAYS))
        .max(1);
    let timeframe = cli_args
        .timeframe
        .unwrap_or_else(|| env_or_default("MARKET_VELOCITY_BACKFILL_TIMEFRAME", DEFAULT_TIMEFRAME))
        .trim()
        .to_ascii_lowercase();
    let page_limit = cli_args
        .page_limit
        .unwrap_or_else(|| parse_env_usize("MARKET_VELOCITY_BACKFILL_LIMIT", DEFAULT_PAGE_LIMIT))
        .clamp(1, DEFAULT_PAGE_LIMIT);
    let batch_size = cli_args
        .batch_size
        .unwrap_or_else(|| {
            parse_env_usize("MARKET_VELOCITY_BACKFILL_BATCH_SIZE", DEFAULT_BATCH_SIZE)
        })
        .max(1);
    let dry_run = cli_args
        .dry_run
        .unwrap_or_else(|| parse_env_bool("MARKET_VELOCITY_BACKFILL_DRY_RUN", false));
    let max_symbols = match cli_args.max_symbols {
        Some(value) => value,
        None => std::env::var("MARKET_VELOCITY_BACKFILL_MAX_SYMBOLS")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok()),
    };
    let request_sleep_ms = cli_args.request_sleep_ms.unwrap_or_else(|| {
        parse_env_u64(
            "MARKET_VELOCITY_BACKFILL_REQUEST_SLEEP_MS",
            DEFAULT_REQUEST_SLEEP_MS,
        )
    });
    let continue_on_error = cli_args
        .continue_on_error
        .unwrap_or_else(|| parse_env_bool("MARKET_VELOCITY_BACKFILL_CONTINUE_ON_ERROR", true));
    candle_interval_ms(&timeframe)?;
    Ok(MarketVelocityBackfillConfig {
        database_url,
        okx_rest_base,
        proxy_url,
        symbols,
        require_4h,
        enabled_strategy_symbols,
        days,
        timeframe,
        page_limit,
        batch_size,
        dry_run,
        max_symbols,
        request_sleep_ms,
        continue_on_error,
    })
}
/// 执行 行情与市场数据 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn run_market_velocity_backfill(
    config: MarketVelocityBackfillConfig,
) -> Result<MarketVelocityBackfillReport> {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for market velocity candle backfill")?;
    let mut symbols = if !config.symbols.is_empty() {
        config.symbols.clone()
    } else if config.enabled_strategy_symbols {
        load_enabled_strategy_backfill_symbols(&pool, &config.timeframe).await?
    } else {
        load_market_velocity_backfill_symbols(&pool, config.require_4h).await?
    };
    symbols.sort();
    symbols.dedup();
    if let Some(max_symbols) = config.max_symbols {
        symbols.truncate(max_symbols);
    }
    let client = build_okx_http_client(config.proxy_url.as_deref())?;
    let end_ms = Utc::now().timestamp_millis();
    let start_ms = end_ms - (config.days as i64 * 24 * 60 * 60 * 1_000);
    let total = symbols.len();
    let mut report = MarketVelocityBackfillReport {
        symbols_total: total,
        symbols_attempted: 0,
        candles_fetched: 0,
        rows_upserted: 0,
        missing_candles_detected: 0,
        gap_repair_symbols: 0,
        dry_run: config.dry_run,
        failed_symbols: Vec::new(),
    };
    info!(
        "market velocity candle backfill started: symbols={}, days={}, timeframe={}, enabled_strategy_symbols={}, dry_run={}, proxy={}",
        total,
        config.days,
        config.timeframe,
        config.enabled_strategy_symbols,
        config.dry_run,
        config.proxy_url.as_deref().unwrap_or("disabled")
    );
    for (index, symbol) in symbols.iter().enumerate() {
        info!(
            "market velocity candle backfill symbol start: {}/{} {}",
            index + 1,
            total,
            symbol
        );
        match backfill_symbol_candles(&pool, &client, &config, symbol, start_ms, end_ms).await {
            Ok(symbol_report) => {
                report.symbols_attempted += 1;
                report.candles_fetched += symbol_report.fetched;
                report.rows_upserted += symbol_report.upserted;
                report.missing_candles_detected += symbol_report.missing_candles_detected;
                if symbol_report.fetch_reason == BackfillWindowReason::GapRepair {
                    report.gap_repair_symbols += 1;
                }
                info!(
                    "market velocity candle backfill symbol done: symbol={}, fetched={}, upserted={}, missing_detected={}, fetch_start_ms={}, fetch_reason={:?}",
                    symbol_report.symbol,
                    symbol_report.fetched,
                    symbol_report.upserted,
                    symbol_report.missing_candles_detected,
                    symbol_report.fetch_start_ms,
                    symbol_report.fetch_reason
                );
            }
            Err(error) if config.continue_on_error => {
                warn!(
                    "market velocity candle backfill symbol failed: symbol={}, error={:#}",
                    symbol, error
                );
                report.failed_symbols.push(symbol.clone());
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "backfill {} candles failed: symbol={symbol}",
                        config.timeframe
                    )
                });
            }
        }
        if index + 1 < total && config.request_sleep_ms > 0 {
            sleep(Duration::from_millis(config.request_sleep_ms)).await;
        }
    }
    Ok(report)
}
/// 同步 行情与市场数据 数据，保证本地状态与外部事实源保持一致。
async fn backfill_symbol_candles(
    pool: &PgPool,
    client: &Client,
    config: &MarketVelocityBackfillConfig,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<SymbolBackfillReport> {
    let candle_ms = candle_interval_ms(&config.timeframe)?;
    let listed_at_ms = load_okx_symbol_list_time_ms(pool, symbol).await?;
    let start_ms = aligned_symbol_start_ms(start_ms, listed_at_ms, candle_ms);
    let continuity =
        load_candle_continuity_status(pool, symbol, &config.timeframe, start_ms, end_ms, candle_ms)
            .await?;
    let backfill_window =
        resolve_incremental_backfill_window(start_ms, end_ms, candle_ms, continuity.clone());
    info!(
        "market velocity candle backfill window resolved: symbol={}, timeframe={}, reason={:?}, fetch_start_ms={}, latest_ts={:?}, actual_count={}, expected_count={}, earliest_gap_start_ts={:?}",
        symbol,
        config.timeframe,
        backfill_window.reason,
        backfill_window.fetch_start_ms,
        continuity.latest_ts,
        continuity.actual_count,
        continuity.expected_count,
        continuity.earliest_gap_start_ts
    );
    let candles = match fetch_okx_history_candles(
        client,
        &config.okx_rest_base,
        symbol,
        &config.timeframe,
        backfill_window.fetch_start_ms,
        end_ms,
        config.page_limit,
        config.request_sleep_ms,
    )
    .await
    {
        Ok(candles) => candles,
        Err(error) => {
            if should_mark_okx_exchange_symbol_deleted(config.dry_run, &error) {
                let rows = mark_okx_exchange_symbol_deleted(pool, symbol)
                    .await
                    .with_context(|| format!("mark OKX exchange symbol deleted: {symbol}"))?;
                warn!(
                    symbol,
                    rows_affected = rows,
                    "marked OKX exchange symbol deleted after missing instrument response"
                );
            } else if config.dry_run && is_okx_missing_instrument_error(&error) {
                warn!(
                    symbol,
                    "dry-run observed missing OKX instrument; exchange metadata was not changed"
                );
            }
            return Err(error);
        }
    };
    let fetched = candles.len();
    let mut upserted = 0;
    if !config.dry_run {
        let model = CandlesModel::new();
        model.create_table(symbol, &config.timeframe).await?;
        for chunk in candles.chunks(config.batch_size) {
            upserted += model
                .upsert_batch(chunk.to_vec(), symbol, &config.timeframe)
                .await?;
        }
    }
    Ok(SymbolBackfillReport {
        symbol: symbol.to_string(),
        fetched,
        upserted,
        missing_candles_detected: continuity.missing_candle_count(),
        fetch_start_ms: backfill_window.fetch_start_ms,
        fetch_reason: backfill_window.reason,
    })
}

/// 上市前不会存在 K 线；用交易所上市时间收窄补数窗口，避免把正常的前置空白反复当成缺口。
async fn load_okx_symbol_list_time_ms(pool: &PgPool, symbol: &str) -> Result<Option<i64>> {
    sqlx::query_scalar(
        r#"
        SELECT NULLIF(raw_payload ->> 'listTime', '')::BIGINT
        FROM exchange_symbols
        WHERE exchange = 'okx'
          AND market_type = 'perpetual'
          AND exchange_symbol = $1
        "#,
    )
    .bind(symbol)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load OKX symbol list time: {symbol}"))
    .map(Option::flatten)
}

fn aligned_symbol_start_ms(
    configured_start_ms: i64,
    listed_at_ms: Option<i64>,
    candle_ms: i64,
) -> i64 {
    let available_start_ms = listed_at_ms
        .unwrap_or(configured_start_ms)
        .max(configured_start_ms);
    align_up_to_candle_boundary(available_start_ms, candle_ms)
}

fn align_up_to_candle_boundary(timestamp_ms: i64, candle_ms: i64) -> i64 {
    let remainder = timestamp_ms.rem_euclid(candle_ms);
    if remainder == 0 {
        timestamp_ms
    } else {
        timestamp_ms.saturating_add(candle_ms - remainder)
    }
}

/// 读取本地 K 线窗口的连续性摘要；已结束但仍未确认的 K 线也视为修复点，避免回测静默跳过历史数据。
async fn load_candle_continuity_status(
    pool: &PgPool,
    symbol: &str,
    timeframe: &str,
    start_ms: i64,
    end_ms: i64,
    candle_ms: i64,
) -> Result<CandleContinuityStatus> {
    let table_name = CandlesModel::get_table_name(symbol, timeframe);
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = 'public'
              AND table_name = $1
        )",
    )
    .bind(&table_name)
    .fetch_one(pool)
    .await
    .with_context(|| format!("check candle table exists: {table_name}"))?;
    if !table_exists {
        return Ok(CandleContinuityStatus::default());
    }

    let quoted_table_name = quote_legacy_table_name(&table_name)?;
    let query = format!(
        r#"
        WITH windowed AS (
          SELECT ts, confirm
          FROM {quoted_table_name}
          WHERE ts >= $1
            AND ts <= $2
        ),
        bounds AS (
          SELECT
            MIN(ts) AS earliest_ts,
            MAX(ts) AS latest_ts,
            COUNT(*) FILTER (
              WHERE confirm = '1'
                 OR ts > $2 - $3
            )::BIGINT AS actual_count
          FROM windowed
        ),
        ordered AS (
          SELECT
            ts,
            LAG(ts) OVER (ORDER BY ts) AS prev_ts
          FROM windowed
        ),
        repair_points AS (
          SELECT ts
          FROM ordered
          WHERE prev_ts IS NOT NULL
            AND ts - prev_ts > $3
          UNION ALL
          SELECT ts
          FROM windowed
          WHERE confirm <> '1'
            AND ts <= $2 - $3
        ),
        gaps AS (
          SELECT MIN(ts) AS earliest_gap_start_ts
          FROM repair_points
        )
        SELECT
          bounds.earliest_ts,
          bounds.latest_ts,
          bounds.actual_count,
          gaps.earliest_gap_start_ts
        FROM bounds
        CROSS JOIN gaps
        "#
    );
    let row = sqlx::query(&query)
        .bind(start_ms)
        .bind(end_ms)
        .bind(candle_ms)
        .fetch_one(pool)
        .await
        .with_context(|| format!("load candle continuity status: {table_name}"))?;
    let earliest_ts = row.get::<Option<i64>, _>("earliest_ts");
    let latest_ts = row.get::<Option<i64>, _>("latest_ts");
    let actual_count = row.get::<i64, _>("actual_count");
    let expected_count = expected_candle_count(Some(start_ms), latest_ts, candle_ms);
    let earliest_gap_start_ts = row.get::<Option<i64>, _>("earliest_gap_start_ts");
    Ok(CandleContinuityStatus {
        earliest_ts,
        latest_ts,
        actual_count,
        expected_count,
        earliest_gap_start_ts,
    })
}

fn resolve_incremental_backfill_window(
    configured_start_ms: i64,
    _end_ms: i64,
    candle_ms: i64,
    continuity: CandleContinuityStatus,
) -> IncrementalBackfillWindow {
    if continuity.latest_ts.is_none() {
        return IncrementalBackfillWindow {
            fetch_start_ms: configured_start_ms,
            reason: BackfillWindowReason::EmptyOrMissingTable,
        };
    }
    if continuity
        .earliest_ts
        .is_some_and(|earliest_ts| earliest_ts > configured_start_ms)
    {
        return IncrementalBackfillWindow {
            fetch_start_ms: configured_start_ms,
            reason: BackfillWindowReason::GapRepair,
        };
    }
    if continuity.has_missing_candles() {
        let repair_anchor = continuity
            .earliest_gap_start_ts
            .or(continuity.earliest_ts)
            .unwrap_or(configured_start_ms);
        return IncrementalBackfillWindow {
            fetch_start_ms: overlap_start_ms(repair_anchor, configured_start_ms, candle_ms),
            reason: BackfillWindowReason::GapRepair,
        };
    }
    IncrementalBackfillWindow {
        fetch_start_ms: overlap_start_ms(
            continuity.latest_ts.unwrap_or(configured_start_ms),
            configured_start_ms,
            candle_ms,
        ),
        reason: BackfillWindowReason::IncrementalTail,
    }
}

fn expected_candle_count(earliest_ts: Option<i64>, latest_ts: Option<i64>, candle_ms: i64) -> i64 {
    match (earliest_ts, latest_ts) {
        (Some(earliest_ts), Some(latest_ts)) if latest_ts >= earliest_ts && candle_ms > 0 => {
            ((latest_ts - earliest_ts) / candle_ms) + 1
        }
        _ => 0,
    }
}

fn overlap_start_ms(anchor_ms: i64, configured_start_ms: i64, candle_ms: i64) -> i64 {
    anchor_ms.saturating_sub(candle_ms).max(configured_start_ms)
}
/// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
pub async fn fetch_okx_history_candles(
    client: &Client,
    okx_rest_base: &str,
    symbol: &str,
    timeframe: &str,
    start_ms: i64,
    end_ms: i64,
    limit: usize,
    request_sleep_ms: u64,
) -> Result<Vec<CandleOkxRespDto>> {
    let mut candles_by_ts = BTreeMap::new();
    let mut after_ms = None;
    let candle_ms = candle_interval_ms(timeframe)?;
    let okx_bar = okx_bar_for_timeframe(timeframe)?;
    let max_pages = max_history_pages(start_ms, end_ms, candle_ms, limit);
    for page_index in 0..max_pages {
        let url = build_okx_history_candles_url(okx_rest_base, symbol, okx_bar, after_ms, limit)?;
        let payload = request_okx_history_candles_page(client, url, symbol).await?;
        if payload.code != "0" {
            bail!(okx_history_candles_api_error(
                &payload.code,
                &payload.msg,
                symbol
            ));
        }
        if payload.data.is_empty() {
            break;
        }
        let mut page_oldest = i64::MAX;
        for row in payload.data {
            let candle = parse_okx_candle_row(row)?;
            let ts = candle
                .ts
                .parse::<i64>()
                .context("parsed OKX candle timestamp should be numeric")?;
            page_oldest = page_oldest.min(ts);
            if start_ms <= ts && ts <= end_ms {
                candles_by_ts.insert(ts, candle);
            }
        }
        if page_oldest <= start_ms {
            break;
        }
        if after_ms.is_some_and(|previous_after| page_oldest >= previous_after) {
            warn!(
                "OKX history-candles pagination did not move older: symbol={}, previous_after={:?}, page_oldest={}",
                symbol, after_ms, page_oldest
            );
            break;
        }
        after_ms = Some(page_oldest);
        if page_index + 1 < max_pages && request_sleep_ms > 0 {
            sleep(Duration::from_millis(request_sleep_ms)).await;
        }
    }
    Ok(candles_by_ts.into_values().collect())
}

/// 请求单页 OKX 历史 K 线，并对限频、服务端错误和瞬时传输失败做同页退避重试。
async fn request_okx_history_candles_page(
    client: &Client,
    url: Url,
    symbol: &str,
) -> Result<OkxHistoryCandlesResponse> {
    for attempt in 0..=OKX_RATE_LIMIT_MAX_RETRIES {
        let response = match client
            .get(url.clone())
            .header("User-Agent", "rust-quant-market-velocity-backfill/1.0")
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) if attempt < OKX_RATE_LIMIT_MAX_RETRIES => {
                let backoff_ms = OKX_RATE_LIMIT_BACKOFF_MS * (attempt as u64 + 1);
                warn!(
                    "OKX history-candles transport failed; retrying page: symbol={}, attempt={}, backoff_ms={}, error={}",
                    symbol,
                    attempt + 1,
                    backoff_ms,
                    error
                );
                sleep(Duration::from_millis(backoff_ms)).await;
                continue;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("request OKX history-candles failed: symbol={symbol}")
                });
            }
        };

        if (response.status() == StatusCode::TOO_MANY_REQUESTS
            || response.status().is_server_error())
            && attempt < OKX_RATE_LIMIT_MAX_RETRIES
        {
            let backoff_ms = OKX_RATE_LIMIT_BACKOFF_MS * (attempt as u64 + 1);
            warn!(
                "OKX history-candles HTTP retry: symbol={}, status={}, attempt={}, backoff_ms={}",
                symbol,
                response.status(),
                attempt + 1,
                backoff_ms
            );
            sleep(Duration::from_millis(backoff_ms)).await;
            continue;
        }

        let response = response
            .error_for_status()
            .with_context(|| format!("OKX history-candles HTTP status failed: symbol={symbol}"))?;
        match response.json::<OkxHistoryCandlesResponse>().await {
            Ok(payload) => return Ok(payload),
            Err(error) if attempt < OKX_RATE_LIMIT_MAX_RETRIES => {
                let backoff_ms = OKX_RATE_LIMIT_BACKOFF_MS * (attempt as u64 + 1);
                warn!(
                    "OKX history-candles response interrupted; retrying page: symbol={}, attempt={}, backoff_ms={}, error={}",
                    symbol,
                    attempt + 1,
                    backoff_ms,
                    error
                );
                sleep(Duration::from_millis(backoff_ms)).await;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("decode OKX history-candles response failed: symbol={symbol}")
                });
            }
        }
    }

    unreachable!("OKX history-candles retry loop always returns on the final attempt")
}
/// 组装 OKX K 线接口错误文本，并保留 code/msg/symbol 供上层识别可落库的永久不可用交易对。
fn okx_history_candles_api_error(code: &str, msg: &str, symbol: &str) -> String {
    format!("OKX history-candles returned code={code} msg={msg} symbol={symbol}")
}

/// 判断 OKX 是否明确返回交易对不存在；这是永久阻塞，应写回 DB 状态避免反复重试。
pub fn is_okx_missing_instrument_error(error: &anyhow::Error) -> bool {
    let error_text = format!("{error:#}");
    error_text.contains(&format!("code={OKX_MISSING_INSTRUMENT_CODE}"))
        && error_text.to_ascii_lowercase().contains("instrument")
}

/// 只有显式写入模式才能把 OKX 不存在的合约同步为已删除，保证 dry-run 不修改元数据。
fn should_mark_okx_exchange_symbol_deleted(dry_run: bool, error: &anyhow::Error) -> bool {
    !dry_run && is_okx_missing_instrument_error(error)
}

/// 标记 OKX 永续交易对为删除状态，后续候选查询只读取 trading/live 可用状态。
pub async fn mark_okx_exchange_symbol_deleted(pool: &PgPool, symbol: &str) -> Result<u64> {
    let result = sqlx::query(mark_okx_exchange_symbol_deleted_sql())
        .bind(symbol.trim().to_ascii_uppercase())
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// 集中维护 OKX 交易对删除标记 SQL，便于 backfill 与 live handoff 共用同一 contract。
fn mark_okx_exchange_symbol_deleted_sql() -> &'static str {
    r#"
        UPDATE exchange_symbols
        SET status = 'deleted',
            updated_at = NOW()
        WHERE exchange = 'okx'
          AND market_type = 'perpetual'
          AND (
            upper(exchange_symbol) = upper($1)
            OR upper(normalized_symbol) = upper($1)
          )
        "#
}

/// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
pub async fn load_market_velocity_backfill_symbols(
    pool: &PgPool,
    require_4h: bool,
) -> Result<Vec<String>> {
    let query = load_market_velocity_backfill_symbols_sql(require_4h);
    let rows = sqlx::query(&query).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("symbol"))
        .collect())
}

/// 加载当前已启用策略在指定周期使用的 OKX 永续交易对，避免低频补数依赖短周期雷达事件。
async fn load_enabled_strategy_backfill_symbols(
    pool: &PgPool,
    timeframe: &str,
) -> Result<Vec<String>> {
    let rows = sqlx::query(load_enabled_strategy_backfill_symbols_sql())
        .bind(timeframe)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load enabled strategy symbols for timeframe {timeframe}"))?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("symbol"))
        .collect())
}

/// 只选择已启用策略且交易所仍可交易的标的，防止退市配置继续触发公共行情请求。
fn load_enabled_strategy_backfill_symbols_sql() -> &'static str {
    r#"
        SELECT DISTINCT upper(config.symbol) AS symbol
        FROM strategy_configs config
        JOIN exchange_symbols exchange_symbol
          ON lower(exchange_symbol.exchange) = lower(config.exchange)
         AND upper(exchange_symbol.normalized_symbol) = upper(config.symbol)
        WHERE config.enabled = TRUE
          AND lower(config.exchange) = 'okx'
          AND lower(config.timeframe) = lower($1)
          AND NULLIF(trim(config.symbol), '') IS NOT NULL
          AND exchange_symbol.market_type = 'perpetual'
          AND lower(exchange_symbol.status) IN ('trading', 'live')
        ORDER BY symbol
        "#
}

/// 生成 Market Velocity 补 K 线候选查询；只允许 OKX 当前可交易的永续合约进入补数链路。
fn load_market_velocity_backfill_symbols_sql(require_4h: bool) -> String {
    let join_4h = if require_4h {
        r#"
        JOIN (
          SELECT upper(replace(replace(table_name, '_candles_4h', ''), '_', '-')) AS symbol
          FROM information_schema.tables
          WHERE table_schema = 'public'
            AND table_name LIKE '%\_candles\_4h' ESCAPE '\'
        ) four_h USING (symbol)
        "#
    } else {
        ""
    };
    format!(
        r#"
        WITH candidates AS (
          SELECT DISTINCT upper(symbol) AS symbol
          FROM market_rank_events
          WHERE event_type IN ('rank_velocity', 'top_entry')
            AND delta_rank >= 3
            AND new_rank BETWEEN 1 AND 50
            AND lower(price_direction) = 'up'
            AND current_price IS NOT NULL
            AND NOT (new_rank <= 10 AND COALESCE(price_change_pct, 0) >= 8.0)
        ),
        available_okx_symbols AS (
          SELECT DISTINCT upper(normalized_symbol) AS symbol
          FROM exchange_symbols
          WHERE exchange = 'okx'
            AND market_type = 'perpetual'
            AND lower(status) IN ('trading', 'live')
        )
        SELECT candidates.symbol
        FROM candidates
        JOIN available_okx_symbols USING (symbol)
        {join_4h}
        ORDER BY candidates.symbol
        "#
    )
}
/// 构建 行情与市场数据 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_okx_history_candles_url(
    okx_rest_base: &str,
    symbol: &str,
    timeframe: &str,
    after_ms: Option<i64>,
    limit: usize,
) -> Result<Url> {
    let base = okx_rest_base.trim_end_matches('/');
    let mut url = Url::parse(&format!("{base}/api/v5/market/history-candles"))
        .context("parse OKX REST base URL")?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("instId", symbol);
        pairs.append_pair("bar", timeframe);
        pairs.append_pair("limit", &limit.to_string());
        if let Some(after_ms) = after_ms {
            pairs.append_pair("after", &after_ms.to_string());
        }
    }
    Ok(url)
}
/// 构建 行情与市场数据 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_okx_http_client(proxy_url: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30));
    if let Some(proxy_url) = proxy_url.map(str::trim).filter(|value| !value.is_empty()) {
        builder = builder.proxy(Proxy::all(proxy_url).context("configure OKX REST proxy")?);
    }
    builder.build().context("build OKX REST HTTP client")
}
/// 判断K 线intervalms，给行情数据流程提供布尔结果。
pub fn candle_interval_ms(timeframe: &str) -> Result<i64> {
    match timeframe.trim().to_ascii_lowercase().as_str() {
        "1m" => Ok(CANDLE_1M_MS),
        "5m" => Ok(CANDLE_5M_MS),
        "15m" => Ok(CANDLE_15M_MS),
        "1h" => Ok(CANDLE_1H_MS),
        "4h" => Ok(CANDLE_4H_MS),
        other => bail!(
            "unsupported market velocity candle backfill timeframe: {other}; supported: 1m, 5m, 15m, 1h, 4h"
        ),
    }
}
/// 提供OKXbarfortimeframe的集中实现，避免行情数据调用方重复处理相同细节。
pub fn okx_bar_for_timeframe(timeframe: &str) -> Result<&'static str> {
    match timeframe.trim().to_ascii_lowercase().as_str() {
        "1m" => Ok("1m"),
        "5m" => Ok("5m"),
        "15m" => Ok("15m"),
        "1h" => Ok("1H"),
        "4h" => Ok("4H"),
        other => {
            bail!(
                "unsupported market velocity OKX candle bar: {other}; supported: 1m, 5m, 15m, 1h, 4h"
            )
        }
    }
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
pub fn parse_okx_candle_row(row: Vec<String>) -> Result<CandleOkxRespDto> {
    if row.len() < 9 {
        bail!(
            "OKX candle row has {} columns, expected at least 9",
            row.len()
        );
    }
    row[0]
        .parse::<i64>()
        .with_context(|| format!("invalid OKX candle timestamp: {}", row[0]))?;
    Ok(CandleOkxRespDto {
        ts: row[0].clone(),
        o: row[1].clone(),
        h: row[2].clone(),
        l: row[3].clone(),
        c: row[4].clone(),
        v: row[5].clone(),
        vol_ccy: row[6].clone(),
        vol_ccy_quote: row[7].clone(),
        confirm: row[8].clone(),
    })
}
/// 计算最大historypages，并把公式边界留在行情数据内部。
pub fn max_history_pages(start_ms: i64, end_ms: i64, candle_ms: i64, limit: usize) -> usize {
    if end_ms <= start_ms || candle_ms <= 0 || limit == 0 {
        return 1;
    }
    let expected_candles = ((end_ms - start_ms) as f64 / candle_ms as f64).ceil() as usize;
    (expected_candles / limit).saturating_add(8).max(1)
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
pub fn parse_symbol_list(value: &str) -> Vec<String> {
    let mut symbols = value
        .split(',')
        .map(str::trim)
        .filter(|symbol| !symbol.is_empty())
        .map(|symbol| symbol.to_ascii_uppercase())
        .collect::<Vec<_>>();
    symbols.sort();
    symbols.dedup();
    symbols
}
/// 解析多个 K 线周期，并保持配置顺序，便于调度器按低周期优先补齐。
pub fn parse_timeframe_list(value: &str) -> Vec<String> {
    let mut timeframes = Vec::new();
    for timeframe in value
        .split(',')
        .map(str::trim)
        .filter(|timeframe| !timeframe.is_empty())
        .map(|timeframe| timeframe.to_ascii_lowercase())
    {
        if !timeframes.contains(&timeframe) {
            timeframes.push(timeframe);
        }
    }
    timeframes
}

/// 根据 CLI 与环境变量解析最终周期列表，CLI 明确参数优先于环境默认值。
fn resolve_backfill_timeframes(cli_args: &MarketVelocityBackfillCliArgs) -> Result<Vec<String>> {
    if cli_args.timeframe.is_some() && cli_args.timeframes.is_some() {
        bail!("use only one of --timeframe or --timeframes");
    }
    let timeframes = if let Some(timeframes) = &cli_args.timeframes {
        timeframes.clone()
    } else if let Some(timeframe) = &cli_args.timeframe {
        vec![timeframe.trim().to_ascii_lowercase()]
    } else if let Some(env_timeframes) =
        first_non_empty_env(&["MARKET_VELOCITY_BACKFILL_TIMEFRAMES"])
    {
        parse_timeframe_list(&env_timeframes)
    } else {
        vec![
            env_or_default("MARKET_VELOCITY_BACKFILL_TIMEFRAME", DEFAULT_TIMEFRAME)
                .trim()
                .to_ascii_lowercase(),
        ]
    };
    if timeframes.is_empty() {
        bail!("market velocity candle backfill requires at least one timeframe");
    }
    for timeframe in &timeframes {
        candle_interval_ms(timeframe)?;
    }
    Ok(timeframes)
}

/// 封装推进arg，减少行情数据调用方重复实现相同细节。
fn next_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("missing value for {flag}"))
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_next<T>(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    next_arg(args, flag)?
        .parse::<T>()
        .with_context(|| format!("invalid value for {flag}"))
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_positive_u64(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<u64> {
    let value = next_arg(args, flag)?;
    parse_positive_u64_value(&value, flag)
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_positive_u64_value(value: &str, flag: &str) -> Result<u64> {
    let parsed = value
        .trim()
        .parse::<u64>()
        .with_context(|| format!("invalid value for {flag}"))?;
    if parsed == 0 {
        bail!("{flag} must be greater than 0");
    }
    Ok(parsed)
}
/// 封装环境变量ordefault，减少行情数据调用方重复实现相同细节。
fn env_or_default(key: &str, default_value: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_value.to_string())
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_env_bool(key: &str, default_value: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "y" | "on"
            )
        })
        .unwrap_or(default_value)
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_env_u64(key: &str, default_value: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default_value)
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_env_usize(key: &str, default_value: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(default_value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_symbols_normalizes_and_deduplicates() {
        assert_eq!(
            parse_symbol_list(" xag-usdt-swap, BTC-USDT-SWAP, xag-USDT-swap "),
            vec!["BTC-USDT-SWAP".to_string(), "XAG-USDT-SWAP".to_string()]
        );
    }
    #[test]
    fn cli_args_support_proxy_and_all_radar_symbols() {
        let args = parse_cli_args_from([
            "--symbols",
            "xag-usdt-swap",
            "--days",
            "30",
            "--proxy-url",
            "http://127.0.0.1:7897",
            "--all-radar-symbols",
            "--dry-run",
        ])
        .unwrap();
        assert_eq!(args.symbols, Some(vec!["XAG-USDT-SWAP".to_string()]));
        assert_eq!(args.days, Some(30));
        assert_eq!(
            args.proxy_url,
            Some(Some("http://127.0.0.1:7897".to_string()))
        );
        assert_eq!(args.require_4h, Some(false));
        assert_eq!(args.dry_run, Some(true));
    }
    #[test]
    fn cli_args_support_fail_fast_for_bulk_backfill() {
        let args = parse_cli_args_from(["--fail-fast"]).unwrap();
        assert_eq!(args.continue_on_error, Some(false));
    }
    #[test]
    fn cli_args_support_rust_native_scheduler_loop_interval() {
        let args = parse_cli_args_from(["--loop-interval-seconds", "300"]).unwrap();
        assert_eq!(args.loop_interval_seconds, Some(300));
        let args = parse_cli_args_from(["--loop-interval-seconds=600"]).unwrap();
        assert_eq!(args.loop_interval_seconds, Some(600));
        let error = parse_cli_args_from(["--loop-interval-seconds", "0"]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("--loop-interval-seconds must be greater than 0"),
            "unexpected error: {error:#}"
        );
    }
    #[test]
    fn cli_args_support_multiple_scheduler_timeframes() {
        let args = parse_cli_args_from(["--timeframes", "1m, 5m,15m"]).unwrap();
        assert_eq!(
            args.timeframes,
            Some(vec!["1m".to_string(), "5m".to_string(), "15m".to_string()])
        );
    }
    #[test]
    fn cli_args_support_enabled_strategy_symbol_source() {
        let args = parse_cli_args_from([
            "--enabled-strategy-symbols",
            "--timeframe",
            "4h",
            "--days",
            "60",
        ])
        .unwrap();
        assert_eq!(args.enabled_strategy_symbols, Some(true));
        assert_eq!(args.timeframe, Some("4h".to_string()));
        assert_eq!(args.days, Some(60));
    }
    #[test]
    fn history_url_paginates_to_older_candles_with_after() {
        let url = build_okx_history_candles_url(
            "https://www.okx.com/",
            "XAG-USDT-SWAP",
            "15m",
            Some(1_781_500_000_000),
            100,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://www.okx.com/api/v5/market/history-candles?instId=XAG-USDT-SWAP&bar=15m&limit=100&after=1781500000000"
        );
    }
    #[test]
    fn parse_okx_candle_row_matches_existing_dto_mapping() {
        let candle = parse_okx_candle_row(vec![
            "1781503200000".to_string(),
            "1".to_string(),
            "2".to_string(),
            "0.9".to_string(),
            "1.5".to_string(),
            "10".to_string(),
            "11".to_string(),
            "12".to_string(),
            "1".to_string(),
        ])
        .unwrap();
        assert_eq!(candle.ts, "1781503200000");
        assert_eq!(candle.v, "10");
        assert_eq!(candle.vol_ccy, "11");
        assert_eq!(candle.vol_ccy_quote, "12");
        assert_eq!(candle.confirm, "1");
    }
    #[test]
    fn parse_okx_candle_row_rejects_short_rows() {
        let error = parse_okx_candle_row(vec!["1781503200000".to_string()]).unwrap_err();
        assert!(error.to_string().contains("expected at least 9"));
    }
    #[test]
    fn max_history_pages_has_buffer_for_60_days_of_15m_candles() {
        let pages = max_history_pages(0, 60 * 24 * 60 * 60 * 1_000, CANDLE_15M_MS, 100);
        assert_eq!(pages, 65);
    }
    #[test]
    fn backfill_window_uses_full_range_when_no_local_candles_exist() {
        let window = resolve_incremental_backfill_window(
            1_000_000,
            2_000_000,
            CANDLE_15M_MS,
            CandleContinuityStatus::default(),
        );
        assert_eq!(window.fetch_start_ms, 1_000_000);
        assert_eq!(window.reason, BackfillWindowReason::EmptyOrMissingTable);
    }
    #[test]
    fn backfill_window_repairs_earliest_detected_gap_with_overlap() {
        let window = resolve_incremental_backfill_window(
            1_000_000,
            5_000_000,
            CANDLE_15M_MS,
            CandleContinuityStatus {
                earliest_ts: Some(1_000_000),
                latest_ts: Some(4_800_000),
                actual_count: 4,
                expected_count: 5,
                earliest_gap_start_ts: Some(3_000_000),
            },
        );
        assert_eq!(window.fetch_start_ms, 3_000_000 - CANDLE_15M_MS);
        assert_eq!(window.reason, BackfillWindowReason::GapRepair);
    }
    #[test]
    fn backfill_window_repairs_leading_gap_from_configured_start() {
        let window = resolve_incremental_backfill_window(
            1_000_000,
            5_000_000,
            CANDLE_15M_MS,
            CandleContinuityStatus {
                earliest_ts: Some(3_000_000),
                latest_ts: Some(4_800_000),
                actual_count: 3,
                expected_count: 3,
                earliest_gap_start_ts: None,
            },
        );
        assert_eq!(window.fetch_start_ms, 1_000_000);
        assert_eq!(window.reason, BackfillWindowReason::GapRepair);
    }
    #[test]
    fn backfill_window_uses_latest_candle_overlap_when_local_series_is_continuous() {
        let window = resolve_incremental_backfill_window(
            1_000_000,
            5_000_000,
            CANDLE_15M_MS,
            CandleContinuityStatus {
                earliest_ts: Some(1_000_000),
                latest_ts: Some(4_800_000),
                actual_count: 5,
                expected_count: 5,
                earliest_gap_start_ts: None,
            },
        );
        assert_eq!(window.fetch_start_ms, 4_800_000 - CANDLE_15M_MS);
        assert_eq!(window.reason, BackfillWindowReason::IncrementalTail);
    }
    #[test]
    fn symbol_start_aligns_configured_time_to_the_next_candle_boundary() {
        assert_eq!(
            aligned_symbol_start_ms(1_000_001, None, CANDLE_15M_MS),
            1_800_000
        );
    }
    #[test]
    fn symbol_start_clamps_to_listing_time_before_alignment() {
        assert_eq!(
            aligned_symbol_start_ms(1_000_000, Some(2_000_001), CANDLE_15M_MS),
            2_700_000
        );
    }
    #[test]
    fn unaligned_scheduler_window_does_not_trigger_false_gap_repair() {
        let configured_start_ms = 1_784_300_865_335;
        let aligned_start_ms = aligned_symbol_start_ms(configured_start_ms, None, CANDLE_1M_MS);
        let latest_ts = aligned_start_ms + CANDLE_1M_MS * 2_879;
        let window = resolve_incremental_backfill_window(
            aligned_start_ms,
            latest_ts + CANDLE_1M_MS,
            CANDLE_1M_MS,
            CandleContinuityStatus {
                earliest_ts: Some(aligned_start_ms),
                latest_ts: Some(latest_ts),
                actual_count: 2_880,
                expected_count: 2_880,
                earliest_gap_start_ts: None,
            },
        );
        assert_eq!(window.reason, BackfillWindowReason::IncrementalTail);
        assert_eq!(window.fetch_start_ms, latest_ts - CANDLE_1M_MS);
    }
    #[test]
    fn candle_continuity_uses_bounds_and_count_to_detect_missing_rows() {
        let status = CandleContinuityStatus {
            earliest_ts: Some(1_000_000),
            latest_ts: Some(1_000_000 + CANDLE_15M_MS * 4),
            actual_count: 4,
            expected_count: 5,
            earliest_gap_start_ts: None,
        };
        assert!(status.has_missing_candles());
    }
    #[test]
    fn candle_continuity_treats_matching_bounds_and_count_as_continuous() {
        let status = CandleContinuityStatus {
            earliest_ts: Some(1_000_000),
            latest_ts: Some(1_000_000 + CANDLE_15M_MS * 4),
            actual_count: 5,
            expected_count: 5,
            earliest_gap_start_ts: None,
        };
        assert!(!status.has_missing_candles());
    }
    #[test]
    fn candle_interval_ms_supports_4h_trend_backfill() {
        assert_eq!(candle_interval_ms("4h").unwrap(), 4 * 60 * 60 * 1_000);
    }
    #[test]
    fn candle_interval_ms_supports_1h_fvg_backfill() {
        assert_eq!(candle_interval_ms("1h").unwrap(), 60 * 60 * 1_000);
    }
    #[test]
    fn candle_interval_ms_supports_1m_scalper_backfill() {
        assert_eq!(candle_interval_ms("1m").unwrap(), 60 * 1_000);
        assert_eq!(okx_bar_for_timeframe("1m").unwrap(), "1m");
    }
    #[test]
    fn okx_bar_for_timeframe_uses_okx_hour_case() {
        assert_eq!(okx_bar_for_timeframe("5m").unwrap(), "5m");
        assert_eq!(okx_bar_for_timeframe("15m").unwrap(), "15m");
        assert_eq!(okx_bar_for_timeframe("1h").unwrap(), "1H");
        assert_eq!(okx_bar_for_timeframe("4h").unwrap(), "4H");
    }
    #[test]
    fn max_history_pages_has_buffer_for_60_days_of_5m_candles() {
        let pages = max_history_pages(0, 60 * 24 * 60 * 60 * 1_000, CANDLE_5M_MS, 100);
        assert_eq!(pages, 180);
    }
    #[test]
    fn max_history_pages_has_buffer_for_60_days_of_1h_candles() {
        let pages = max_history_pages(0, 60 * 24 * 60 * 60 * 1_000, CANDLE_1H_MS, 100);
        assert_eq!(pages, 22);
    }
    #[test]
    fn max_history_pages_has_buffer_for_60_days_of_4h_candles() {
        let pages = max_history_pages(0, 60 * 24 * 60 * 60 * 1_000, 4 * 60 * 60 * 1_000, 100);
        assert_eq!(pages, 11);
    }
    #[test]
    fn backfill_symbol_scan_uses_only_active_okx_symbols() {
        let sql = load_market_velocity_backfill_symbols_sql(false);
        assert!(
            sql.contains("exchange_symbols"),
            "backfill must consult exchange_symbols before requesting OKX candles: {sql}"
        );
        assert!(
            sql.contains("available_okx_symbols"),
            "backfill should use a dedicated available-symbol CTE before selecting candidates: {sql}"
        );
        let normalized_sql = sql.to_ascii_lowercase();
        assert!(
            normalized_sql.contains("lower(status) in ('trading', 'live')"),
            "deleted or unsupported OKX symbols must be excluded by status: {sql}"
        );
        assert!(
            sql.contains("JOIN available_okx_symbols USING (symbol)"),
            "unavailable OKX symbols must not reach history-candles requests: {sql}"
        );
    }

    #[test]
    fn enabled_strategy_symbol_scan_is_timeframe_scoped_and_exchange_safe() {
        let sql = load_enabled_strategy_backfill_symbols_sql().to_ascii_lowercase();
        assert!(sql.contains("from strategy_configs"));
        assert!(sql.contains("config.enabled = true"));
        assert!(sql.contains("lower(config.timeframe) = lower($1)"));
        assert!(sql.contains("lower(config.exchange) = 'okx'"));
        assert!(sql.contains("exchange_symbol.market_type = 'perpetual'"));
        assert!(sql.contains("lower(exchange_symbol.status) in ('trading', 'live')"));
    }

    #[test]
    fn okx_51001_is_missing_instrument_error() {
        let error = anyhow!(okx_history_candles_api_error(
            "51001",
            "Instrument ID doesn't exist.",
            "IP-USDT-SWAP"
        ));
        assert!(is_okx_missing_instrument_error(&error));
        let transient = anyhow!(okx_history_candles_api_error(
            "50011",
            "Rate limit reached.",
            "BTC-USDT-SWAP"
        ));
        assert!(!is_okx_missing_instrument_error(&transient));
    }

    #[test]
    fn dry_run_does_not_mark_missing_okx_instrument_deleted() {
        let missing = anyhow!(
            "OKX history-candles returned code={} msg=instrument missing",
            OKX_MISSING_INSTRUMENT_CODE
        );

        assert!(!should_mark_okx_exchange_symbol_deleted(true, &missing));
        assert!(should_mark_okx_exchange_symbol_deleted(false, &missing));
    }

    #[test]
    fn okx_missing_instrument_mark_sql_sets_deleted_status() {
        let sql = mark_okx_exchange_symbol_deleted_sql().to_ascii_lowercase();
        assert!(sql.contains("update exchange_symbols"));
        assert!(sql.contains("set status = 'deleted'"));
        assert!(sql.contains("exchange = 'okx'"));
        assert!(sql.contains("market_type = 'perpetual'"));
        assert!(sql.contains("upper(exchange_symbol) = upper($1)"));
        assert!(sql.contains("upper(normalized_symbol) = upper($1)"));
    }
}
