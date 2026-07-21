use super::args::{RankActivationSource, RankPriceDirection, RankUniverse};
use super::{quoted_4h_candle_table, Args, CandidateTrade};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashSet, VecDeque};

const FOUR_HOURS_MS: i64 = 4 * 60 * 60 * 1000;
const ROLLING_VOLUME_BARS: usize = 6;

/// 使用本地已确认 4H K 线重建的横截面成交额排名激活审计。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct RankActivationReport {
    /// 动量事件来源；重建代理与归档生产雷达使用不同标签，禁止混淆口径。
    proxy: &'static str,
    /// 事件源可用历史的首个事件时刻，Unix 毫秒；重建代理则为加载到的首个 4H 快照。
    source_event_start_ts: Option<i64>,
    /// 事件源可用历史的最后事件时刻，Unix 毫秒；用于暴露归档覆盖边界。
    source_event_end_ts: Option<i64>,
    /// 横截面币池口径：回测交易对或本地全部可用 4H 交易对。
    universe_scope: &'static str,
    /// 参与横截面排名的交易对数量；本地分表口径仍会保留幸存者偏差。
    universe_symbols: usize,
    /// 成功形成横截面排名的 4H 时点数量。
    rank_snapshots: usize,
    /// 单个排名时点实际覆盖的最少交易对数量。
    min_symbols_per_snapshot: usize,
    /// 单个排名时点实际覆盖的最多交易对数量。
    max_symbols_per_snapshot: usize,
    /// 达到排名跃升阈值的历史事件数。
    activation_events: usize,
    /// 至少出现过一次排名激活事件的交易对数量。
    symbols_with_events: usize,
    /// 排名比较回看的 4H K 线根数。
    lookback_bars: usize,
    /// 排名比较周期内绝对价格涨跌幅下界，单位百分比。
    min_price_change_pct: Option<f64>,
    /// 排名比较周期内绝对价格涨跌幅上界，单位百分比。
    max_price_change_pct: Option<f64>,
    /// 排名比较周期要求的价格冲击方向。
    price_direction: &'static str,
    /// 排名至少向前跃升的名次数。
    min_delta: i32,
    /// 排名跃升上限；None 表示不排除极端跃升。
    max_delta: Option<i32>,
    /// 事件后允许 Vegas 入场的最长 4H K 线根数。
    valid_for_bars: usize,
    /// 事件后至少等待的完整 4H K 线根数。
    min_wait_bars: usize,
    /// 可选 RSI 下界，包含边界。
    min_rsi: Option<f64>,
    /// 可选 RSI 上界，不包含边界。
    max_rsi: Option<f64>,
    /// 应用排名代理前的完整 Vegas 候选交易数。
    raw_candidate_trades: usize,
    /// 入场时刻实际落在事件源首尾覆盖区间内的原始 Vegas 候选数。
    raw_candidate_trades_in_source_coverage: usize,
    /// 已在质量基线回测中出现、无需再次接受排名过滤的交易数。
    passthrough_trades: usize,
    /// 非基线交易中落入历史排名激活窗口的候选交易数。
    rank_eligible_trades: usize,
    /// 继续通过可选 RSI 区间的最终候选交易数。
    final_eligible_trades: usize,
    /// 最终候选中入场时刻实际落在事件源首尾覆盖区间内的交易数。
    final_eligible_trades_in_source_coverage: usize,
    /// 最终非基线候选距离最近一次激活事件的 4H K 线年龄分布。
    final_activation_age_histogram_bars: BTreeMap<usize, usize>,
    /// 最终非基线候选激活年龄的最小值，单位 4H K 线根数。
    final_activation_age_min_bars: Option<usize>,
    /// 最终非基线候选激活年龄的中位数，单位 4H K 线根数。
    final_activation_age_p50_bars: Option<usize>,
    /// 最终非基线候选激活年龄的第 90 百分位，单位 4H K 线根数。
    final_activation_age_p90_bars: Option<usize>,
    /// 最终非基线候选激活年龄的最大值，单位 4H K 线根数。
    final_activation_age_max_bars: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct RankActivationConfig {
    /// 排名至少向前跃升的名次数。
    min_delta: i32,
    /// 排名跃升上限；None 表示不限制。
    max_delta: Option<i32>,
    /// 排名比较回看的 4H K 线根数。
    lookback_bars: usize,
    /// 排名比较周期内绝对价格涨跌幅下界，单位百分比。
    min_price_change_pct: Option<f64>,
    /// 排名比较周期内绝对价格涨跌幅上界，单位百分比。
    max_price_change_pct: Option<f64>,
    /// 排名比较周期要求的价格冲击方向。
    price_direction: RankPriceDirection,
    /// 排名事件允许激活入场的 4H K 线根数。
    valid_for_bars: usize,
    /// 排名事件后至少等待的完整 4H K 线根数。
    min_wait_bars: usize,
    /// 可选 RSI 下界，包含边界。
    min_rsi: Option<f64>,
    /// 可选 RSI 上界，不包含边界。
    max_rsi: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct QuoteVolumeBar {
    /// 4H K 线开盘时刻，Unix 毫秒时间戳。
    ts: i64,
    /// 使用收盘价乘基础币成交量估算的单根 K 线计价币成交额，单位 USDT。
    quote_volume: f64,
}

#[derive(Debug, Clone, Copy)]
struct CrossSectionPoint {
    symbol_index: usize,
    quote_volume: f64,
    close: f64,
}

#[derive(Debug, Clone)]
struct HistoricalRankSnapshot {
    ranks: Vec<Option<i32>>,
    closes: Vec<Option<f64>>,
}

struct ReconstructedEvents {
    /// 按所选横截面币池索引保存的排名激活时刻，Unix 毫秒时间戳且升序。
    by_symbol: Vec<Vec<i64>>,
    /// 成功构建的 4H 横截面排名数量。
    snapshot_count: usize,
    /// 单个横截面排名包含的最少交易对数量。
    min_symbols_per_snapshot: usize,
    /// 单个横截面排名包含的最多交易对数量。
    max_symbols_per_snapshot: usize,
    /// 全部交易对触发的排名激活事件总数。
    event_count: usize,
    /// 事件源参与排名的交易对数量。
    universe_symbols: usize,
    /// 事件源可用历史的首个时刻，Unix 毫秒。
    source_start_ts: Option<i64>,
    /// 事件源可用历史的最后时刻，Unix 毫秒。
    source_end_ts: Option<i64>,
}

/// 在单账户容量回放之前应用历史横截面排名代理；未配置时完全保留旧行为。
pub(super) async fn apply_rank_activation(
    pool: &PgPool,
    rank_event_pool: Option<&PgPool>,
    trades: &mut Vec<CandidateTrade>,
    args: Args,
) -> Result<Option<RankActivationReport>> {
    let Some(min_delta) = args.rank_activation_min_delta else {
        return Ok(None);
    };
    let config = RankActivationConfig {
        min_delta,
        max_delta: args.rank_activation_max_delta,
        lookback_bars: args.rank_activation_lookback_bars,
        min_price_change_pct: args.rank_activation_min_price_change_pct,
        max_price_change_pct: args.rank_activation_max_price_change_pct,
        price_direction: args.rank_activation_price_direction,
        valid_for_bars: args.rank_activation_valid_for_bars,
        min_wait_bars: args.rank_activation_min_wait_bars,
        min_rsi: args.rank_activation_min_rsi,
        max_rsi: args.rank_activation_max_rsi,
    };
    let symbols = load_universe_symbols(pool, args).await?;
    if symbols.is_empty() {
        bail!("rank activation requires at least one backtest symbol");
    }
    let raw_candidate_trades = trades.len();
    let passthrough_entries = load_passthrough_entries(pool, args).await?;
    let passthrough_trades = trades
        .iter()
        .filter(|trade| passthrough_entries.contains(&trade_entry_key(trade)))
        .count();
    let Some(first_open_ts) = trades.iter().map(|trade| trade.open_ts).min() else {
        return Ok(Some(empty_report(
            symbols.len(),
            args.rank_universe,
            args.rank_activation_source,
            passthrough_trades,
            config,
        )));
    };
    let last_open_ts = trades
        .iter()
        .map(|trade| trade.open_ts)
        .max()
        .expect("non-empty candidate trades");
    let history_bars = config
        .lookback_bars
        .saturating_add(config.valid_for_bars)
        .saturating_add(ROLLING_VOLUME_BARS);
    let history_ms = i64::try_from(history_bars)
        .unwrap_or(i64::MAX / FOUR_HOURS_MS)
        .saturating_mul(FOUR_HOURS_MS);
    let start_ts = first_open_ts.saturating_sub(history_ms);
    let events = match args.rank_activation_source {
        RankActivationSource::Reconstructed4h => {
            let snapshots =
                load_rank_snapshot_inputs(pool, &symbols, start_ts, last_open_ts).await?;
            reconstruct_events(&snapshots, symbols.len(), config)
        }
        RankActivationSource::MarketRankEvents => {
            let rank_event_pool = rank_event_pool
                .context("market-rank-events activation requires MARKET_RANK_DATABASE_URL")?;
            load_market_rank_events(rank_event_pool, &symbols, start_ts, last_open_ts, config)
                .await?
        }
    };
    let raw_candidate_trades_in_source_coverage = trades
        .iter()
        .filter(|trade| {
            timestamp_in_source_coverage(
                trade.open_ts,
                events.source_start_ts,
                events.source_end_ts,
            )
        })
        .count();

    let rank_eligible_trades = trades
        .iter()
        .filter(|trade| {
            if passthrough_entries.contains(&trade_entry_key(trade)) {
                return false;
            }
            symbols
                .binary_search(&trade.symbol)
                .ok()
                .is_some_and(|index| {
                    matching_event_age_bars(&events.by_symbol[index], trade.open_ts, config)
                        .is_some()
                })
        })
        .count();
    trades.retain(|trade| {
        if passthrough_entries.contains(&trade_entry_key(trade)) {
            return true;
        }
        let Ok(index) = symbols.binary_search(&trade.symbol) else {
            return false;
        };
        matching_event_age_bars(&events.by_symbol[index], trade.open_ts, config).is_some()
            && rsi_allowed(trade.entry_rsi, config)
    });
    let final_eligible_trades_in_source_coverage = trades
        .iter()
        .filter(|trade| {
            timestamp_in_source_coverage(
                trade.open_ts,
                events.source_start_ts,
                events.source_end_ts,
            )
        })
        .count();
    let final_activation_age_histogram_bars = trades
        .iter()
        .filter(|trade| !passthrough_entries.contains(&trade_entry_key(trade)))
        .filter_map(|trade| {
            let index = symbols.binary_search(&trade.symbol).ok()?;
            matching_event_age_bars(&events.by_symbol[index], trade.open_ts, config)
        })
        .fold(BTreeMap::new(), |mut histogram, age| {
            *histogram.entry(age).or_insert(0) += 1;
            histogram
        });

    Ok(Some(RankActivationReport {
        proxy: args.rank_activation_source.report_label(),
        source_event_start_ts: events.source_start_ts,
        source_event_end_ts: events.source_end_ts,
        universe_scope: args.rank_universe.report_label(),
        universe_symbols: events.universe_symbols,
        rank_snapshots: events.snapshot_count,
        min_symbols_per_snapshot: events.min_symbols_per_snapshot,
        max_symbols_per_snapshot: events.max_symbols_per_snapshot,
        activation_events: events.event_count,
        symbols_with_events: events
            .by_symbol
            .iter()
            .filter(|timestamps| !timestamps.is_empty())
            .count(),
        lookback_bars: config.lookback_bars,
        min_price_change_pct: config.min_price_change_pct,
        max_price_change_pct: config.max_price_change_pct,
        price_direction: config.price_direction.report_label(),
        min_delta: config.min_delta,
        max_delta: config.max_delta,
        valid_for_bars: config.valid_for_bars,
        min_wait_bars: config.min_wait_bars,
        min_rsi: config.min_rsi,
        max_rsi: config.max_rsi,
        raw_candidate_trades,
        raw_candidate_trades_in_source_coverage,
        passthrough_trades,
        rank_eligible_trades,
        final_eligible_trades: trades.len(),
        final_eligible_trades_in_source_coverage,
        final_activation_age_min_bars: final_activation_age_histogram_bars
            .first_key_value()
            .map(|(age, _)| *age),
        final_activation_age_p50_bars: activation_age_percentile(
            &final_activation_age_histogram_bars,
            50,
        ),
        final_activation_age_p90_bars: activation_age_percentile(
            &final_activation_age_histogram_bars,
            90,
        ),
        final_activation_age_max_bars: final_activation_age_histogram_bars
            .last_key_value()
            .map(|(age, _)| *age),
        final_activation_age_histogram_bars,
    }))
}

fn empty_report(
    universe_symbols: usize,
    universe: RankUniverse,
    source: RankActivationSource,
    passthrough_trades: usize,
    config: RankActivationConfig,
) -> RankActivationReport {
    RankActivationReport {
        proxy: source.report_label(),
        source_event_start_ts: None,
        source_event_end_ts: None,
        universe_scope: universe.report_label(),
        universe_symbols,
        rank_snapshots: 0,
        min_symbols_per_snapshot: 0,
        max_symbols_per_snapshot: 0,
        activation_events: 0,
        symbols_with_events: 0,
        lookback_bars: config.lookback_bars,
        min_price_change_pct: config.min_price_change_pct,
        max_price_change_pct: config.max_price_change_pct,
        price_direction: config.price_direction.report_label(),
        min_delta: config.min_delta,
        max_delta: config.max_delta,
        valid_for_bars: config.valid_for_bars,
        min_wait_bars: config.min_wait_bars,
        min_rsi: config.min_rsi,
        max_rsi: config.max_rsi,
        raw_candidate_trades: 0,
        raw_candidate_trades_in_source_coverage: 0,
        passthrough_trades,
        rank_eligible_trades: 0,
        final_eligible_trades: 0,
        final_eligible_trades_in_source_coverage: 0,
        final_activation_age_histogram_bars: BTreeMap::new(),
        final_activation_age_min_bars: None,
        final_activation_age_p50_bars: None,
        final_activation_age_p90_bars: None,
        final_activation_age_max_bars: None,
    }
}

type TradeEntryKey = (String, i64, String);

fn trade_entry_key(trade: &CandidateTrade) -> TradeEntryKey {
    (trade.symbol.clone(), trade.open_ts, trade.side.clone())
}

async fn load_passthrough_entries(pool: &PgPool, args: Args) -> Result<HashSet<TradeEntryKey>> {
    let (Some(id_min), Some(id_max)) = (args.rank_passthrough_id_min, args.rank_passthrough_id_max)
    else {
        return Ok(HashSet::new());
    };
    let rows = sqlx::query(
        "SELECT DISTINCT inst_id AS symbol, option_type AS side, \
         (EXTRACT(EPOCH FROM (open_position_time AT TIME ZONE 'Asia/Shanghai')) * 1000)::bigint \
             AS open_ts \
         FROM back_test_detail \
         WHERE back_test_id BETWEEN $1 AND $2 \
           AND option_type IN ('long', 'short')",
    )
    .bind(id_min)
    .bind(id_max)
    .fetch_all(pool)
    .await
    .context("load quality-baseline passthrough entries")?;
    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("symbol")?,
                row.try_get::<i64, _>("open_ts")?,
                row.try_get::<String, _>("side")?,
            ))
        })
        .collect()
}

async fn load_universe_symbols(pool: &PgPool, args: Args) -> Result<Vec<String>> {
    match args.rank_universe {
        RankUniverse::Backtest => {
            let rows = sqlx::query(
                "SELECT DISTINCT inst_type FROM back_test_log \
                 WHERE id BETWEEN $1 AND $2 ORDER BY inst_type",
            )
            .bind(args.backtest_id_min)
            .bind(args.backtest_id_max)
            .fetch_all(pool)
            .await
            .context("load backtest rank activation universe")?;
            rows.into_iter()
                .map(|row| {
                    row.try_get("inst_type")
                        .context("read backtest rank universe symbol")
                })
                .collect()
        }
        RankUniverse::AllAvailable4h => {
            let rows = sqlx::query(
                "SELECT tablename FROM pg_catalog.pg_tables \
                 WHERE schemaname = 'public' \
                   AND right(tablename, length('_candles_4h')) = '_candles_4h' \
                 ORDER BY tablename",
            )
            .fetch_all(pool)
            .await
            .context("load all available 4H rank activation symbols")?;
            rows.into_iter()
                .map(|row| {
                    let table_name = row
                        .try_get::<String, _>("tablename")
                        .context("read 4H candle table name")?;
                    symbol_from_4h_table(&table_name)
                })
                .collect()
        }
    }
}

fn symbol_from_4h_table(table_name: &str) -> Result<String> {
    let symbol = table_name
        .strip_suffix("_candles_4h")
        .filter(|value| !value.is_empty())
        .context("invalid 4H candle table name")?;
    Ok(symbol.to_ascii_uppercase())
}

/// 只用连续六根已确认 4H K 线估算滚动 24H 成交额；遇到缺口即重置，避免缺失数据抬高排名。
async fn load_rank_snapshot_inputs(
    pool: &PgPool,
    symbols: &[String],
    start_ts: i64,
    end_ts: i64,
) -> Result<BTreeMap<i64, Vec<CrossSectionPoint>>> {
    let mut snapshots = BTreeMap::<i64, Vec<CrossSectionPoint>>::new();
    for (symbol_index, symbol) in symbols.iter().enumerate() {
        let table_name = quoted_4h_candle_table(symbol)?;
        let query = format!(
            "SELECT ts, c::double precision AS close, \
             vol_ccy::double precision AS base_volume FROM {table_name} \
             WHERE ts BETWEEN $1 AND $2 AND confirm = '1' ORDER BY ts"
        );
        let rows = sqlx::query(&query)
            .bind(start_ts)
            .bind(end_ts)
            .fetch_all(pool)
            .await
            .with_context(|| format!("load 4H quote-volume history for {symbol}"))?;
        let mut rolling = VecDeque::<QuoteVolumeBar>::with_capacity(ROLLING_VOLUME_BARS);
        let mut previous_ts = None;
        for row in rows {
            let ts = row.try_get::<i64, _>("ts")?;
            let close = row.try_get::<f64, _>("close")?;
            let base_volume = row.try_get::<f64, _>("base_volume")?;
            if !close.is_finite() || close <= 0.0 || !base_volume.is_finite() || base_volume < 0.0 {
                bail!("invalid quote-volume candle for {symbol} at {ts}");
            }
            if previous_ts.is_some_and(|previous| ts - previous != FOUR_HOURS_MS) {
                rolling.clear();
            }
            previous_ts = Some(ts);
            rolling.push_back(QuoteVolumeBar {
                ts,
                quote_volume: close * base_volume,
            });
            if rolling.len() > ROLLING_VOLUME_BARS {
                rolling.pop_front();
            }
            if rolling.len() == ROLLING_VOLUME_BARS {
                let quote_volume = rolling.iter().map(|bar| bar.quote_volume).sum::<f64>();
                let snapshot_ts = rolling.back().expect("rolling window is populated").ts;
                snapshots
                    .entry(snapshot_ts)
                    .or_default()
                    .push(CrossSectionPoint {
                        symbol_index,
                        quote_volume,
                        close,
                    });
            }
        }
    }
    Ok(snapshots)
}

/// 在每个 4H 时点先完成全币池排序，再与严格早于当前时点的排名比较，防止使用未来成交量。
fn reconstruct_events(
    snapshots: &BTreeMap<i64, Vec<CrossSectionPoint>>,
    symbol_count: usize,
    config: RankActivationConfig,
) -> ReconstructedEvents {
    let mut ranks_by_ts = BTreeMap::<i64, HistoricalRankSnapshot>::new();
    let mut min_symbols = usize::MAX;
    let mut max_symbols = 0_usize;
    for (ts, points) in snapshots {
        let mut ranked = points.clone();
        ranked.sort_by(|left, right| {
            right
                .quote_volume
                .total_cmp(&left.quote_volume)
                .then_with(|| left.symbol_index.cmp(&right.symbol_index))
        });
        let mut ranks = vec![None; symbol_count];
        let mut closes = vec![None; symbol_count];
        for (position, point) in ranked.iter().enumerate() {
            ranks[point.symbol_index] = i32::try_from(position + 1).ok();
            closes[point.symbol_index] = Some(point.close);
        }
        min_symbols = min_symbols.min(ranked.len());
        max_symbols = max_symbols.max(ranked.len());
        ranks_by_ts.insert(*ts, HistoricalRankSnapshot { ranks, closes });
    }

    let lookback_ms = i64::try_from(config.lookback_bars)
        .unwrap_or(i64::MAX / FOUR_HOURS_MS)
        .saturating_mul(FOUR_HOURS_MS);
    let mut by_symbol = vec![Vec::<i64>::new(); symbol_count];
    let mut event_count = 0_usize;
    for (ts, current_snapshot) in &ranks_by_ts {
        let Some(previous_snapshot) = ranks_by_ts.get(&ts.saturating_sub(lookback_ms)) else {
            continue;
        };
        for symbol_index in 0..symbol_count {
            let (Some(previous), Some(current)) = (
                previous_snapshot.ranks[symbol_index],
                current_snapshot.ranks[symbol_index],
            ) else {
                continue;
            };
            let delta = previous - current;
            if delta >= config.min_delta
                && config
                    .max_delta
                    .map_or(true, |max_delta| delta <= max_delta)
                && price_change_allowed(
                    previous_snapshot.closes[symbol_index],
                    current_snapshot.closes[symbol_index],
                    config,
                )
            {
                by_symbol[symbol_index].push(*ts);
                event_count += 1;
            }
        }
    }
    ReconstructedEvents {
        by_symbol,
        snapshot_count: ranks_by_ts.len(),
        min_symbols_per_snapshot: if ranks_by_ts.is_empty() {
            0
        } else {
            min_symbols
        },
        max_symbols_per_snapshot: max_symbols,
        event_count,
        universe_symbols: symbol_count,
        source_start_ts: ranks_by_ts.first_key_value().map(|(ts, _)| *ts),
        source_end_ts: ranks_by_ts.last_key_value().map(|(ts, _)| *ts),
    }
}

/// 从显式只读归档库加载生产雷达事件；重复扫描事件只刷新激活窗口，不直接生成交易。
async fn load_market_rank_events(
    pool: &PgPool,
    symbols: &[String],
    start_ts: i64,
    end_ts: i64,
    config: RankActivationConfig,
) -> Result<ReconstructedEvents> {
    let metadata = sqlx::query(
        "SELECT COUNT(DISTINCT upper(symbol))::bigint AS universe_symbols, \
         (EXTRACT(EPOCH FROM MIN(detected_at)) * 1000)::bigint AS source_start_ts, \
         (EXTRACT(EPOCH FROM MAX(detected_at)) * 1000)::bigint AS source_end_ts \
         FROM market_rank_events \
         WHERE lower(exchange) = 'okx' \
           AND event_type = 'rank_velocity' \
           AND timeframe = '24小时'",
    )
    .fetch_one(pool)
    .await
    .context("load archived market-rank event coverage")?;
    let universe_symbols = usize::try_from(metadata.try_get::<i64, _>("universe_symbols")?)
        .context("archived rank universe exceeds usize")?;
    let source_start_ts = metadata.try_get::<Option<i64>, _>("source_start_ts")?;
    let source_end_ts = metadata.try_get::<Option<i64>, _>("source_end_ts")?;

    let rows = sqlx::query(
        "SELECT upper(symbol) AS symbol, \
         (EXTRACT(EPOCH FROM detected_at) * 1000)::bigint AS event_ts \
         FROM market_rank_events \
         WHERE lower(exchange) = 'okx' \
           AND event_type = 'rank_velocity' \
           AND timeframe = '24小时' \
           AND upper(symbol) = ANY($1::text[]) \
           AND detected_at BETWEEN to_timestamp($2::double precision / 1000.0) \
                               AND to_timestamp($3::double precision / 1000.0) \
           AND delta_rank >= $4 \
           AND ($5::integer IS NULL OR delta_rank <= $5) \
           AND ($6::double precision IS NULL OR abs(price_change_pct) >= $6) \
           AND ($7::double precision IS NULL OR abs(price_change_pct) <= $7) \
           AND ($8::text = 'any' \
                OR ($8::text = 'up' AND price_change_pct >= 0) \
                OR ($8::text = 'down' AND price_change_pct <= 0)) \
         ORDER BY detected_at, symbol, id",
    )
    .bind(symbols)
    .bind(start_ts)
    .bind(end_ts)
    .bind(config.min_delta)
    .bind(config.max_delta)
    .bind(config.min_price_change_pct)
    .bind(config.max_price_change_pct)
    .bind(config.price_direction.report_label())
    .fetch_all(pool)
    .await
    .context("load archived market-rank activation events")?;

    let mut by_symbol = vec![Vec::<i64>::new(); symbols.len()];
    let mut event_count = 0_usize;
    for row in rows {
        let symbol = row.try_get::<String, _>("symbol")?;
        let event_ts = row.try_get::<i64, _>("event_ts")?;
        let Ok(symbol_index) = symbols.binary_search(&symbol) else {
            continue;
        };
        let timestamps = &mut by_symbol[symbol_index];
        if timestamps.last().copied() != Some(event_ts) {
            timestamps.push(event_ts);
            event_count += 1;
        }
    }

    Ok(ReconstructedEvents {
        by_symbol,
        snapshot_count: 0,
        min_symbols_per_snapshot: 0,
        max_symbols_per_snapshot: 0,
        event_count,
        universe_symbols,
        source_start_ts,
        source_end_ts,
    })
}

fn price_change_allowed(
    previous_close: Option<f64>,
    current_close: Option<f64>,
    config: RankActivationConfig,
) -> bool {
    if config.min_price_change_pct.is_none() && config.max_price_change_pct.is_none() {
        return true;
    }
    let (Some(previous), Some(current)) = (previous_close, current_close) else {
        return false;
    };
    if !previous.is_finite() || previous <= 0.0 || !current.is_finite() || current <= 0.0 {
        return false;
    }
    let signed_change_pct = ((current / previous) - 1.0) * 100.0;
    let comparable_change_pct = match config.price_direction {
        RankPriceDirection::Any => signed_change_pct.abs(),
        RankPriceDirection::Up if signed_change_pct >= 0.0 => signed_change_pct,
        RankPriceDirection::Down if signed_change_pct <= 0.0 => -signed_change_pct,
        RankPriceDirection::Up | RankPriceDirection::Down => return false,
    };
    comparable_change_pct >= config.min_price_change_pct.unwrap_or(0.0)
        && comparable_change_pct <= config.max_price_change_pct.unwrap_or(f64::INFINITY)
}

/// 判断交易入场时刻是否位于事件源明确提供的首尾覆盖区间内；缺失任一边界时保守返回 false。
fn timestamp_in_source_coverage(
    timestamp: i64,
    source_start_ts: Option<i64>,
    source_end_ts: Option<i64>,
) -> bool {
    matches!(
        (source_start_ts, source_end_ts),
        (Some(start), Some(end)) if start <= timestamp && timestamp <= end
    )
}

/// 返回入场窗口中最近一次事件的年龄；最短等待保证不会在排名冲击当根追单。
fn matching_event_age_bars(
    events: &[i64],
    entry_ts: i64,
    config: RankActivationConfig,
) -> Option<usize> {
    let earliest = entry_ts.saturating_sub(
        i64::try_from(config.valid_for_bars)
            .unwrap_or(i64::MAX / FOUR_HOURS_MS)
            .saturating_mul(FOUR_HOURS_MS),
    );
    let latest = entry_ts.saturating_sub(
        i64::try_from(config.min_wait_bars)
            .unwrap_or(i64::MAX / FOUR_HOURS_MS)
            .saturating_mul(FOUR_HOURS_MS),
    );
    let latest_index = events.partition_point(|event_ts| *event_ts <= latest);
    let event_ts = *events.get(latest_index.checked_sub(1)?)?;
    if event_ts < earliest {
        return None;
    }
    usize::try_from(entry_ts.saturating_sub(event_ts) / FOUR_HOURS_MS).ok()
}

#[cfg(test)]
fn event_in_entry_window(events: &[i64], entry_ts: i64, config: RankActivationConfig) -> bool {
    matching_event_age_bars(events, entry_ts, config).is_some()
}

fn activation_age_percentile(
    histogram: &BTreeMap<usize, usize>,
    percentile: usize,
) -> Option<usize> {
    let total = histogram.values().sum::<usize>();
    if total == 0 {
        return None;
    }
    let target_index = (total - 1).saturating_mul(percentile.min(100)) / 100;
    let mut cumulative = 0_usize;
    for (age, count) in histogram {
        cumulative = cumulative.saturating_add(*count);
        if cumulative > target_index {
            return Some(*age);
        }
    }
    histogram.last_key_value().map(|(age, _)| *age)
}

fn rsi_allowed(rsi: Option<f64>, config: RankActivationConfig) -> bool {
    if config.min_rsi.is_none() && config.max_rsi.is_none() {
        return true;
    }
    let Some(rsi) = rsi.filter(|value| value.is_finite()) else {
        return false;
    };
    rsi >= config.min_rsi.unwrap_or(f64::NEG_INFINITY)
        && rsi < config.max_rsi.unwrap_or(f64::INFINITY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> RankActivationConfig {
        RankActivationConfig {
            min_delta: 1,
            max_delta: Some(2),
            lookback_bars: 1,
            min_price_change_pct: None,
            max_price_change_pct: None,
            price_direction: RankPriceDirection::Any,
            valid_for_bars: 3,
            min_wait_bars: 1,
            min_rsi: Some(25.0),
            max_rsi: Some(55.0),
        }
    }

    #[test]
    fn reconstructs_only_causal_rank_improvements() {
        let mut snapshots = BTreeMap::new();
        snapshots.insert(
            0,
            vec![
                CrossSectionPoint {
                    symbol_index: 0,
                    quote_volume: 400.0,
                    close: 100.0,
                },
                CrossSectionPoint {
                    symbol_index: 1,
                    quote_volume: 300.0,
                    close: 100.0,
                },
                CrossSectionPoint {
                    symbol_index: 2,
                    quote_volume: 200.0,
                    close: 100.0,
                },
                CrossSectionPoint {
                    symbol_index: 3,
                    quote_volume: 100.0,
                    close: 100.0,
                },
            ],
        );
        snapshots.insert(
            FOUR_HOURS_MS,
            vec![
                CrossSectionPoint {
                    symbol_index: 0,
                    quote_volume: 400.0,
                    close: 100.0,
                },
                CrossSectionPoint {
                    symbol_index: 1,
                    quote_volume: 500.0,
                    close: 106.0,
                },
                CrossSectionPoint {
                    symbol_index: 2,
                    quote_volume: 200.0,
                    close: 100.0,
                },
                CrossSectionPoint {
                    symbol_index: 3,
                    quote_volume: 100.0,
                    close: 100.0,
                },
            ],
        );

        let events = reconstruct_events(&snapshots, 4, config());

        assert_eq!(events.event_count, 1);
        assert_eq!(events.by_symbol[1], vec![FOUR_HOURS_MS]);
        assert!(event_in_entry_window(
            &events.by_symbol[1],
            FOUR_HOURS_MS * 2,
            config()
        ));
        assert_eq!(
            matching_event_age_bars(&events.by_symbol[1], FOUR_HOURS_MS * 2, config()),
            Some(1)
        );
        assert!(!event_in_entry_window(
            &events.by_symbol[1],
            FOUR_HOURS_MS,
            config()
        ));
    }

    #[test]
    fn rsi_band_is_inclusive_below_and_exclusive_above() {
        assert!(rsi_allowed(Some(25.0), config()));
        assert!(rsi_allowed(Some(54.99), config()));
        assert!(!rsi_allowed(Some(55.0), config()));
        assert!(!rsi_allowed(None, config()));
    }

    #[test]
    fn converts_legacy_4h_table_name_to_strategy_symbol() {
        assert_eq!(
            symbol_from_4h_table("eth-usdt-swap_candles_4h").expect("valid table"),
            "ETH-USDT-SWAP"
        );
        assert!(symbol_from_4h_table("_candles_4h").is_err());
    }

    #[test]
    fn price_change_filter_can_require_a_bullish_causal_move() {
        let mut filtered = config();
        filtered.min_price_change_pct = Some(5.0);
        filtered.max_price_change_pct = Some(10.0);
        filtered.price_direction = RankPriceDirection::Up;

        assert!(price_change_allowed(Some(100.0), Some(106.0), filtered));
        assert!(!price_change_allowed(Some(100.0), Some(94.0), filtered));
        assert!(!price_change_allowed(Some(100.0), Some(102.0), filtered));
        assert!(!price_change_allowed(Some(100.0), Some(112.0), filtered));

        filtered.price_direction = RankPriceDirection::Down;
        assert!(price_change_allowed(Some(100.0), Some(94.0), filtered));
        assert!(!price_change_allowed(Some(100.0), Some(106.0), filtered));
    }

    #[test]
    fn activation_age_uses_latest_event_inside_wait_window() {
        let events = [FOUR_HOURS_MS, FOUR_HOURS_MS * 3, FOUR_HOURS_MS * 5];
        let entry_ts = FOUR_HOURS_MS * 6;

        assert_eq!(
            matching_event_age_bars(&events, entry_ts, config()),
            Some(1)
        );

        let mut wait_two = config();
        wait_two.min_wait_bars = 2;
        assert_eq!(
            matching_event_age_bars(&events, entry_ts, wait_two),
            Some(3)
        );
    }

    #[test]
    fn activation_age_percentiles_follow_trade_counts() {
        let histogram = BTreeMap::from([(1, 2), (4, 1), (9, 2)]);

        assert_eq!(activation_age_percentile(&histogram, 0), Some(1));
        assert_eq!(activation_age_percentile(&histogram, 50), Some(4));
        assert_eq!(activation_age_percentile(&histogram, 90), Some(9));
        assert_eq!(activation_age_percentile(&BTreeMap::new(), 50), None);
    }

    #[test]
    fn source_coverage_requires_both_inclusive_boundaries() {
        assert!(timestamp_in_source_coverage(20, Some(20), Some(30)));
        assert!(timestamp_in_source_coverage(30, Some(20), Some(30)));
        assert!(!timestamp_in_source_coverage(19, Some(20), Some(30)));
        assert!(!timestamp_in_source_coverage(31, Some(20), Some(30)));
        assert!(!timestamp_in_source_coverage(25, None, Some(30)));
    }
}
