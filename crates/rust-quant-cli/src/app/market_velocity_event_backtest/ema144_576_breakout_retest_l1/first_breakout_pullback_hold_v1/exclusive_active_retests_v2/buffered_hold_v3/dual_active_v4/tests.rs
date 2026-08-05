use super::*;

fn pattern_bar(idx: usize, close: f64, ema144: f64, ema576: f64) -> PatternBar {
    PatternBar {
        ts: idx as i64 * super::super::super::super::super::MS_15M,
        high: close + 0.2,
        low: close - 0.2,
        close,
        ema144: Some(ema144),
        ema576: Some(ema576),
        atr14: Some(2.0),
    }
}

fn push_long_activation(bars: &mut Vec<PatternBar>) {
    for _ in 0..REGIME_WINDOW_BARS {
        let idx = bars.len();
        bars.push(pattern_bar(idx, 99.0, 98.0, 100.0));
    }
    let first_idx = bars.len();
    bars.push(pattern_bar(first_idx, 100.5, 98.5, 100.0));
    let confirmation_idx = bars.len();
    bars.push(pattern_bar(confirmation_idx, 101.0, 99.0, 100.0));
}

fn push_short_activation(bars: &mut Vec<PatternBar>) {
    for _ in 0..REGIME_WINDOW_BARS {
        let idx = bars.len();
        bars.push(pattern_bar(idx, 101.0, 102.0, 100.0));
    }
    let first_idx = bars.len();
    bars.push(pattern_bar(first_idx, 99.5, 101.5, 100.0));
    let confirmation_idx = bars.len();
    bars.push(pattern_bar(confirmation_idx, 99.0, 101.0, 100.0));
}

fn scan(bars: &[PatternBar]) -> Vec<V2CandidateCore> {
    let mut machine = DualActiveMachine::new();
    let mut candidates = Vec::new();
    for idx in 0..bars.len() {
        candidates.extend(machine.step(bars, idx).candidates);
    }
    candidates
}

#[test]
fn opposite_activation_does_not_delete_existing_long() {
    let mut bars = Vec::new();
    push_long_activation(&mut bars);
    push_short_activation(&mut bars);
    let departure_idx = bars.len();
    bars.push(pattern_bar(departure_idx, 101.0, 100.0, 100.0));
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 99.6, 100.0, 100.0);
    retest.low = 99.5;
    bars.push(retest);

    let candidates = scan(&bars);
    assert!(candidates
        .iter()
        .any(|candidate| candidate.active.direction == Direction::Long));
}

#[test]
fn each_direction_rearms_independently() {
    let mut bars = Vec::new();
    push_long_activation(&mut bars);
    push_short_activation(&mut bars);
    let long_departure_idx = bars.len();
    bars.push(pattern_bar(long_departure_idx, 101.0, 100.0, 100.0));
    let long_retest_idx = bars.len();
    let mut long_retest = pattern_bar(long_retest_idx, 99.6, 100.0, 100.0);
    long_retest.low = 99.5;
    bars.push(long_retest);
    let short_departure_idx = bars.len();
    bars.push(pattern_bar(short_departure_idx, 99.0, 100.0, 100.0));
    let short_retest_idx = bars.len();
    let mut short_retest = pattern_bar(short_retest_idx, 100.4, 100.0, 100.0);
    short_retest.high = 100.5;
    bars.push(short_retest);

    let candidates = scan(&bars);
    assert!(candidates
        .iter()
        .any(|candidate| candidate.active.direction == Direction::Long));
    assert!(candidates
        .iter()
        .any(|candidate| candidate.active.direction == Direction::Short));
}

#[test]
fn v4_decision_remains_outcome_blind() {
    let summary = V4Summary {
        candidate_count: 20_000,
        by_direction: BTreeMap::from([("long", 10_000), ("short", 10_000)]),
        by_cross_phase: BTreeMap::new(),
        by_symbol: (0..8).map(|idx| (idx.to_string(), 1)).collect(),
        by_month_utc: (0..6).map(|idx| (idx.to_string(), 1)).collect(),
        effective_market_events: 100,
        stages: V4StageCounts::default(),
    };
    let audits = TARGETS
        .iter()
        .map(|target| TargetAudit {
            name: target.name,
            symbol: target.symbol,
            direction: target.direction.label(),
            start_ms: target.start_ms,
            end_ms: target.end_ms,
            expectation: target.expectation.label(),
            passed: true,
            matched_signal_timestamps_ms: if target.expectation == TargetExpectation::MustMatch {
                vec![target.start_ms]
            } else {
                Vec::new()
            },
        })
        .collect::<Vec<_>>();
    let inputs = target_input_template()
        .into_iter()
        .map(|mut coverage| {
            coverage.ready = true;
            coverage.ready_candles = coverage.expected_candles;
            coverage
        })
        .collect::<Vec<_>>();

    let decision = decide_v4(&summary, &audits, &inputs);
    assert_eq!(decision.status, "coverage_pass_ready_for_l2_prereg");
    assert!(!decision.outcome_evaluation_performed);
}
