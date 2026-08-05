use super::*;

fn point(volume_event: bool) -> IndicatorPoint {
    IndicatorPoint {
        volume_event,
        ..IndicatorPoint::default()
    }
}

fn intent_fixture() -> (Vec<Candle>, IndicatorSeries) {
    let candles = (0..4)
        .map(|index| Candle {
            timestamp_ms: index as i64 * 900_000,
            open: 100.0,
            high: 102.0,
            low: 98.0,
            close: 101.0,
            volume: 10.0,
        })
        .collect::<Vec<_>>();
    let mut points = vec![IndicatorPoint::default(); 4];
    points[3].atr14 = Some(2.0);
    points[3].filtered_volume_ratio = Some(3.0);
    (candles, IndicatorSeries { points })
}

#[test]
fn nearest_anchor_is_limited_to_t_minus_5_through_t_minus_32() {
    let mut points = vec![point(false); 50];
    points[44].volume_event = true; // t-5
    points[45].volume_event = true; // t-4, must be excluded
    let series = IndicatorSeries { points };

    assert_eq!(nearest_volume_anchor(&series, 49), Some((44, 5)));
}

#[test]
fn nearest_anchor_does_not_fallback_after_selection() {
    let mut points = vec![point(false); 50];
    points[42].volume_event = true;
    points[30].volume_event = true;
    let series = IndicatorSeries { points };

    assert_eq!(nearest_volume_anchor(&series, 49), Some((42, 7)));
}

#[test]
fn recent_macd_death_cross_covers_signal_and_previous_two_bars_only() {
    for cross_index in [3_usize, 4, 5] {
        let mut points = vec![IndicatorPoint::default(); 6];
        for point in &mut points {
            point.macd_histogram = Some(1.0);
        }
        for point in &mut points[cross_index..] {
            point.macd_histogram = Some(-1.0);
        }
        let series = IndicatorSeries { points };

        assert_eq!(recent_macd_death_cross(&series, 5), Some(true));
    }

    let mut points = vec![IndicatorPoint::default(); 6];
    for point in &mut points {
        point.macd_histogram = Some(1.0);
    }
    for point in &mut points[2..] {
        point.macd_histogram = Some(-1.0);
    }
    let series = IndicatorSeries { points };
    assert_eq!(recent_macd_death_cross(&series, 5), Some(false));
}

#[test]
fn recent_macd_death_cross_fails_closed_when_history_is_missing() {
    let mut points = vec![IndicatorPoint::default(); 6];
    for point in &mut points {
        point.macd_histogram = Some(-1.0);
    }
    points[3].macd_histogram = None;

    assert_eq!(
        recent_macd_death_cross(&IndicatorSeries { points }, 5),
        None
    );
}

#[test]
fn ema_short_structure_line_excludes_the_signal_bar() {
    let candles = vec![
        Candle {
            timestamp_ms: 0,
            open: 100.0,
            high: 102.0,
            low: 95.0,
            close: 99.0,
            volume: 1.0,
        },
        Candle {
            timestamp_ms: 900_000,
            open: 99.0,
            high: 100.0,
            low: 98.0,
            close: 99.0,
            volume: 1.0,
        },
        Candle {
            timestamp_ms: 1_800_000,
            open: 99.0,
            high: 100.0,
            low: 90.0,
            close: 94.0,
            volume: 1.0,
        },
    ];

    assert_eq!(prior_extremes(&candles, 2, 2), Some((102.0, 95.0)));
    assert!(candles[2].close < 95.0);
}

#[test]
fn bollinger_reclaim_owns_exit_when_old_long_family_overlaps() {
    let (candles, indicators) = intent_fixture();
    let intent = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Long,
        IntentContext {
            rsi_pattern_long: true,
            three_bar_long: true,
            bollinger_lower_reclaim: Some(BollingerLowerReclaimLongResult {
                stop_price: 97.9,
                target_price: 105.0,
                display_volume_ratio: Some(4.0),
            }),
            ema596_reclaim_departure: Some(Ema596ReclaimDepartureLongResult {
                stop_price: 97.0,
                target_price: 110.0,
                display_volume_ratio: 5.0,
            }),
            take_profit_atr: Some(3.0),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV3,
    )
    .expect("overlapping long families should still build one intent");

    assert_eq!(intent.exit_policy, ExitPolicy::BollingerLowerReclaim);
    assert_eq!(intent.stop_price, Some(97.9));
    assert_eq!(intent.target_price, Some(105.0));
    assert!(intent
        .families
        .contains(&SignalFamily::BollingerLowerReclaimLong));
    assert!(intent.families.contains(&SignalFamily::RsiOversoldPattern));
}

#[test]
fn effort_no_result_owns_exit_when_old_short_family_overlaps() {
    let (candles, indicators) = intent_fixture();
    let intent = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Short,
        IntentContext {
            rsi_pattern_short: true,
            false_breakout: Some(FalseBreakoutSignal {
                frozen_high: 108.0,
                volume_ratio: 3.0,
                take_profit_atr: 2.7,
            }),
            effort_no_result: Some(EffortNoResultShortResult {
                stop_price: 110.0,
                activation_ticks: 50,
                target_ticks: 75,
                display_volume_ratio: Some(5.0),
            }),
            take_profit_atr: Some(3.0),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV3,
    )
    .expect("overlapping short families should still build one intent");

    assert_eq!(intent.exit_policy, ExitPolicy::EffortNoResult);
    assert_eq!(intent.stop_price, Some(110.0));
    assert_eq!(intent.activation_ticks, Some(50));
    assert_eq!(intent.target_ticks, Some(75));
    assert!(intent.families.contains(&SignalFamily::EffortNoResultShort));
    assert!(intent
        .families
        .contains(&SignalFamily::AnchorFalseBreakShort));
}

#[test]
fn v20_failed_acceptance_uses_frozen_stop_and_lower_boundary_target() {
    let (candles, indicators) = intent_fixture();
    let intent = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Short,
        IntentContext {
            upthrust_failed_acceptance: Some(UpthrustFailedAcceptanceSignal {
                frozen_stop_high: 108.1,
                frozen_target_low: 99.0,
                breakout_volume_ratio: 5.81,
                right_side_confirmed: false,
                target_consumption_ratio: None,
                active_parent_horizontal_anchor: None,
            }),
            take_profit_atr: Some(3.6),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV20,
    )
    .expect("V20 failed acceptance should build a fixed structural intent");

    assert_eq!(intent.exit_policy, ExitPolicy::Fixed);
    assert!((intent.stop_price.expect("stop must exist") - 108.1).abs() < 1e-9);
    assert_eq!(intent.target_price, Some(99.0));
    assert_eq!(intent.volume_ratio, Some(5.81));
    assert!(intent
        .families
        .contains(&SignalFamily::AnchorUpthrustFailedAcceptanceShort));
}

#[test]
fn v21_right_side_confirmation_uses_an_independent_family_identity() {
    let (candles, indicators) = intent_fixture();
    let intent = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Short,
        IntentContext {
            upthrust_failed_acceptance: Some(UpthrustFailedAcceptanceSignal {
                frozen_stop_high: 108.1,
                frozen_target_low: 99.0,
                breakout_volume_ratio: 5.81,
                right_side_confirmed: true,
                target_consumption_ratio: Some(0.2),
                active_parent_horizontal_anchor: None,
            }),
            take_profit_atr: Some(3.6),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV20,
    )
    .expect("V21 confirmation should preserve the frozen structural plan");

    assert!((intent.stop_price.expect("stop must exist") - 108.1).abs() < 1e-9);
    assert_eq!(intent.target_price, Some(99.0));
    assert_eq!(intent.anchor_upthrust_target_consumption_ratio, Some(0.2));
    assert!(intent
        .families
        .contains(&SignalFamily::AnchorUpthrustFailedAcceptanceRightSideShort));
    assert!(!intent
        .families
        .contains(&SignalFamily::AnchorUpthrustFailedAcceptanceShort));
}

#[test]
fn v19_cancels_false_breakout_pending_when_signal_bar_has_long_lower_wick() {
    let candles = vec![
        Candle {
            timestamp_ms: 0,
            open: 44.0,
            high: 44.1,
            low: 43.8,
            close: 44.0,
            volume: 10.0,
        },
        Candle {
            timestamp_ms: 900_000,
            open: 43.30,
            high: 43.35,
            low: 43.15,
            close: 43.28,
            volume: 10.0,
        },
    ];
    let indicators = IndicatorSeries {
        points: vec![IndicatorPoint::default(); candles.len()],
    };
    let pending = FalseBreakoutPending {
        breakout_index: 0,
        anchor_high: 43.92,
        anchor_low: 43.30,
        breakout_open: 43.80,
        breakout_high: 44.1,
        breakout_volume: 10.0,
        volume_ratio: 3.07,
        take_profit_atr: 2.7,
        active_parent_horizontal_anchor: None,
    };
    let mut v18 = SignalState {
        false_breakout_pending: Some(pending.clone()),
        ..SignalState::default()
    };
    let mut v19 = SignalState {
        false_breakout_pending: Some(pending),
        reject_false_breakout_lower_wick: true,
        ..SignalState::default()
    };

    let v18_signal = v18
        .update_breakout_states(
            &candles,
            &indicators,
            1,
            0.2,
            0.01,
            Some(2.7),
            43.5,
            43.6,
            43.7,
            37.57,
        )
        .0;
    let v19_signal = v19
        .update_breakout_states(
            &candles,
            &indicators,
            1,
            0.2,
            0.01,
            Some(2.7),
            43.5,
            43.6,
            43.7,
            37.57,
        )
        .0;

    assert!(v18_signal.is_some());
    assert!(v19_signal.is_none());
    assert!(v19.false_breakout_pending.is_none());
}
