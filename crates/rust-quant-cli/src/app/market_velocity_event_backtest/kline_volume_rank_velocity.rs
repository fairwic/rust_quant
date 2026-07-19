use super::{MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection, RadarEvent, MS_15M};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashMap, VecDeque};

const VOLUME_RANK_LOOKBACK_CANDLES: usize = 96;

#[derive(Debug, Clone)]
struct VolumeRankCandle {
    ts: i64,
    open: f64,
    close: f64,
    quote_turnover: f64,
}

#[derive(Debug, Clone)]
struct VolumeRankPoint {
    symbol: String,
    candle: VolumeRankCandle,
    rolling_quote_turnover: f64,
}

#[derive(Debug, Clone)]
struct RankedVolumePoint {
    point: VolumeRankPoint,
    rank: i32,
}

/// 从冻结的 15m universe 重建成交额排名加速事件。
///
/// 历史表没有保存交易所原始 quote 成交额，因此用 `vol_ccy * close` 近似；排名只在
/// 所有成员同时具有完整 96 根已确认 K 线时生成，避免数据缺口改变分母而制造假跃升。
pub(super) async fn load_kline_volume_rank_events(
    pool: &PgPool,
    symbols: &[String],
    args: &MarketVelocityEventBacktestArgs,
) -> Result<Vec<RadarEvent>> {
    let load_start_ms = args.event_start_ms.map(|event_start_ms| {
        event_start_ms.saturating_sub(
            i64::try_from(VOLUME_RANK_LOOKBACK_CANDLES + 1)
                .unwrap_or(i64::MAX / MS_15M)
                .saturating_mul(MS_15M),
        )
    });
    let mut candles_by_symbol = HashMap::with_capacity(symbols.len());
    for symbol in symbols {
        let table_name = quote_identifier(&format!("{}_candles_15m", symbol.to_ascii_lowercase()));
        let query = format!(
            "SELECT ts, o, c, vol_ccy FROM {table_name} \
             WHERE confirm = '1' \
               AND ($1::bigint IS NULL OR ts + {MS_15M} >= $1) \
               AND ($2::bigint IS NULL OR ts + {MS_15M} <= $2) \
             ORDER BY ts"
        );
        let rows = sqlx::query(&query)
            .bind(load_start_ms)
            .bind(args.event_end_ms)
            .fetch_all(pool)
            .await
            .with_context(|| format!("load 15m quote turnover inputs from {table_name}"))?;
        let candles = rows
            .into_iter()
            .map(|row| {
                let close = parse_f64(row.get::<String, _>("c").as_str())?;
                let volume_ccy = parse_f64(row.get::<String, _>("vol_ccy").as_str())?;
                Ok(VolumeRankCandle {
                    ts: row.get("ts"),
                    open: parse_f64(row.get::<String, _>("o").as_str())?,
                    close,
                    quote_turnover: volume_ccy * close,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        candles_by_symbol.insert(symbol.clone(), candles);
    }
    Ok(build_kline_volume_rank_events(&candles_by_symbol, args))
}

/// 以纯内存方式构造排名事件，确保数据库加载与时序算法可以分别测试。
fn build_kline_volume_rank_events(
    candles_by_symbol: &HashMap<String, Vec<VolumeRankCandle>>,
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<RadarEvent> {
    let universe_size = candles_by_symbol.len();
    if universe_size < 2 {
        return Vec::new();
    }
    let mut points_by_event_ts: BTreeMap<i64, Vec<VolumeRankPoint>> = BTreeMap::new();
    for (symbol, candles) in candles_by_symbol {
        append_rolling_volume_points(symbol, candles, &mut points_by_event_ts);
    }

    let ranked_by_event_ts = points_by_event_ts
        .into_iter()
        .filter_map(|(event_ts, points)| {
            (points.len() == universe_size).then(|| (event_ts, rank_volume_points(points)))
        })
        .collect::<BTreeMap<_, _>>();

    let mut events = Vec::new();
    for (event_ts, current_points) in &ranked_by_event_ts {
        if args
            .event_start_ms
            .is_some_and(|event_start_ms| *event_ts < event_start_ms)
            || args
                .event_end_ms
                .is_some_and(|event_end_ms| *event_ts > event_end_ms)
        {
            continue;
        }
        let Some(previous_points) = ranked_by_event_ts.get(&event_ts.saturating_sub(MS_15M)) else {
            continue;
        };
        let previous_by_symbol = previous_points
            .iter()
            .map(|point| (point.point.symbol.as_str(), point))
            .collect::<HashMap<_, _>>();
        let two_snapshots_ago_by_symbol = ranked_by_event_ts
            .get(&event_ts.saturating_sub(MS_15M.saturating_mul(2)))
            .map(|points| {
                points
                    .iter()
                    .map(|point| (point.point.symbol.as_str(), point))
                    .collect::<HashMap<_, _>>()
            });
        for current in current_points {
            let Some(previous) = previous_by_symbol.get(current.point.symbol.as_str()) else {
                continue;
            };
            let delta_rank = previous.rank.saturating_sub(current.rank);
            if delta_rank < args.min_delta_rank
                || args
                    .max_delta_rank
                    .is_some_and(|max_delta_rank| delta_rank > max_delta_rank)
                || (args.kline_volume_rank_require_turnover_growth
                    && current.point.rolling_quote_turnover
                        <= previous.point.rolling_quote_turnover)
                || (args.kline_volume_rank_require_consecutive_improvement
                    && !two_snapshots_ago_by_symbol.as_ref().is_some_and(|points| {
                        points
                            .get(current.point.symbol.as_str())
                            .is_some_and(|earlier| {
                                earlier.rank.saturating_sub(previous.rank) >= args.min_delta_rank
                            })
                    }))
                || !accepts_trade_direction(&current.point.candle, args)
            {
                continue;
            }
            let raw_price_change_pct = candle_price_change_pct(&current.point.candle);
            if args
                .min_price_change_pct
                .is_some_and(|minimum| raw_price_change_pct.abs() < minimum)
                || args
                    .max_price_change_pct
                    .is_some_and(|maximum| raw_price_change_pct.abs() > maximum)
            {
                continue;
            }
            events.push(RadarEvent {
                id: synthetic_event_id(&current.point.symbol, *event_ts),
                exchange: "okx".to_string(),
                symbol: current.point.symbol.clone(),
                ts: *event_ts,
                detected_at: Utc
                    .timestamp_millis_opt(*event_ts)
                    .single()
                    .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
                    .unwrap_or_else(|| event_ts.to_string()),
                new_rank: current.rank,
                delta_rank,
                current_price: current.point.candle.close,
                price_change_pct: event_price_change_pct(
                    raw_price_change_pct,
                    args.trade_direction,
                ),
            });
        }
    }
    events.sort_by_key(|event| (event.ts, event.id));
    events
}

/// 为一个标的生成连续 96 根 K 线的滚动成交额点。
///
/// 缺口或非法值会清空窗口，因为用更早 K 线补足 96 条记录会把超过 24 小时的数据
/// 伪装成生产扫描器的 24 小时成交额。
fn append_rolling_volume_points(
    symbol: &str,
    candles: &[VolumeRankCandle],
    points_by_event_ts: &mut BTreeMap<i64, Vec<VolumeRankPoint>>,
) {
    let mut window = VecDeque::with_capacity(VOLUME_RANK_LOOKBACK_CANDLES + 1);
    let mut rolling_quote_turnover = 0.0;
    let mut previous_ts = None;
    for candle in candles {
        if previous_ts.is_some_and(|ts| candle.ts != ts + MS_15M)
            || !valid_volume_rank_candle(candle)
        {
            window.clear();
            rolling_quote_turnover = 0.0;
        }
        previous_ts = Some(candle.ts);
        if !valid_volume_rank_candle(candle) {
            continue;
        }
        window.push_back(candle.quote_turnover);
        rolling_quote_turnover += candle.quote_turnover;
        if window.len() > VOLUME_RANK_LOOKBACK_CANDLES {
            rolling_quote_turnover -= window.pop_front().unwrap_or(0.0);
        }
        if window.len() == VOLUME_RANK_LOOKBACK_CANDLES {
            points_by_event_ts
                .entry(candle.ts.saturating_add(MS_15M))
                .or_default()
                .push(VolumeRankPoint {
                    symbol: symbol.to_string(),
                    candle: candle.clone(),
                    rolling_quote_turnover,
                });
        }
    }
}

/// 对同一时点的成交额点进行稳定排名。
fn rank_volume_points(mut points: Vec<VolumeRankPoint>) -> Vec<RankedVolumePoint> {
    points.sort_by(|left, right| {
        right
            .rolling_quote_turnover
            .total_cmp(&left.rolling_quote_turnover)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    points
        .into_iter()
        .enumerate()
        .map(|(index, point)| RankedVolumePoint {
            point,
            rank: i32::try_from(index + 1).unwrap_or(i32::MAX),
        })
        .collect()
}

/// 判断当前 K 线是否符合旧 `kline_15m` 事件源的方向门禁。
fn accepts_trade_direction(
    candle: &VolumeRankCandle,
    args: &MarketVelocityEventBacktestArgs,
) -> bool {
    match args.trade_direction {
        MarketVelocityTradeDirection::Long if args.entry_defer_bearish_continuation => {
            candle.close != candle.open
        }
        MarketVelocityTradeDirection::Long => candle.close > candle.open,
        MarketVelocityTradeDirection::Short if args.entry_defer_bullish_continuation => {
            candle.close != candle.open
        }
        MarketVelocityTradeDirection::Short => candle.close < candle.open,
        MarketVelocityTradeDirection::Both => candle.close != candle.open,
    }
}

/// 校验排名输入；零或负成交量会让稀疏标的产生不可靠的名次跳变，因此失败关闭。
fn valid_volume_rank_candle(candle: &VolumeRankCandle) -> bool {
    candle.open.is_finite()
        && candle.open > 0.0
        && candle.close.is_finite()
        && candle.close > 0.0
        && candle.quote_turnover.is_finite()
        && candle.quote_turnover > 0.0
}

/// 返回当前完成 K 线的原始方向涨跌幅。
fn candle_price_change_pct(candle: &VolumeRankCandle) -> f64 {
    (candle.close - candle.open) / candle.open * 100.0
}

/// 保留既有事件方向契约：long 为正、short 为负，both 使用当前 K 线原始方向。
fn event_price_change_pct(
    raw_price_change_pct: f64,
    direction: MarketVelocityTradeDirection,
) -> f64 {
    match direction {
        MarketVelocityTradeDirection::Long => raw_price_change_pct.abs(),
        MarketVelocityTradeDirection::Short => -raw_price_change_pct.abs(),
        MarketVelocityTradeDirection::Both => raw_price_change_pct,
    }
}

/// 生成可复现的内存事件 ID，不占用数据库事件序列。
fn synthetic_event_id(symbol: &str, detected_ms: i64) -> i64 {
    let mut hash = 17_i64;
    for byte in symbol.as_bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(i64::from(*byte));
    }
    let symbol_component = (hash.unsigned_abs() % 1_000_000) as i64;
    symbol_component
        .saturating_mul(10_000_000)
        .saturating_add((detected_ms / MS_15M).rem_euclid(10_000_000))
}

/// 解析数据库数值文本，并把坏数据定位到排名输入层。
fn parse_f64(value: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .with_context(|| format!("parse volume rank numeric value {value}"))
}

/// 转义动态 K 线表名；表名只来自已解析的 information_schema 候选。
fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::MarketVelocityEventSource;

    fn args() -> MarketVelocityEventBacktestArgs {
        MarketVelocityEventBacktestArgs {
            event_source: MarketVelocityEventSource::Kline15m,
            kline_volume_rank_velocity: true,
            trade_direction: MarketVelocityTradeDirection::Both,
            min_delta_rank: 3,
            ..MarketVelocityEventBacktestArgs::default()
        }
    }

    fn candle(index: usize, quote_turnover: f64) -> VolumeRankCandle {
        VolumeRankCandle {
            ts: i64::try_from(index).unwrap_or(i64::MAX) * MS_15M,
            open: 1.0,
            close: 1.01,
            quote_turnover,
        }
    }

    fn four_symbol_fixture() -> HashMap<String, Vec<VolumeRankCandle>> {
        [("A", 400.0), ("B", 300.0), ("C", 200.0), ("D", 100.0)]
            .into_iter()
            .map(|(symbol, turnover)| {
                let mut candles = (0..=VOLUME_RANK_LOOKBACK_CANDLES)
                    .map(|index| candle(index, turnover))
                    .collect::<Vec<_>>();
                if symbol == "D" {
                    candles[VOLUME_RANK_LOOKBACK_CANDLES].quote_turnover = 50_000.0;
                }
                (symbol.to_string(), candles)
            })
            .collect()
    }

    #[test]
    fn emits_event_only_after_current_completed_candle_changes_rank() {
        let events = build_kline_volume_rank_events(&four_symbol_fixture(), &args());

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].symbol, "D");
        assert_eq!(events[0].ts, 97 * MS_15M);
        assert_eq!(events[0].new_rank, 1);
        assert_eq!(events[0].delta_rank, 3);
    }

    #[test]
    fn skips_timestamp_when_any_universe_member_has_no_snapshot() {
        let mut candles = four_symbol_fixture();
        candles.get_mut("C").unwrap().pop();

        assert!(build_kline_volume_rank_events(&candles, &args()).is_empty());
    }

    #[test]
    fn resets_rolling_window_after_gap_or_nonpositive_turnover() {
        for invalid_fixture in [
            |candle: &mut VolumeRankCandle| candle.ts += MS_15M,
            |candle: &mut VolumeRankCandle| candle.quote_turnover = 0.0,
        ] {
            let mut candles = four_symbol_fixture();
            invalid_fixture(&mut candles.get_mut("D").unwrap()[20]);
            assert!(build_kline_volume_rank_events(&candles, &args()).is_empty());
        }
    }

    #[test]
    fn bearish_rank_impulse_requires_long_defer_mode() {
        let mut candles = four_symbol_fixture();
        let impulse = &mut candles.get_mut("D").unwrap()[VOLUME_RANK_LOOKBACK_CANDLES];
        impulse.open = 1.1;
        impulse.close = 1.0;

        let mut long_args = args();
        long_args.trade_direction = MarketVelocityTradeDirection::Long;
        assert!(build_kline_volume_rank_events(&candles, &long_args).is_empty());

        long_args.entry_defer_bearish_continuation = true;
        let events = build_kline_volume_rank_events(&candles, &long_args);
        assert_eq!(events.len(), 1);
        assert!(events[0].price_change_pct > 0.0);
    }

    #[test]
    fn applies_price_change_and_rank_bounds_to_reconstructed_events() {
        let candles = four_symbol_fixture();
        let mut filtered = args();
        filtered.max_delta_rank = Some(2);
        assert!(build_kline_volume_rank_events(&candles, &filtered).is_empty());

        filtered.max_delta_rank = None;
        filtered.min_price_change_pct = Some(2.0);
        assert!(build_kline_volume_rank_events(&candles, &filtered).is_empty());
    }

    #[test]
    fn optional_growth_gate_rejects_rank_gain_caused_only_by_competitor_decay() {
        let mut candles = [("A", 400.0), ("B", 300.0), ("C", 101.0), ("D", 100.0)]
            .into_iter()
            .map(|(symbol, turnover)| {
                (
                    symbol.to_string(),
                    (0..=VOLUME_RANK_LOOKBACK_CANDLES)
                        .map(|index| candle(index, turnover))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<HashMap<_, _>>();
        candles.get_mut("C").unwrap()[VOLUME_RANK_LOOKBACK_CANDLES].quote_turnover = 1.0;
        let mut rank_args = args();
        rank_args.min_delta_rank = 1;

        let events = build_kline_volume_rank_events(&candles, &rank_args);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].symbol, "D");

        rank_args.kline_volume_rank_require_turnover_growth = true;
        assert!(build_kline_volume_rank_events(&candles, &rank_args).is_empty());
    }

    #[test]
    fn consecutive_gate_emits_only_after_two_completed_rank_improvements() {
        let mut candles = [
            ("A", 500.0),
            ("B", 400.0),
            ("C", 300.0),
            ("D", 200.0),
            ("E", 100.0),
        ]
        .into_iter()
        .map(|(symbol, turnover)| {
            (
                symbol.to_string(),
                (0..=(VOLUME_RANK_LOOKBACK_CANDLES + 1))
                    .map(|index| candle(index, turnover))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<HashMap<_, _>>();
        let e = candles.get_mut("E").unwrap();
        e[VOLUME_RANK_LOOKBACK_CANDLES].quote_turnover = 25_000.0;
        e[VOLUME_RANK_LOOKBACK_CANDLES + 1].quote_turnover = 25_000.0;
        let mut rank_args = args();
        rank_args.min_delta_rank = 1;
        rank_args.kline_volume_rank_require_consecutive_improvement = true;

        let events = build_kline_volume_rank_events(&candles, &rank_args);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].symbol, "E");
        assert_eq!(events[0].ts, 98 * MS_15M);
        assert_eq!(events[0].new_rank, 1);
    }
}
