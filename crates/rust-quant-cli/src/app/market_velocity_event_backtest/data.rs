use super::directional_reversal::{
    BTC_BROAD_DIRECTION_LOOKBACK_CANDLES, EXHAUSTION_CURRENT_CLUSTER_CANDLES,
    EXHAUSTION_VOLUME_LOOKBACK_CANDLES,
};
use super::historical_universe::HistoricalUniverseSchedule;
use super::kline_volume_rank_velocity::load_kline_volume_rank_events;
use super::one_shot_trend_state::{scan_one_shot_trend_events, OneShotTrendScanStats};
use super::{
    build_computed_candles, BacktestCandle, BacktestDataSet, CandlePair, FvgEntryMode,
    MarketVelocityEventBacktestArgs, MarketVelocityEventSource, MarketVelocityTrendTimeframe,
    RadarEvent, FAST_MOMENTUM_BOLLINGER_PERIOD, FAST_MOMENTUM_RSI_PERIOD, MS_15M, MS_1H, MS_4H,
    PAPER_OUTCOME_HORIZONS,
};
use anyhow::{bail, Context, Result};
use sqlx::{PgPool, Row};
use std::collections::{BTreeSet, HashMap};
const FAST_15M_CONTEXT_WARMUP_CANDLES: usize = 96;
/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
pub(super) async fn load_backtest_data(
    pool: &PgPool,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<BacktestDataSet> {
    let historical_universe = HistoricalUniverseSchedule::from_args(args)?;
    let pairs = load_candle_pairs(pool, args, historical_universe.as_ref()).await?;
    if args.entry_once_per_opposite_trend_state || args.entry_once_per_historical_trend_state {
        return load_one_shot_trend_state_data(pool, args, pairs).await;
    }
    let symbols = pairs
        .iter()
        .map(|pair| pair.symbol.clone())
        .collect::<Vec<_>>();
    let events = load_events(pool, &symbols, args, historical_universe.as_ref()).await?;
    let candle_window = candle_load_window_ms(args, &events);
    let mut candles_15m = HashMap::new();
    let mut candles_1h = HashMap::new();
    let mut candles_4h = HashMap::new();
    let mut candles_15m_computed = HashMap::new();
    let mut candles_4h_computed = HashMap::new();
    let load_1h_candles = should_load_1h_candles(args);
    let load_4h_candles = should_load_4h_candles(args);
    for pair in &pairs {
        let raw_15m = load_candles(pool, &pair.candles_15m, candle_window).await?;
        let raw_1h = match (load_1h_candles, pair.candles_1h.as_deref()) {
            (true, Some(table_name)) => load_candles(pool, table_name, candle_window).await?,
            _ => Vec::new(),
        };
        let raw_4h = if load_4h_candles {
            load_candles(pool, &pair.candles_4h, candle_window).await?
        } else {
            Vec::new()
        };
        candles_15m_computed.insert(
            pair.symbol.clone(),
            build_computed_candles(raw_15m.clone(), args.entry_period),
        );
        candles_4h_computed.insert(
            pair.symbol.clone(),
            build_computed_candles(raw_4h.clone(), args.entry_period),
        );
        candles_15m.insert(pair.symbol.clone(), raw_15m);
        candles_1h.insert(pair.symbol.clone(), raw_1h);
        candles_4h.insert(pair.symbol.clone(), raw_4h);
    }
    Ok(BacktestDataSet {
        historical_universe_version: historical_universe
            .as_ref()
            .map(|schedule| schedule.version.clone()),
        pairs,
        candles_15m,
        candles_1h,
        candles_4h,
        candles_15m_computed,
        candles_4h_computed,
        events,
    })
}

/// 一次性趋势研究从每个 symbol 的完整已存历史开始推进状态，避免窗口起点伪造重新武装。
async fn load_one_shot_trend_state_data(
    pool: &PgPool,
    args: &MarketVelocityEventBacktestArgs,
    pairs: Vec<CandlePair>,
) -> Result<BacktestDataSet> {
    let event_end_ms = args
        .event_end_ms
        .context("one-shot trend state requires event end")?;
    let tail_ms = PAPER_OUTCOME_HORIZONS
        .iter()
        .map(|(_, horizon_ms)| *horizon_ms)
        .max()
        .unwrap_or(0)
        .saturating_add(retest_post_signal_wait_ms(args))
        .saturating_add(MS_15M);
    let candle_window = Some((0, event_end_ms.saturating_add(tail_ms)));
    let mut candles_15m = HashMap::new();
    let mut candles_15m_computed = HashMap::new();
    let mut events = Vec::new();
    let mut aggregate = OneShotTrendScanStats::default();

    for pair in &pairs {
        let raw_15m = load_candles(pool, &pair.candles_15m, candle_window).await?;
        let computed = build_computed_candles(raw_15m.clone(), args.entry_period);
        let scan = scan_one_shot_trend_events(&pair.symbol, &computed, args);
        aggregate.armed_episodes += scan.stats.armed_episodes;
        aggregate.neutral_resets += scan.stats.neutral_resets;
        aggregate.valid_setups_before_dedup += scan.stats.valid_setups_before_dedup;
        aggregate.emitted_setups += scan.stats.emitted_setups;
        events.extend(scan.events);
        candles_15m.insert(pair.symbol.clone(), raw_15m);
        candles_15m_computed.insert(pair.symbol.clone(), computed);
    }
    events.sort_by_key(|event| (event.ts, event.id));
    println!(
        "one_shot_trend_state_scan\tsymbols={}\tarmed_episodes={}\tneutral_resets={}\tvalid_setups_before_dedup={}\temitted_setups={}",
        pairs.len(),
        aggregate.armed_episodes,
        aggregate.neutral_resets,
        aggregate.valid_setups_before_dedup,
        aggregate.emitted_setups,
    );
    Ok(BacktestDataSet {
        historical_universe_version: None,
        pairs,
        candles_15m,
        candles_1h: HashMap::new(),
        candles_4h: HashMap::new(),
        candles_15m_computed,
        candles_4h_computed: HashMap::new(),
        events,
    })
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
async fn load_candle_pairs(
    pool: &PgPool,
    args: &MarketVelocityEventBacktestArgs,
    historical_universe: Option<&HistoricalUniverseSchedule>,
) -> Result<Vec<CandlePair>> {
    if let Some(schedule) = historical_universe {
        return load_historical_candle_pairs(pool, args, schedule).await;
    }
    let sql = candidate_symbols_sql(args);
    let mut query = sqlx::query(sql)
        .bind(args.min_delta_rank)
        .bind(args.max_delta_rank)
        .bind(args.min_price_change_pct)
        .bind(args.max_price_change_pct)
        .bind(args.trade_direction.label())
        .bind(args.event_start_ms)
        .bind(args.event_end_ms);
    if args.event_source == MarketVelocityEventSource::Kline15m {
        query = query
            .bind(i64::try_from(args.sample_limit).unwrap_or(i64::MAX))
            .bind(args.sample_seed.as_str());
    }
    let rows = query
        .fetch_all(pool)
        .await
        .context("load market velocity candle table pairs")?;
    Ok(rows
        .into_iter()
        .map(|row| CandlePair {
            symbol: row.get("symbol"),
            candles_15m: row.get("candles_15m"),
            candles_1h: row.try_get("candles_1h").ok(),
            candles_4h: row.get("candles_4h"),
        })
        .collect())
}

/// manifest 模式按成员并集精确加载，不再让 sample limit 或随机种子改变币池。
async fn load_historical_candle_pairs(
    pool: &PgPool,
    args: &MarketVelocityEventBacktestArgs,
    schedule: &HistoricalUniverseSchedule,
) -> Result<Vec<CandlePair>> {
    let symbols = schedule.union_symbols();
    let sql = if should_load_4h_candles(args) {
        r#"
        WITH candidates AS (SELECT unnest($1::text[]) AS symbol)
        SELECT
          candidates.symbol,
          t15.table_name AS candles_15m,
          t1.table_name AS candles_1h,
          t4.table_name AS candles_4h
        FROM candidates
        JOIN information_schema.tables t15
          ON t15.table_schema = 'public'
         AND t15.table_name = lower(candidates.symbol) || '_candles_15m'
        LEFT JOIN information_schema.tables t1
          ON t1.table_schema = 'public'
         AND t1.table_name = lower(candidates.symbol) || '_candles_1h'
        JOIN information_schema.tables t4
          ON t4.table_schema = 'public'
         AND t4.table_name = lower(candidates.symbol) || '_candles_4h'
        ORDER BY candidates.symbol
        "#
    } else {
        r#"
        WITH candidates AS (SELECT unnest($1::text[]) AS symbol)
        SELECT
          candidates.symbol,
          t15.table_name AS candles_15m,
          t1.table_name AS candles_1h,
          t15.table_name AS candles_4h
        FROM candidates
        JOIN information_schema.tables t15
          ON t15.table_schema = 'public'
         AND t15.table_name = lower(candidates.symbol) || '_candles_15m'
        LEFT JOIN information_schema.tables t1
          ON t1.table_schema = 'public'
         AND t1.table_name = lower(candidates.symbol) || '_candles_1h'
        ORDER BY candidates.symbol
        "#
    };
    let rows = sqlx::query(sql)
        .bind(&symbols)
        .fetch_all(pool)
        .await
        .context("load historical universe candle table pairs")?;
    let pairs = rows
        .into_iter()
        .map(|row| CandlePair {
            symbol: row.get("symbol"),
            candles_15m: row.get("candles_15m"),
            candles_1h: row.try_get("candles_1h").ok(),
            candles_4h: row.get("candles_4h"),
        })
        .collect::<Vec<_>>();
    let loaded = pairs
        .iter()
        .map(|pair| pair.symbol.clone())
        .collect::<BTreeSet<_>>();
    let missing = symbols
        .iter()
        .filter(|symbol| !loaded.contains(*symbol))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "historical universe is missing required candle tables: {}",
            missing.join(",")
        );
    }
    Ok(pairs)
}
/// 判断候选symbolsSQL，给回测策略流程提供布尔结果。
fn candidate_symbols_sql(args: &MarketVelocityEventBacktestArgs) -> &'static str {
    match args.event_source {
        MarketVelocityEventSource::Episodes => {
            r#"
            WITH candidates AS (
              SELECT DISTINCT upper(symbol) AS symbol
              FROM market_velocity_episodes
              WHERE event_type IN ('rank_velocity', 'top_entry')
                AND status IN ('active', 'closed')
                AND COALESCE(max_delta_rank, latest_delta_rank, 0) >= $1
                AND ($2::int IS NULL OR COALESCE(max_delta_rank, latest_delta_rank, 0) <= $2)
                AND (
                  ($5 = 'long' AND lower(price_direction) = 'up')
                  OR ($5 = 'short' AND lower(price_direction) = 'down')
                  OR ($5 = 'both' AND lower(price_direction) IN ('up', 'down'))
                )
                AND current_price IS NOT NULL
                AND ($3::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $3)
                AND ($4::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) <= $4)
                AND ($6::bigint IS NULL OR started_at >= to_timestamp($6::double precision / 1000.0))
                AND ($7::bigint IS NULL OR started_at <= to_timestamp($7::double precision / 1000.0))
            )
            SELECT
              candidates.symbol,
              t15.table_name AS candles_15m,
              t1.table_name AS candles_1h,
              t4.table_name AS candles_4h
            FROM candidates
            JOIN information_schema.tables t15
              ON t15.table_schema = 'public'
             AND t15.table_name = lower(candidates.symbol) || '_candles_15m'
            LEFT JOIN information_schema.tables t1
              ON t1.table_schema = 'public'
             AND t1.table_name = lower(candidates.symbol) || '_candles_1h'
            JOIN information_schema.tables t4
              ON t4.table_schema = 'public'
             AND t4.table_name = lower(candidates.symbol) || '_candles_4h'
            ORDER BY candidates.symbol
            "#
        }
        MarketVelocityEventSource::RawEvents | MarketVelocityEventSource::RawState => {
            r#"
            WITH candidates AS (
              SELECT DISTINCT upper(symbol) AS symbol
              FROM market_rank_events
              WHERE event_type = 'rank_velocity'
                AND COALESCE(timeframe, '') = '15分钟'
                AND delta_rank >= $1
                AND ($2::int IS NULL OR delta_rank <= $2)
                AND (
                  ($5 = 'long' AND lower(price_direction) = 'up')
                  OR ($5 = 'short' AND lower(price_direction) = 'down')
                  OR ($5 = 'both' AND lower(price_direction) IN ('up', 'down'))
                )
                AND current_price IS NOT NULL
                AND ($3::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $3)
                AND ($4::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) <= $4)
                AND ($6::bigint IS NULL OR detected_at >= to_timestamp($6::double precision / 1000.0))
                AND ($7::bigint IS NULL OR detected_at <= to_timestamp($7::double precision / 1000.0))
            )
            SELECT
              candidates.symbol,
              t15.table_name AS candles_15m,
              t1.table_name AS candles_1h,
              t4.table_name AS candles_4h
            FROM candidates
            JOIN information_schema.tables t15
              ON t15.table_schema = 'public'
             AND t15.table_name = lower(candidates.symbol) || '_candles_15m'
            LEFT JOIN information_schema.tables t1
              ON t1.table_schema = 'public'
             AND t1.table_name = lower(candidates.symbol) || '_candles_1h'
            JOIN information_schema.tables t4
              ON t4.table_schema = 'public'
             AND t4.table_name = lower(candidates.symbol) || '_candles_4h'
            ORDER BY candidates.symbol
            "#
        }
        MarketVelocityEventSource::Kline15m
            if args.kline_current_live_only && should_load_4h_candles(args) =>
        {
            r#"
            WITH candidates AS (
              SELECT
                upper(replace(table_name, '_candles_15m', '')) AS symbol,
                table_name AS candles_15m
              FROM information_schema.tables
              WHERE table_schema = 'public'
                AND table_name LIKE '%\_candles\_15m' ESCAPE '\'
            )
            SELECT
              candidates.symbol,
              candidates.candles_15m,
              t1.table_name AS candles_1h,
              t4.table_name AS candles_4h
            FROM candidates
            JOIN exchange_symbols exchange_symbol
              ON exchange_symbol.exchange = 'okx'
             AND exchange_symbol.market_type = 'perpetual'
             AND exchange_symbol.status = 'live'
             AND exchange_symbol.contract_type = 'linear'
             AND exchange_symbol.exchange_symbol = candidates.symbol
             AND exchange_symbol.exchange_symbol LIKE '%-USDT-SWAP'
            LEFT JOIN information_schema.tables t1
              ON t1.table_schema = 'public'
             AND t1.table_name = lower(candidates.symbol) || '_candles_1h'
            JOIN information_schema.tables t4
              ON t4.table_schema = 'public'
             AND t4.table_name = lower(candidates.symbol) || '_candles_4h'
            ORDER BY md5($9::text || ':' || candidates.symbol)
            LIMIT $8
            "#
        }
        MarketVelocityEventSource::Kline15m if args.kline_current_live_only => {
            r#"
            WITH candidates AS (
              SELECT
                upper(replace(table_name, '_candles_15m', '')) AS symbol,
                table_name AS candles_15m
              FROM information_schema.tables
              WHERE table_schema = 'public'
                AND table_name LIKE '%\_candles\_15m' ESCAPE '\'
            )
            SELECT
              candidates.symbol,
              candidates.candles_15m,
              t1.table_name AS candles_1h,
              candidates.candles_15m AS candles_4h
            FROM candidates
            JOIN exchange_symbols exchange_symbol
              ON exchange_symbol.exchange = 'okx'
             AND exchange_symbol.market_type = 'perpetual'
             AND exchange_symbol.status = 'live'
             AND exchange_symbol.contract_type = 'linear'
             AND exchange_symbol.exchange_symbol = candidates.symbol
             AND exchange_symbol.exchange_symbol LIKE '%-USDT-SWAP'
            LEFT JOIN information_schema.tables t1
              ON t1.table_schema = 'public'
             AND t1.table_name = lower(candidates.symbol) || '_candles_1h'
            ORDER BY md5($9::text || ':' || candidates.symbol)
            LIMIT $8
            "#
        }
        MarketVelocityEventSource::Kline15m if should_load_4h_candles(args) => {
            r#"
            WITH candidates AS (
              SELECT
                upper(replace(table_name, '_candles_15m', '')) AS symbol,
                table_name AS candles_15m
              FROM information_schema.tables
              WHERE table_schema = 'public'
                AND table_name LIKE '%\_candles\_15m' ESCAPE '\'
            )
            SELECT
              candidates.symbol,
              candidates.candles_15m,
              t1.table_name AS candles_1h,
              t4.table_name AS candles_4h
            FROM candidates
            LEFT JOIN information_schema.tables t1
              ON t1.table_schema = 'public'
             AND t1.table_name = lower(candidates.symbol) || '_candles_1h'
            JOIN information_schema.tables t4
              ON t4.table_schema = 'public'
             AND t4.table_name = lower(candidates.symbol) || '_candles_4h'
            ORDER BY md5($9::text || ':' || candidates.symbol)
            LIMIT $8
            "#
        }
        MarketVelocityEventSource::Kline15m => {
            r#"
            WITH candidates AS (
              SELECT
                upper(replace(table_name, '_candles_15m', '')) AS symbol,
                table_name AS candles_15m
              FROM information_schema.tables
              WHERE table_schema = 'public'
                AND table_name LIKE '%\_candles\_15m' ESCAPE '\'
            )
            SELECT
              candidates.symbol,
              candidates.candles_15m,
              t1.table_name AS candles_1h,
              candidates.candles_15m AS candles_4h
            FROM candidates
            LEFT JOIN information_schema.tables t1
              ON t1.table_schema = 'public'
             AND t1.table_name = lower(candidates.symbol) || '_candles_1h'
            ORDER BY md5($9::text || ':' || candidates.symbol)
            LIMIT $8
            "#
        }
    }
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
async fn load_candles(
    pool: &PgPool,
    table_name: &str,
    window_ms: Option<(i64, i64)>,
) -> Result<Vec<BacktestCandle>> {
    let table_name = quote_identifier(table_name);
    let rows = match window_ms {
        Some((start_ms, end_ms)) => {
            let query = format!(
                "SELECT ts, o, h, l, c, vol FROM {table_name} WHERE confirm = '1' AND ts >= $1 AND ts <= $2 ORDER BY ts"
            );
            sqlx::query(&query)
                .bind(start_ms)
                .bind(end_ms)
                .fetch_all(pool)
                .await
        }
        None => {
            let query = format!(
                "SELECT ts, o, h, l, c, vol FROM {table_name} WHERE confirm = '1' ORDER BY ts"
            );
            sqlx::query(&query).fetch_all(pool).await
        }
    }
    .with_context(|| format!("load candles from {table_name}"))?;
    rows.into_iter()
        .map(|row| {
            Ok(BacktestCandle {
                ts: row.get::<i64, _>("ts"),
                open: parse_f64(row.get::<String, _>("o").as_str())?,
                high: parse_f64(row.get::<String, _>("h").as_str())?,
                low: parse_f64(row.get::<String, _>("l").as_str())?,
                close: parse_f64(row.get::<String, _>("c").as_str())?,
                volume: parse_f64(row.get::<String, _>("vol").as_str())?,
            })
        })
        .collect()
}
/// 判断当前研究参数是否需要读取 1H K 线，避免纯 15m/4h 方案反复拉取无用数据。
fn should_load_1h_candles(args: &MarketVelocityEventBacktestArgs) -> bool {
    args.trend_timeframe == MarketVelocityTrendTimeframe::OneHour
        || matches!(
            args.fvg_entry_mode,
            FvgEntryMode::M15To1h | FvgEntryMode::H1To4h
        )
}
/// 判断当前研究参数是否需要读取 4H K 线，避免无高周期门槛时被历史 4H 数据拖慢。
fn should_load_4h_candles(args: &MarketVelocityEventBacktestArgs) -> bool {
    args.trend_timeframe == MarketVelocityTrendTimeframe::FourHour
        || matches!(args.fvg_entry_mode, FvgEntryMode::H1To4h)
}
/// 根据实际候选事件裁剪 K 线加载范围，避免参数扫描时反复拉取全量历史 K 线。
fn candle_load_window_ms(
    args: &MarketVelocityEventBacktestArgs,
    events: &[RadarEvent],
) -> Option<(i64, i64)> {
    let first_event_ts = events.iter().map(|event| event.ts).min()?;
    let last_event_ts = events.iter().map(|event| event.ts).max()?;
    let warmup_ms = trend_warmup_ms(args)
        .max(entry_warmup_ms(args))
        .max(fvg_warmup_ms(args));
    let max_outcome_horizon_ms = PAPER_OUTCOME_HORIZONS
        .iter()
        .map(|(_, horizon_ms)| *horizon_ms)
        .max()
        .unwrap_or(0);
    let post_signal_wait_ms = fvg_post_signal_wait_ms(args)
        .max(retest_post_signal_wait_ms(args))
        .max(deferred_reversal_post_signal_wait_ms(args))
        .saturating_add(candle_window_tail_buffer_ms(args));
    Some((
        first_event_ts.saturating_sub(warmup_ms),
        last_event_ts
            .saturating_add(max_outcome_horizon_ms)
            .saturating_add(post_signal_wait_ms),
    ))
}
fn trend_warmup_ms(args: &MarketVelocityEventBacktestArgs) -> i64 {
    match args.trend_timeframe {
        MarketVelocityTrendTimeframe::FourHour => candle_count_ms(args.entry_period + 3, MS_4H),
        MarketVelocityTrendTimeframe::OneHour => candle_count_ms(args.entry_period + 3, MS_1H),
        MarketVelocityTrendTimeframe::Off => 0,
    }
}
fn entry_warmup_ms(args: &MarketVelocityEventBacktestArgs) -> i64 {
    let mut warmup_candles = args
        .entry_period
        .saturating_add(3)
        .max(FAST_15M_CONTEXT_WARMUP_CANDLES);
    if args.entry_min_rsi.is_some()
        || args.entry_max_rsi.is_some()
        || args.entry_min_rsi_delta.is_some()
    {
        warmup_candles = warmup_candles.max(
            FAST_MOMENTUM_RSI_PERIOD
                .saturating_add(args.entry_rsi_delta_lookback_candles)
                .saturating_add(3),
        );
    }
    if args.entry_bollinger_breakout || args.entry_min_bollinger_bandwidth_expansion_pct.is_some() {
        warmup_candles = warmup_candles.max(FAST_MOMENTUM_BOLLINGER_PERIOD.saturating_add(3));
    }
    if args.entry_min_recent_drawdown_pct.is_some() {
        warmup_candles = warmup_candles.max(
            args.entry_recent_drawdown_lookback_candles
                .saturating_add(3),
        );
    }
    if args.entry_min_opposite_net_move_pct.is_some()
        || args.entry_min_opposite_duration_candles.is_some()
    {
        let opposite_move_candles = args
            .entry_min_opposite_duration_candles
            .unwrap_or_default()
            .max(args.entry_opposite_move_lookback_candles);
        warmup_candles = warmup_candles.max(opposite_move_candles.saturating_add(3));
    }
    if args.entry_min_exhaustion_volume_dominance_ratio.is_some() {
        warmup_candles = warmup_candles.max(
            EXHAUSTION_VOLUME_LOOKBACK_CANDLES.saturating_add(EXHAUSTION_CURRENT_CLUSTER_CANDLES),
        );
    }
    if args.entry_btc_384_min_directional_net_move_pct.is_some() {
        warmup_candles = warmup_candles.max(BTC_BROAD_DIRECTION_LOOKBACK_CANDLES.saturating_add(3));
    }
    candle_count_ms(warmup_candles, MS_15M)
}
fn fvg_warmup_ms(args: &MarketVelocityEventBacktestArgs) -> i64 {
    match args.fvg_entry_mode {
        FvgEntryMode::Off => 0,
        FvgEntryMode::M15SelfAfterSignal | FvgEntryMode::M15ImpulseRetrace => {
            candle_count_ms(args.fvg_lookback_candles.saturating_add(3), MS_15M)
        }
        FvgEntryMode::M15To1h => {
            candle_count_ms(args.fvg_lookback_candles.saturating_add(3), MS_1H)
        }
        FvgEntryMode::H1To4h => candle_count_ms(args.fvg_lookback_candles.saturating_add(3), MS_4H),
    }
}
fn fvg_post_signal_wait_ms(args: &MarketVelocityEventBacktestArgs) -> i64 {
    match args.fvg_entry_mode {
        FvgEntryMode::Off => 0,
        FvgEntryMode::M15To1h
        | FvgEntryMode::M15SelfAfterSignal
        | FvgEntryMode::M15ImpulseRetrace => candle_count_ms(args.fvg_max_wait_candles, MS_15M),
        FvgEntryMode::H1To4h => candle_count_ms(args.fvg_max_wait_candles, MS_1H),
    }
}
fn retest_post_signal_wait_ms(args: &MarketVelocityEventBacktestArgs) -> i64 {
    if args.entry_retest_after_signal {
        candle_count_ms(args.entry_retest_max_wait_candles, MS_15M)
    } else {
        0
    }
}
fn deferred_reversal_post_signal_wait_ms(args: &MarketVelocityEventBacktestArgs) -> i64 {
    if args.entry_defer_bearish_continuation {
        candle_count_ms(args.entry_defer_max_wait_candles.saturating_add(1), MS_15M)
    } else {
        0
    }
}
fn candle_window_tail_buffer_ms(args: &MarketVelocityEventBacktestArgs) -> i64 {
    if should_load_4h_candles(args) {
        MS_4H
    } else if should_load_1h_candles(args) {
        MS_1H
    } else {
        MS_15M
    }
}
/// 将 K 线根数转换成毫秒跨度，溢出时按 i64 上限饱和，避免极端参数破坏窗口计算。
fn candle_count_ms(count: usize, candle_ms: i64) -> i64 {
    i64::try_from(count)
        .unwrap_or(i64::MAX / candle_ms)
        .saturating_mul(candle_ms)
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
async fn load_events(
    pool: &PgPool,
    symbols: &[String],
    args: &MarketVelocityEventBacktestArgs,
    historical_universe: Option<&HistoricalUniverseSchedule>,
) -> Result<Vec<RadarEvent>> {
    if symbols.is_empty() {
        return Ok(Vec::new());
    }
    if args.event_source == MarketVelocityEventSource::Kline15m {
        if args.kline_volume_rank_velocity {
            return load_kline_volume_rank_events(pool, symbols, args, historical_universe).await;
        }
        let mut events = load_kline_15m_events(pool, symbols, args).await?;
        if let Some(schedule) = historical_universe {
            events.retain(|event| schedule.allows(&event.symbol, event.ts));
        }
        return Ok(events);
    }
    let sql = event_source_sql(args);
    let rows = sqlx::query(sql)
        .bind(symbols)
        .bind(args.min_delta_rank)
        .bind(args.max_delta_rank)
        .bind(args.min_price_change_pct)
        .bind(args.max_price_change_pct)
        .bind(args.trade_direction.label())
        .bind(args.event_start_ms)
        .bind(args.event_end_ms)
        .fetch_all(pool)
        .await
        .context("load market velocity radar events")?;
    rows.into_iter()
        .map(|row| {
            Ok(RadarEvent {
                id: row.get("id"),
                exchange: row.get("exchange"),
                symbol: row.get("symbol"),
                ts: row.get("detected_ms"),
                detected_at: row.get("detected_at"),
                new_rank: row.get("new_rank"),
                delta_rank: row.get("delta_rank"),
                current_price: parse_f64(row.get::<String, _>("current_price").as_str())?,
                price_change_pct: parse_f64(row.get::<String, _>("price_change_pct").as_str())?,
            })
        })
        .collect()
}
/// 封装事件sourcesql，减少回测策略调用方重复实现相同细节。
fn event_source_sql(args: &MarketVelocityEventBacktestArgs) -> &'static str {
    match args.event_source {
        MarketVelocityEventSource::Episodes => {
            r#"
            SELECT
              id::bigint AS id,
              lower(exchange) AS exchange,
              upper(symbol) AS symbol,
              floor(extract(epoch from started_at) * 1000)::bigint AS detected_ms,
              started_at::text AS detected_at,
              COALESCE(best_new_rank, latest_new_rank)::int AS new_rank,
              COALESCE(max_delta_rank, latest_delta_rank)::int AS delta_rank,
              current_price::text AS current_price,
              COALESCE(price_change_pct, 0)::text AS price_change_pct
            FROM market_velocity_episodes
            WHERE upper(symbol) = ANY($1)
              AND event_type IN ('rank_velocity', 'top_entry')
              AND status IN ('active', 'closed')
              AND COALESCE(max_delta_rank, latest_delta_rank, 0) >= $2
              AND ($3::int IS NULL OR COALESCE(max_delta_rank, latest_delta_rank, 0) <= $3)
              AND (
                ($6 = 'long' AND lower(price_direction) = 'up')
                OR ($6 = 'short' AND lower(price_direction) = 'down')
                OR ($6 = 'both' AND lower(price_direction) IN ('up', 'down'))
              )
              AND current_price IS NOT NULL
              AND ($4::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $4)
              AND ($5::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) <= $5)
              AND ($7::bigint IS NULL OR started_at >= to_timestamp($7::double precision / 1000.0))
              AND ($8::bigint IS NULL OR started_at <= to_timestamp($8::double precision / 1000.0))
            ORDER BY started_at, id
            "#
        }
        MarketVelocityEventSource::RawEvents => {
            r#"
            SELECT
              id::bigint AS id,
              lower(exchange) AS exchange,
              upper(symbol) AS symbol,
              floor(extract(epoch from detected_at) * 1000)::bigint AS detected_ms,
              detected_at::text AS detected_at,
              new_rank::int AS new_rank,
              delta_rank::int AS delta_rank,
              current_price::text AS current_price,
              COALESCE(price_change_pct, 0)::text AS price_change_pct
            FROM market_rank_events
            WHERE upper(symbol) = ANY($1)
              AND event_type = 'rank_velocity'
              AND COALESCE(timeframe, '') = '15分钟'
              AND delta_rank >= $2
              AND ($3::int IS NULL OR delta_rank <= $3)
              AND (
                ($6 = 'long' AND lower(price_direction) = 'up')
                OR ($6 = 'short' AND lower(price_direction) = 'down')
                OR ($6 = 'both' AND lower(price_direction) IN ('up', 'down'))
              )
              AND current_price IS NOT NULL
              AND ($4::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $4)
              AND ($5::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) <= $5)
              AND ($7::bigint IS NULL OR detected_at >= to_timestamp($7::double precision / 1000.0))
              AND ($8::bigint IS NULL OR detected_at <= to_timestamp($8::double precision / 1000.0))
            ORDER BY detected_at, id
            "#
        }
        MarketVelocityEventSource::RawState => {
            r#"
            WITH filtered AS (
              SELECT
                id,
                exchange,
                symbol,
                detected_at,
                new_rank,
                delta_rank,
                current_price,
                price_change_pct,
                floor(extract(epoch from detected_at) / 900) AS detected_15m_bucket
              FROM market_rank_events
              WHERE upper(symbol) = ANY($1)
                AND event_type = 'rank_velocity'
                AND COALESCE(timeframe, '') = '15分钟'
                AND delta_rank >= $2
                AND ($3::int IS NULL OR delta_rank <= $3)
                AND (
                  ($6 = 'long' AND lower(price_direction) = 'up')
                  OR ($6 = 'short' AND lower(price_direction) = 'down')
                  OR ($6 = 'both' AND lower(price_direction) IN ('up', 'down'))
                )
                AND current_price IS NOT NULL
                AND ($4::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) >= $4)
                AND ($5::double precision IS NULL OR ABS(COALESCE(price_change_pct, 0)) <= $5)
                AND ($7::bigint IS NULL OR detected_at >= to_timestamp($7::double precision / 1000.0))
                AND ($8::bigint IS NULL OR detected_at <= to_timestamp($8::double precision / 1000.0))
            )
            SELECT *
            FROM (
              SELECT DISTINCT ON (upper(symbol), detected_15m_bucket)
                id::bigint AS id,
                lower(exchange) AS exchange,
                upper(symbol) AS symbol,
                floor(extract(epoch from detected_at) * 1000)::bigint AS detected_ms,
                detected_at::text AS detected_at,
                new_rank::int AS new_rank,
                delta_rank::int AS delta_rank,
                current_price::text AS current_price,
                COALESCE(price_change_pct, 0)::text AS price_change_pct
              FROM filtered
              ORDER BY upper(symbol), detected_15m_bucket, detected_at, id
            ) deduped
            ORDER BY detected_ms, id
            "#
        }
        MarketVelocityEventSource::Kline15m => "SELECT 1::bigint AS id WHERE false",
    }
}
/// 从已抽样的 15m K 线表直接生成 synthetic radar events，用来验证信号逻辑本身。
async fn load_kline_15m_events(
    pool: &PgPool,
    symbols: &[String],
    args: &MarketVelocityEventBacktestArgs,
) -> Result<Vec<RadarEvent>> {
    let mut events = Vec::new();
    for symbol in symbols {
        let table_name = format!("{}_candles_15m", symbol.to_ascii_lowercase());
        let query = kline_15m_events_sql(&table_name);
        let rows = sqlx::query(&query)
            .bind(args.event_start_ms)
            .bind(args.event_end_ms)
            .bind(args.min_price_change_pct)
            .bind(args.max_price_change_pct)
            .bind(args.trade_direction.label())
            .bind(args.entry_defer_bearish_continuation)
            .bind(args.entry_defer_bullish_continuation)
            .fetch_all(pool)
            .await
            .with_context(|| format!("load synthetic 15m kline events from {table_name}"))?;
        for row in rows {
            let detected_ms: i64 = row.get("detected_ms");
            let open = parse_f64(row.get::<String, _>("open_price").as_str())?;
            let close = parse_f64(row.get::<String, _>("current_price").as_str())?;
            let price_change_pct = kline_event_price_change_pct(open, close, args);
            events.push(RadarEvent {
                id: synthetic_kline_event_id(symbol, detected_ms),
                exchange: "okx".to_string(),
                symbol: symbol.to_string(),
                ts: detected_ms,
                detected_at: row.get("detected_at"),
                new_rank: 0,
                delta_rank: 0,
                current_price: close,
                price_change_pct,
            });
        }
    }
    events.sort_by_key(|event| (event.ts, event.id));
    Ok(events)
}
/// 生成单个 15m K 线表的 synthetic event 查询；表名必须先经过 identifier quoting。
fn kline_15m_events_sql(table_name: &str) -> String {
    let table_name = quote_identifier(table_name);
    format!(
        r#"
        SELECT
          ts + 900000 AS detected_ms,
          to_timestamp((ts + 900000)::double precision / 1000.0)::text AS detected_at,
          o::text AS open_price,
          c::text AS current_price
        FROM {table_name}
        WHERE ($1::bigint IS NULL OR ts + 900000 >= $1)
          AND ($2::bigint IS NULL OR ts + 900000 <= $2)
          AND confirm = '1'
          AND o::double precision > 0
          AND (
            ($5 = 'long' AND (($6 = false AND c::double precision > o::double precision) OR ($6 = true AND c::double precision <> o::double precision)))
            OR ($5 = 'short' AND (($7 = false AND c::double precision < o::double precision) OR ($7 = true AND c::double precision <> o::double precision)))
            OR ($5 = 'both' AND c::double precision <> o::double precision)
          )
          AND ($3::double precision IS NULL OR CASE
            WHEN $5 = 'short' AND $7 = true THEN ABS((c::double precision - o::double precision) / o::double precision * 100.0)
            WHEN $5 = 'short' THEN (o::double precision - c::double precision) / o::double precision * 100.0
            WHEN $5 = 'long' AND $6 = true THEN ABS((c::double precision - o::double precision) / o::double precision * 100.0)
            WHEN $5 = 'both' THEN ABS((c::double precision - o::double precision) / o::double precision * 100.0)
            ELSE (c::double precision - o::double precision) / o::double precision * 100.0
          END >= $3)
          AND ($4::double precision IS NULL OR CASE
            WHEN $5 = 'short' AND $7 = true THEN ABS((c::double precision - o::double precision) / o::double precision * 100.0)
            WHEN $5 = 'short' THEN (o::double precision - c::double precision) / o::double precision * 100.0
            WHEN $5 = 'long' AND $6 = true THEN ABS((c::double precision - o::double precision) / o::double precision * 100.0)
            WHEN $5 = 'both' THEN ABS((c::double precision - o::double precision) / o::double precision * 100.0)
            ELSE (c::double precision - o::double precision) / o::double precision * 100.0
          END <= $4)
        ORDER BY ts
        "#
    )
}
/// 按研究方向给 synthetic event 写入方向性，后续复用既有 long/short 入口评估。
fn kline_event_price_change_pct(
    open: f64,
    close: f64,
    args: &MarketVelocityEventBacktestArgs,
) -> f64 {
    let raw = valid_open_price(open)
        .then_some((close - open) / open * 100.0)
        .unwrap_or(0.0);
    match args.trade_direction {
        super::MarketVelocityTradeDirection::Long => raw.abs(),
        super::MarketVelocityTradeDirection::Short => -raw.abs(),
        super::MarketVelocityTradeDirection::Both => raw,
    }
}
fn valid_open_price(open: f64) -> bool {
    open.is_finite() && open > 0.0
}
fn synthetic_kline_event_id(symbol: &str, detected_ms: i64) -> i64 {
    let mut hash = 17_i64;
    for byte in symbol.as_bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(i64::from(*byte));
    }
    let symbol_component = (hash.unsigned_abs() % 1_000_000) as i64;
    symbol_component
        .saturating_mul(10_000_000)
        .saturating_add((detected_ms / MS_15M).rem_euclid(10_000_000))
}
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
fn parse_f64(value: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .with_context(|| format!("parse numeric value {value}"))
}
fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{
        parse_cli_args_from, FvgEntryMode, MarketVelocityEventBacktestArgs,
        MarketVelocityEventSource, MarketVelocityTrendTimeframe, MS_15M, MS_1H, MS_4H,
    };
    #[test]
    fn episode_event_source_reads_episode_table() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::Episodes,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let sql = event_source_sql(&args);
        assert!(sql.contains("FROM market_velocity_episodes"));
        assert!(!sql.contains("FROM market_rank_events"));
    }
    #[test]
    fn raw_event_source_keeps_legacy_rank_event_table() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::RawEvents,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let sql = event_source_sql(&args);
        assert!(sql.contains("FROM market_rank_events"));
    }
    #[test]
    fn raw_state_event_source_deduplicates_scanner_hits_by_15m_candle() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::RawState,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let sql = event_source_sql(&args);
        assert!(sql.contains("FROM market_rank_events"));
        assert!(sql.contains("DISTINCT ON (upper(symbol), detected_15m_bucket)"));
        assert!(sql.contains("floor(extract(epoch from detected_at) / 900)"));
        assert!(sql.contains("ORDER BY upper(symbol), detected_15m_bucket, detected_at, id"));
    }
    #[test]
    fn raw_rank_event_sources_only_consume_15m_rank_velocity_events() {
        for event_source in [
            MarketVelocityEventSource::RawEvents,
            MarketVelocityEventSource::RawState,
        ] {
            let args = MarketVelocityEventBacktestArgs {
                event_source,
                ..MarketVelocityEventBacktestArgs::default()
            };
            let candidate_sql = candidate_symbols_sql(&args);
            let event_sql = event_source_sql(&args);

            assert!(candidate_sql.contains("event_type = 'rank_velocity'"));
            assert!(event_sql.contains("event_type = 'rank_velocity'"));
            assert!(candidate_sql.contains("COALESCE(timeframe, '') = '15分钟'"));
            assert!(event_sql.contains("COALESCE(timeframe, '') = '15分钟'"));
            assert!(!candidate_sql.contains("event_type IN ('rank_velocity', 'top_entry')"));
            assert!(!event_sql.contains("event_type IN ('rank_velocity', 'top_entry')"));
        }
    }
    #[test]
    fn kline_15m_event_source_reads_candle_tables_without_market_rank_events() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::Kline15m,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let candidate_sql = candidate_symbols_sql(&args);
        let event_sql = kline_15m_events_sql("btc-usdt-swap_candles_15m");

        assert!(candidate_sql.contains("information_schema.tables"));
        assert!(candidate_sql.contains("LIKE '%\\_candles\\_15m' ESCAPE '\\'"));
        assert!(candidate_sql.contains("ORDER BY md5($9::text || ':' || candidates.symbol)"));
        assert!(candidate_sql.contains("LIMIT $8"));
        assert!(!candidate_sql.contains("market_rank_events"));
        assert!(event_sql.contains("ts + 900000 AS detected_ms"));
        assert!(event_sql.contains("confirm = '1'"));
        assert!(!event_sql.contains("market_rank_events"));
    }
    #[test]
    fn current_live_kline_universe_excludes_deleted_contracts_without_rank_events() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::Kline15m,
            kline_current_live_only: true,
            trend_timeframe: MarketVelocityTrendTimeframe::Off,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let candidate_sql = candidate_symbols_sql(&args);

        assert!(candidate_sql.contains("JOIN exchange_symbols exchange_symbol"));
        assert!(candidate_sql.contains("exchange_symbol.status = 'live'"));
        assert!(candidate_sql.contains("exchange_symbol.contract_type = 'linear'"));
        assert!(candidate_sql.contains("LIKE '%-USDT-SWAP'"));
        assert!(!candidate_sql.contains("market_rank_events"));
    }
    #[test]
    fn kline_15m_event_source_requires_real_4h_table_when_4h_trend_is_enabled() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::Kline15m,
            trend_timeframe: MarketVelocityTrendTimeframe::FourHour,
            fvg_entry_mode: FvgEntryMode::Off,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let candidate_sql = candidate_symbols_sql(&args);

        assert!(candidate_sql.contains("JOIN information_schema.tables t4"));
        assert!(candidate_sql.contains("t4.table_name AS candles_4h"));
        assert!(
            !candidate_sql.contains("COALESCE(t4.table_name, candidates.candles_15m)"),
            "4h trend backtests must fail closed when the 4h table is missing"
        );
    }
    #[test]
    fn kline_15m_cli_default_allows_missing_4h_table_when_trend_not_requested() {
        let args = parse_cli_args_from(["--event-source", "kline_15m"]).unwrap();
        let candidate_sql = candidate_symbols_sql(&args);

        assert!(candidate_sql.contains("candidates.candles_15m AS candles_4h"));
        assert!(!candidate_sql.contains("JOIN information_schema.tables t4"));
    }
    #[test]
    fn kline_15m_event_source_allows_missing_4h_table_when_4h_data_is_unused() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::Kline15m,
            trend_timeframe: MarketVelocityTrendTimeframe::Off,
            fvg_entry_mode: FvgEntryMode::Off,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let candidate_sql = candidate_symbols_sql(&args);

        assert!(candidate_sql.contains("candidates.candles_15m AS candles_4h"));
        assert!(!candidate_sql.contains("JOIN information_schema.tables t4"));
    }
    #[test]
    fn kline_15m_event_source_filters_completed_candles_by_trade_direction() {
        let event_sql = kline_15m_events_sql("btc-usdt-swap_candles_15m");

        assert!(event_sql.contains("$6 = false AND c::double precision > o::double precision"));
        assert!(event_sql.contains("$6 = true AND c::double precision <> o::double precision"));
        assert!(event_sql.contains("$7 = false AND c::double precision < o::double precision"));
        assert!(event_sql.contains("$7 = true AND c::double precision <> o::double precision"));
        assert!(event_sql.contains("$5 = 'both' AND c::double precision <> o::double precision"));
        assert!(event_sql.contains("$5 = 'long' AND $6 = true THEN ABS"));
        assert!(event_sql.contains("$5 = 'short' AND $7 = true THEN ABS"));
    }
    #[test]
    fn candle_loader_skips_1h_when_trend_and_fvg_do_not_need_it() {
        let args = MarketVelocityEventBacktestArgs {
            trend_timeframe: MarketVelocityTrendTimeframe::Off,
            fvg_entry_mode: FvgEntryMode::Off,
            ..MarketVelocityEventBacktestArgs::default()
        };

        assert!(!should_load_1h_candles(&args));
    }
    #[test]
    fn candle_loader_reads_1h_for_1h_trend_or_cross_timeframe_fvg() {
        let one_hour_trend = MarketVelocityEventBacktestArgs {
            trend_timeframe: MarketVelocityTrendTimeframe::OneHour,
            fvg_entry_mode: FvgEntryMode::Off,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let fvg_15m_to_1h = MarketVelocityEventBacktestArgs {
            trend_timeframe: MarketVelocityTrendTimeframe::FourHour,
            fvg_entry_mode: FvgEntryMode::M15To1h,
            ..MarketVelocityEventBacktestArgs::default()
        };

        assert!(should_load_1h_candles(&one_hour_trend));
        assert!(should_load_1h_candles(&fvg_15m_to_1h));
    }
    #[test]
    fn candle_loader_skips_4h_when_trend_and_fvg_do_not_need_it() {
        let args = MarketVelocityEventBacktestArgs {
            trend_timeframe: MarketVelocityTrendTimeframe::Off,
            fvg_entry_mode: FvgEntryMode::Off,
            ..MarketVelocityEventBacktestArgs::default()
        };

        assert!(!should_load_4h_candles(&args));
    }
    #[test]
    fn candle_loader_reads_4h_for_4h_trend_or_1h_to_4h_fvg() {
        let four_hour_trend = MarketVelocityEventBacktestArgs {
            trend_timeframe: MarketVelocityTrendTimeframe::FourHour,
            fvg_entry_mode: FvgEntryMode::Off,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let fvg_1h_to_4h = MarketVelocityEventBacktestArgs {
            trend_timeframe: MarketVelocityTrendTimeframe::Off,
            fvg_entry_mode: FvgEntryMode::H1To4h,
            ..MarketVelocityEventBacktestArgs::default()
        };

        assert!(should_load_4h_candles(&four_hour_trend));
        assert!(should_load_4h_candles(&fvg_1h_to_4h));
    }
    #[test]
    fn raw_state_event_source_filters_by_max_price_change_pct() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::RawState,
            max_price_change_pct: Some(15.0),
            ..MarketVelocityEventBacktestArgs::default()
        };
        let candidate_sql = candidate_symbols_sql(&args);
        let event_sql = event_source_sql(&args);
        assert!(candidate_sql.contains("ABS(COALESCE(price_change_pct, 0)) <= $4"));
        assert!(event_sql.contains("ABS(COALESCE(price_change_pct, 0)) <= $5"));
    }
    #[test]
    fn raw_state_event_source_filters_by_event_time_window() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::RawState,
            event_start_ms: Some(1717200000000),
            event_end_ms: Some(1719791999999),
            ..MarketVelocityEventBacktestArgs::default()
        };
        let candidate_sql = candidate_symbols_sql(&args);
        let event_sql = event_source_sql(&args);
        assert!(candidate_sql.contains(
            "($6::bigint IS NULL OR detected_at >= to_timestamp($6::double precision / 1000.0))"
        ));
        assert!(candidate_sql.contains(
            "($7::bigint IS NULL OR detected_at <= to_timestamp($7::double precision / 1000.0))"
        ));
        assert!(event_sql.contains(
            "($7::bigint IS NULL OR detected_at >= to_timestamp($7::double precision / 1000.0))"
        ));
        assert!(event_sql.contains(
            "($8::bigint IS NULL OR detected_at <= to_timestamp($8::double precision / 1000.0))"
        ));
        assert!(!candidate_sql.contains("floor(extract(epoch from detected_at) * 1000)"));
        assert!(!event_sql.contains("floor(extract(epoch from detected_at) * 1000)::bigint >="));
    }
    #[test]
    fn episode_event_source_filters_by_event_time_window() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::Episodes,
            event_start_ms: Some(1717200000000),
            event_end_ms: Some(1719791999999),
            ..MarketVelocityEventBacktestArgs::default()
        };
        let candidate_sql = candidate_symbols_sql(&args);
        let event_sql = event_source_sql(&args);
        assert!(candidate_sql.contains(
            "($6::bigint IS NULL OR started_at >= to_timestamp($6::double precision / 1000.0))"
        ));
        assert!(candidate_sql.contains(
            "($7::bigint IS NULL OR started_at <= to_timestamp($7::double precision / 1000.0))"
        ));
        assert!(event_sql.contains(
            "($7::bigint IS NULL OR started_at >= to_timestamp($7::double precision / 1000.0))"
        ));
        assert!(event_sql.contains(
            "($8::bigint IS NULL OR started_at <= to_timestamp($8::double precision / 1000.0))"
        ));
        assert!(!candidate_sql.contains("floor(extract(epoch from started_at) * 1000)"));
        assert!(!event_sql.contains("floor(extract(epoch from started_at) * 1000)::bigint >="));
    }
    #[test]
    fn raw_event_source_does_not_filter_by_new_rank() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::RawEvents,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let candidate_sql = candidate_symbols_sql(&args);
        let event_sql = event_source_sql(&args);
        assert!(!candidate_sql.contains("new_rank BETWEEN"));
        assert!(!event_sql.contains("new_rank BETWEEN"));
        assert!(!candidate_sql.contains("new_rank <"));
        assert!(!event_sql.contains("new_rank <"));
    }
    #[test]
    fn episode_event_source_does_not_filter_by_new_rank() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::Episodes,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let candidate_sql = candidate_symbols_sql(&args);
        let event_sql = event_source_sql(&args);
        assert!(!candidate_sql.contains("best_new_rank, latest_new_rank) BETWEEN"));
        assert!(!event_sql.contains("best_new_rank, latest_new_rank) BETWEEN"));
        assert!(!candidate_sql.contains("best_new_rank, latest_new_rank) <"));
        assert!(!event_sql.contains("best_new_rank, latest_new_rank) <"));
    }
    #[test]
    fn episode_event_source_keeps_closed_historical_episodes_in_backtests() {
        let args = MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::Episodes,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let candidate_sql = candidate_symbols_sql(&args);
        let event_sql = event_source_sql(&args);
        assert!(candidate_sql.contains("status IN ('active', 'closed')"));
        assert!(event_sql.contains("status IN ('active', 'closed')"));
        assert!(!candidate_sql.contains("status = 'active'"));
        assert!(!event_sql.contains("status = 'active'"));
    }
    #[test]
    fn candidate_symbols_sql_binds_trade_direction_after_price_filter() {
        let args = MarketVelocityEventBacktestArgs::default();
        let sql = candidate_symbols_sql(&args);
        assert!(sql.contains("$5 = 'long'"));
        assert!(!sql.contains("$9 = 'long'"));
    }
    #[test]
    fn candle_load_window_covers_indicator_warmup_and_outcome_horizon() {
        let args = MarketVelocityEventBacktestArgs {
            entry_period: 20,
            trend_timeframe: MarketVelocityTrendTimeframe::FourHour,
            fvg_entry_mode: FvgEntryMode::Off,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let first_event_ts = 2_000_000_000_000;
        let last_event_ts = first_event_ts + 900_000;
        let events = vec![sample_event(first_event_ts), sample_event(last_event_ts)];

        let (start_ms, end_ms) = candle_load_window_ms(&args, &events).unwrap();

        assert!(start_ms <= first_event_ts - 23 * MS_4H);
        assert!(start_ms <= first_event_ts - 23 * MS_15M);
        assert!(end_ms >= last_event_ts + 48 * 60 * 60 * 1_000 + MS_4H);
    }
    #[test]
    fn candle_load_window_covers_opposite_duration_history() {
        let args = MarketVelocityEventBacktestArgs {
            trend_timeframe: MarketVelocityTrendTimeframe::Off,
            entry_min_opposite_duration_candles: Some(120),
            ..MarketVelocityEventBacktestArgs::default()
        };
        let event_ts = 2_000_000_000_000;

        let (start_ms, _) = candle_load_window_ms(&args, &[sample_event(event_ts)]).unwrap();

        assert!(start_ms <= event_ts - 123 * MS_15M);
    }
    #[test]
    fn candle_load_window_covers_exhaustion_volume_history() {
        let args = MarketVelocityEventBacktestArgs {
            trend_timeframe: MarketVelocityTrendTimeframe::Off,
            entry_min_exhaustion_volume_dominance_ratio: Some(1.0),
            ..MarketVelocityEventBacktestArgs::default()
        };
        let event_ts = 2_000_000_000_000;

        let (start_ms, _) = candle_load_window_ms(&args, &[sample_event(event_ts)]).unwrap();

        assert!(start_ms <= event_ts - 99 * MS_15M);
    }
    #[test]
    fn candle_load_window_covers_btc_broad_direction_history() {
        let args = MarketVelocityEventBacktestArgs {
            trend_timeframe: MarketVelocityTrendTimeframe::Off,
            entry_btc_384_min_directional_net_move_pct: Some(0.0),
            ..MarketVelocityEventBacktestArgs::default()
        };
        let event_ts = 2_000_000_000_000;

        let (start_ms, _) = candle_load_window_ms(&args, &[sample_event(event_ts)]).unwrap();

        assert!(start_ms <= event_ts - 387 * MS_15M);
    }
    #[test]
    fn candle_load_window_covers_fvg_warmup_and_wait_only_when_fvg_enabled() {
        let args = MarketVelocityEventBacktestArgs {
            trend_timeframe: MarketVelocityTrendTimeframe::Off,
            fvg_entry_mode: FvgEntryMode::H1To4h,
            fvg_lookback_candles: 40,
            fvg_max_wait_candles: 24,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let first_event_ts = 2_000_000_000_000;
        let last_event_ts = first_event_ts + 900_000;
        let events = vec![sample_event(first_event_ts), sample_event(last_event_ts)];

        let (start_ms, end_ms) = candle_load_window_ms(&args, &events).unwrap();

        assert!(start_ms <= first_event_ts - 43 * MS_4H);
        assert!(end_ms >= last_event_ts + 48 * 60 * 60 * 1_000 + 24 * MS_1H + MS_4H);
    }
    #[test]
    fn candle_load_window_skips_unused_4h_warmup_for_fast_15m_backtests() {
        let args = MarketVelocityEventBacktestArgs {
            entry_period: 20,
            trend_timeframe: MarketVelocityTrendTimeframe::Off,
            fvg_entry_mode: FvgEntryMode::Off,
            fvg_lookback_candles: 40,
            fvg_max_wait_candles: 24,
            entry_retest_after_signal: false,
            entry_retest_max_wait_candles: 8,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let first_event_ts = 2_000_000_000_000;
        let last_event_ts = first_event_ts + 900_000;
        let events = vec![sample_event(first_event_ts), sample_event(last_event_ts)];

        let (start_ms, end_ms) = candle_load_window_ms(&args, &events).unwrap();

        assert_eq!(start_ms, first_event_ts - 96 * MS_15M);
        assert_eq!(end_ms, last_event_ts + 48 * 60 * 60 * 1_000 + MS_15M);
    }
    #[test]
    fn candle_load_window_covers_deferred_reversal_confirmation_and_entry() {
        let args = MarketVelocityEventBacktestArgs {
            trend_timeframe: MarketVelocityTrendTimeframe::Off,
            entry_defer_bearish_continuation: true,
            entry_defer_max_wait_candles: 3,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let event_ts = 2_000_000_000_000;
        let events = vec![sample_event(event_ts)];

        let (_, end_ms) = candle_load_window_ms(&args, &events).unwrap();

        assert_eq!(end_ms, event_ts + 48 * 60 * 60 * 1_000 + 5 * MS_15M);
    }
    #[test]
    fn candle_load_window_returns_none_without_events() {
        let args = MarketVelocityEventBacktestArgs::default();

        assert!(candle_load_window_ms(&args, &[]).is_none());
    }
    fn sample_event(ts: i64) -> RadarEvent {
        RadarEvent {
            id: ts,
            exchange: "okx".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            ts,
            detected_at: "2033-05-18T03:33:20Z".to_string(),
            new_rank: 1,
            delta_rank: 11,
            current_price: 100.0,
            price_change_pct: 4.0,
        }
    }
}
