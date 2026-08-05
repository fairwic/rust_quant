use super::*;

fn pattern_bar(idx: usize, close: f64, ema144: f64, ema576: f64) -> PatternBar {
    PatternBar {
        ts: idx as i64 * super::super::super::MS_15M,
        high: close + 0.5,
        low: close - 0.5,
        close,
        ema144: Some(ema144),
        ema576: Some(ema576),
        atr14: Some(2.0),
    }
}

fn push_long_regime_and_breakout(bars: &mut Vec<PatternBar>) {
    for _ in 0..REGIME_WINDOW_BARS {
        let idx = bars.len();
        bars.push(pattern_bar(idx, 99.0, 98.0, 100.0));
    }
    let first_idx = bars.len();
    bars.push(pattern_bar(first_idx, 100.5, 98.5, 100.0));
    let confirmation_idx = bars.len();
    bars.push(pattern_bar(confirmation_idx, 101.0, 99.0, 100.0));
}

fn push_short_regime_and_breakout(bars: &mut Vec<PatternBar>) {
    for _ in 0..REGIME_WINDOW_BARS {
        let idx = bars.len();
        bars.push(pattern_bar(idx, 101.0, 102.0, 100.0));
    }
    let first_idx = bars.len();
    bars.push(pattern_bar(first_idx, 99.5, 101.5, 100.0));
    let confirmation_idx = bars.len();
    bars.push(pattern_bar(confirmation_idx, 99.0, 101.0, 100.0));
}

fn scan(bars: &[PatternBar]) -> (Vec<V2CandidateCore>, V2StageCounts) {
    let mut machine = ExclusiveActiveMachine::new();
    let mut candidates = Vec::new();
    let mut stages = V2StageCounts::default();
    for idx in 0..bars.len() {
        let step = machine.step(bars, idx);
        stages.qualified_regimes += step.qualified_regimes;
        stages.activated_directions += usize::from(step.activated_direction);
        stages.opposite_direction_replacements += usize::from(step.replaced_opposite_direction);
        stages.retest_rearms += usize::from(step.retest_rearmed);
        stages.failed_retests += usize::from(step.failed_retest);
        if let Some(candidate) = step.candidate {
            stages.held_retests += 1;
            candidates.push(candidate);
        }
    }
    (candidates, stages)
}

fn push_long_hold(bars: &mut Vec<PatternBar>) {
    let idx = bars.len();
    let mut retest = pattern_bar(idx, 99.6, 99.5, 100.0);
    retest.low = 99.4;
    bars.push(retest);
}

#[test]
fn active_long_can_rearm_and_emit_multiple_retests() {
    let mut bars = Vec::new();
    push_long_regime_and_breakout(&mut bars);
    push_long_hold(&mut bars);
    let departure_idx = bars.len();
    bars.push(pattern_bar(departure_idx, 101.0, 100.0, 100.0));
    push_long_hold(&mut bars);

    let (candidates, stages) = scan(&bars);
    assert_eq!(candidates.len(), 2);
    assert!(candidates
        .iter()
        .all(|candidate| candidate.active.direction == Direction::Long));
    assert_eq!(stages.activated_directions, 1);
    assert_eq!(stages.held_retests, 2);
}

#[test]
fn opposite_activation_replaces_active_direction() {
    let mut bars = Vec::new();
    push_long_regime_and_breakout(&mut bars);
    push_long_hold(&mut bars);
    push_short_regime_and_breakout(&mut bars);
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 100.4, 100.5, 100.0);
    retest.high = 100.6;
    bars.push(retest);

    let (candidates, stages) = scan(&bars);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].active.direction, Direction::Long);
    assert_eq!(candidates[1].active.direction, Direction::Short);
    assert_eq!(stages.opposite_direction_replacements, 1);
}

#[test]
fn failed_touch_keeps_direction_but_requires_new_departure() {
    let mut bars = Vec::new();
    push_long_regime_and_breakout(&mut bars);
    let failed_idx = bars.len();
    let mut failed = pattern_bar(failed_idx, 99.6, 99.5, 100.0);
    failed.low = 98.8;
    bars.push(failed);
    push_long_hold(&mut bars);
    let departure_idx = bars.len();
    bars.push(pattern_bar(departure_idx, 101.0, 100.0, 100.0));
    push_long_hold(&mut bars);

    let (candidates, stages) = scan(&bars);
    assert_eq!(candidates.len(), 1);
    assert_eq!(stages.failed_retests, 1);
}

#[test]
fn staying_inside_zone_does_not_repeat_signals() {
    let mut bars = Vec::new();
    push_long_regime_and_breakout(&mut bars);
    push_long_hold(&mut bars);
    for _ in 0..10 {
        push_long_hold(&mut bars);
    }

    let (candidates, _) = scan(&bars);
    assert_eq!(candidates.len(), 1);
}

#[test]
fn new_activation_clears_previously_armed_opposite_qualification() {
    let mut bars = Vec::new();
    push_short_regime_and_breakout(&mut bars);
    push_long_regime_and_breakout(&mut bars);
    push_long_hold(&mut bars);

    let (candidates, stages) = scan(&bars);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].active.direction, Direction::Long);
    assert_eq!(stages.opposite_direction_replacements, 1);
}

#[test]
fn v2_decision_never_reads_outcome() {
    let summary = V2Summary {
        candidate_count: 2_000,
        by_direction: BTreeMap::from([("long", 1_000), ("short", 1_000)]),
        by_cross_phase: BTreeMap::new(),
        by_symbol: (0..8).map(|idx| (idx.to_string(), 1)).collect(),
        by_month_utc: (0..6).map(|idx| (idx.to_string(), 1)).collect(),
        effective_market_events: 100,
        stages: V2StageCounts::default(),
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

    let decision = decide_v2(&summary, &audits, &inputs);
    assert_eq!(decision.status, "coverage_pass_ready_for_l2_prereg");
    assert!(!decision.outcome_evaluation_performed);
}
