use super::*;

fn pattern_bar(idx: usize, close: f64, ema144: f64, ema576: f64) -> PatternBar {
    PatternBar {
        ts: idx as i64 * super::super::MS_15M,
        high: close + 0.5,
        low: close - 0.5,
        close,
        ema144: Some(ema144),
        ema576: Some(ema576),
        atr14: Some(2.0),
    }
}

fn long_regime() -> Vec<PatternBar> {
    (0..REGIME_WINDOW_BARS)
        .map(|idx| pattern_bar(idx, 99.0, 98.0, 100.0))
        .collect()
}

fn push_long_breakout(bars: &mut Vec<PatternBar>) -> usize {
    let first_idx = bars.len();
    bars.push(pattern_bar(first_idx, 100.5, 98.5, 100.0));
    let confirmation_idx = bars.len();
    bars.push(pattern_bar(confirmation_idx, 101.0, 99.0, 100.0));
    confirmation_idx
}

fn scan(direction: Direction, bars: &[PatternBar]) -> (Vec<CandidateCore>, L1StageCounts) {
    let mut machine = DirectionMachine::new(direction);
    let mut candidates = Vec::new();
    let mut stages = L1StageCounts::default();
    for idx in 0..bars.len() {
        let step = machine.step(bars, idx);
        stages.qualified_regimes += usize::from(step.regime_qualified);
        stages.confirmed_first_breakouts += usize::from(step.confirmed_breakout);
        stages.failed_first_retests += usize::from(step.failed_first_retest);
        stages.retest_timeouts += usize::from(step.retest_timeout);
        if let Some(candidate) = step.candidate {
            stages.held_first_retests += 1;
            candidates.push(candidate);
        }
    }
    (candidates, stages)
}

#[test]
fn long_requires_80_percent_price_below_ema576() {
    let mut bars = long_regime();
    for bar in bars.iter_mut().take(29) {
        bar.close = 100.2;
        bar.high = 100.7;
        bar.low = 99.7;
    }
    push_long_breakout(&mut bars);
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 99.6, 99.5, 100.0);
    retest.low = 99.4;
    bars.push(retest);

    let (candidates, stages) = scan(Direction::Long, &bars);
    assert!(candidates.is_empty());
    assert_eq!(stages.confirmed_first_breakouts, 0);
}

#[test]
fn long_breakout_then_first_ema144_hold_is_candidate() {
    let mut bars = long_regime();
    let breakout_idx = push_long_breakout(&mut bars);
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 99.6, 99.5, 100.0);
    retest.low = 99.4;
    bars.push(retest);

    let (candidates, stages) = scan(Direction::Long, &bars);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].breakout_idx, breakout_idx);
    assert_eq!(candidates[0].signal_idx, retest_idx);
    assert_eq!(stages.confirmed_first_breakouts, 1);
    assert_eq!(stages.held_first_retests, 1);
}

#[test]
fn short_rule_is_exact_mirror() {
    let mut bars = (0..REGIME_WINDOW_BARS)
        .map(|idx| pattern_bar(idx, 101.0, 102.0, 100.0))
        .collect::<Vec<_>>();
    let first_idx = bars.len();
    bars.push(pattern_bar(first_idx, 99.5, 101.5, 100.0));
    let breakout_idx = bars.len();
    bars.push(pattern_bar(breakout_idx, 99.0, 101.0, 100.0));
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 100.4, 100.5, 100.0);
    retest.high = 100.6;
    bars.push(retest);

    let (candidates, _) = scan(Direction::Short, &bars);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].breakout_idx, breakout_idx);
    assert_eq!(candidates[0].signal_idx, retest_idx);
}

#[test]
fn deep_or_close_failed_first_touch_consumes_episode() {
    let mut bars = long_regime();
    push_long_breakout(&mut bars);
    let failed_idx = bars.len();
    let mut failed = pattern_bar(failed_idx, 99.6, 99.5, 100.0);
    failed.low = 98.8;
    bars.push(failed);
    let second_idx = bars.len();
    let mut second = pattern_bar(second_idx, 99.6, 99.5, 100.0);
    second.low = 99.4;
    bars.push(second);

    let (candidates, stages) = scan(Direction::Long, &bars);
    assert!(candidates.is_empty());
    assert_eq!(stages.failed_first_retests, 1);
}

#[test]
fn retest_on_576th_bar_is_valid() {
    let mut bars = long_regime();
    let breakout_idx = push_long_breakout(&mut bars);
    for _ in 1..RETEST_WAIT_BARS {
        let idx = bars.len();
        bars.push(pattern_bar(idx, 103.0, 100.0, 100.5));
    }
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 100.1, 100.0, 100.5);
    retest.low = 99.9;
    bars.push(retest);

    let (candidates, _) = scan(Direction::Long, &bars);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].signal_idx - breakout_idx, RETEST_WAIT_BARS);
}

#[test]
fn stale_touch_after_576_bars_is_rejected() {
    let mut bars = long_regime();
    push_long_breakout(&mut bars);
    for _ in 0..RETEST_WAIT_BARS {
        let idx = bars.len();
        bars.push(pattern_bar(idx, 103.0, 100.0, 100.5));
    }
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 100.1, 100.0, 100.5);
    retest.low = 99.9;
    bars.push(retest);

    let (candidates, stages) = scan(Direction::Long, &bars);
    assert!(candidates.is_empty());
    assert_eq!(stages.retest_timeouts, 1);
}

#[test]
fn ema144_touch_without_fresh_mirror_breakout_cannot_short() {
    let mut bars = long_regime();
    let touch_idx = bars.len();
    let mut touch = pattern_bar(touch_idx, 98.2, 98.0, 100.0);
    touch.high = 98.1;
    bars.push(touch);

    let (candidates, stages) = scan(Direction::Short, &bars);
    assert!(candidates.is_empty());
    assert_eq!(stages.confirmed_first_breakouts, 0);
}

#[test]
fn consumed_episode_cannot_rearm_without_144_fresh_bars() {
    let mut bars = long_regime();
    push_long_breakout(&mut bars);
    let first_retest_idx = bars.len();
    let mut first_retest = pattern_bar(first_retest_idx, 99.6, 99.5, 100.0);
    first_retest.low = 99.4;
    bars.push(first_retest);
    let below_idx = bars.len();
    bars.push(pattern_bar(below_idx, 99.0, 98.0, 100.0));
    push_long_breakout(&mut bars);
    let second_retest_idx = bars.len();
    let mut second_retest = pattern_bar(second_retest_idx, 99.6, 99.5, 100.0);
    second_retest.low = 99.4;
    bars.push(second_retest);

    let (candidates, stages) = scan(Direction::Long, &bars);
    assert_eq!(candidates.len(), 1);
    assert_eq!(stages.confirmed_first_breakouts, 1);
}

#[test]
fn decision_never_claims_outcome_evaluation() {
    let summary = L1Summary {
        candidate_count: 100,
        by_direction: BTreeMap::from([("long", 50), ("short", 50)]),
        by_cross_phase: BTreeMap::new(),
        by_symbol: (0..8).map(|idx| (idx.to_string(), 1)).collect(),
        by_month_utc: (0..6).map(|idx| (idx.to_string(), 1)).collect(),
        effective_market_events: 20,
        stages: L1StageCounts::default(),
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

    let decision = decide(&summary, &audits, &inputs);
    assert_eq!(decision.status, "coverage_pass_ready_for_l2_prereg");
    assert!(!decision.outcome_evaluation_performed);
}
