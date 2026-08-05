use super::*;

fn candle(timestamp_ms: i64, open: f64, high: f64, low: f64, close: f64) -> Candle {
    Candle {
        timestamp_ms,
        open,
        high,
        low,
        close,
        volume: 1.0,
    }
}

fn fixed_long(exit_time_ms: i64, exit_price: f64, reason: ExitReason) -> Trade {
    Trade {
        direction: Direction::Long,
        families: vec![SignalFamily::StrictVisualConsolidationBreakLong],
        exit_policy: ExitPolicy::Fixed,
        signal_counter_trend_ema_age_bars_capped_600: None,
        counter_trend_structure_breakout_line: None,
        counter_trend_structure_confirmed: false,
        counter_trend_two_r_trailing_activated: false,
        range_partial_one_r_taken: false,
        range_two_r_trailing_activated: false,
        signal_time_ms: 0,
        entry_time_ms: 900_000,
        exit_time_ms,
        entry_price: 100.0,
        exit_price,
        initial_stop: 98.0,
        exit_reason: reason,
        gross_pnl: exit_price - 100.0,
        net_pnl: exit_price - 100.0,
        initial_risk: 2.0,
        net_r: (exit_price - 100.0) / 2.0,
        anchor_upthrust_target_consumption_ratio: None,
        volume_ratio: Some(3.0),
        rsi: Some(60.0),
    }
}

#[test]
fn completed_high_activates_protection_only_from_next_candle() {
    let trade = fixed_long(2_700_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 101.0, 99.0, 100.2),
        candle(1_800_000, 100.5, 100.8, 99.8, 100.0),
        candle(2_700_000, 100.0, 100.1, 98.0, 98.5),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, 0.5).expect("simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.exit_time_ms, 1_800_000);
    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenStop);
    assert!(nearly_equal(simulated.exit_price, 100.2));
}

#[test]
fn activation_candle_cannot_use_its_own_intrabar_reversal() {
    let trade = fixed_long(1_800_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 101.2, 99.0, 99.5),
        candle(1_800_000, 99.5, 100.0, 98.0, 98.5),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, 0.5).expect("simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenGapOpen);
    assert_eq!(simulated.exit_time_ms, 1_800_000);
    assert!(nearly_equal(simulated.exit_price, 99.5));
}

#[test]
fn half_r_waits_for_cost_floor_when_initial_risk_is_too_narrow() {
    let mut trade = fixed_long(2_700_000, 99.8, ExitReason::StopLoss);
    trade.initial_stop = 99.8;
    trade.initial_risk = 0.2;
    let candles = vec![
        candle(900_000, 100.0, 100.1, 99.9, 100.05),
        candle(1_800_000, 100.05, 100.2, 100.0, 100.15),
        candle(2_700_000, 100.15, 100.2, 99.8, 99.9),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, 0.5).expect("simulation");

    assert_eq!(simulated.activation_time_ms, Some(1_800_000));
    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenGapOpen);
    assert!(nearly_equal(simulated.exit_price, 100.15));
}

#[test]
fn one_r_variant_does_not_activate_on_half_r_high() {
    let trade = fixed_long(1_800_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 101.9, 99.5, 101.0),
        candle(1_800_000, 101.0, 101.2, 98.0, 98.5),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, 1.0).expect("simulation");

    assert_eq!(simulated.activation_time_ms, None);
    assert_eq!(simulated.kind, CounterfactualExitKind::BaselineUnchanged);
    assert!(nearly_equal(simulated.exit_price, 98.0));
}

#[test]
fn completed_close_does_not_activate_on_one_r_upper_wick() {
    let trade = fixed_long(1_800_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 102.5, 99.5, 101.5),
        candle(1_800_000, 101.5, 101.8, 98.0, 98.5),
    ];

    let simulated = simulate_trade_with_evidence(
        &trade,
        &candles,
        0.1,
        1.0,
        ActivationEvidence::CompletedClose,
    )
    .expect("simulation");

    assert_eq!(simulated.activation_time_ms, None);
    assert_eq!(simulated.kind, CounterfactualExitKind::BaselineUnchanged);
    assert!(nearly_equal(simulated.exit_price, 98.0));
}

#[test]
fn completed_close_activates_protection_only_from_next_candle() {
    let trade = fixed_long(2_700_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 102.2, 99.0, 102.1),
        candle(1_800_000, 101.0, 101.5, 100.1, 100.5),
        candle(2_700_000, 100.5, 100.8, 98.0, 98.5),
    ];

    let simulated = simulate_trade_with_evidence(
        &trade,
        &candles,
        0.1,
        1.0,
        ActivationEvidence::CompletedClose,
    )
    .expect("simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.exit_time_ms, 1_800_000);
    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenStop);
    assert!(nearly_equal(simulated.exit_price, 100.2));
}

#[test]
fn completed_close_gap_after_activation_uses_next_open() {
    let trade = fixed_long(2_700_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 102.2, 99.0, 102.1),
        candle(1_800_000, 99.5, 100.0, 99.0, 99.8),
        candle(2_700_000, 99.8, 100.0, 98.0, 98.5),
    ];

    let simulated = simulate_trade_with_evidence(
        &trade,
        &candles,
        0.1,
        1.0,
        ActivationEvidence::CompletedClose,
    )
    .expect("simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.exit_time_ms, 1_800_000);
    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenGapOpen);
    assert!(nearly_equal(simulated.exit_price, 99.5));
}

#[test]
fn target_wins_when_default_path_reaches_it_before_net_break_even() {
    let trade = fixed_long(1_800_000, 106.0, ExitReason::TakeProfit);
    let candles = vec![
        candle(900_000, 100.0, 101.0, 99.5, 100.8),
        candle(1_800_000, 101.0, 106.5, 100.5, 105.0),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, 0.5).expect("simulation");

    assert_eq!(simulated.kind, CounterfactualExitKind::BaselineUnchanged);
    assert!(nearly_equal(simulated.exit_price, 106.0));
}

#[test]
fn tick_rounded_long_net_break_even_covers_eight_bps_per_side() {
    let stop = long_net_break_even_price(100.0, 0.1);
    let protected_r = net_r(Direction::Long, 100.0, stop, 2.0, 8.0);

    assert!(nearly_equal(stop, 100.2));
    assert!(protected_r >= 0.0);
}
