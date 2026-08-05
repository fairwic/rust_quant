use super::*;

/// 构造只包含因果边界所需字段的 15 分钟测试 K 线。
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

/// 构造冻结风险为 2 个价格单位的严格横盘 Fixed 多单。
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

/// 失效只认完成收盘，盘中下影刺穿冻结上沿不能触发退出。
#[test]
fn upper_wick_retest_without_close_below_does_not_fail() {
    let trade = fixed_long(2_700_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 102.3, 99.8, 102.1),
        candle(1_800_000, 102.0, 102.2, 99.0, 100.0),
        candle(2_700_000, 100.0, 100.2, 98.0, 98.5),
    ];

    let simulated = simulate_trade(&trade, &candles, 99.5).expect("simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.failure_close_time_ms, None);
    assert_eq!(simulated.kind, StructuralExitKind::BaselineUnchanged);
    assert!(nearly_equal(simulated.exit_price, 98.0));
}

/// 完成棒确认失效后必须等待下一根真实开盘，不能按失效收盘价成交。
#[test]
fn completed_failure_close_exits_at_next_actual_open() {
    let trade = fixed_long(3_600_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 102.3, 99.8, 102.1),
        candle(1_800_000, 101.0, 101.2, 99.0, 99.4),
        candle(2_700_000, 99.2, 100.0, 98.8, 99.5),
        candle(3_600_000, 99.5, 99.7, 98.0, 98.5),
    ];

    let simulated = simulate_trade(&trade, &candles, 99.5).expect("simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.failure_close_time_ms, Some(1_800_000));
    assert!(nearly_equal(simulated.failure_close_price.unwrap(), 99.4));
    assert_eq!(simulated.exit_time_ms, 2_700_000);
    assert!(nearly_equal(simulated.exit_price, 99.2));
    assert_eq!(
        simulated.kind,
        StructuralExitKind::RangeUpperFailureNextOpen
    );
}

/// 1R 上影不能替代完成收盘激活，避免使用未完成路径信息。
#[test]
fn activation_requires_completed_close_instead_of_intrabar_high() {
    let trade = fixed_long(2_700_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 103.0, 99.8, 101.9),
        candle(1_800_000, 101.0, 101.2, 99.0, 99.4),
        candle(2_700_000, 99.4, 99.6, 98.0, 98.5),
    ];

    let simulated = simulate_trade(&trade, &candles, 99.5).expect("simulation");

    assert_eq!(simulated.activation_time_ms, None);
    assert_eq!(simulated.failure_close_time_ms, None);
    assert_eq!(simulated.kind, StructuralExitKind::BaselineUnchanged);
}

/// 激活状态从下一根开始生效，确认激活的同一根不能反向自失效。
#[test]
fn activation_close_cannot_also_be_the_failure_close() {
    let trade = fixed_long(3_600_000, 98.0, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 102.3, 99.8, 102.1),
        candle(1_800_000, 102.0, 102.2, 99.6, 100.0),
        candle(2_700_000, 100.0, 100.2, 99.0, 99.4),
        candle(3_600_000, 99.4, 99.6, 98.0, 98.5),
    ];

    let simulated = simulate_trade(&trade, &candles, 99.5).expect("simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.failure_close_time_ms, Some(2_700_000));
    assert_eq!(simulated.exit_time_ms, 3_600_000);
}

/// 原目标若在候选失效棒盘中先成交，收盘后已没有持仓可供结构退出。
#[test]
fn baseline_target_on_possible_failure_candle_wins_before_close() {
    let trade = fixed_long(1_800_000, 106.0, ExitReason::TakeProfit);
    let candles = vec![
        candle(900_000, 100.0, 102.3, 99.8, 102.1),
        candle(1_800_000, 101.0, 106.5, 99.0, 99.4),
    ];

    let simulated = simulate_trade(&trade, &candles, 99.5).expect("simulation");

    assert_eq!(simulated.activation_time_ms, Some(900_000));
    assert_eq!(simulated.failure_close_time_ms, None);
    assert_eq!(simulated.kind, StructuralExitKind::BaselineUnchanged);
    assert!(nearly_equal(simulated.exit_price, 106.0));
}

/// 下一开盘与原成交完全相同时保留原身份，避免虚增“改变退出”覆盖率。
#[test]
fn identical_baseline_fill_at_scheduled_open_is_not_counted_as_changed() {
    let trade = fixed_long(2_700_000, 99.2, ExitReason::StopLoss);
    let candles = vec![
        candle(900_000, 100.0, 102.3, 99.8, 102.1),
        candle(1_800_000, 101.0, 101.2, 99.0, 99.4),
        candle(2_700_000, 99.2, 99.4, 98.8, 99.0),
    ];

    let simulated = simulate_trade(&trade, &candles, 99.5).expect("simulation");

    assert_eq!(simulated.failure_close_time_ms, Some(1_800_000));
    assert_eq!(simulated.kind, StructuralExitKind::BaselineUnchanged);
    assert_eq!(simulated.exit_time_ms, 2_700_000);
    assert!(nearly_equal(simulated.exit_price, 99.2));
}
