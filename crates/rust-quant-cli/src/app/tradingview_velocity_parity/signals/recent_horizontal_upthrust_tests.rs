use super::*;

const CANDLE_INTERVAL_MS: i64 = 15 * 60 * 1_000;
const SHIB_START_MS: i64 = 1_783_271_700_000;

fn indicators(count: usize, breakout_index: usize, volume_ratio: f64) -> IndicatorSeries {
    let mut points = vec![IndicatorPoint::default(); count];
    points[breakout_index].volume_event = true;
    points[breakout_index].filtered_volume_ratio = Some(volume_ratio);
    IndicatorSeries { points }
}

fn evaluate_breakout_state(
    state: &mut SignalState,
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    tick_size: f64,
) -> Option<UpthrustFailedAcceptanceSignal> {
    let (_, signal, _) = state.update_breakout_states(
        candles,
        indicators,
        index,
        1.0,
        tick_size,
        Some(3.6),
        1.0,
        2.0,
        3.0,
        67.0,
    );
    signal
}

fn recent_horizontal_state(variant: AnchorUpthrustResearchVariant) -> SignalState {
    SignalState {
        enable_upthrust_failed_acceptance: true,
        anchor_upthrust_research_variant: variant,
        ..SignalState::default()
    }
}

#[test]
fn recent_horizontal_first_break_can_form_a_v20_timing_signal() {
    let mut candles = (0..20)
        .map(|index| Candle {
            timestamp_ms: index as i64 * CANDLE_INTERVAL_MS,
            open: 98.75,
            high: if index % 3 == 0 { 100.0 } else { 99.7 },
            low: if index % 3 == 1 { 97.5 } else { 97.8 },
            close: 98.75,
            volume: 10.0,
        })
        .collect::<Vec<_>>();
    candles.extend([
        Candle {
            timestamp_ms: 20 * CANDLE_INTERVAL_MS,
            open: 99.8,
            high: 100.5,
            low: 99.7,
            close: 100.3,
            volume: 100.0,
        },
        Candle {
            timestamp_ms: 21 * CANDLE_INTERVAL_MS,
            open: 100.4,
            high: 100.6,
            low: 99.6,
            close: 99.8,
            volume: 60.0,
        },
    ]);
    let indicators = indicators(candles.len(), 20, 6.0);
    let mut state =
        recent_horizontal_state(AnchorUpthrustResearchVariant::RecentHorizontalFirstBreak);

    assert!(evaluate_breakout_state(&mut state, &candles, &indicators, 20, 0.1).is_none());
    assert!(state.recent_horizontal_upthrust_pending.is_some());
    let signal = evaluate_breakout_state(&mut state, &candles, &indicators, 21, 0.1)
        .expect("the first rejection bar should reclaim the frozen horizontal upper boundary");

    assert_eq!(signal.frozen_target_low, 97.5);
    assert!(!signal.right_side_confirmed);
    assert!(state.recent_horizontal_upthrust_pending.is_none());

    let mut v26 = recent_horizontal_state(AnchorUpthrustResearchVariant::ActiveParentHorizontal);
    assert!(evaluate_breakout_state(&mut v26, &candles, &indicators, 20, 0.1).is_none());
    let signal = evaluate_breakout_state(&mut v26, &candles, &indicators, 21, 0.1)
        .expect("V26 should preserve a real close above the completed parent range");
    let evidence = signal
        .active_parent_horizontal_anchor
        .expect("V26 must expose its causal anchor in the candidate ledger");
    assert_eq!(evidence.start_time_ms, 0);
    assert_eq!(evidence.end_time_ms, 19 * CANDLE_INTERVAL_MS);
    assert_eq!(evidence.length_bars, 20);
    assert_eq!(evidence.upper, 100.0);
    assert_eq!(evidence.lower, 97.5);
    assert_eq!(evidence.breakout_open, None);

    let mut v27 = recent_horizontal_state(
        AnchorUpthrustResearchVariant::ActiveParentHorizontalBreakoutBodyRejection,
    );
    assert!(evaluate_breakout_state(&mut v27, &candles, &indicators, 20, 0.1).is_none());
    let signal = evaluate_breakout_state(&mut v27, &candles, &indicators, 21, 0.1)
        .expect("V27 should accept a close that fully negates the breakout body");
    let evidence = signal
        .active_parent_horizontal_anchor
        .expect("V27 must expose body-rejection evidence in the candidate ledger");
    assert_eq!(evidence.breakout_open, Some(99.8));
    assert_eq!(evidence.confirmation_close, Some(99.8));
    assert_eq!(evidence.breakout_body_rejection_depth_ticks, Some(0.0));
    assert_eq!(evidence.normalized_breakout_body_rejection_depth, None);
    assert_eq!(evidence.normalized_breakout_excess, None);

    let mut v28_candles = candles.clone();
    v28_candles[21] = Candle {
        timestamp_ms: 21 * CANDLE_INTERVAL_MS,
        open: 100.4,
        high: 100.6,
        low: 99.3,
        close: 99.5,
        volume: 60.0,
    };
    let mut v28 = recent_horizontal_state(
        AnchorUpthrustResearchVariant::ActiveParentHorizontalNormalizedBodyRejection10Pct,
    );
    assert!(evaluate_breakout_state(&mut v28, &v28_candles, &indicators, 20, 0.1).is_none());
    let signal = evaluate_breakout_state(&mut v28, &v28_candles, &indicators, 21, 0.1)
        .expect("V28 should accept rejection deeper than ten percent of the parent range");
    let evidence = signal
        .active_parent_horizontal_anchor
        .expect("V28 must expose normalized rejection evidence");
    assert!(
        (evidence
            .normalized_breakout_body_rejection_depth
            .expect("ratio")
            - 0.12)
            .abs()
            < 1e-12
    );
    assert_eq!(evidence.normalized_breakout_excess, None);

    let mut rejected_v29 = recent_horizontal_state(
        AnchorUpthrustResearchVariant::ActiveParentHorizontalShallowBreakoutExcess10Pct,
    );
    assert!(evaluate_breakout_state(&mut rejected_v29, &candles, &indicators, 20, 0.1).is_none());
    assert!(rejected_v29.recent_horizontal_upthrust_pending.is_some());
    assert!(evaluate_breakout_state(&mut rejected_v29, &candles, &indicators, 21, 0.1).is_none());
    assert!(rejected_v29.recent_horizontal_upthrust_pending.is_none());

    let mut boundary_v29_candles = candles.clone();
    boundary_v29_candles[20].close = 100.25;
    let mut boundary_v29 = recent_horizontal_state(
        AnchorUpthrustResearchVariant::ActiveParentHorizontalShallowBreakoutExcess10Pct,
    );
    assert!(evaluate_breakout_state(
        &mut boundary_v29,
        &boundary_v29_candles,
        &indicators,
        20,
        0.1,
    )
    .is_none());
    assert!(boundary_v29.recent_horizontal_upthrust_pending.is_some());
    let signal = evaluate_breakout_state(
        &mut boundary_v29,
        &boundary_v29_candles,
        &indicators,
        21,
        0.1,
    )
    .expect("V29 should include a breakout exactly ten percent above the parent range");
    let evidence = signal
        .active_parent_horizontal_anchor
        .expect("V29 must expose normalized breakout-excess evidence");
    assert!((evidence.normalized_breakout_excess.expect("ratio") - 0.10).abs() < 1e-12);
    assert_eq!(evidence.normalized_breakout_body_rejection_depth, None);
    assert_eq!(evidence.edge_transition_count, None);

    let mut rejected_v30 = recent_horizontal_state(
        AnchorUpthrustResearchVariant::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct,
    );
    assert!(evaluate_breakout_state(
        &mut rejected_v30,
        &boundary_v29_candles,
        &indicators,
        20,
        0.1,
    )
    .is_none());
    assert!(
        rejected_v30.recent_horizontal_upthrust_pending.is_some(),
        "V30 must arm the same V29 setup before applying its strict-subset gate"
    );
    assert!(evaluate_breakout_state(
        &mut rejected_v30,
        &boundary_v29_candles,
        &indicators,
        21,
        0.1,
    )
    .is_none());
    assert!(rejected_v30.recent_horizontal_upthrust_pending.is_none());
}

#[test]
fn v30_accepts_a_parent_with_three_pre_breakout_edge_transitions() {
    let mut candles = [
        (100.2, 101.0, 99.45, 100.5, 10.0),
        (100.0, 100.55, 99.45, 99.8, 10.0),
        (99.6, 100.55, 99.0, 99.5, 10.0),
        (99.8, 100.55, 99.5, 100.2, 10.0),
        (100.2, 101.0, 99.45, 100.4, 10.0),
        (100.0, 100.55, 99.45, 99.8, 10.0),
        (99.6, 100.55, 99.0, 99.6, 10.0),
        (99.8, 100.55, 99.5, 100.1, 10.0),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (open, high, low, close, volume))| Candle {
        timestamp_ms: index as i64 * CANDLE_INTERVAL_MS,
        open,
        high,
        low,
        close,
        volume,
    })
    .collect::<Vec<_>>();
    candles.extend([
        Candle {
            timestamp_ms: 8 * CANDLE_INTERVAL_MS,
            open: 100.8,
            high: 101.3,
            low: 100.7,
            close: 101.1,
            volume: 100.0,
        },
        Candle {
            timestamp_ms: 9 * CANDLE_INTERVAL_MS,
            open: 100.9,
            high: 100.95,
            low: 100.6,
            close: 100.68,
            volume: 60.0,
        },
    ]);
    let indicators = indicators(candles.len(), 8, 6.0);
    let mut state = recent_horizontal_state(
        AnchorUpthrustResearchVariant::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct,
    );

    assert!(evaluate_breakout_state(&mut state, &candles, &indicators, 8, 0.1).is_none());
    let signal = evaluate_breakout_state(&mut state, &candles, &indicators, 9, 0.1)
        .expect("three completed edge transitions should qualify the V30 parent range");
    let evidence = signal
        .active_parent_horizontal_anchor
        .expect("V30 must expose causal edge-transition evidence");

    assert_eq!(evidence.edge_transition_count, Some(3));
    assert!((evidence.normalized_breakout_excess.expect("ratio") - 0.05).abs() < 1e-12);
}

#[test]
fn v24_accepts_close_back_without_breaking_the_breakout_high() {
    let mut candles = (0..20)
        .map(|index| Candle {
            timestamp_ms: index as i64 * CANDLE_INTERVAL_MS,
            open: 98.75,
            high: if index % 3 == 0 { 100.0 } else { 99.7 },
            low: if index % 3 == 1 { 97.5 } else { 97.8 },
            close: 98.75,
            volume: 10.0,
        })
        .collect::<Vec<_>>();
    candles.extend([
        Candle {
            timestamp_ms: 20 * CANDLE_INTERVAL_MS,
            open: 99.8,
            high: 101.0,
            low: 99.7,
            close: 100.3,
            volume: 100.0,
        },
        Candle {
            timestamp_ms: 21 * CANDLE_INTERVAL_MS,
            open: 100.4,
            high: 100.8,
            low: 99.4,
            close: 99.7,
            volume: 60.0,
        },
    ]);
    let indicators = indicators(candles.len(), 20, 6.0);

    let mut v23 =
        recent_horizontal_state(AnchorUpthrustResearchVariant::RecentHorizontalFirstBreak);
    assert!(evaluate_breakout_state(&mut v23, &candles, &indicators, 20, 0.1).is_none());
    assert!(evaluate_breakout_state(&mut v23, &candles, &indicators, 21, 0.1).is_none());

    let mut v24 =
        recent_horizontal_state(AnchorUpthrustResearchVariant::RecentHorizontalFirstBreakCloseBack);
    assert!(evaluate_breakout_state(&mut v24, &candles, &indicators, 20, 0.1).is_none());
    let signal = evaluate_breakout_state(&mut v24, &candles, &indicators, 21, 0.1)
        .expect("V24 should accept a completed close back without a second high sweep");

    assert_eq!(signal.frozen_stop_high, 101.1);
    assert_eq!(signal.frozen_target_low, 97.5);
}

#[test]
fn v25_direction_efficiency_variants_reject_the_icp_directional_recovery() {
    let rows = [
        (2.128, 2.132, 2.111, 2.118, 3_157_011.0),
        (2.119, 2.149, 2.119, 2.135, 3_720_299.0),
        (2.134, 2.136, 2.119, 2.129, 1_312_033.0),
        (2.130, 2.134, 2.122, 2.125, 1_691_463.0),
        (2.124, 2.129, 2.107, 2.123, 3_787_173.0),
        (2.122, 2.136, 2.120, 2.127, 2_046_501.0),
        (2.127, 2.138, 2.125, 2.138, 1_125_647.0),
        (2.137, 2.150, 2.137, 2.144, 2_614_470.0),
        (2.143, 2.149, 2.139, 2.141, 985_349.0),
        (2.141, 2.145, 2.128, 2.142, 2_375_676.0),
        (2.141, 2.141, 2.130, 2.131, 1_429_113.0),
        (2.131, 2.150, 2.125, 2.150, 8_652_277.0),
        (2.149, 2.166, 2.141, 2.164, 7_872_816.0),
        (2.165, 2.166, 2.146, 2.148, 4_822_321.0),
    ];
    let candles = rows
        .into_iter()
        .enumerate()
        .map(|(index, (open, high, low, close, volume))| Candle {
            timestamp_ms: index as i64 * CANDLE_INTERVAL_MS,
            open,
            high,
            low,
            close,
            volume,
        })
        .collect::<Vec<_>>();
    let indicators = indicators(candles.len(), 12, 4.079_784_078_526_321_5);

    let mut v24 =
        recent_horizontal_state(AnchorUpthrustResearchVariant::RecentHorizontalFirstBreakCloseBack);
    assert!(evaluate_breakout_state(&mut v24, &candles, &indicators, 12, 0.001).is_none());
    assert!(v24.recent_horizontal_upthrust_pending.is_some());
    assert!(evaluate_breakout_state(&mut v24, &candles, &indicators, 13, 0.001).is_some());

    for variant in [
        AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency30,
        AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency35,
        AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency40,
        AnchorUpthrustResearchVariant::ActiveParentHorizontal,
        AnchorUpthrustResearchVariant::ActiveParentHorizontalBreakoutBodyRejection,
        AnchorUpthrustResearchVariant::ActiveParentHorizontalNormalizedBodyRejection10Pct,
        AnchorUpthrustResearchVariant::ActiveParentHorizontalShallowBreakoutExcess10Pct,
        AnchorUpthrustResearchVariant::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct,
    ] {
        let mut state = recent_horizontal_state(variant);
        assert!(evaluate_breakout_state(&mut state, &candles, &indicators, 12, 0.001).is_none());
        assert!(state.recent_horizontal_upthrust_pending.is_none());
        assert!(evaluate_breakout_state(&mut state, &candles, &indicators, 13, 0.001).is_none());
    }
}

#[test]
fn shib_continuation_is_rejected_by_all_recent_horizontal_variants() {
    let rows = [
        (4.346e-6, 4.369e-6, 4.346e-6, 4.350e-6, 50_911_800_000.0),
        (4.350e-6, 4.351e-6, 4.335e-6, 4.335e-6, 9_396_100_000.0),
        (4.334e-6, 4.350e-6, 4.333e-6, 4.349e-6, 4_492_900_000.0),
        (4.350e-6, 4.360e-6, 4.347e-6, 4.352e-6, 11_447_400_000.0),
        (4.353e-6, 4.363e-6, 4.353e-6, 4.359e-6, 11_404_500_000.0),
        (4.358e-6, 4.367e-6, 4.358e-6, 4.364e-6, 6_798_500_000.0),
        (4.364e-6, 4.371e-6, 4.360e-6, 4.361e-6, 7_146_600_000.0),
        (4.360e-6, 4.372e-6, 4.358e-6, 4.367e-6, 25_751_600_000.0),
        (4.368e-6, 4.369e-6, 4.354e-6, 4.361e-6, 15_232_500_000.0),
        (4.362e-6, 4.365e-6, 4.358e-6, 4.359e-6, 2_520_400_000.0),
        (4.359e-6, 4.364e-6, 4.358e-6, 4.362e-6, 3_898_800_000.0),
        (4.361e-6, 4.373e-6, 4.361e-6, 4.370e-6, 9_468_900_000.0),
        (4.370e-6, 4.371e-6, 4.363e-6, 4.364e-6, 3_279_500_000.0),
        (4.364e-6, 4.364e-6, 4.360e-6, 4.363e-6, 7_959_100_000.0),
        (4.363e-6, 4.365e-6, 4.362e-6, 4.364e-6, 4_736_100_000.0),
        (4.365e-6, 4.377e-6, 4.353e-6, 4.370e-6, 55_545_700_000.0),
        (4.370e-6, 4.389e-6, 4.366e-6, 4.381e-6, 25_140_500_000.0),
        (4.382e-6, 4.390e-6, 4.375e-6, 4.386e-6, 15_214_600_000.0),
        (4.386e-6, 4.420e-6, 4.385e-6, 4.403e-6, 37_487_600_000.0),
        (4.404e-6, 4.410e-6, 4.395e-6, 4.408e-6, 17_627_300_000.0),
        (4.407e-6, 4.431e-6, 4.404e-6, 4.429e-6, 66_770_000_000.0),
        (4.430e-6, 4.448e-6, 4.408e-6, 4.410e-6, 58_774_100_000.0),
    ];
    let candles = rows
        .into_iter()
        .enumerate()
        .map(|(index, (open, high, low, close, volume))| Candle {
            timestamp_ms: SHIB_START_MS + index as i64 * CANDLE_INTERVAL_MS,
            open,
            high,
            low,
            close,
            volume,
        })
        .collect::<Vec<_>>();
    let indicators = indicators(candles.len(), 20, 6.17);

    let mut v20 = SignalState {
        enable_upthrust_failed_acceptance: true,
        ..SignalState::default()
    };
    assert!(evaluate_breakout_state(&mut v20, &candles, &indicators, 20, 1e-9).is_none());
    assert!(evaluate_breakout_state(&mut v20, &candles, &indicators, 21, 1e-9).is_some());

    for variant in [
        AnchorUpthrustResearchVariant::RecentHorizontalFirstBreak,
        AnchorUpthrustResearchVariant::RecentHorizontalFirstBreakCloseBack,
        AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency30,
        AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency35,
        AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency40,
        AnchorUpthrustResearchVariant::ActiveParentHorizontal,
        AnchorUpthrustResearchVariant::ActiveParentHorizontalBreakoutBodyRejection,
        AnchorUpthrustResearchVariant::ActiveParentHorizontalNormalizedBodyRejection10Pct,
        AnchorUpthrustResearchVariant::ActiveParentHorizontalShallowBreakoutExcess10Pct,
        AnchorUpthrustResearchVariant::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct,
    ] {
        let mut state = recent_horizontal_state(variant);
        assert!(evaluate_breakout_state(&mut state, &candles, &indicators, 20, 1e-9).is_none());
        assert!(state.recent_horizontal_upthrust_pending.is_none());
        assert!(evaluate_breakout_state(&mut state, &candles, &indicators, 21, 1e-9).is_none());
    }
}
