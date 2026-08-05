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

fn step_with_bar(
    machine: &mut PersistentQualificationFiniteEpisodeMachine,
    bars: &mut Vec<PatternBar>,
    close: f64,
    ema144: f64,
    ema576: f64,
) -> V5StepResult {
    let idx = bars.len();
    bars.push(pattern_bar(idx, close, ema144, ema576));
    machine.step(bars, idx)
}

fn qualify_short(
    machine: &mut PersistentQualificationFiniteEpisodeMachine,
    bars: &mut Vec<PatternBar>,
) {
    for _ in 0..REGIME_WINDOW_BARS {
        step_with_bar(machine, bars, 101.0, 102.0, 100.0);
    }
    assert!(machine.short_qualification.latched.is_some());
}

fn qualify_long(
    machine: &mut PersistentQualificationFiniteEpisodeMachine,
    bars: &mut Vec<PatternBar>,
) {
    for _ in 0..REGIME_WINDOW_BARS {
        step_with_bar(machine, bars, 99.0, 98.0, 100.0);
    }
    assert!(machine.long_qualification.latched.is_some());
}

#[test]
fn opposite_two_close_break_invalidates_short_before_touch() {
    let mut machine = PersistentQualificationFiniteEpisodeMachine::new();
    let mut bars = Vec::new();
    qualify_short(&mut machine, &mut bars);

    step_with_bar(&mut machine, &mut bars, 99.5, 102.0, 100.0);
    let activation = step_with_bar(&mut machine, &mut bars, 99.0, 102.0, 100.0);
    assert_eq!(
        activation.episode_started.map(|active| active.direction),
        Some(Direction::Short)
    );
    assert!(machine
        .active
        .is_some_and(|active| active.rearmed_idx.is_some()));

    let first_close_back = step_with_bar(&mut machine, &mut bars, 100.5, 102.0, 100.0);
    assert!(first_close_back.invalidation.is_none());
    assert!(machine.active.is_some());

    // 第二根站回 EMA576 的确认 K 即使盘中触到旧空头 EMA144，也必须先中断旧 episode。
    let invalidation = step_with_bar(&mut machine, &mut bars, 101.8, 102.0, 100.0);
    assert_eq!(
        invalidation
            .invalidation
            .map(|event| event.invalidated_active.direction),
        Some(Direction::Short)
    );
    assert!(invalidation.candidate.is_none());
    assert!(machine.active.is_none());

    let later_old_retest = step_with_bar(&mut machine, &mut bars, 101.0, 102.0, 100.0);
    assert!(later_old_retest.candidate.is_none());
}

#[test]
fn opposite_two_close_break_is_mirrored_for_long() {
    let mut machine = PersistentQualificationFiniteEpisodeMachine::new();
    let mut bars = Vec::new();
    qualify_long(&mut machine, &mut bars);

    step_with_bar(&mut machine, &mut bars, 100.5, 98.0, 100.0);
    let activation = step_with_bar(&mut machine, &mut bars, 101.0, 98.0, 100.0);
    assert_eq!(
        activation.episode_started.map(|active| active.direction),
        Some(Direction::Long)
    );

    step_with_bar(&mut machine, &mut bars, 99.5, 98.0, 100.0);
    let invalidation = step_with_bar(&mut machine, &mut bars, 98.2, 98.0, 100.0);
    assert_eq!(
        invalidation
            .invalidation
            .map(|event| event.invalidated_active.direction),
        Some(Direction::Long)
    );
    assert!(invalidation.candidate.is_none());
    assert!(machine.active.is_none());
}

#[test]
fn historical_qualification_can_start_a_new_episode_after_invalidation() {
    let mut machine = PersistentQualificationFiniteEpisodeMachine::new();
    let mut bars = Vec::new();
    qualify_long(&mut machine, &mut bars);
    let qualification_ts = machine
        .long_qualification
        .latched
        .expect("long qualification")
        .qualified_ts;

    step_with_bar(&mut machine, &mut bars, 100.5, 98.0, 100.0);
    step_with_bar(&mut machine, &mut bars, 101.0, 98.0, 100.0);
    step_with_bar(&mut machine, &mut bars, 99.5, 98.0, 100.0);
    let invalidation = step_with_bar(&mut machine, &mut bars, 99.0, 98.0, 100.0);
    assert!(invalidation.invalidation.is_some());
    assert!(machine.active.is_none());

    step_with_bar(&mut machine, &mut bars, 100.5, 98.0, 100.0);
    let restarted = step_with_bar(&mut machine, &mut bars, 101.0, 98.0, 100.0);
    let active = restarted.episode_started.expect("long episode restarted");
    assert_eq!(active.direction, Direction::Long);
    assert_eq!(active.qualified_ts, qualification_ts);
}

#[test]
fn relation_flip_preserves_latched_qualification_but_resets_current_run() {
    let mut tracker = PersistentQualificationTracker::new(Direction::Long);
    let mut last = None;
    for idx in 0..REGIME_WINDOW_BARS {
        last = tracker.step(pattern_bar(idx, 99.0, 98.0, 100.0).ready().unwrap());
    }
    assert!(last.is_some());
    let qualified_ts = tracker.latched.expect("latched qualification").qualified_ts;

    tracker.step(
        pattern_bar(REGIME_WINDOW_BARS, 101.0, 102.0, 100.0)
            .ready()
            .unwrap(),
    );
    assert_eq!(tracker.relation_age_bars, 0);
    assert_eq!(tracker.latched.unwrap().qualified_ts, qualified_ts);
}

#[test]
fn v5_decision_is_outcome_blind_when_all_preregistered_gates_pass() {
    let summary = V5Summary {
        candidate_count: V5_MIN_CANDIDATES,
        by_direction: BTreeMap::from([("long", 4_500), ("short", 4_500)]),
        by_cross_phase: BTreeMap::new(),
        by_symbol: (0..8).map(|idx| (idx.to_string(), 1)).collect(),
        by_month_utc: (0..6).map(|idx| (idx.to_string(), 1)).collect(),
        effective_market_events: 100,
        stages: V5StageCounts::default(),
    };
    let audits = V5_TARGETS
        .iter()
        .map(|target| TargetAudit {
            name: target.name,
            symbol: target.symbol,
            direction: target.direction.label(),
            start_ms: target.start_ms,
            end_ms: target.end_ms,
            expectation: target.expectation.label(),
            passed: true,
            matched_signal_timestamps_ms: if target.expectation == V5TargetExpectation::MustMatch {
                vec![target.start_ms]
            } else {
                Vec::new()
            },
        })
        .collect::<Vec<_>>();
    let inputs = v5_target_input_template()
        .into_iter()
        .map(|mut coverage| {
            coverage.ready_candles = coverage.expected_candles;
            coverage.ready = true;
            coverage
        })
        .collect::<Vec<_>>();
    let lifecycle = V5BtcLifecycleAudit {
        v3_reported_short_breakout_ts_ms: BTC_WRONG_SHORT_BREAKOUT_MS,
        first_short_episode_breakout_ts_ms: Some(BTC_WRONG_SHORT_BREAKOUT_MS),
        last_short_episode_breakout_before_old_signal_ts_ms: Some(BTC_WRONG_SHORT_BREAKOUT_MS),
        invalidated_ts_ms: Some(BTC_WRONG_SHORT_BREAKOUT_MS + 1),
        invalidation_confirmation_direction: Some("long"),
        old_wrong_signal_ts_ms: BTC_WRONG_SHORT_SIGNAL_MS,
        new_short_episode_start_timestamps_ms: Vec::new(),
        passed: true,
    };

    let decision = decide_v5(&summary, &audits, &inputs, &lifecycle);
    assert_eq!(decision.status, "coverage_pass_ready_for_l2_prereg");
    assert!(!decision.outcome_evaluation_performed);
}
