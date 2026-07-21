use super::*;

fn seed() -> CompressedBreakdownSeed {
    CompressedBreakdownSeed {
        detail_id: 1,
        symbol: "XRP-USDT-SWAP".to_string(),
        signal_ts: 0,
        short_entry_price: 90.0,
        frozen_short_stop: 100.0,
    }
}

fn candle(index: i64, high: f64, low: f64, close: f64) -> Candle {
    Candle {
        ts: index * FOUR_HOURS_MS,
        open: close,
        high,
        low,
        close,
    }
}

#[test]
fn regime_lookup_excludes_the_signal_bar() {
    let regimes = BTreeMap::from([(0, BtcRegime::Bull), (FOUR_HOURS_MS, BtcRegime::Bear)]);
    assert_eq!(
        regime_before(&regimes, FOUR_HOURS_MS),
        Some(BtcRegime::Bull)
    );
}

#[test]
fn macd_value_lookup_excludes_the_signal_bar() {
    let values = BTreeMap::from([(0, -1.0), (FOUR_HOURS_MS, 2.0)]);
    assert_eq!(value_before(&values, FOUR_HOURS_MS), Some(-1.0));
}

#[test]
fn confirmation_enters_only_after_a_completed_close_above_frozen_stop() {
    let candles = vec![
        candle(1, 105.0, 90.0, 99.0),
        candle(2, 104.0, 95.0, 101.0),
        candle(3, 122.0, 100.0, 120.0),
    ];
    let SeedOutcome::Trade(trade) = evaluate_seed(&seed(), &candles).unwrap() else {
        panic!("expected completed reclaim trade")
    };
    assert_eq!(trade.entry_price, 101.0);
    assert_eq!(trade.initial_stop, 95.0);
    assert_eq!(trade.exit_reason, "target_2r");
    assert_eq!(trade.gross_r, 2.0);
}

#[test]
fn entry_confirmation_bar_never_triggers_its_own_stop() {
    let candles = vec![
        candle(1, 102.0, 100.0, 101.0),
        candle(2, 104.0, 100.5, 103.0),
    ];
    let SeedOutcome::Trade(trade) = evaluate_seed(&seed(), &candles).unwrap() else {
        panic!("expected completed reclaim trade")
    };
    assert_eq!(trade.exit_reason, "target_2r");
}

#[test]
fn same_future_bar_stop_and_target_uses_conservative_stop() {
    let candles = vec![candle(1, 105.0, 95.0, 101.0), candle(2, 120.0, 90.0, 105.0)];
    let SeedOutcome::Trade(trade) = evaluate_seed(&seed(), &candles).unwrap() else {
        panic!("expected completed reclaim trade")
    };
    assert_eq!(trade.exit_reason, "stop");
    assert_eq!(trade.gross_r, -1.0);
}

#[test]
fn standard_cost_is_stricter_than_gross_and_double_cost() {
    let standard = net_r_after_costs(100.0, 120.0, 10.0, 0, FUNDING_INTERVAL_MS, 5.0, 1.0);
    let doubled = net_r_after_costs(100.0, 120.0, 10.0, 0, FUNDING_INTERVAL_MS, 10.0, 2.0);
    assert!(standard < 2.0);
    assert!(doubled < standard);
}

#[test]
fn macd_midpoint_reclaim_requires_a_bullish_completed_bar() {
    let candles = vec![
        Candle {
            ts: 0,
            open: 110.0,
            high: 112.0,
            low: 90.0,
            close: 90.0,
        },
        candle(1, 105.0, 95.0, 101.0),
        Candle {
            ts: 2 * FOUR_HOURS_MS,
            open: 100.0,
            high: 120.0,
            low: 98.0,
            close: 115.0,
        },
        candle(3, 170.0, 110.0, 165.0),
    ];
    let SeedOutcome::Trade(trade) = evaluate_macd_midpoint_seed(&seed(), &candles).unwrap() else {
        panic!("expected completed MACD midpoint reclaim")
    };
    assert_eq!(trade.entry_price, 115.0);
    assert_eq!(trade.initial_stop, 90.0);
}
