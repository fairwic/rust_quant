use super::*;

fn pattern_bar(
    idx: usize,
    high: f64,
    low: f64,
    close: f64,
    ema144: f64,
    ema576: f64,
) -> PatternBar {
    PatternBar {
        ts: idx as i64 * super::super::super::super::super::super::super::super::MS_15M,
        high,
        low,
        close,
        ema144: Some(ema144),
        ema576: Some(ema576),
        atr14: Some(2.0),
    }
}

fn step_with_bar(
    machine: &mut V8Machine,
    bars: &mut Vec<PatternBar>,
    high: f64,
    low: f64,
    close: f64,
    ema144: f64,
    ema576: f64,
) -> V8StepResult {
    let idx = bars.len();
    bars.push(pattern_bar(idx, high, low, close, ema144, ema576));
    machine.step(bars, idx)
}

fn activate_long(machine: &mut V8Machine, bars: &mut Vec<PatternBar>) {
    for _ in 0..REGIME_WINDOW_BARS {
        step_with_bar(machine, bars, 99.2, 98.8, 99.0, 98.0, 100.0);
    }
    step_with_bar(machine, bars, 100.7, 100.3, 100.5, 98.5, 100.0);
    let activation = step_with_bar(machine, bars, 101.2, 100.8, 101.0, 99.0, 100.0);
    assert_eq!(
        activation.episode_started.map(|active| active.direction),
        Some(Direction::Long)
    );
}

fn activate_short(machine: &mut V8Machine, bars: &mut Vec<PatternBar>) {
    for _ in 0..REGIME_WINDOW_BARS {
        step_with_bar(machine, bars, 101.2, 100.8, 101.0, 102.0, 100.0);
    }
    step_with_bar(machine, bars, 99.7, 99.3, 99.5, 101.5, 100.0);
    let activation = step_with_bar(machine, bars, 99.2, 98.8, 99.0, 101.0, 100.0);
    assert_eq!(
        activation.episode_started.map(|active| active.direction),
        Some(Direction::Short)
    );
}

#[test]
fn wick_only_breach_consumes_long_arm_without_terminating_episode() {
    let mut machine = V8Machine::new();
    let mut bars = Vec::new();
    activate_long(&mut machine, &mut bars);

    let wick_only = step_with_bar(&mut machine, &mut bars, 101.0, 98.2, 99.2, 99.0, 100.0);
    assert_eq!(wick_only.wick_only_failed_retests, 1);
    assert!(wick_only.close_invalidations.is_empty());
    assert!(wick_only.candidates.is_empty());
    assert!(machine
        .long_active
        .is_some_and(|active| active.rearmed_idx.is_none()));
}

#[test]
fn long_close_breach_terminates_episode_even_when_arm_is_consumed() {
    let mut machine = V8Machine::new();
    let mut bars = Vec::new();
    activate_long(&mut machine, &mut bars);
    step_with_bar(&mut machine, &mut bars, 101.0, 98.2, 99.2, 99.0, 100.0);
    assert!(machine
        .long_active
        .is_some_and(|active| active.rearmed_idx.is_none()));

    let invalidated = step_with_bar(&mut machine, &mut bars, 99.0, 98.0, 98.2, 99.0, 100.0);
    assert_eq!(invalidated.close_invalidations.len(), 1);
    assert_eq!(
        invalidated.close_invalidations[0].active.direction,
        Direction::Long
    );
    assert!(machine.long_active.is_none());
}

#[test]
fn short_close_breach_is_mirrored_and_independent_of_arm() {
    let mut machine = V8Machine::new();
    let mut bars = Vec::new();
    activate_short(&mut machine, &mut bars);
    let held = step_with_bar(&mut machine, &mut bars, 101.2, 100.8, 100.9, 101.0, 100.0);
    assert_eq!(held.candidates.len(), 1);
    assert!(machine
        .short_active
        .is_some_and(|active| active.rearmed_idx.is_none()));

    let invalidated = step_with_bar(&mut machine, &mut bars, 102.0, 101.6, 101.8, 101.0, 100.0);
    assert_eq!(invalidated.close_invalidations.len(), 1);
    assert_eq!(
        invalidated.close_invalidations[0].active.direction,
        Direction::Short
    );
    assert!(machine.short_active.is_none());
}

#[test]
fn v8_decision_is_outcome_blind_when_all_preregistered_gates_pass() {
    let summary = V8Summary {
        candidate_count: V8_MIN_CANDIDATES,
        by_direction: BTreeMap::from([("long", 2_500), ("short", 2_500)]),
        by_cross_phase: BTreeMap::new(),
        by_symbol: (0..8).map(|idx| (idx.to_string(), 1)).collect(),
        by_month_utc: (0..6).map(|idx| (idx.to_string(), 1)).collect(),
        effective_market_events: 100,
        stages: V8StageCounts::default(),
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
    let lifecycle = V8BtcLifecycleAudit {
        last_short_episode_breakout_before_old_signal_ts_ms: Some(1),
        invalidated_ts_ms: Some(BTC_JULY18_END_MS),
        invalidation_close_to_ema144_directional_atr: Some(-0.31),
        old_wrong_signal_ts_ms: BTC_WRONG_SHORT_SIGNAL_MS,
        new_short_episode_start_timestamps_ms: Vec::new(),
        passed: true,
    };

    let decision = decide_v8(&summary, &audits, &inputs, &lifecycle, true);
    assert_eq!(decision.status, "coverage_pass_ready_for_l2_prereg");
    assert!(!decision.outcome_evaluation_performed);
}
