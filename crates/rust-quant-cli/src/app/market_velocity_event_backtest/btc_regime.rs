use super::directional_reversal::{
    benchmark_abs_net_move_pct_before_entry, benchmark_directional_net_move_pct_before_entry,
    BTC_BROAD_DIRECTION_LOOKBACK_CANDLES, BTC_REGIME_LOOKBACK_CANDLES,
};
use super::{
    BacktestCandle, ConfirmedEvent, MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
};
use std::collections::HashMap;

/// 只保留同时满足短期震荡与可选广义方向的 BTC 市场状态；缺历史时失败关闭。
pub(super) fn filter_confirmed_events_by_btc_regime(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<ConfirmedEvent> {
    if args.entry_btc_96_max_abs_net_move_pct.is_none()
        && args.entry_btc_384_min_directional_net_move_pct.is_none()
        && !args.entry_btc_require_current_directional_candle
    {
        return confirmed.to_vec();
    }
    let Some(btc_candles) = candles_15m.get("BTC-USDT-SWAP") else {
        return Vec::new();
    };
    confirmed
        .iter()
        .filter(|event| btc_regime_allows_entry(btc_candles, event, args))
        .cloned()
        .collect()
}

fn btc_regime_allows_entry(
    btc_candles: &[BacktestCandle],
    event: &ConfirmedEvent,
    args: &MarketVelocityEventBacktestArgs,
) -> bool {
    let short_term_allowed = args
        .entry_btc_96_max_abs_net_move_pct
        .is_none_or(|maximum| {
            benchmark_abs_net_move_pct_before_entry(
                btc_candles,
                event.entry_ts,
                BTC_REGIME_LOOKBACK_CANDLES,
            )
            .is_some_and(|move_pct| move_pct < maximum)
        });
    let broad_direction_allowed =
        args.entry_btc_384_min_directional_net_move_pct
            .is_none_or(|minimum| {
                benchmark_directional_net_move_pct_before_entry(
                    btc_candles,
                    event.entry_ts,
                    BTC_BROAD_DIRECTION_LOOKBACK_CANDLES,
                    event.direction,
                )
                .is_some_and(|move_pct| move_pct >= minimum)
            });
    let immediate_direction_allowed = !args.entry_btc_require_current_directional_candle
        || benchmark_current_candle_matches_direction(btc_candles, event.entry_ts, event.direction);
    short_term_allowed && broad_direction_allowed && immediate_direction_allowed
}

/// 入场只能使用当时已经收盘的 BTC K 线；十字星不提供方向确认。
fn benchmark_current_candle_matches_direction(
    candles: &[BacktestCandle],
    entry_ts: i64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    let completed_count = candles.partition_point(|candle| candle.ts + super::MS_15M <= entry_ts);
    let Some(current) = completed_count
        .checked_sub(1)
        .and_then(|idx| candles.get(idx))
    else {
        return false;
    };
    if !current.open.is_finite()
        || !current.close.is_finite()
        || current.open <= 0.0
        || current.close <= 0.0
    {
        return false;
    }
    match direction {
        MarketVelocityTradeDirection::Long => current.close > current.open,
        MarketVelocityTradeDirection::Short => current.close < current.open,
        MarketVelocityTradeDirection::Both => false,
    }
}

pub(super) fn print_btc_regime_filter_report(
    before: &[ConfirmedEvent],
    after: &[ConfirmedEvent],
    args: &MarketVelocityEventBacktestArgs,
) {
    if args.entry_btc_96_max_abs_net_move_pct.is_none()
        && args.entry_btc_384_min_directional_net_move_pct.is_none()
        && !args.entry_btc_require_current_directional_candle
    {
        return;
    }
    println!(
        "btc_regime_filter\tbefore={}\tafter={}\tshort_lookback_candles={}\tmax_abs_net_move_pct={}\tbroad_lookback_candles={}\tmin_directional_net_move_pct={}\trequire_current_directional_candle={}",
        before.len(),
        after.len(),
        BTC_REGIME_LOOKBACK_CANDLES,
        args.entry_btc_96_max_abs_net_move_pct
            .map_or_else(|| "off".to_string(), |value| value.to_string()),
        BTC_BROAD_DIRECTION_LOOKBACK_CANDLES,
        args.entry_btc_384_min_directional_net_move_pct
            .map_or_else(|| "off".to_string(), |value| value.to_string()),
        args.entry_btc_require_current_directional_candle,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{
        MarketVelocityTradeDirection, RadarEvent, MS_15M,
    };

    fn btc_candles() -> Vec<BacktestCandle> {
        (0..384)
            .map(|idx| {
                let close = 100.0 + idx as f64 * 0.05;
                BacktestCandle {
                    ts: idx * MS_15M,
                    open: close,
                    high: close,
                    low: close,
                    close,
                    volume: 10.0,
                }
            })
            .collect()
    }

    fn confirmed_event(direction: MarketVelocityTradeDirection) -> ConfirmedEvent {
        ConfirmedEvent {
            event: RadarEvent {
                id: 1,
                exchange: "okx".to_string(),
                symbol: "ETH-USDT-SWAP".to_string(),
                ts: 383 * MS_15M,
                detected_at: "2026-06-15T00:00:00Z".to_string(),
                new_rank: 10,
                delta_rank: 12,
                current_price: 100.0,
                price_change_pct: 3.0,
            },
            direction,
            entry_ts: 384 * MS_15M,
            entry_price: 100.0,
            entry_idx: 384,
            trigger: "breakout_previous_high".to_string(),
            structure_stop_loss_price: None,
            structure_stop_loss_source: None,
        }
    }

    #[test]
    fn broad_direction_gate_keeps_only_signals_aligned_with_btc() {
        let args = MarketVelocityEventBacktestArgs {
            entry_btc_384_min_directional_net_move_pct: Some(0.0),
            ..MarketVelocityEventBacktestArgs::default()
        };
        let candles = HashMap::from([("BTC-USDT-SWAP".to_string(), btc_candles())]);
        let confirmed = vec![
            confirmed_event(MarketVelocityTradeDirection::Long),
            confirmed_event(MarketVelocityTradeDirection::Short),
        ];

        let filtered = filter_confirmed_events_by_btc_regime(&confirmed, &candles, &args);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].direction, MarketVelocityTradeDirection::Long);
    }

    #[test]
    fn broad_direction_gate_fails_closed_without_btc_history() {
        let args = MarketVelocityEventBacktestArgs {
            entry_btc_384_min_directional_net_move_pct: Some(0.0),
            ..MarketVelocityEventBacktestArgs::default()
        };
        let confirmed = vec![confirmed_event(MarketVelocityTradeDirection::Long)];

        assert!(
            filter_confirmed_events_by_btc_regime(&confirmed, &HashMap::new(), &args).is_empty()
        );
    }

    #[test]
    fn immediate_gate_uses_the_latest_completed_btc_candle_symmetrically() {
        let args = MarketVelocityEventBacktestArgs {
            entry_btc_require_current_directional_candle: true,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let mut history = btc_candles();
        history[383].open = history[383].close - 1.0;
        history.push(BacktestCandle {
            ts: 384 * MS_15M,
            open: 120.0,
            high: 120.0,
            low: 110.0,
            close: 110.0,
            volume: 10.0,
        });
        let candles = HashMap::from([("BTC-USDT-SWAP".to_string(), history)]);
        let confirmed = vec![
            confirmed_event(MarketVelocityTradeDirection::Long),
            confirmed_event(MarketVelocityTradeDirection::Short),
        ];

        let filtered = filter_confirmed_events_by_btc_regime(&confirmed, &candles, &args);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].direction, MarketVelocityTradeDirection::Long);
    }

    #[test]
    fn immediate_gate_rejects_doji_and_missing_btc_candles() {
        let args = MarketVelocityEventBacktestArgs {
            entry_btc_require_current_directional_candle: true,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let confirmed = vec![confirmed_event(MarketVelocityTradeDirection::Long)];
        let doji = HashMap::from([("BTC-USDT-SWAP".to_string(), btc_candles())]);

        assert!(filter_confirmed_events_by_btc_regime(&confirmed, &doji, &args).is_empty());
        assert!(
            filter_confirmed_events_by_btc_regime(&confirmed, &HashMap::new(), &args).is_empty()
        );
    }
}
