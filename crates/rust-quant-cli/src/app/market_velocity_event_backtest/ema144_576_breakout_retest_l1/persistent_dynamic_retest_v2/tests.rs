use super::*;

fn bar(idx: usize, close: f64, ema144: f64, ema576: f64) -> PatternBar {
    PatternBar {
        ts: idx as i64 * MS_15M,
        high: close + 0.5,
        low: close - 0.5,
        close,
        ema144: Some(ema144),
        ema576: Some(ema576),
        atr14: Some(2.0),
    }
}

fn long_active_bars() -> Vec<PatternBar> {
    let mut bars = (0..REQUIRED_QUALIFICATION_BARS)
        .map(|idx| bar(idx, 99.0, 98.0, 100.0))
        .collect::<Vec<_>>();
    let first = bars.len();
    bars.push(bar(first, 100.5, 98.5, 100.0));
    let confirmation = bars.len();
    bars.push(bar(confirmation, 101.5, 99.0, 100.0));
    bars
}

fn scan(bars: &[PatternBar]) -> (PersistentTransitionMachine, Vec<CandidateCore>) {
    scan_with_policy(bars, QualificationPolicy::LatestOnly)
}

fn scan_with_policy(
    bars: &[PatternBar],
    policy: QualificationPolicy,
) -> (PersistentTransitionMachine, Vec<CandidateCore>) {
    let mut machine = PersistentTransitionMachine::new_with_policy(policy);
    let mut candidates = Vec::new();
    for idx in 0..bars.len() {
        if let Some(candidate) = machine.step(bars, idx).candidate {
            candidates.push(candidate);
        }
    }
    (machine, candidates)
}

#[test]
fn v3_can_reuse_a_recent_long_qualification_after_a_short_transition() {
    let mut bars = long_active_bars();
    for _ in 0..REQUIRED_QUALIFICATION_BARS {
        let idx = bars.len();
        bars.push(bar(idx, 104.0, 102.0, 100.0));
    }
    let first_breakdown = bars.len();
    bars.push(bar(first_breakdown, 99.5, 102.0, 100.0));
    let short_confirmation = bars.len();
    bars.push(bar(short_confirmation, 98.5, 102.0, 100.0));
    let first_rebreak = bars.len();
    bars.push(bar(first_rebreak, 100.5, 101.5, 100.0));
    let long_confirmation = bars.len();
    bars.push(bar(long_confirmation, 102.0, 100.5, 100.0));
    let reexpand_idx = bars.len();
    bars.push(bar(reexpand_idx, 103.0, 101.0, 100.0));
    let touch_idx = bars.len();
    bars.push(bar(touch_idx, 101.0, 101.0, 100.0));

    let (machine, candidates) = scan_with_policy(
        &bars,
        QualificationPolicy::RecentDual {
            max_age_bars: QUALIFICATION_MEMORY_BARS,
            refresh_while_sustained: false,
        },
    );

    assert_eq!(
        machine.active.expect("long reactivated").direction,
        Direction::Long
    );
    assert_eq!(
        candidates.last().expect("long touch").direction,
        Direction::Long
    );
}

#[test]
fn v4_expires_from_the_last_sustained_qualification_bar_not_the_first() {
    let mut bars = long_active_bars();
    for _ in 0..600 {
        let idx = bars.len();
        bars.push(bar(idx, 104.0, 98.0, 100.0));
    }
    for _ in 0..REQUIRED_QUALIFICATION_BARS {
        let idx = bars.len();
        bars.push(bar(idx, 104.0, 102.0, 100.0));
    }
    let first_breakdown = bars.len();
    bars.push(bar(first_breakdown, 99.5, 102.0, 100.0));
    let short_confirmation = bars.len();
    bars.push(bar(short_confirmation, 98.5, 102.0, 100.0));
    let first_rebreak = bars.len();
    bars.push(bar(first_rebreak, 100.5, 101.5, 100.0));
    let long_confirmation = bars.len();
    bars.push(bar(long_confirmation, 102.0, 100.5, 100.0));

    let (v3_machine, _) = scan_with_policy(
        &bars,
        QualificationPolicy::RecentDual {
            max_age_bars: QUALIFICATION_MEMORY_BARS,
            refresh_while_sustained: false,
        },
    );
    let (v4_machine, _) = scan_with_policy(
        &bars,
        QualificationPolicy::RecentDual {
            max_age_bars: QUALIFICATION_MEMORY_BARS,
            refresh_while_sustained: true,
        },
    );

    assert_eq!(
        v3_machine.active.expect("V3 remains short").direction,
        Direction::Short
    );
    assert_eq!(
        v4_machine.active.expect("V4 reactivates long").direction,
        Direction::Long
    );
}

#[test]
fn long_transition_activates_only_after_144_bars_and_effective_breakout() {
    let bars = long_active_bars();
    let (machine, candidates) = scan(&bars);

    assert!(candidates.is_empty());
    assert_eq!(
        machine.active.expect("long active").direction,
        Direction::Long
    );
    assert_eq!(
        machine.retest_arm.expect("retest arm").armed_idx,
        bars.len() - 1
    );
}

#[test]
fn first_touch_uses_previous_completed_ema_anchor_and_does_not_require_close_hold() {
    let mut bars = long_active_bars();
    let idx = bars.len();
    let mut touch = bar(idx, 98.0, 99.2, 100.0);
    touch.low = 97.5;
    bars.push(touch);

    let (_, candidates) = scan(&bars);
    assert_eq!(candidates.len(), 1);
    let candidate = candidate_from_core("BTC-USDT-SWAP", candidates[0]).expect("candidate");
    assert_eq!(candidate.anchor_ema144, 99.0);
    assert_eq!(candidate.touch_zone_boundary, 99.6);
    assert!(!candidate.close_holds_current_ema144);
}

#[test]
fn a_new_reexpansion_allows_one_new_touch_in_the_same_active_transition() {
    let mut bars = long_active_bars();
    let first_touch_idx = bars.len();
    bars.push(bar(first_touch_idx, 99.0, 99.2, 100.0));
    let reexpand_idx = bars.len();
    bars.push(bar(reexpand_idx, 103.0, 100.0, 100.5));
    let second_touch_idx = bars.len();
    bars.push(bar(second_touch_idx, 100.0, 100.0, 100.5));

    let (_, candidates) = scan(&bars);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].signal_idx, first_touch_idx);
    assert_eq!(candidates[1].signal_idx, second_touch_idx);
}

#[test]
fn long_active_persists_after_ema_cross_until_full_short_transition() {
    let mut bars = long_active_bars();
    for _ in 0..277 {
        let idx = bars.len();
        bars.push(bar(idx, 104.0, 102.0, 100.0));
    }
    let touch_idx = bars.len();
    bars.push(bar(touch_idx, 101.0, 102.0, 100.0));

    let (machine, candidates) = scan(&bars);
    assert_eq!(
        machine.qualified.expect("short qualified").direction,
        Qualification::Short
    );
    assert_eq!(
        machine.active.expect("long remains active").direction,
        Direction::Long
    );
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].direction, Direction::Long);
}

#[test]
fn full_mirror_transition_switches_active_direction_and_arms_short_retest() {
    let mut bars = long_active_bars();
    for _ in 0..REQUIRED_QUALIFICATION_BARS {
        let idx = bars.len();
        bars.push(bar(idx, 104.0, 102.0, 100.0));
    }
    let first_breakdown = bars.len();
    bars.push(bar(first_breakdown, 99.5, 102.0, 100.0));
    let confirmation = bars.len();
    bars.push(bar(confirmation, 98.5, 102.0, 100.0));
    let touch_idx = bars.len();
    bars.push(bar(touch_idx, 101.0, 102.0, 100.0));

    let (machine, candidates) = scan(&bars);
    assert_eq!(
        machine.active.expect("short active").direction,
        Direction::Short
    );
    assert_eq!(
        candidates.last().expect("short touch").direction,
        Direction::Short
    );
}

#[test]
fn touch_buffer_includes_exactly_point_30_atr() {
    let mut bars = long_active_bars();
    let idx = bars.len();
    let mut touch = bar(idx, 100.0, 99.2, 100.0);
    touch.low = 99.6;
    bars.push(touch);

    let (_, candidates) = scan(&bars);
    assert_eq!(candidates.len(), 1);
}

#[test]
fn passing_l1_decision_still_reports_no_outcome_evaluation() {
    let mut summary = V2Summary {
        candidate_count: 10,
        by_direction: BTreeMap::from([("long", 8), ("short", 2)]),
        by_cross_phase: BTreeMap::new(),
        by_close_hold: BTreeMap::new(),
        by_symbol: (0..4).map(|idx| (idx.to_string(), 1)).collect(),
        by_month_utc: (0..3).map(|idx| (idx.to_string(), 1)).collect(),
        effective_market_events: 5,
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
            matched_signal_timestamps_ms: vec![target.start_ms],
            matched: true,
        })
        .collect::<Vec<_>>();
    let inputs = super::super::target_input_template()
        .into_iter()
        .map(|mut coverage| {
            coverage.ready = true;
            coverage.ready_candles = coverage.expected_candles;
            coverage
        })
        .collect::<Vec<_>>();

    let decision = decide(&summary, &audits, &inputs, V6_VARIANT);
    assert_eq!(decision.status, "coverage_pass_ready_for_l2_prereg");
    assert!(!decision.outcome_evaluation_performed);

    summary.candidate_count = 50_601;
    let v8_decision = decide(&summary, &audits, &inputs, V8_VARIANT);
    assert_eq!(v8_decision.status, "stop_coverage_gate_failed");
    assert_eq!(
        v8_decision
            .gates
            .get("v8_candidate_reduction_between_30_and_85_pct"),
        Some(&false)
    );
}
