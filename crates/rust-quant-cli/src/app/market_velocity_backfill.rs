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

mod candle_writer_cutover;
mod history_pipeline;

use candle_writer_cutover::{resolve_excluded_symbols, retain_legacy_owned_symbols};
#[cfg(test)]
use history_pipeline::okx_history_candles_api_error;
use history_pipeline::{
    aligned_symbol_start_ms, load_candle_continuity_status, load_okx_symbol_list_time_ms,
    resolve_incremental_backfill_window,
};
pub use history_pipeline::{
    build_okx_history_candles_url, build_okx_http_client, candle_interval_ms,
    fetch_okx_history_candles, is_okx_missing_instrument_error, max_history_pages,
    okx_bar_for_timeframe, parse_okx_candle_row,
};
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
    /// 已由其他 Market owner 接管、不得由本 backfill 再写入的交易对。
    pub excluded_symbols: Vec<String>,
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
    /// 本轮必须排除的交易对；用于单 writer cutover。
    pub excluded_symbols: Option<Vec<String>>,
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
            "--exclude-symbols" => {
                parsed.excluded_symbols = Some(parse_symbol_list(&next_arg(&mut args, &arg)?))
            }
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
        "Usage: market_velocity_candle_backfill [--symbols BTC-USDT-SWAP,ETH-USDT-SWAP] [--exclude-symbols ETH-USDT-SWAP] [--days 60] [--timeframe 1m|5m|15m|1h|4h] [--timeframes 1m,5m,15m] [--enabled-strategy-symbols] [--proxy-url http://127.0.0.1:7897] [--dry-run|--write] [--all-radar-symbols] [--loop-interval-seconds 300]"
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
    let excluded_symbols = resolve_excluded_symbols(cli_args.excluded_symbols);
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
        excluded_symbols,
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
    retain_legacy_owned_symbols(&mut symbols, &config.excluded_symbols);
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
mod tests;
