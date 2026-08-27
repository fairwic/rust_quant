use super::*;
use rust_quant_cli::app::tradingview_velocity_parity::{
    StrictVisualLongEntryEvent, StrictVisualLongEntryState,
};

fn candle(timestamp_ms: i64) -> Candle {
    Candle {
        timestamp_ms,
        open: 100.0,
        high: 101.0,
        low: 99.0,
        close: 100.5,
        volume: 10.0,
    }
}

/// 构造上下沿交替触碰的八棒横盘，供保留率门禁测试冻结同一 V23 来源。
fn strict_visual_edge_candle(index: usize, upper: bool) -> Candle {
    let (open, high, low, close) = if upper {
        (100.4, 101.0, 100.2, 100.7)
    } else {
        (99.6, 99.8, 99.0, 99.3)
    };
    Candle {
        timestamp_ms: index as i64 * CANDLE_INTERVAL_MS,
        open,
        high,
        low,
        close,
        volume: 100.0,
    }
}

/// 冻结指定方向的强突破来源；只改变研究版本，不改变 OHLC 或风险证据。
fn arm_v23_retention_source(
    direction: Direction,
    variant: StrictVisualBreakoutResearchVariant,
) -> (Vec<Candle>, StrictVisualLongEntryState) {
    let mut candles = (0..8)
        .map(|index| strict_visual_edge_candle(index, index % 2 == 0))
        .collect::<Vec<_>>();
    let mut state = StrictVisualLongEntryState::default();
    for index in 0..8 {
        state.update(&candles, index, 0.01, None, false, None, None, variant);
    }
    let (open, high, low, close) = match direction {
        Direction::Long => (100.9, 101.25, 100.85, 101.11),
        Direction::Short => (99.1, 99.15, 98.75, 98.89),
    };
    candles.push(Candle {
        timestamp_ms: 8 * CANDLE_INTERVAL_MS,
        open,
        high,
        low,
        close,
        volume: 1.0,
    });
    assert!(matches!(
        state.update(&candles, 8, 0.01, Some(1.0), false, None, None, variant),
        Some(StrictVisualLongEntryEvent::AcceptanceArmed(_))
    ));
    (candles, state)
}

/// 生成触碰冻结边界、并保留指定比例越界幅度的完成确认棒。
fn retention_confirmation_candle(direction: Direction, ratio: f64, index: usize) -> Candle {
    let close = match direction {
        Direction::Long => 101.0 + 0.11 * ratio,
        Direction::Short => 99.0 - 0.11 * ratio,
    };
    let (open, high, low) = match direction {
        Direction::Long => (101.1, 101.2, 100.99),
        Direction::Short => (98.9, 99.01, 98.8),
    };
    Candle {
        timestamp_ms: index as i64 * CANDLE_INTERVAL_MS,
        open,
        high,
        low,
        close,
        volume: 1.0,
    }
}

#[test]
fn replay_window_rejects_internal_gap() {
    let candles = vec![
        candle(0),
        candle(CANDLE_INTERVAL_MS),
        candle(3 * CANDLE_INTERVAL_MS),
    ];
    let coverage =
        replay_window_coverage(&candles, 0, 3 * CANDLE_INTERVAL_MS).expect("aligned window");

    assert_eq!(coverage.expected, 4);
    assert_eq!(coverage.loaded, 3);
    assert!(!coverage.is_complete);
}

#[test]
fn replay_window_accepts_exact_slice_with_extra_warmup() {
    let candles = vec![
        candle(-CANDLE_INTERVAL_MS),
        candle(0),
        candle(CANDLE_INTERVAL_MS),
        candle(2 * CANDLE_INTERVAL_MS),
    ];
    let coverage =
        replay_window_coverage(&candles, 0, 2 * CANDLE_INTERVAL_MS).expect("aligned window");

    assert_eq!(
        coverage,
        ReplayWindowCoverage {
            expected: 3,
            loaded: 3,
            is_complete: true,
        }
    );
}

#[test]
fn parser_requires_explicit_candidate_and_keeps_legacy_default() {
    let legacy = parse_args(Vec::<String>::new()).expect("legacy arguments");
    assert_eq!(legacy.rule_version, ParityRuleVersion::Current3cbbc9d8);

    let v9 = parse_args([
        "--rule-version".to_owned(),
        "candidate-v9".to_owned(),
        "--output".to_owned(),
        "report.json".to_owned(),
    ])
    .expect("V9 arguments");
    assert_eq!(v9.rule_version, ParityRuleVersion::CandidateV9);

    let v10 = parse_args(["--rule-version".to_owned(), "candidate-v10".to_owned()])
        .expect("V10 arguments");
    assert_eq!(v10.rule_version, ParityRuleVersion::CandidateV10);

    let v11 = parse_args(["--rule-version".to_owned(), "candidate-v11".to_owned()])
        .expect("V11 arguments");
    assert_eq!(v11.rule_version, ParityRuleVersion::CandidateV11);

    let v12 = parse_args(["--rule-version".to_owned(), "candidate-v12".to_owned()])
        .expect("V12 arguments");
    assert_eq!(v12.rule_version, ParityRuleVersion::CandidateV12);

    let v13 = parse_args(["--rule-version".to_owned(), "candidate-v13".to_owned()])
        .expect("V13 arguments");
    assert_eq!(v13.rule_version, ParityRuleVersion::CandidateV13);

    let v14 = parse_args(["--rule-version".to_owned(), "candidate-v14".to_owned()])
        .expect("V14 arguments");
    assert_eq!(v14.rule_version, ParityRuleVersion::CandidateV14);

    let v15 = parse_args(["--rule-version".to_owned(), "candidate-v15".to_owned()])
        .expect("V15 arguments");
    assert_eq!(v15.rule_version, ParityRuleVersion::CandidateV15);

    let v16 = parse_args(["--rule-version".to_owned(), "candidate-v16".to_owned()])
        .expect("V16 arguments");
    assert_eq!(v16.rule_version, ParityRuleVersion::CandidateV16);

    let v17 = parse_args(["--rule-version".to_owned(), "candidate-v17".to_owned()])
        .expect("V17 arguments");
    assert_eq!(v17.rule_version, ParityRuleVersion::CandidateV17);

    let v18 = parse_args(["--rule-version".to_owned(), "candidate-v18".to_owned()])
        .expect("V18 arguments");
    assert_eq!(v18.rule_version, ParityRuleVersion::CandidateV18);

    let v19 = parse_args(["--rule-version".to_owned(), "candidate-v19".to_owned()])
        .expect("V19 arguments");
    assert_eq!(v19.rule_version, ParityRuleVersion::CandidateV19);

    let ablation = parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--ema-short-variant".to_owned(),
        "structure-break".to_owned(),
    ])
    .expect("V19 EMA short ablation arguments");
    assert_eq!(
        ablation.ema_short_variant,
        EmaShortResearchVariant::StructureBreak
    );
    let depth = parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--ema-short-variant".to_owned(),
        "structure-break-depth-0-20-atr".to_owned(),
    ])
    .expect("V19 EMA short structure depth arguments");
    assert_eq!(
        depth.ema_short_variant,
        EmaShortResearchVariant::StructureBreakDepth20
    );
    let slope_regime = parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--ema-short-variant".to_owned(),
        "structure-break-ema676-falling-20".to_owned(),
    ])
    .expect("V19 EMA676 slope regime arguments");
    assert_eq!(
        slope_regime.ema_short_variant,
        EmaShortResearchVariant::StructureBreakEma676Falling20
    );
    let ema_long_ladder = parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--ema-trend-long-variant".to_owned(),
        "weekly-p80-tp-floor-2-5-body-0-003-distance-1-5-atr".to_owned(),
    ])
    .expect("V19 EMA trend long ladder arguments");
    assert_eq!(
        ema_long_ladder.ema_trend_long_variant,
        EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003Distance15
    );
    let break_depth = parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--ema-trend-long-variant".to_owned(),
        "weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-4-atr".to_owned(),
    ])
    .expect("V19 EMA trend long breakout-depth arguments");
    assert_eq!(
        break_depth.ema_trend_long_variant,
        EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth40
    );
    let distance_135 = parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--ema-trend-long-variant".to_owned(),
        "weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-3-atr-distance-1-35-atr".to_owned(),
    ])
    .expect("V19 EMA trend long 0.30 depth and 1.35 distance arguments");
    assert_eq!(
        distance_135.ema_trend_long_variant,
        EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance135
    );
    let distance_150 = parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--ema-trend-long-variant".to_owned(),
        "weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-3-atr-distance-1-5-atr".to_owned(),
    ])
    .expect("V19 EMA trend long 0.30 depth and 1.50 distance arguments");
    assert_eq!(
        distance_150.ema_trend_long_variant,
        EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance150
    );
    let bullish_acceptance = parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--ema-trend-long-variant".to_owned(),
        "weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-3-atr-bullish-acceptance".to_owned(),
    ])
    .expect("V19 EMA trend long bullish retest acceptance arguments");
    assert_eq!(
        bullish_acceptance.ema_trend_long_variant,
        EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth30BullishAcceptance
    );
    let conservative_target_gap = parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--ema-trend-long-variant".to_owned(),
        "conservative-target-gap".to_owned(),
    ])
    .expect("V19 conservative EMA trend long target-gap arguments");
    assert_eq!(
        conservative_target_gap.ema_trend_long_variant,
        EmaTrendLongResearchVariant::ConservativeTargetGap
    );
    let sell_climax = parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--ema-trend-long-variant".to_owned(),
        "conservative-target-gap".to_owned(),
        "--sell-climax-base-reclaim-variant".to_owned(),
        "v1".to_owned(),
    ])
    .expect("V19 sell climax base reclaim arguments");
    assert_eq!(
        sell_climax.sell_climax_base_reclaim_variant,
        SellClimaxBaseReclaimResearchVariant::V1
    );
    assert!(parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--sell-climax-base-reclaim-variant".to_owned(),
        "v1".to_owned(),
    ])
    .is_err());
    assert!(parse_args([
        "--rule-version".to_owned(),
        "candidate-v18".to_owned(),
        "--ema-short-variant".to_owned(),
        "distance-guard".to_owned(),
    ])
    .is_err());
    assert!(parse_args([
        "--rule-version".to_owned(),
        "candidate-v19".to_owned(),
        "--ema-short-variant".to_owned(),
        "distance-guard".to_owned(),
        "--ema-trend-long-variant".to_owned(),
        "weekly-volume-p80".to_owned(),
    ])
    .is_err());
}

#[test]
fn next_bar_audit_requires_bearish_body_and_both_volume_contractions() {
    let signal = Candle {
        volume: 100.0,
        ..candle(0)
    };
    let eligible = Candle {
        open: 101.0,
        close: 100.0,
        volume: 40.0,
        ..candle(CANDLE_INTERVAL_MS)
    };
    let too_large_vs_median = Candle {
        volume: 41.0,
        ..eligible
    };
    let bullish = Candle {
        open: 100.0,
        close: 101.0,
        ..eligible
    };

    assert!(is_low_volume_bearish_next(signal, eligible, 40.0));
    assert!(!is_low_volume_bearish_next(
        signal,
        too_large_vs_median,
        40.0
    ));
    assert!(!is_low_volume_bearish_next(signal, bullish, 40.0));
}

#[test]
fn v23_retention_v25_and_v26_reuse_the_unified_retest_execution_cost_contract() {
    let baseline = ReplayConfig::current_pine_v2("TEST", 0.01, 0, CANDLE_INTERVAL_MS);
    for variant in [
        StrictVisualBreakoutResearchVariant::V23UnifiedRetestNoShortGates,
        StrictVisualBreakoutResearchVariant::V23AcceptanceRetention75CostReplay,
        StrictVisualBreakoutResearchVariant::V25ImmediateWeakDepartureConsume,
        StrictVisualBreakoutResearchVariant::V26NarrowRangeDynamicAnchor,
    ] {
        let adjusted = cost_adjusted_replay_config(baseline.clone(), variant);
        let contract = execution_cost_contract(variant);

        assert_eq!(adjusted.entry_cost_bps(), 3.0);
        assert_eq!(adjusted.exit_cost_bps(), 6.0);
        assert_eq!(contract.entry_fee_bps, 2.0);
        assert_eq!(contract.entry_slippage_bps, 1.0);
        assert_eq!(contract.exit_fee_bps, 5.0);
        assert_eq!(contract.exit_slippage_bps, 1.0);
    }
}

#[test]
fn retention_75_filters_only_at_the_original_v23_confirmation_and_consumes_source() {
    let baseline = StrictVisualBreakoutResearchVariant::V23UnifiedRetestNoShortGates;
    let candidate = StrictVisualBreakoutResearchVariant::V23AcceptanceRetention75CostReplay;

    for direction in [Direction::Long, Direction::Short] {
        let (mut baseline_candles, mut baseline_state) =
            arm_v23_retention_source(direction, baseline);
        baseline_candles.push(retention_confirmation_candle(direction, 0.50, 9));
        assert!(matches!(
            baseline_state.update(
                &baseline_candles,
                9,
                0.01,
                Some(1.0),
                false,
                None,
                None,
                baseline,
            ),
            Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(_))
        ));

        let (mut candidate_candles, mut candidate_state) =
            arm_v23_retention_source(direction, candidate);
        candidate_candles.push(retention_confirmation_candle(direction, 0.50, 9));
        assert!(candidate_state
            .update(
                &candidate_candles,
                9,
                0.01,
                Some(1.0),
                false,
                None,
                None,
                candidate,
            )
            .is_none());

        candidate_candles.push(retention_confirmation_candle(direction, 1.00, 10));
        assert!(candidate_state
            .update(
                &candidate_candles,
                10,
                0.01,
                Some(1.0),
                false,
                None,
                None,
                candidate,
            )
            .and_then(StrictVisualLongEntryEvent::entry_signal)
            .is_none());
    }
}

#[test]
fn retention_75_accepts_the_exact_symmetric_boundary_for_both_directions() {
    let variant = StrictVisualBreakoutResearchVariant::V23AcceptanceRetention75CostReplay;
    for direction in [Direction::Long, Direction::Short] {
        let (mut candles, mut state) = arm_v23_retention_source(direction, variant);
        candles.push(retention_confirmation_candle(direction, 0.75, 9));
        let Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(signal)) =
            state.update(&candles, 9, 0.01, Some(1.0), false, None, None, variant)
        else {
            panic!("75% retention boundary must confirm without direction bias");
        };
        assert_eq!(signal.direction, direction);
        assert_eq!(signal.signal_index, 9);
    }
}
