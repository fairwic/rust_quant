use super::*;

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
