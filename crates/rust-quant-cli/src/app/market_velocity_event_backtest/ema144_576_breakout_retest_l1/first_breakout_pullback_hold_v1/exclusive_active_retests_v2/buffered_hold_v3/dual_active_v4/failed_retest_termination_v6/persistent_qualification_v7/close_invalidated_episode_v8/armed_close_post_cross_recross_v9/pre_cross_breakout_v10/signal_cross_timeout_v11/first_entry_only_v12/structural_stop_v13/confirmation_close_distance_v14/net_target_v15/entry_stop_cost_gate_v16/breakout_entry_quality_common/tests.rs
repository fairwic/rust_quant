use super::*;
use crate::app::market_velocity_event_backtest::BacktestCandle;

/// 生成只包含当前测试所需指标的完成 K。
fn candle(ts: i64, close: f64, ema144: f64, atr14: f64) -> ComputedCandle {
    ComputedCandle {
        candle: BacktestCandle {
            ts,
            open: close,
            high: close + 0.5,
            low: close - 0.5,
            close,
            volume: 1.0,
        },
        volume_ccy: None,
        sma: None,
        ema: None,
        ema12: None,
        ema144: Some(ema144),
        ema169: None,
        ema696: None,
        previous_volume_avg: None,
        previous_range_avg: None,
        rsi14: None,
        atr14: Some(atr14),
        bollinger_middle: None,
        bollinger_upper: None,
        bollinger_lower: None,
        bollinger_bandwidth_pct: None,
        macd_line: None,
        macd_signal_line: None,
        macd_histogram: None,
    }
}

#[test]
fn qualification_cycle_rejects_a_relation_flip_even_if_relation_returns() {
    let candles = vec![
        candle(0, 100.0, 101.0, 1.0),
        candle(1, 100.0, 99.0, 1.0),
        candle(2, 100.0, 101.0, 1.0),
    ];
    let ema576 = vec![Some(100.0); candles.len()];
    let evidence = qualification_cycle_evidence(&candles, &ema576, "short", 0, 2).unwrap();
    assert!(!evidence.passed);
    assert_eq!(evidence.first_failure_ts_ms, Some(1));
}

#[test]
fn breakout_distance_uses_confirmation_close_not_wick() {
    let mut candles = vec![candle(0, 102.4, 99.0, 1.0)];
    candles[0].candle.high = 110.0;
    let evidence = breakout_distance_evidence(
        &candles,
        &[Some(100.0)],
        "long",
        0,
        BREAKOUT_DISTANCE_ATR,
        "test_distance_blocker",
    )
    .unwrap();
    assert!(!evidence.passed);
    assert!((evidence.metric_value - 2.4).abs() < 1e-12);
}

#[test]
fn relaxed_breakout_distance_accepts_exactly_two_atr() {
    let candles = vec![candle(0, 102.0, 99.0, 1.0)];
    let evidence = breakout_distance_evidence(
        &candles,
        &[Some(100.0)],
        "long",
        0,
        RELAXED_BREAKOUT_DISTANCE_ATR,
        "test_relaxed_distance_blocker",
    )
    .unwrap();
    assert!(evidence.passed);
    assert_eq!(evidence.threshold, 2.0);
}

#[test]
fn acceptance_requires_eight_prior_closes_and_no_early_retest() {
    let candles = (0..10)
        .map(|idx| candle(idx, 101.0, 90.0, 1.0))
        .collect::<Vec<_>>();
    let ema576 = vec![Some(100.0); candles.len()];
    let evidence = acceptance_evidence(
        &candles,
        &ema576,
        "long",
        1,
        9,
        AcceptanceBoundary::Ema144RetestZone,
    )
    .unwrap();
    assert!(evidence.passed);
    assert_eq!(evidence.metric_value, 8.0);

    let early = acceptance_evidence(
        &candles,
        &ema576,
        "long",
        1,
        6,
        AcceptanceBoundary::Ema144RetestZone,
    )
    .unwrap();
    assert!(!early.passed);
    assert_eq!(early.metric_value, 6.0);
}

#[test]
fn ema576_hold_allows_touch_but_rejects_an_intrabar_cross() {
    let mut candles = (0..10)
        .map(|idx| candle(idx, 101.0, 90.0, 1.0))
        .collect::<Vec<_>>();
    let ema576 = vec![Some(100.0); candles.len()];
    candles[4].candle.low = 100.0;
    let touch = acceptance_evidence(
        &candles,
        &ema576,
        "long",
        1,
        9,
        AcceptanceBoundary::Ema576IntrabarHold,
    )
    .unwrap();
    assert!(touch.passed);

    candles[4].candle.low = 99.9;
    let crossed = acceptance_evidence(
        &candles,
        &ema576,
        "long",
        1,
        9,
        AcceptanceBoundary::Ema576IntrabarHold,
    )
    .unwrap();
    assert!(!crossed.passed);
    assert_eq!(crossed.first_failure_ts_ms, Some(4));
}

#[test]
fn composite_gate_requires_all_three_frozen_components() {
    let mut candles = (0..10)
        .map(|idx| candle(idx, 101.0, 90.0, 1.0))
        .collect::<Vec<_>>();
    candles[1].candle.close = 102.5;
    let ema576 = vec![Some(100.0); candles.len()];
    let passing = composite_evidence(
        &candles,
        &ema576,
        "long",
        0,
        1,
        9,
        BREAKOUT_DISTANCE_ATR,
        AcceptanceBoundary::Ema144RetestZone,
    )
    .unwrap();
    assert!(passing.passed);
    assert_eq!(passing.metric_value, 7.0);

    candles[1].candle.close = 102.4;
    let failing = composite_evidence(
        &candles,
        &ema576,
        "long",
        0,
        1,
        9,
        BREAKOUT_DISTANCE_ATR,
        AcceptanceBoundary::Ema144RetestZone,
    )
    .unwrap();
    assert!(!failing.passed);
    assert_eq!(failing.metric_value, 5.0);
}

#[test]
fn relation_cross_before_signal_invalidates_the_old_breakout_episode() {
    let mut candles = (0..10)
        .map(|idx| candle(idx, 101.0, 99.0, 1.0))
        .collect::<Vec<_>>();
    candles[1].candle.close = 102.0;
    for candle in candles.iter_mut().skip(5) {
        candle.ema144 = Some(101.0);
    }
    let ema576 = vec![Some(100.0); candles.len()];

    let v22 = composite_evidence(
        &candles,
        &ema576,
        "long",
        0,
        1,
        9,
        RELAXED_BREAKOUT_DISTANCE_ATR,
        AcceptanceBoundary::Ema576IntrabarHold,
    )
    .unwrap();
    assert!(v22.passed);

    let v23 = composite_relation_intact_until_signal_evidence(&candles, &ema576, "long", 0, 1, 9)
        .unwrap();
    assert!(!v23.passed);
    assert_eq!(v23.metric_value, 6.0);
    assert_eq!(v23.first_failure_ts_ms, Some(5));
}

#[test]
fn mirrored_short_relation_cross_before_signal_is_also_invalid() {
    let mut candles = (0..10)
        .map(|idx| candle(idx, 99.0, 101.0, 1.0))
        .collect::<Vec<_>>();
    candles[1].candle.close = 98.0;
    for candle in candles.iter_mut().skip(5) {
        candle.ema144 = Some(99.0);
    }
    let ema576 = vec![Some(100.0); candles.len()];

    let v22 = composite_evidence(
        &candles,
        &ema576,
        "short",
        0,
        1,
        9,
        RELAXED_BREAKOUT_DISTANCE_ATR,
        AcceptanceBoundary::Ema576IntrabarHold,
    )
    .unwrap();
    assert!(v22.passed);

    let v23 = composite_relation_intact_until_signal_evidence(&candles, &ema576, "short", 0, 1, 9)
        .unwrap();
    assert!(!v23.passed);
    assert_eq!(v23.metric_value, 6.0);
    assert_eq!(v23.first_failure_ts_ms, Some(5));
}

#[test]
fn acceptance_window_long_uses_any_of_the_first_eight_highs() {
    let mut candles = (0..10)
        .map(|idx| candle(idx, 101.0, 99.0, 1.0))
        .collect::<Vec<_>>();
    candles[1].candle.close = 101.1;
    candles[4].candle.high = 102.0;
    let ema576 = vec![Some(100.0); candles.len()];

    let v23 = composite_relation_intact_until_signal_evidence(&candles, &ema576, "long", 0, 1, 9)
        .unwrap();
    assert!(!v23.passed);

    let v24 = composite_acceptance_window_extreme_relation_intact_until_signal_evidence(
        &candles, &ema576, "long", 0, 1, 9,
    )
    .unwrap();
    assert!(v24.passed);
    assert_eq!(v24.metric_value, 2.0);
}

#[test]
fn acceptance_window_short_mirrors_the_low_extreme() {
    let mut candles = (0..10)
        .map(|idx| candle(idx, 99.0, 101.0, 1.0))
        .collect::<Vec<_>>();
    candles[1].candle.close = 98.9;
    candles[6].candle.low = 98.0;
    let ema576 = vec![Some(100.0); candles.len()];

    let v24 = composite_acceptance_window_extreme_relation_intact_until_signal_evidence(
        &candles, &ema576, "short", 0, 1, 9,
    )
    .unwrap();
    assert!(v24.passed);
    assert_eq!(v24.metric_value, 2.0);
}

#[test]
fn acceptance_window_does_not_use_an_extreme_after_the_eighth_close() {
    let mut candles = (0..11)
        .map(|idx| candle(idx, 101.0, 99.0, 1.0))
        .collect::<Vec<_>>();
    candles[8].candle.high = 103.0;
    let ema576 = vec![Some(100.0); candles.len()];

    let v24 = composite_acceptance_window_extreme_relation_intact_until_signal_evidence(
        &candles, &ema576, "long", 0, 1, 10,
    )
    .unwrap();
    assert!(!v24.passed);
    assert_eq!(v24.metric_value, 1.5);
}

#[test]
fn acceptance_window_extreme_does_not_bypass_close_or_intrabar_hold() {
    let mut candles = (0..10)
        .map(|idx| candle(idx, 101.0, 99.0, 1.0))
        .collect::<Vec<_>>();
    candles[4].candle.high = 102.5;
    candles[5].candle.low = 99.9;
    let ema576 = vec![Some(100.0); candles.len()];

    let intrabar_cross = composite_acceptance_window_extreme_relation_intact_until_signal_evidence(
        &candles, &ema576, "long", 0, 1, 9,
    )
    .unwrap();
    assert!(!intrabar_cross.passed);

    candles[5].candle.low = 100.5;
    candles[6].candle.close = 99.9;
    let opposite_close = composite_acceptance_window_extreme_relation_intact_until_signal_evidence(
        &candles, &ema576, "long", 0, 1, 9,
    )
    .unwrap();
    assert!(!opposite_close.passed);
}

#[test]
fn six_close_window_accepts_a_long_extreme_by_the_sixth_close() {
    let mut candles = (0..8)
        .map(|idx| candle(idx, 101.0, 99.0, 1.0))
        .collect::<Vec<_>>();
    candles[1].candle.close = 101.1;
    candles[4].candle.high = 102.0;
    let ema576 = vec![Some(100.0); candles.len()];

    let v25 = composite_acceptance_window_extreme_six_close_relation_intact_until_signal_evidence(
        &candles, &ema576, "long", 0, 1, 7,
    )
    .unwrap();

    assert!(v25.passed);
    assert_eq!(v25.metric_value, 2.0);
    assert_eq!(v25.quality_confirmed_ts_ms, Some(7));
}

#[test]
fn six_close_window_mirrors_the_short_extreme() {
    let mut candles = (0..8)
        .map(|idx| candle(idx, 99.0, 101.0, 1.0))
        .collect::<Vec<_>>();
    candles[1].candle.close = 98.9;
    candles[5].candle.low = 98.0;
    let ema576 = vec![Some(100.0); candles.len()];

    let v25 = composite_acceptance_window_extreme_six_close_relation_intact_until_signal_evidence(
        &candles, &ema576, "short", 0, 1, 7,
    )
    .unwrap();

    assert!(v25.passed);
    assert_eq!(v25.metric_value, 2.0);
    assert_eq!(v25.quality_confirmed_ts_ms, Some(7));
}

#[test]
fn seventh_close_cannot_backfill_v25_but_remains_inside_v24() {
    let mut candles = (0..10)
        .map(|idx| candle(idx, 101.0, 99.0, 1.0))
        .collect::<Vec<_>>();
    candles[6].candle.high = 102.5;
    let ema576 = vec![Some(100.0); candles.len()];

    let v25 = composite_acceptance_window_extreme_six_close_relation_intact_until_signal_evidence(
        &candles, &ema576, "long", 0, 1, 9,
    )
    .unwrap();
    let v24 = composite_acceptance_window_extreme_relation_intact_until_signal_evidence(
        &candles, &ema576, "long", 0, 1, 9,
    )
    .unwrap();

    assert!(!v25.passed);
    assert_eq!(v25.metric_value, 1.5);
    assert!(v24.passed);
    assert_eq!(v24.metric_value, 2.5);
}

#[test]
fn six_close_window_cannot_confirm_before_the_sixth_close_completes() {
    let mut candles = (0..8)
        .map(|idx| candle(idx, 101.0, 99.0, 1.0))
        .collect::<Vec<_>>();
    candles[3].candle.high = 102.0;
    let ema576 = vec![Some(100.0); candles.len()];

    let early =
        composite_acceptance_window_extreme_six_close_relation_intact_until_signal_evidence(
            &candles, &ema576, "long", 0, 1, 5,
        )
        .unwrap();
    let completed =
        composite_acceptance_window_extreme_six_close_relation_intact_until_signal_evidence(
            &candles, &ema576, "long", 0, 1, 6,
        )
        .unwrap();

    assert!(!early.passed);
    assert!(completed.passed);
    assert_eq!(completed.quality_confirmed_ts_ms, Some(6));
}

#[test]
fn frozen_candidate_set_hash_uses_sorted_ids_with_trailing_newlines() {
    let ids = BTreeMap::from([("B".to_owned(), ()), ("A".to_owned(), ())]);

    assert_eq!(
        frozen_candidate_set_sha256(ids.keys()),
        "daee1cd25194ae952d046ad9b9c81d3c07dc5332440b58d6d7461b248be56712"
    );
}
