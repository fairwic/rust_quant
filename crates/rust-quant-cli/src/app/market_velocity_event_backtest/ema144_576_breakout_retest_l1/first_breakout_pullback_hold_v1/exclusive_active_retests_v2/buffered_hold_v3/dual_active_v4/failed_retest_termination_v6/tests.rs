use super::*;

fn pattern_bar(idx: usize, close: f64, ema144: f64, ema576: f64) -> PatternBar {
    PatternBar {
        ts: idx as i64 * super::super::super::super::super::super::MS_15M,
        high: close + 0.2,
        low: close - 0.2,
        close,
        ema144: Some(ema144),
        ema576: Some(ema576),
        atr14: Some(2.0),
    }
}

fn step_with_bar(
    machine: &mut FailedRetestTerminationMachine,
    bars: &mut Vec<PatternBar>,
    close: f64,
    ema144: f64,
    ema576: f64,
) -> V6StepResult {
    let idx = bars.len();
    bars.push(pattern_bar(idx, close, ema144, ema576));
    machine.step(bars, idx)
}

fn activate_short(machine: &mut FailedRetestTerminationMachine, bars: &mut Vec<PatternBar>) {
    for _ in 0..REGIME_WINDOW_BARS {
        step_with_bar(machine, bars, 101.0, 102.0, 100.0);
    }
    step_with_bar(machine, bars, 99.5, 101.5, 100.0);
    let activation = step_with_bar(machine, bars, 99.0, 101.0, 100.0);
    assert_eq!(
        activation.episode_started.map(|active| active.direction),
        Some(Direction::Short)
    );
}

fn activate_long(machine: &mut FailedRetestTerminationMachine, bars: &mut Vec<PatternBar>) {
    for _ in 0..REGIME_WINDOW_BARS {
        step_with_bar(machine, bars, 99.0, 98.0, 100.0);
    }
    step_with_bar(machine, bars, 100.5, 98.5, 100.0);
    let activation = step_with_bar(machine, bars, 101.0, 99.0, 100.0);
    assert_eq!(
        activation.episode_started.map(|active| active.direction),
        Some(Direction::Long)
    );
}

#[test]
fn failed_short_retest_terminates_episode_and_blocks_later_rearm() {
    let mut machine = FailedRetestTerminationMachine::new();
    let mut bars = Vec::new();
    activate_short(&mut machine, &mut bars);
    assert!(machine.short_active.is_some());

    let failed = step_with_bar(&mut machine, &mut bars, 101.8, 101.0, 100.0);
    assert_eq!(failed.failed_retest_invalidations.len(), 1);
    assert_eq!(
        failed.failed_retest_invalidations[0].active.direction,
        Direction::Short
    );
    assert!(failed.candidates.is_empty());
    assert!(machine.short_active.is_none());

    let later_departure = step_with_bar(&mut machine, &mut bars, 99.0, 101.0, 100.0);
    assert_eq!(later_departure.retest_rearms, 0);
    assert!(later_departure.candidates.is_empty());
}

#[test]
fn failed_long_retest_termination_is_mirrored() {
    let mut machine = FailedRetestTerminationMachine::new();
    let mut bars = Vec::new();
    activate_long(&mut machine, &mut bars);

    let failed = step_with_bar(&mut machine, &mut bars, 98.2, 99.0, 100.0);
    assert_eq!(failed.failed_retest_invalidations.len(), 1);
    assert_eq!(
        failed.failed_retest_invalidations[0].active.direction,
        Direction::Long
    );
    assert!(machine.long_active.is_none());
}

#[test]
fn successful_retest_keeps_v4_repeat_rearm_behavior() {
    let mut machine = FailedRetestTerminationMachine::new();
    let mut bars = Vec::new();
    activate_short(&mut machine, &mut bars);

    let held = step_with_bar(&mut machine, &mut bars, 100.4, 101.0, 100.0);
    assert_eq!(held.candidates.len(), 1);
    assert!(held.failed_retest_invalidations.is_empty());
    assert!(machine
        .short_active
        .is_some_and(|active| active.rearmed_idx.is_none()));

    let rearmed = step_with_bar(&mut machine, &mut bars, 99.0, 101.0, 100.0);
    assert_eq!(rearmed.retest_rearms, 1);
    assert!(machine
        .short_active
        .is_some_and(|active| active.rearmed_idx.is_some()));
}

#[test]
fn v6_decision_is_outcome_blind_when_all_preregistered_gates_pass() {
    let summary = V6Summary {
        candidate_count: V6_MIN_CANDIDATES,
        by_direction: BTreeMap::from([("long", 2_500), ("short", 2_500)]),
        by_cross_phase: BTreeMap::new(),
        by_symbol: (0..8).map(|idx| (idx.to_string(), 1)).collect(),
        by_month_utc: (0..6).map(|idx| (idx.to_string(), 1)).collect(),
        effective_market_events: 100,
        stages: V6StageCounts::default(),
    };
    let audits = V6_TARGETS
        .iter()
        .map(|target| TargetAudit {
            name: target.name,
            symbol: target.symbol,
            direction: target.direction.label(),
            start_ms: target.start_ms,
            end_ms: target.end_ms,
            expectation: target.expectation.label(),
            passed: true,
            matched_signal_timestamps_ms: if target.expectation == V6TargetExpectation::MustMatch {
                vec![target.start_ms]
            } else {
                Vec::new()
            },
        })
        .collect::<Vec<_>>();
    let inputs = v6_target_input_template()
        .into_iter()
        .map(|mut coverage| {
            coverage.ready_candles = coverage.expected_candles;
            coverage.ready = true;
            coverage
        })
        .collect::<Vec<_>>();
    let lifecycle = V6BtcLifecycleAudit {
        last_short_episode_breakout_before_old_signal_ts_ms: Some(1),
        invalidated_ts_ms: Some(2),
        invalidation_retest_extreme_to_ema144_atr: Some(-0.31),
        invalidation_close_to_ema144_directional_atr: Some(-0.31),
        old_wrong_signal_ts_ms: BTC_WRONG_SHORT_SIGNAL_MS,
        new_short_episode_start_timestamps_ms: Vec::new(),
        passed: true,
    };

    let decision = decide_v6(&summary, &audits, &inputs, &lifecycle);
    assert_eq!(decision.status, "coverage_pass_ready_for_l2_prereg");
    assert!(!decision.outcome_evaluation_performed);
}
