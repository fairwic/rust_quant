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

fn fixed_short(exit_time_ms: i64, exit_price: f64, reason: ExitReason) -> Trade {
    Trade {
        direction: Direction::Short,
        families: vec![SignalFamily::EmaTrendShort],
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
        initial_stop: 102.0,
        exit_reason: reason,
        gross_pnl: 100.0 - exit_price,
        net_pnl: 100.0 - exit_price,
        initial_risk: 2.0,
        net_r: (100.0 - exit_price) / 2.0,
        anchor_upthrust_target_consumption_ratio: None,
        volume_ratio: Some(3.0),
        rsi: Some(40.0),
    }
}

fn twenty_bar_history_before_entry() -> Vec<Candle> {
    (0..20)
        .map(|index| candle((index as i64 - 19) * 900_000, 99.0, 99.5, 97.5, 99.0))
        .collect()
}

#[test]
fn intrabar_one_r_touch_does_not_activate_without_completed_close() {
    let trade = fixed_short(2_700_000, 102.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 101.0, 97.0, 99.0),
        candle(1_800_000, 99.0, 100.0, 97.0, 98.5),
        candle(2_700_000, 98.5, 102.0, 98.0, 101.0),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, ProtectionMode::CompletedCloseOneR)
        .expect("simulation");

    assert_eq!(simulated.kind, CounterfactualExitKind::BaselineUnchanged);
    assert_eq!(simulated.activation_time_ms, None);
    assert_eq!(simulated.exit_price, 102.0);
}

#[test]
fn completed_close_activates_only_for_the_next_bar() {
    let trade = fixed_short(2_700_000, 102.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 100.5, 97.0, 98.0),
        candle(1_800_000, 98.0, 99.9, 97.5, 98.5),
        candle(2_700_000, 98.5, 102.0, 98.0, 101.0),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, ProtectionMode::CompletedCloseOneR)
        .expect("simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenStop);
    assert_eq!(simulated.exit_time_ms, 1_800_000);
    assert!(nearly_equal(simulated.exit_price, 99.8));
}

#[test]
fn next_bar_gap_over_protection_fills_at_actual_open() {
    let trade = fixed_short(2_700_000, 102.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 100.5, 97.0, 98.0),
        candle(1_800_000, 100.4, 101.0, 99.0, 100.0),
        candle(2_700_000, 100.0, 102.0, 99.0, 101.0),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, ProtectionMode::CompletedCloseOneR)
        .expect("simulation");

    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenGapOpen);
    assert_eq!(simulated.exit_price, 100.4);
}

#[test]
fn target_wins_when_default_path_reaches_low_before_net_break_even() {
    let trade = fixed_short(1_800_000, 95.5, ExitReason::TakeProfit);
    let candles = vec![
        candle(900_000, 100.0, 100.5, 97.0, 98.0),
        candle(1_800_000, 98.0, 102.0, 95.0, 96.0),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, ProtectionMode::CompletedCloseOneR)
        .expect("simulation");

    assert_eq!(simulated.kind, CounterfactualExitKind::BaselineUnchanged);
    assert_eq!(simulated.exit_price, 95.5);
}

#[test]
fn net_break_even_wins_when_default_path_reaches_high_before_target() {
    let trade = fixed_short(1_800_000, 95.0, ExitReason::TakeProfit);
    let candles = vec![
        candle(900_000, 100.0, 100.5, 97.0, 98.0),
        candle(1_800_000, 98.0, 100.0, 94.0, 96.0),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, ProtectionMode::CompletedCloseOneR)
        .expect("simulation");

    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenStop);
    assert!(nearly_equal(simulated.exit_price, 99.8));
}

#[test]
fn eight_bps_cost_reconstruction_matches_fixed_one_unit_trade() {
    let trade = fixed_short(1_800_000, 99.8, ExitReason::StopLoss);
    let expected_cost = (trade.entry_price + trade.exit_price) * 8.0 / 10_000.0;

    let reconstructed = net_r(
        trade.direction,
        trade.entry_price,
        trade.exit_price,
        trade.initial_risk,
        8.0,
    );

    assert!(nearly_equal(
        reconstructed,
        (trade.gross_pnl - expected_cost) / trade.initial_risk
    ));
}

#[test]
fn structure_mode_requires_one_r_break_and_retest_on_three_separate_closes() {
    let trade = fixed_short(4_500_000, 102.0, ExitReason::StopLoss);
    let mut candles = twenty_bar_history_before_entry();
    candles.extend([
        candle(900_000, 100.0, 100.5, 97.0, 98.0),
        candle(1_800_000, 98.0, 98.2, 96.5, 96.8),
        candle(2_700_000, 96.8, 97.2, 96.4, 96.9),
        candle(3_600_000, 96.9, 100.0, 96.5, 99.0),
        candle(4_500_000, 99.0, 102.0, 98.0, 101.0),
    ]);

    let simulated = simulate_trade(
        &trade,
        &candles,
        0.1,
        ProtectionMode::StructureBreakFailedRetest,
    )
    .expect("simulation");

    assert_eq!(simulated.one_r_confirmation_time_ms, Some(900_000));
    assert_eq!(simulated.structure_break_time_ms, Some(1_800_000));
    assert_eq!(simulated.failed_retest_time_ms, Some(2_700_000));
    assert_eq!(simulated.activation_time_ms, Some(2_700_000));
    assert!(nearly_equal(simulated.structure_line.expect("line"), 97.0));
    assert_eq!(simulated.exit_time_ms, 3_600_000);
    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenStop);
}

#[test]
fn structure_break_candle_cannot_also_be_its_own_failed_retest() {
    let trade = fixed_short(3_600_000, 102.0, ExitReason::StopLoss);
    let mut candles = twenty_bar_history_before_entry();
    candles.extend([
        candle(900_000, 100.0, 100.5, 97.0, 98.0),
        candle(1_800_000, 98.0, 97.2, 96.5, 96.8),
        candle(2_700_000, 96.8, 96.9, 96.0, 96.2),
        candle(3_600_000, 96.2, 102.0, 96.0, 101.0),
    ]);

    let simulated = simulate_trade(
        &trade,
        &candles,
        0.1,
        ProtectionMode::StructureBreakFailedRetest,
    )
    .expect("simulation");

    assert_eq!(simulated.structure_break_time_ms, Some(1_800_000));
    assert_eq!(simulated.failed_retest_time_ms, None);
    assert_eq!(simulated.activation_time_ms, None);
    assert_eq!(simulated.kind, CounterfactualExitKind::BaselineUnchanged);
}

#[test]
fn retest_that_closes_above_frozen_line_does_not_activate() {
    let trade = fixed_short(5_400_000, 102.0, ExitReason::StopLoss);
    let mut candles = twenty_bar_history_before_entry();
    candles.extend([
        candle(900_000, 100.0, 100.5, 97.0, 98.0),
        candle(1_800_000, 98.0, 98.2, 96.5, 96.8),
        candle(2_700_000, 96.8, 97.3, 96.6, 97.1),
        candle(3_600_000, 97.1, 97.2, 96.5, 96.9),
        candle(4_500_000, 96.9, 100.0, 96.4, 99.0),
        candle(5_400_000, 99.0, 102.0, 98.0, 101.0),
    ]);

    let simulated = simulate_trade(
        &trade,
        &candles,
        0.1,
        ProtectionMode::StructureBreakFailedRetest,
    )
    .expect("simulation");

    assert_eq!(simulated.failed_retest_time_ms, Some(3_600_000));
    assert_eq!(simulated.activation_time_ms, Some(3_600_000));
    assert_eq!(simulated.exit_time_ms, 4_500_000);
}

#[test]
fn first_structure_line_stays_frozen_after_deeper_breaks() {
    let mut candles = twenty_bar_history_before_entry();
    candles.extend([
        candle(900_000, 100.0, 100.5, 97.0, 98.0),
        candle(1_800_000, 98.0, 98.2, 96.5, 96.8),
        candle(2_700_000, 96.8, 96.9, 95.5, 95.8),
    ]);
    let mut state = ProtectionState::default();

    update_protection_state(
        ProtectionMode::StructureBreakFailedRetest,
        &mut state,
        &candles,
        20,
        98.0,
    );
    update_protection_state(
        ProtectionMode::StructureBreakFailedRetest,
        &mut state,
        &candles,
        21,
        98.0,
    );
    update_protection_state(
        ProtectionMode::StructureBreakFailedRetest,
        &mut state,
        &candles,
        22,
        98.0,
    );

    assert_eq!(state.structure_break_time_ms, Some(1_800_000));
    assert!(nearly_equal(state.structure_line.expect("line"), 97.0));
    assert_eq!(state.failed_retest_time_ms, None);
}
