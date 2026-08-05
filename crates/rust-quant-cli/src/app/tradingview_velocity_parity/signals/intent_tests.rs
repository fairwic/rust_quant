use super::*;
use crate::app::tradingview_velocity_parity::StrictVisualRangeEvidence;

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

fn strict_visual_signal(
    range_length_bars: usize,
    short_range_one_r_target: bool,
) -> StrictVisualBreakoutSignal {
    StrictVisualBreakoutSignal {
        range: StrictVisualRangeEvidence {
            start_index: 0,
            first_confirmation_index: 1,
            boundary_confirmation_index: 1,
            length_bars: range_length_bars,
            upper: 100.0,
            lower: 98.0,
            containment_ratio: 0.9,
            direction_efficiency: 0.1,
            edge_transition_count: 4,
            upper_touch_groups: 3,
            lower_touch_groups: 3,
            upper_drift_ratio: 0.1,
            lower_drift_ratio: 0.1,
        },
        direction: Direction::Long,
        breakout_index: 2,
        signal_index: 3,
        breakout_open: 100.0,
        breakout_close: 102.0,
        breakout_candle_extreme_stop_price: None,
        breakout_candle_extreme_stop_min_atr_multiple: None,
        breakout_body_midpoint: 101.0,
        source_atr: 2.0,
        source_volume_ratio: 3.0,
        source_take_profit_atr: 4.5,
        volume_gate_applied: true,
        short_range_one_r_target,
        retest_band: 0.5,
        breakout_excess: 2.0,
        required_acceptance_close: 100.0,
        measured_move_target_price: None,
        external_structure: None,
    }
}

#[test]
fn strict_visual_v4_short_range_uses_initial_stop_as_one_r_target() {
    let (candles, indicators) = intent_fixture();
    let short = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Long,
        IntentContext {
            strict_visual_breakout: Some(strict_visual_signal(32, true)),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV20,
    )
    .expect("V4 short-range intent should remain valid");
    let long = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Long,
        IntentContext {
            strict_visual_breakout: Some(strict_visual_signal(48, false)),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV20,
    )
    .expect("V4 long-range intent should retain the volume target");

    assert_eq!(short.exit_policy, ExitPolicy::Fixed);
    assert_eq!(short.target_ticks, short.stop_ticks);
    assert_eq!(short.strict_visual_range_length_bars, Some(32));
    assert_eq!(short.strict_visual_range_height, Some(2.0));
    assert_eq!(short.strict_visual_short_range_one_r_target, Some(true));
    assert_eq!(long.stop_ticks, Some(30));
    assert_eq!(long.target_ticks, Some(90));
    assert_eq!(long.strict_visual_range_length_bars, Some(48));
    assert_eq!(long.strict_visual_range_height, Some(2.0));
    assert_eq!(long.strict_visual_short_range_one_r_target, Some(false));
}

#[test]
fn symmetric_strict_visual_contract_uses_measured_targets_for_both_directions() {
    let (candles, indicators) = intent_fixture();
    let mut long_signal = strict_visual_signal(48, false);
    long_signal.volume_gate_applied = false;
    long_signal.source_take_profit_atr = 0.0;
    long_signal.measured_move_target_price = Some(102.0);
    let long = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Long,
        IntentContext {
            strict_visual_breakout: Some(long_signal),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV20,
    )
    .expect("long measured target should not require a volume tier");

    let mut short_signal = long_signal;
    short_signal.direction = Direction::Short;
    short_signal.measured_move_target_price = Some(96.0);
    let short = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Short,
        IntentContext {
            strict_visual_breakout: Some(short_signal),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV20,
    )
    .expect("short mirror should not require a volume tier");

    assert_eq!(long.target_price, Some(102.0));
    assert!(long
        .families
        .contains(&SignalFamily::StrictVisualConsolidationBreakLong));
    assert_eq!(short.target_price, Some(96.0));
    assert!(short
        .families
        .contains(&SignalFamily::StrictVisualConsolidationBreakShort));
}

#[test]
fn v11_breakout_candle_extreme_stop_overrides_other_stop_shapes_for_both_directions() {
    let (candles, indicators) = intent_fixture();
    let mut long_signal = strict_visual_signal(48, false);
    long_signal.volume_gate_applied = false;
    long_signal.source_take_profit_atr = 0.0;
    long_signal.measured_move_target_price = Some(104.0);
    long_signal.breakout_candle_extreme_stop_price = Some(97.5);
    let long = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Long,
        IntentContext {
            strict_visual_breakout: Some(long_signal),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV20,
    )
    .expect("V11 long should freeze its breakout-candle stop");

    let mut short_signal = long_signal;
    short_signal.direction = Direction::Short;
    short_signal.measured_move_target_price = Some(96.0);
    short_signal.breakout_candle_extreme_stop_price = Some(102.5);
    let short = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Short,
        IntentContext {
            strict_visual_breakout: Some(short_signal),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV20,
    )
    .expect("V11 short should freeze its breakout-candle stop");

    assert_eq!(long.stop_price, Some(97.5));
    assert_eq!(long.stop_ticks, None);
    assert!(long.strict_visual_breakout_candle_extreme_stop);
    assert_eq!(long.target_price, Some(104.0));
    assert_eq!(short.stop_price, Some(102.5));
    assert_eq!(short.stop_ticks, None);
    assert!(short.strict_visual_breakout_candle_extreme_stop);
    assert_eq!(short.target_price, Some(96.0));
}

#[test]
fn v12_freezes_one_atr_ticks_without_changing_the_v11_structure_or_target() {
    let (candles, indicators) = intent_fixture();
    let mut signal = strict_visual_signal(48, false);
    signal.volume_gate_applied = false;
    signal.source_take_profit_atr = 0.0;
    signal.measured_move_target_price = Some(104.0);
    signal.breakout_candle_extreme_stop_price = Some(97.5);
    signal.breakout_candle_extreme_stop_min_atr_multiple = Some(1.0);

    let intent = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Long,
        IntentContext {
            strict_visual_breakout: Some(signal),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV20,
    )
    .expect("V12 should freeze both the structural stop and one-ATR tick floor");

    assert_eq!(intent.stop_price, Some(97.5));
    assert_eq!(intent.stop_ticks, Some(20));
    assert_eq!(intent.target_price, Some(104.0));
    assert!(intent.strict_visual_breakout_candle_extreme_stop);
}

#[test]
fn strict_visual_v4_does_not_override_a_higher_priority_structural_exit() {
    let (candles, indicators) = intent_fixture();
    let intent = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Long,
        IntentContext {
            strict_visual_breakout: Some(strict_visual_signal(8, true)),
            bollinger_lower_reclaim: Some(BollingerLowerReclaimLongResult {
                stop_price: 97.0,
                target_price: 105.0,
                display_volume_ratio: Some(3.0),
            }),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV20,
    )
    .expect("coexisting structural exit should remain valid");

    assert_eq!(intent.exit_policy, ExitPolicy::BollingerLowerReclaim);
    assert_eq!(intent.stop_price, Some(97.0));
    assert_eq!(intent.target_price, Some(105.0));
    assert_eq!(intent.target_ticks, None);
}

#[test]
fn v4_counter_trend_intent_uses_independent_exit_policy() {
    let (candles, indicators) = intent_fixture();
    let context = IntentContext {
        divergence: Divergence {
            bullish: true,
            ..Divergence::default()
        },
        counter_trend_long: true,
        long_structure_target: Some(105.0),
        counter_trend_ema_age_audit: Some(42),
        take_profit_atr: Some(3.0),
        ..IntentContext::default()
    };
    let v3 = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Long,
        context,
        ParityRuleVersion::CandidateV3,
    )
    .expect("V3 counter-trend intent should remain valid");
    let v4 = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Long,
        context,
        ParityRuleVersion::CandidateV4,
    )
    .expect("V4 counter-trend intent should remain valid");

    assert_eq!(v3.exit_policy, ExitPolicy::CounterTrendStructure);
    assert_eq!(v4.exit_policy, ExitPolicy::CounterTrendStructureV4);
    assert_eq!(v3.target_price, Some(105.0));
    assert_eq!(v3.target_price, v4.target_price);
    assert_eq!(v3.signal_counter_trend_ema_age_bars_capped_600, None);
    assert_eq!(v4.signal_counter_trend_ema_age_bars_capped_600, Some(42));
    assert_eq!(v4.counter_trend_structure_breakout_line, None);
}

#[test]
fn v5_counter_trend_intent_freezes_age_and_only_mature_age_uses_v5_exit() {
    let (candles, indicators) = intent_fixture();
    let young = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Long,
        IntentContext {
            counter_trend_long: true,
            long_structure_target: Some(105.0),
            v5_counter_trend_plan: Some(RsiCounterTrendPlanV5 {
                ema_alignment_age: 42,
                target_price: 105.0,
                structure_breakout_line: 103.0,
            }),
            counter_trend_ema_age_audit: Some(42),
            take_profit_atr: Some(3.0),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV5,
    )
    .expect("V5 strict counter-trend intent should remain valid");

    assert_eq!(young.exit_policy, ExitPolicy::CounterTrendStructureV4);
    assert_eq!(young.signal_counter_trend_ema_age_bars_capped_600, Some(42));
    assert_eq!(young.counter_trend_structure_breakout_line, Some(103.0));
    assert_eq!(young.target_price, Some(105.0));

    let mature = build_intent(
        &candles,
        &indicators,
        3,
        0.1,
        Direction::Long,
        IntentContext {
            counter_trend_long: true,
            long_structure_target: Some(110.0),
            v5_counter_trend_plan: Some(RsiCounterTrendPlanV5 {
                ema_alignment_age: 600,
                target_price: 110.0,
                structure_breakout_line: 103.0,
            }),
            counter_trend_ema_age_audit: Some(600),
            take_profit_atr: Some(3.0),
            ..IntentContext::default()
        },
        ParityRuleVersion::CandidateV5,
    )
    .expect("V5 mature counter-trend intent should remain valid");

    assert_eq!(mature.exit_policy, ExitPolicy::RsiCounterTrendAgeV5);
    assert_eq!(
        mature.signal_counter_trend_ema_age_bars_capped_600,
        Some(600)
    );
}
