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

fn fixed_trade(
    direction: Direction,
    exit_time_ms: i64,
    exit_price: f64,
    reason: ExitReason,
) -> Trade {
    let entry_price = 100.0;
    let initial_stop = match direction {
        Direction::Long => 98.0,
        Direction::Short => 102.0,
    };
    let gross_pnl = direction.gross_pnl(entry_price, exit_price);
    Trade {
        direction,
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
        entry_price,
        exit_price,
        initial_stop,
        exit_reason: reason,
        gross_pnl,
        net_pnl: gross_pnl,
        initial_risk: 2.0,
        net_r: gross_pnl / 2.0,
        anchor_upthrust_target_consumption_ratio: None,
        volume_ratio: Some(3.0),
        rsi: Some(60.0),
    }
}

#[test]
fn net_break_even_rounds_to_the_safe_tick_for_both_directions() {
    let long_stop = net_break_even_price(Direction::Long, 100.0, 0.1, 8.0);
    let short_stop = net_break_even_price(Direction::Short, 100.0, 0.1, 8.0);

    assert!(nearly_equal(long_stop, 100.2));
    assert!(nearly_equal(short_stop, 99.8));
    assert!(net_r(Direction::Long, 100.0, long_stop, 2.0, 8.0) >= 0.0);
    assert!(net_r(Direction::Short, 100.0, short_stop, 2.0, 8.0) >= 0.0);
}

#[test]
fn activation_uses_one_height_or_the_cost_floor_whichever_is_farther() {
    let long_be = net_break_even_price(Direction::Long, 100.0, 0.1, 8.0);
    let short_be = net_break_even_price(Direction::Short, 100.0, 0.1, 8.0);

    assert!(nearly_equal(
        activation_price(Direction::Long, 100.0, 1.0, long_be),
        101.0
    ));
    assert!(nearly_equal(
        activation_price(Direction::Short, 100.0, 1.0, short_be),
        99.0
    ));
    assert!(nearly_equal(
        activation_price(Direction::Long, 100.0, 0.05, long_be),
        long_be
    ));
    assert!(nearly_equal(
        activation_price(Direction::Short, 100.0, 0.05, short_be),
        short_be
    ));
}

#[test]
fn long_activation_candle_cannot_retroactively_trigger_its_own_stop() {
    let trade = fixed_trade(Direction::Long, 2_700_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 101.2, 99.0, 99.5),
        candle(1_800_000, 100.5, 100.7, 100.1, 100.4),
        candle(2_700_000, 100.4, 100.5, 98.0, 98.5),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, 1.0).expect("long simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.exit_time_ms, 1_800_000);
    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenStop);
    assert!(nearly_equal(simulated.exit_price, 100.2));
}

#[test]
fn short_mirror_activates_on_completed_low_and_stops_from_next_candle() {
    let trade = fixed_trade(Direction::Short, 2_700_000, 102.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 101.0, 98.8, 100.5),
        candle(1_800_000, 99.5, 100.0, 99.2, 99.9),
        candle(2_700_000, 99.9, 102.0, 99.8, 101.5),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, 1.0).expect("short simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.exit_time_ms, 1_800_000);
    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenStop);
    assert!(nearly_equal(simulated.exit_price, 99.8));
}

#[test]
fn gap_after_activation_uses_the_actual_next_open() {
    let trade = fixed_trade(Direction::Long, 2_700_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 101.2, 99.8, 101.0),
        candle(1_800_000, 99.0, 99.5, 98.8, 99.2),
        candle(2_700_000, 99.2, 99.3, 98.0, 98.5),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, 1.0).expect("gap simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.exit_time_ms, 1_800_000);
    assert_eq!(simulated.kind, CounterfactualExitKind::NetBreakEvenGapOpen);
    assert!(nearly_equal(simulated.exit_price, 99.0));
}

#[test]
fn original_target_keeps_priority_when_the_path_reaches_it_first() {
    let trade = fixed_trade(Direction::Long, 1_800_000, 106.0, ExitReason::TakeProfit);
    let candles = vec![
        candle(900_000, 100.0, 101.2, 99.8, 101.0),
        candle(1_800_000, 101.0, 106.5, 100.5, 105.0),
    ];

    let simulated = simulate_trade(&trade, &candles, 0.1, 1.0).expect("target simulation");

    assert_eq!(simulated.kind, CounterfactualExitKind::BaselineUnchanged);
    assert!(nearly_equal(simulated.exit_price, 106.0));
}
