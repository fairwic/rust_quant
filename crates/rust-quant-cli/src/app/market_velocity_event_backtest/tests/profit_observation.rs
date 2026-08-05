use super::*;

#[test]
fn simulate_trade_profit_observation_waits_for_a_later_completed_candle() {
    let candles = vec![
        BacktestCandle {
            ts: MS_15M,
            open: 100.0,
            high: 101.5,
            low: 99.5,
            close: 100.2,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 2,
            open: 100.2,
            high: 101.2,
            low: 99.8,
            close: 100.4,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 3,
            open: 100.4,
            high: 100.8,
            low: 99.9,
            close: 100.4,
            volume: 10.0,
        },
    ];
    let result = simulate_trade(
        &candles,
        0,
        MS_15M,
        100.0,
        MarketVelocityTradeDirection::Long,
        0.02,
        2.4,
        MS_15M * 4,
        None,
        None,
        None,
        true,
    );
    assert_eq!(
        result.reason,
        "profit_observation_pre_one_close_below_0_25r"
    );
    assert_eq!(result.exit_ts, MS_15M * 3);
    assert!((result.r.unwrap() - 0.2).abs() < 1e-12);
}

#[test]
fn simulate_trade_profit_observation_arms_target_relative_stop_for_next_candle() {
    let candles = vec![
        BacktestCandle {
            ts: MS_15M,
            open: 100.0,
            high: 101.0,
            low: 99.5,
            close: 100.5,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 2,
            open: 100.5,
            high: 102.4,
            low: 100.4,
            close: 102.0,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 3,
            open: 102.0,
            high: 102.1,
            low: 100.5,
            close: 101.0,
            volume: 10.0,
        },
    ];
    let result = simulate_trade(
        &candles,
        0,
        MS_15M,
        100.0,
        MarketVelocityTradeDirection::Long,
        0.02,
        2.4,
        MS_15M * 4,
        None,
        None,
        None,
        true,
    );
    assert_eq!(result.reason, "profit_protect_stop_hit");
    assert!((result.r.unwrap() - 0.3).abs() < 1e-12);
}

#[test]
fn simulate_trade_profit_observation_matches_strict_dynamic_target_boundary() {
    let candles = vec![
        BacktestCandle {
            ts: MS_15M,
            open: 100.0,
            high: 102.0,
            low: 99.5,
            close: 101.0,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 2,
            open: 101.0,
            high: 104.0,
            low: 100.8,
            close: 103.2,
            volume: 10.0,
        },
        BacktestCandle {
            ts: MS_15M * 3,
            open: 103.2,
            high: 103.4,
            low: 102.8,
            close: 103.0,
            volume: 10.0,
        },
    ];
    let result = simulate_trade(
        &candles,
        0,
        MS_15M,
        100.0,
        MarketVelocityTradeDirection::Long,
        0.02,
        2.0,
        MS_15M * 4,
        None,
        None,
        None,
        true,
    );

    assert_eq!(result.reason, "profit_protect_stop_hit");
    assert!((result.r.unwrap() - 1.5).abs() < 1e-12);
}
