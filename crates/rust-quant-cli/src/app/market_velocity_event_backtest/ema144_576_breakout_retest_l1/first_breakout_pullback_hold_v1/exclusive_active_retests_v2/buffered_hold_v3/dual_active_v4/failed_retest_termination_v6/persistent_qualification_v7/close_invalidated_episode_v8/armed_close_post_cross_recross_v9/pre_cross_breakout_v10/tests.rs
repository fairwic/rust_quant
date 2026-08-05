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
        ts: idx as i64
            * super::super::super::super::super::super::super::super::super::super::MS_15M,
        high,
        low,
        close,
        ema144: Some(ema144),
        ema576: Some(ema576),
        atr14: Some(2.0),
    }
}

fn step_with_bar(
    machine: &mut V9Machine,
    bars: &mut Vec<PatternBar>,
    high: f64,
    low: f64,
    close: f64,
    ema144: f64,
    ema576: f64,
) -> V9StepResult {
    let idx = bars.len();
    bars.push(pattern_bar(idx, high, low, close, ema144, ema576));
    machine.step(bars, idx)
}

fn qualify_short(machine: &mut V9Machine, bars: &mut Vec<PatternBar>) {
    for _ in 0..REGIME_WINDOW_BARS {
        step_with_bar(machine, bars, 101.2, 100.8, 101.0, 102.0, 100.0);
    }
    assert!(machine.short_qualification.latched.is_some());
}

#[test]
fn post_cross_price_breakout_cannot_create_new_short_episode() {
    let mut machine = V9Machine::with_pre_cross_activation(true);
    let mut bars = Vec::new();
    qualify_short(&mut machine, &mut bars);

    step_with_bar(&mut machine, &mut bars, 99.7, 99.3, 99.5, 99.0, 100.0);
    let late_breakout = step_with_bar(&mut machine, &mut bars, 99.2, 98.8, 99.0, 99.0, 100.0);
    assert!(late_breakout.episode_started.is_none());
    assert!(machine.short_active.is_none());
}

#[test]
fn pre_cross_breakout_can_start_and_later_emit_post_cross_retest() {
    let mut machine = V9Machine::with_pre_cross_activation(true);
    let mut bars = Vec::new();
    qualify_short(&mut machine, &mut bars);

    step_with_bar(&mut machine, &mut bars, 99.7, 99.3, 99.5, 101.0, 100.0);
    let activation = step_with_bar(&mut machine, &mut bars, 99.2, 98.8, 99.0, 101.0, 100.0);
    assert_eq!(
        activation
            .episode_started
            .map(|active| active.core.direction),
        Some(Direction::Short)
    );
    assert!(activation
        .episode_started
        .is_some_and(|active| !active.post_cross_seen));

    let cross = step_with_bar(&mut machine, &mut bars, 98.2, 97.8, 98.0, 99.0, 100.0);
    assert_eq!(cross.post_cross_latches.len(), 1);
    let held = step_with_bar(&mut machine, &mut bars, 99.1, 98.7, 98.9, 99.0, 100.0);
    assert_eq!(held.candidates.len(), 1);
    assert!(machine
        .short_active
        .is_some_and(|active| active.post_cross_seen));
}

#[test]
fn v10_decision_is_outcome_blind_when_all_preregistered_gates_pass() {
    let summary = V9Summary {
        candidate_count: V10_MIN_CANDIDATES,
        by_direction: BTreeMap::from([("long", 2_500), ("short", 2_500)]),
        by_cross_phase: BTreeMap::new(),
        by_symbol: (0..8).map(|idx| (idx.to_string(), 1)).collect(),
        by_month_utc: (0..6).map(|idx| (idx.to_string(), 1)).collect(),
        effective_market_events: 100,
        stages: V9StageCounts::default(),
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
    let lifecycle = V9BtcLifecycleAudit {
        historical_short_qualification_ts_ms: Some(1),
        last_short_episode_breakout_before_old_signal_ts_ms: Some(2),
        post_cross_seen_ts_ms: Some(3),
        rearm_timestamps_ms: vec![4],
        held_retest_timestamps_ms: vec![5],
        wick_only_failed_retest_timestamps_ms: Vec::new(),
        invalidated_ts_ms: Some(BTC_JULY18_END_MS),
        invalidation_reason: Some("post_cross_opposite_two_close_ema576_breakout"),
        invalidation_confirmation_direction: Some("long"),
        old_wrong_signal_ts_ms: BTC_WRONG_SHORT_SIGNAL_MS,
        new_short_episode_start_timestamps_ms: Vec::new(),
        passed: true,
    };

    let decision = decide_v10(&summary, &audits, &inputs, &lifecycle, true);
    assert_eq!(decision.status, "coverage_pass_ready_for_l2_prereg");
    assert!(!decision.outcome_evaluation_performed);
}
