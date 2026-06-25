use super::env_parse::first_non_empty_env;
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use okx::dto::market_dto::CandleOkxRespDto;
use reqwest::{Client, Proxy, Url};
use rust_quant_market::models::CandlesModel;
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
const DEFAULT_REQUEST_SLEEP_MS: u64 = 150;
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
    /// proxyURL；为空时使用默认值或表示不限制。
    pub proxy_url: Option<Option<String>>,
    /// 是否要求 4 小时级别数据；为空时使用默认策略。
    pub require_4h: Option<bool>,
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
            "--proxy-url" => parsed.proxy_url = Some(Some(next_arg(&mut args, &arg)?)),
            "--no-proxy" => parsed.proxy_url = Some(None),
            "--require-4h" => parsed.require_4h = Some(true),
            "--all-radar-symbols" => parsed.require_4h = Some(false),
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
        "Usage: market_velocity_candle_backfill [--symbols BTC-USDT-SWAP,ETH-USDT-SWAP] [--days 60] [--proxy-url http://127.0.0.1:7897] [--dry-run|--write] [--all-radar-symbols] [--loop-interval-seconds 300]"
    );
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
    let mut symbols = if config.symbols.is_empty() {
        load_market_velocity_backfill_symbols(&pool, config.require_4h).await?
    } else {
        config.symbols.clone()
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
        dry_run: config.dry_run,
        failed_symbols: Vec::new(),
    };
    info!(
        "market velocity candle backfill started: symbols={}, days={}, timeframe={}, dry_run={}, proxy={}",
        total,
        config.days,
        config.timeframe,
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
        match backfill_symbol_candles(&client, &config, symbol, start_ms, end_ms).await {
            Ok(symbol_report) => {
                report.symbols_attempted += 1;
                report.candles_fetched += symbol_report.fetched;
                report.rows_upserted += symbol_report.upserted;
                info!(
                    "market velocity candle backfill symbol done: symbol={}, fetched={}, upserted={}",
                    symbol_report.symbol, symbol_report.fetched, symbol_report.upserted
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
    }
    Ok(report)
}
/// 同步 行情与市场数据 数据，保证本地状态与外部事实源保持一致。
async fn backfill_symbol_candles(
    client: &Client,
    config: &MarketVelocityBackfillConfig,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<SymbolBackfillReport> {
    let candles = fetch_okx_history_candles(
        client,
        &config.okx_rest_base,
        symbol,
        &config.timeframe,
        start_ms,
        end_ms,
        config.page_limit,
        config.request_sleep_ms,
    )
    .await?;
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
    })
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
        let payload = client
            .get(url)
            .header("User-Agent", "rust-quant-market-velocity-backfill/1.0")
            .send()
            .await
            .with_context(|| format!("request OKX history-candles failed: symbol={symbol}"))?
            .error_for_status()
            .with_context(|| format!("OKX history-candles HTTP status failed: symbol={symbol}"))?
            .json::<OkxHistoryCandlesResponse>()
            .await
            .with_context(|| {
                format!("decode OKX history-candles response failed: symbol={symbol}")
            })?;
        if payload.code != "0" {
            bail!(
                "OKX history-candles returned code={} msg={} symbol={}",
                payload.code,
                payload.msg,
                symbol
            );
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
/// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
pub async fn load_market_velocity_backfill_symbols(
    pool: &PgPool,
    require_4h: bool,
) -> Result<Vec<String>> {
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
    let query = format!(
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
        )
        SELECT candidates.symbol
        FROM candidates
        {join_4h}
        ORDER BY candidates.symbol
        "#
    );
    let rows = sqlx::query(&query).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("symbol"))
        .collect())
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
        "15m" => Ok(CANDLE_15M_MS),
        "1h" => Ok(CANDLE_1H_MS),
        "4h" => Ok(CANDLE_4H_MS),
        other => bail!(
            "unsupported market velocity candle backfill timeframe: {other}; supported: 15m, 1h, 4h"
        ),
    }
}
/// 提供OKXbarfortimeframe的集中实现，避免行情数据调用方重复处理相同细节。
pub fn okx_bar_for_timeframe(timeframe: &str) -> Result<&'static str> {
    match timeframe.trim().to_ascii_lowercase().as_str() {
        "15m" => Ok("15m"),
        "1h" => Ok("1H"),
        "4h" => Ok("4H"),
        other => {
            bail!("unsupported market velocity OKX candle bar: {other}; supported: 15m, 1h, 4h")
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
    fn history_url_paginates_to_older_candles_with_after() {
        let url = build_okx_history_candles_url(
            "https://www.okx.com/",
            "XAG-USDT-SWAP",
            "15m",
            Some(1_781_500_000_000),
            100,
        )
        .unwrap();
        assert_eq!(url.as_str(), "https://www.okx.com/api/v5/market/history-candles?instId=XAG-USDT-SWAP&bar=15m&limit=100&after=1781500000000");
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
    fn candle_interval_ms_supports_4h_trend_backfill() {
        assert_eq!(candle_interval_ms("4h").unwrap(), 4 * 60 * 60 * 1_000);
    }
    #[test]
    fn candle_interval_ms_supports_1h_fvg_backfill() {
        assert_eq!(candle_interval_ms("1h").unwrap(), 60 * 60 * 1_000);
    }
    #[test]
    fn okx_bar_for_timeframe_uses_okx_hour_case() {
        assert_eq!(okx_bar_for_timeframe("15m").unwrap(), "15m");
        assert_eq!(okx_bar_for_timeframe("1h").unwrap(), "1H");
        assert_eq!(okx_bar_for_timeframe("4h").unwrap(), "4H");
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
}
