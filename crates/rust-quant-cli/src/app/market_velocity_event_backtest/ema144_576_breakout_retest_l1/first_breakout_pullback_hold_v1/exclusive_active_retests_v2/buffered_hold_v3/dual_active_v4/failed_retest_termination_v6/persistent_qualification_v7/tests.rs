use super::*;

fn pattern_bar(idx: usize, close: f64, ema144: f64, ema576: f64) -> PatternBar {
    PatternBar {
        ts: idx as i64 * super::super::super::super::super::super::super::MS_15M,
        high: close + 0.2,
        low: close - 0.2,
        close,
        ema144: Some(ema144),
        ema576: Some(ema576),
        atr14: Some(2.0),
    }
}

fn step_with_bar(
    machine: &mut V7Machine,
    bars: &mut Vec<PatternBar>,
    close: f64,
    ema144: f64,
    ema576: f64,
) -> V6StepResult {
    let idx = bars.len();
    bars.push(pattern_bar(idx, close, ema144, ema576));
    machine.step(bars, idx)
}

#[test]
fn failed_episode_can_restart_only_after_new_breakout_using_historical_qualification() {
    let mut machine = V7Machine::new();
    let mut bars = Vec::new();
    for _ in 0..REGIME_WINDOW_BARS {
        step_with_bar(&mut machine, &mut bars, 99.0, 98.0, 100.0);
    }
    let qualification_ts = machine
        .long_qualification
        .latched
        .expect("long qualification")
        .qualified_ts;

    step_with_bar(&mut machine, &mut bars, 100.5, 98.5, 100.0);
    let first_episode = step_with_bar(&mut machine, &mut bars, 101.0, 99.0, 100.0);
    assert_eq!(
        first_episode.episode_started.map(|active| active.direction),
        Some(Direction::Long)
    );
    let failed = step_with_bar(&mut machine, &mut bars, 98.2, 99.0, 100.0);
    assert_eq!(failed.failed_retest_invalidations.len(), 1);
    assert!(machine.long_active.is_none());
    assert_eq!(
        machine
            .long_qualification
            .latched
            .expect("qualification survives")
            .qualified_ts,
        qualification_ts
    );

    step_with_bar(&mut machine, &mut bars, 100.5, 99.0, 100.0);
    let restarted = step_with_bar(&mut machine, &mut bars, 101.0, 99.0, 100.0);
    let active = restarted.episode_started.expect("new long episode");
    assert_eq!(active.direction, Direction::Long);
    assert_eq!(active.qualified_ts, qualification_ts);
}

#[test]
fn relation_flip_keeps_latch_but_input_gap_clears_it() {
    let mut machine = V7Machine::new();
    let mut bars = Vec::new();
    for _ in 0..REGIME_WINDOW_BARS {
        step_with_bar(&mut machine, &mut bars, 99.0, 98.0, 100.0);
    }
    step_with_bar(&mut machine, &mut bars, 101.0, 102.0, 100.0);
    assert!(machine.long_qualification.latched.is_some());

    let idx = bars.len();
    bars.push(PatternBar {
        ts: idx as i64 * super::super::super::super::super::super::super::MS_15M,
        high: 1.0,
        low: 1.0,
        close: 1.0,
        ema144: None,
        ema576: Some(1.0),
        atr14: Some(1.0),
    });
    machine.step(&bars, idx);
    assert!(machine.long_qualification.latched.is_none());
}

#[test]
fn v7_decision_is_outcome_blind_when_all_preregistered_gates_pass() {
    let summary = V7Summary {
        candidate_count: V7_MIN_CANDIDATES,
        by_direction: BTreeMap::from([("long", 2_500), ("short", 2_500)]),
        by_cross_phase: BTreeMap::new(),
        by_symbol: (0..8).map(|idx| (idx.to_string(), 1)).collect(),
        by_month_utc: (0..6).map(|idx| (idx.to_string(), 1)).collect(),
        effective_market_events: 100,
        stages: V7StageCounts::default(),
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
        invalidated_ts_ms: Some(BTC_JULY18_END_MS),
        invalidation_retest_extreme_to_ema144_atr: Some(-0.31),
        invalidation_close_to_ema144_directional_atr: Some(-0.31),
        old_wrong_signal_ts_ms: BTC_WRONG_SHORT_SIGNAL_MS,
        new_short_episode_start_timestamps_ms: Vec::new(),
        passed: true,
    };

    let decision = decide_v7(&summary, &audits, &inputs, &lifecycle, true);
    assert_eq!(decision.status, "coverage_pass_ready_for_l2_prereg");
    assert!(!decision.outcome_evaluation_performed);
}
