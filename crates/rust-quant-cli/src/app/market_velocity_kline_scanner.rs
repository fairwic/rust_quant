use super::env_parse::first_non_empty_env;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use rust_decimal::Decimal;
use rust_quant_domain::entities::MarketRankEvent;
use rust_quant_market::models::quote_legacy_table_name;
use rust_quant_services::market::build_kline_15m_rank_velocity_event;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};

const DEFAULT_LOOKBACK_MINUTES: i64 = 30;
const DEFAULT_MIN_PRICE_CHANGE_PCT: f64 = 0.0;
const DEFAULT_PER_SYMBOL_LIMIT: usize = 4;
const CANDLE_15M_MS: i64 = 15 * 60 * 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityKlineScannerConfig {
    /// quant_core 数据库连接串；scanner 只读取 K 线表并写入 Core 的事件表。
    pub database_url: String,
    /// 显式扫描的交易对；为空时从 OKX 可交易合约和已有 15m K 线表取交集。
    pub symbols: Option<Vec<String>>,
    /// 向后扫描最近多少分钟的已完成 15m K 线。
    pub lookback_minutes: i64,
    /// 触发层最小 15m 上涨幅度百分比，策略级过滤仍在 entry confirmation 中执行。
    pub min_price_change_pct: f64,
    /// 触发层最大 15m 上涨幅度百分比；为空时不限制。
    pub max_price_change_pct: Option<f64>,
    /// 每个交易对最多转换多少根候选 15m K 线。
    pub per_symbol_limit: usize,
    /// 自动发现交易对时最多扫描多少个；为空时不限制。
    pub max_symbols: Option<usize>,
    /// dry-run 只输出统计，不写入 market_rank_events。
    pub dry_run: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarketVelocityKlineScannerCliArgs {
    /// 显式扫描的交易对列表。
    pub symbols: Option<Vec<String>>,
    /// 向后扫描最近多少分钟。
    pub lookback_minutes: Option<i64>,
    /// 最小 15m 上涨幅度百分比。
    pub min_price_change_pct: Option<f64>,
    /// 最大 15m 上涨幅度百分比。
    pub max_price_change_pct: Option<Option<f64>>,
    /// 每个交易对最多转换多少根候选 K 线。
    pub per_symbol_limit: Option<usize>,
    /// 自动发现交易对时最多扫描多少个。
    pub max_symbols: Option<Option<usize>>,
    /// 是否 dry-run；None 表示使用默认 dry-run。
    pub dry_run: Option<bool>,
    /// 循环调度间隔秒数；为空时只运行一次。
    pub loop_interval_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KlineScanSymbol {
    /// 交易对或资产符号。
    pub symbol: String,
    /// 15m K 线分表名。
    pub table_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KlineScanCandle {
    /// 交易对或资产符号。
    pub symbol: String,
    /// K 线开始时间，Unix 毫秒时间戳。
    pub candle_open_ts_ms: i64,
    /// 开盘价。
    pub open_price: Decimal,
    /// 收盘价。
    pub close_price: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketVelocityKlineScannerReport {
    /// 扫描到的交易对数量。
    pub symbols_total: usize,
    /// 命中的候选 K 线数量。
    pub candidate_events: usize,
    /// 新写入的事件数量。
    pub events_inserted: usize,
    /// 因已存在同源同 candle 事件而跳过的数量。
    pub duplicate_events: usize,
    /// 是否 dry-run。
    pub dry_run: bool,
}

/// 解析 K 线 scanner CLI 参数，保持默认 dry-run，避免误写生产事件表。
pub fn parse_cli_args_from<I, S>(args: I) -> Result<MarketVelocityKlineScannerCliArgs>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut parsed = MarketVelocityKlineScannerCliArgs::default();
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--symbols" => parsed.symbols = Some(parse_symbol_list(&next_arg(&mut args, &arg)?)),
            "--lookback-minutes" => parsed.lookback_minutes = Some(parse_next(&mut args, &arg)?),
            "--min-price-change-pct" => {
                parsed.min_price_change_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--max-price-change-pct" => {
                parsed.max_price_change_pct = Some(Some(parse_next(&mut args, &arg)?))
            }
            "--no-max-price-change-pct" => parsed.max_price_change_pct = Some(None),
            "--per-symbol-limit" => parsed.per_symbol_limit = Some(parse_next(&mut args, &arg)?),
            "--max-symbols" => parsed.max_symbols = Some(Some(parse_next(&mut args, &arg)?)),
            "--no-max-symbols" => parsed.max_symbols = Some(None),
            "--dry-run" => parsed.dry_run = Some(true),
            "--write" => parsed.dry_run = Some(false),
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
                print_market_velocity_kline_scanner_usage();
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    Ok(parsed)
}

/// 合并环境变量和 CLI 参数，生成 scanner 的最终运行配置。
pub fn config_from_env_and_args(
    args: MarketVelocityKlineScannerCliArgs,
) -> Result<MarketVelocityKlineScannerConfig> {
    let database_url = first_non_empty_env(&[
        "QUANT_CORE_DATABASE_URL",
        "POSTGRES_QUANT_CORE_DATABASE_URL",
    ])
    .context("market_velocity_kline_scanner requires QUANT_CORE_DATABASE_URL")?;
    let config = MarketVelocityKlineScannerConfig {
        database_url,
        symbols: args.symbols,
        lookback_minutes: args.lookback_minutes.unwrap_or(DEFAULT_LOOKBACK_MINUTES),
        min_price_change_pct: args
            .min_price_change_pct
            .unwrap_or(DEFAULT_MIN_PRICE_CHANGE_PCT),
        max_price_change_pct: args.max_price_change_pct.unwrap_or(None),
        per_symbol_limit: args.per_symbol_limit.unwrap_or(DEFAULT_PER_SYMBOL_LIMIT),
        max_symbols: args.max_symbols.unwrap_or(None),
        dry_run: args.dry_run.unwrap_or(true),
    };
    validate_config(&config)?;
    Ok(config)
}

/// 执行一次 15m K 线候选扫描，并按配置决定是否写入 market_rank_events。
pub async fn run_market_velocity_kline_scanner(
    config: MarketVelocityKlineScannerConfig,
) -> Result<MarketVelocityKlineScannerReport> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .context("connect quant_core database for market_velocity_kline_scanner")?;
    run_market_velocity_kline_scanner_with_pool(&pool, &config).await
}

/// 使用调用方提供的数据库连接执行 scanner，便于测试和 scheduler 复用。
pub async fn run_market_velocity_kline_scanner_with_pool(
    pool: &PgPool,
    config: &MarketVelocityKlineScannerConfig,
) -> Result<MarketVelocityKlineScannerReport> {
    validate_config(config)?;
    let now_ms = Utc::now().timestamp_millis();
    let lookback_start_ms = now_ms - config.lookback_minutes * 60 * 1000;
    let symbols = load_scan_symbols(pool, config).await?;
    let mut report = MarketVelocityKlineScannerReport {
        symbols_total: symbols.len(),
        candidate_events: 0,
        events_inserted: 0,
        duplicate_events: 0,
        dry_run: config.dry_run,
    };
    for symbol in symbols {
        let candles =
            load_kline_15m_candidate_events(pool, &symbol, now_ms, lookback_start_ms, config)
                .await?;
        report.candidate_events += candles.len();
        for candle in candles {
            let event = build_kline_15m_rank_velocity_event(
                &candle.symbol,
                candle.candle_open_ts_ms,
                candle.open_price,
                candle.close_price,
            )?;
            if config.dry_run {
                continue;
            }
            match insert_kline_rank_event_if_absent(pool, &event).await? {
                Some(_) => report.events_inserted += 1,
                None => report.duplicate_events += 1,
            }
        }
    }
    Ok(report)
}

fn validate_config(config: &MarketVelocityKlineScannerConfig) -> Result<()> {
    if config.lookback_minutes <= 0 {
        bail!("lookback_minutes must be positive");
    }
    if config.min_price_change_pct < 0.0 {
        bail!("min_price_change_pct must be non-negative");
    }
    if config
        .max_price_change_pct
        .is_some_and(|max| max < config.min_price_change_pct)
    {
        bail!("max_price_change_pct must be greater than or equal to min_price_change_pct");
    }
    if config.per_symbol_limit == 0 {
        bail!("per_symbol_limit must be positive");
    }
    if config.max_symbols == Some(0) {
        bail!("max_symbols must be positive when set");
    }
    Ok(())
}

async fn load_scan_symbols(
    pool: &PgPool,
    config: &MarketVelocityKlineScannerConfig,
) -> Result<Vec<KlineScanSymbol>> {
    if let Some(symbols) = &config.symbols {
        let mut result = Vec::new();
        for symbol in symbols {
            if let Some(row) = load_explicit_scan_symbol(pool, symbol).await? {
                result.push(row);
            }
        }
        return Ok(result);
    }
    let rows = sqlx::query(candidate_symbols_sql())
        .bind(config.max_symbols.map(|value| value as i64))
        .fetch_all(pool)
        .await
        .context("load market_velocity_kline_scanner symbols")?;
    rows.into_iter()
        .map(|row| {
            Ok(KlineScanSymbol {
                symbol: row.get("symbol"),
                table_name: row.get("table_name"),
            })
        })
        .collect()
}

async fn load_explicit_scan_symbol(pool: &PgPool, symbol: &str) -> Result<Option<KlineScanSymbol>> {
    let row = sqlx::query(explicit_symbol_sql())
        .bind(symbol)
        .fetch_optional(pool)
        .await
        .with_context(|| format!("load explicit 15m kline scan symbol {symbol}"))?;
    row.map(|row| {
        Ok(KlineScanSymbol {
            symbol: row.get("symbol"),
            table_name: row.get("table_name"),
        })
    })
    .transpose()
}

pub fn candidate_symbols_sql() -> &'static str {
    r#"
        WITH available_okx_symbols AS (
          SELECT DISTINCT upper(normalized_symbol) AS symbol
          FROM exchange_symbols
          WHERE exchange = 'okx'
            AND market_type = 'perpetual'
            AND lower(status) IN ('trading', 'live')
        ),
        available_candle_tables AS (
          SELECT
            upper(replace(table_name, '_candles_15m', '')) AS symbol,
            table_name
          FROM information_schema.tables
          WHERE table_schema = 'public'
            AND table_name LIKE '%\_candles\_15m' ESCAPE '\'
        )
        SELECT available_candle_tables.symbol, available_candle_tables.table_name
        FROM available_candle_tables
        JOIN available_okx_symbols USING (symbol)
        WHERE upper(replace(available_candle_tables.symbol, '-', '')) NOT LIKE 'LINKUSDT%'
        ORDER BY available_candle_tables.symbol
        LIMIT $1
        "#
}

fn explicit_symbol_sql() -> &'static str {
    r#"
        WITH requested AS (
          SELECT upper($1::text) AS symbol, lower($1::text) || '_candles_15m' AS table_name
        )
        SELECT requested.symbol, requested.table_name
        FROM requested
        JOIN exchange_symbols
          ON upper(exchange_symbols.normalized_symbol) = requested.symbol
         AND exchange_symbols.exchange = 'okx'
         AND exchange_symbols.market_type = 'perpetual'
         AND lower(exchange_symbols.status) IN ('trading', 'live')
        JOIN information_schema.tables
          ON information_schema.tables.table_schema = 'public'
         AND information_schema.tables.table_name = requested.table_name
        WHERE upper(replace(requested.symbol, '-', '')) NOT LIKE 'LINKUSDT%'
        LIMIT 1
        "#
}

async fn load_kline_15m_candidate_events(
    pool: &PgPool,
    symbol: &KlineScanSymbol,
    now_ms: i64,
    lookback_start_ms: i64,
    config: &MarketVelocityKlineScannerConfig,
) -> Result<Vec<KlineScanCandle>> {
    let query = kline_15m_candidate_events_sql(&symbol.table_name)?;
    let rows = sqlx::query(&query)
        .bind(now_ms)
        .bind(lookback_start_ms)
        .bind(config.min_price_change_pct)
        .bind(config.max_price_change_pct)
        .bind(config.per_symbol_limit as i64)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load 15m kline scanner events for {}", symbol.symbol))?;
    rows.into_iter()
        .map(|row| {
            Ok(KlineScanCandle {
                symbol: symbol.symbol.clone(),
                candle_open_ts_ms: row.get("ts"),
                open_price: parse_decimal(row.get::<String, _>("o"), "open price")?,
                close_price: parse_decimal(row.get::<String, _>("c"), "close price")?,
            })
        })
        .collect()
}

pub fn kline_15m_candidate_events_sql(table_name: &str) -> Result<String> {
    let quoted_table_name = quote_legacy_table_name(table_name)?;
    Ok(format!(
        r#"
        SELECT ts, o, c
        FROM {quoted_table_name}
        WHERE ts + {CANDLE_15M_MS} <= $1
          AND ts + {CANDLE_15M_MS} >= $2
          AND confirm = '1'
          AND o::double precision > 0
          AND c::double precision > o::double precision
          AND ((c::double precision - o::double precision) / o::double precision * 100.0) >= $3
          AND ($4::double precision IS NULL OR ((c::double precision - o::double precision) / o::double precision * 100.0) <= $4)
        ORDER BY ts DESC
        LIMIT $5
        "#
    ))
}

fn parse_decimal(value: String, label: &str) -> Result<Decimal> {
    value
        .parse::<Decimal>()
        .with_context(|| format!("parse {label}: {value}"))
}

async fn insert_kline_rank_event_if_absent(
    pool: &PgPool,
    event: &MarketRankEvent,
) -> Result<Option<i64>> {
    let technical_snapshot = event.technical_snapshot.as_ref();
    sqlx::query_scalar::<_, i64>(insert_kline_rank_event_if_absent_sql())
        .bind(&event.exchange)
        .bind(&event.symbol)
        .bind(event.event_type.as_str())
        .bind(&event.timeframe)
        .bind(event.old_rank)
        .bind(event.new_rank)
        .bind(event.delta_rank)
        .bind(event.volume_24h_quote)
        .bind(event.current_price)
        .bind(event.previous_price)
        .bind(event.price_change_pct)
        .bind(&event.price_direction)
        .bind(technical_snapshot.map(|snapshot| snapshot.timeframe.as_str()))
        .bind(technical_snapshot.map(|snapshot| snapshot.period))
        .bind(technical_snapshot.map(|snapshot| snapshot.close_price))
        .bind(technical_snapshot.map(|snapshot| snapshot.ma_value))
        .bind(technical_snapshot.map(|snapshot| snapshot.ema_value))
        .bind(technical_snapshot.map(|snapshot| snapshot.ma_distance_pct))
        .bind(technical_snapshot.map(|snapshot| snapshot.ema_distance_pct))
        .bind(technical_snapshot.map(|snapshot| snapshot.ma_state.as_str()))
        .bind(technical_snapshot.map(|snapshot| snapshot.ema_state.as_str()))
        .bind(technical_snapshot.map(|snapshot| snapshot.candle_count))
        .bind(technical_snapshot.map(|snapshot| snapshot.snapshot_at))
        .bind(&event.technical_snapshot_status)
        .bind(event.detected_at)
        .bind(&event.source)
        .bind(&event.notification_state)
        .fetch_optional(pool)
        .await
        .context("insert kline scanner market_rank_event if absent")
}

pub fn insert_kline_rank_event_if_absent_sql() -> &'static str {
    r#"
        INSERT INTO market_rank_events
            (exchange, symbol, event_type, timeframe, old_rank, new_rank, delta_rank,
             volume_24h_quote, current_price, previous_price, price_change_pct,
             price_direction, technical_timeframe, technical_period, technical_close_price,
             technical_ma_value, technical_ema_value, technical_ma_distance_pct,
             technical_ema_distance_pct, technical_ma_state, technical_ema_state,
             technical_candle_count, technical_snapshot_at, technical_snapshot_status,
             detected_at, source, notification_state)
        SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
               $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
               $21, $22, $23, $24, $25, $26, $27
        WHERE NOT EXISTS (
          SELECT 1
          FROM market_rank_events
          WHERE lower(exchange) = lower($1)
            AND symbol = $2
            AND event_type = $3
            AND COALESCE(timeframe, '') = COALESCE($4::text, '')
            AND detected_at = $25
            AND source = $26
        )
        RETURNING id
        "#
}

fn parse_symbol_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .collect()
}

fn next_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("{flag} requires a value"))
}

fn parse_next<T>(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let raw = next_arg(args, flag)?;
    raw.parse::<T>()
        .map_err(|error| anyhow::anyhow!("{flag} has invalid value {raw}: {error}"))
}

fn parse_positive_u64(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<u64> {
    parse_positive_u64_value(&next_arg(args, flag)?, flag)
}

fn parse_positive_u64_value(value: &str, flag: &str) -> Result<u64> {
    let parsed = value
        .trim()
        .parse::<u64>()
        .map_err(|error| anyhow::anyhow!("{flag} has invalid value {value}: {error}"))?;
    if parsed == 0 {
        bail!("{flag} must be positive");
    }
    Ok(parsed)
}

fn print_market_velocity_kline_scanner_usage() {
    println!(
        "Usage: market_velocity_kline_scanner [--symbols BTC-USDT-SWAP,ETH-USDT-SWAP] [--lookback-minutes 30] [--min-price-change-pct 0.0] [--max-price-change-pct 8.0|--no-max-price-change-pct] [--per-symbol-limit 4] [--max-symbols 100|--no-max-symbols] [--dry-run|--write] [--loop-interval-seconds 60]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_symbols_sql_reads_15m_candle_tables_without_rank_events() {
        let sql = candidate_symbols_sql();

        assert!(sql.contains("information_schema.tables"));
        assert!(sql.contains("LIKE '%\\_candles\\_15m' ESCAPE '\\'"));
        assert!(sql.contains("exchange_symbols"));
        assert!(sql.contains("available_okx_symbols"));
        assert!(!sql.contains("market_rank_events"));
    }

    #[test]
    fn kline_15m_candidate_events_sql_reads_completed_recent_up_candles() {
        let sql = kline_15m_candidate_events_sql("btc-usdt-swap_candles_15m").unwrap();

        assert!(sql.contains("\"btc-usdt-swap_candles_15m\""));
        assert!(sql.contains("ts + 900000 <= $1"));
        assert!(sql.contains("ts + 900000 >= $2"));
        assert!(sql.contains("c::double precision > o::double precision"));
        assert!(sql.contains("ORDER BY ts DESC"));
        assert!(!sql.contains("market_rank_events"));
    }

    #[test]
    fn insert_kline_rank_event_sql_deduplicates_source_symbol_and_detected_at() {
        let sql = insert_kline_rank_event_if_absent_sql();

        assert!(sql.contains("INSERT INTO market_rank_events"));
        assert!(sql.contains("WHERE NOT EXISTS"));
        assert!(sql.contains("lower(exchange) = lower($1)"));
        assert!(sql.contains("symbol = $2"));
        assert!(sql.contains("detected_at = $25"));
        assert!(sql.contains("source = $26"));
    }

    #[test]
    fn parse_cli_args_defaults_to_dry_run_and_accepts_write() {
        let default_args = parse_cli_args_from(Vec::<String>::new()).unwrap();
        assert_eq!(default_args.dry_run, None);

        let write_args = parse_cli_args_from(["--write"]).unwrap();
        assert_eq!(write_args.dry_run, Some(false));
    }

    #[test]
    fn parse_cli_args_accepts_loop_interval_and_rejects_zero() {
        let args = parse_cli_args_from(["--loop-interval-seconds=60"]).unwrap();
        assert_eq!(args.loop_interval_seconds, Some(60));

        let error = parse_cli_args_from(["--loop-interval-seconds", "0"]).unwrap_err();
        assert!(error.to_string().contains("must be positive"));
    }
}
