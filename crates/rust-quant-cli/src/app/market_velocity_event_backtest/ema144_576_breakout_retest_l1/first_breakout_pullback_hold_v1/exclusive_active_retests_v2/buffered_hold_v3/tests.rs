use super::*;

fn pattern_bar(idx: usize, close: f64, ema144: f64, ema576: f64) -> PatternBar {
    PatternBar {
        ts: idx as i64 * super::super::super::super::MS_15M,
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

fn scan_with_buffer(bars: &[PatternBar], buffer_atr: f64) -> (usize, usize) {
    let mut machine = ExclusiveActiveMachine::with_close_hold_buffer(buffer_atr);
    let mut candidates = 0;
    let mut failures = 0;
    for idx in 0..bars.len() {
        let step = machine.step(bars, idx);
        candidates += usize::from(step.candidate.is_some());
        failures += usize::from(step.failed_retest);
    }
    (candidates, failures)
}

#[test]
fn long_close_inside_buffer_is_hold_only_in_v3() {
    let mut bars = Vec::new();
    push_long_activation(&mut bars);
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 99.6, 100.0, 99.0);
    retest.low = 99.5;
    bars.push(retest);

    assert_eq!(scan_with_buffer(&bars, 0.0), (0, 1));
    assert_eq!(scan_with_buffer(&bars, V3_CLOSE_HOLD_BUFFER_ATR), (1, 0));
}

#[test]
fn short_close_inside_buffer_is_mirrored() {
    let mut bars = Vec::new();
    push_short_activation(&mut bars);
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 100.4, 100.0, 101.0);
    retest.high = 100.5;
    bars.push(retest);

    assert_eq!(scan_with_buffer(&bars, 0.0), (0, 1));
    assert_eq!(scan_with_buffer(&bars, V3_CLOSE_HOLD_BUFFER_ATR), (1, 0));
}

#[test]
fn close_and_extreme_outside_buffer_remain_failed() {
    let mut bars = Vec::new();
    push_long_activation(&mut bars);
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 99.3, 100.0, 99.0);
    retest.low = 99.2;
    bars.push(retest);

    assert_eq!(scan_with_buffer(&bars, V3_CLOSE_HOLD_BUFFER_ATR), (0, 1));
}

#[test]
fn v3_decision_remains_outcome_blind() {
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

    let decision = decide_v3(&summary, &audits, &inputs);
    assert_eq!(decision.status, "coverage_pass_ready_for_l2_prereg");
    assert!(!decision.outcome_evaluation_performed);
}
