use super::*;
use crate::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::MS_15M;

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

#[test]
fn opposite_transition_does_not_delete_still_valid_long_active_latch() {
    let mut bars = (0..REQUIRED_QUALIFICATION_BARS)
        .map(|idx| bar(idx, 99.0, 98.0, 100.0))
        .collect::<Vec<_>>();
    let first_breakout = bars.len();
    bars.push(bar(first_breakout, 100.5, 98.5, 100.0));
    let long_confirmation = bars.len();
    bars.push(bar(long_confirmation, 101.5, 99.0, 100.0));
    for _ in 0..REQUIRED_QUALIFICATION_BARS {
        let idx = bars.len();
        bars.push(bar(idx, 104.0, 102.0, 100.0));
    }
    let first_breakdown = bars.len();
    bars.push(bar(first_breakdown, 99.5, 102.0, 100.0));
    let short_confirmation = bars.len();
    bars.push(bar(short_confirmation, 98.5, 102.0, 100.0));
    let long_reexpand = bars.len();
    bars.push(bar(long_reexpand, 104.0, 102.0, 100.0));
    let long_touch = bars.len();
    bars.push(bar(long_touch, 102.0, 102.0, 100.0));

    let mut machine = IndependentActiveMachine::new();
    let mut candidates = Vec::new();
    for idx in 0..bars.len() {
        candidates.extend(machine.step(&bars, idx).candidates);
    }

    assert!(machine.long.active.is_some());
    assert!(machine.short.active.is_some());
    assert_eq!(
        candidates.last().expect("long touch").direction,
        Direction::Long
    );
    assert_eq!(
        candidates.last().expect("long touch").signal_idx,
        long_touch
    );
}

#[test]
fn completed_close_side_of_ema576_clears_the_opposite_retest_arm() {
    let mut machine = IndependentActiveMachine::new();
    let mut bars = (0..REQUIRED_QUALIFICATION_BARS)
        .map(|idx| bar(idx, 99.0, 98.0, 100.0))
        .collect::<Vec<_>>();
    let first = bars.len();
    bars.push(bar(first, 100.5, 98.5, 100.0));
    let confirmation = bars.len();
    bars.push(bar(confirmation, 101.5, 99.0, 100.0));
    for idx in 0..bars.len() {
        machine.step(&bars, idx);
    }
    assert!(machine.long.retest_arm.is_some());

    let below_slow_idx = bars.len();
    let mut below_slow = bar(below_slow_idx, 99.8, 98.0, 100.0);
    below_slow.low = 99.7;
    bars.push(below_slow);
    machine.step(&bars, below_slow_idx);

    assert!(machine.long.retest_arm.is_none());
}

#[test]
fn v6_keeps_an_established_qualification_and_transition_without_expiry() {
    let bars = (0..700)
        .map(|idx| bar(idx, 104.0, 100.0, 100.0))
        .collect::<Vec<_>>();
    let mut machine = IndependentActiveMachine::new_with_config(None, false, false);
    machine.long.qualified = Some(QualifiedState {
        direction: Qualification::Long,
        qualified_idx: 0,
        qualified_ts: 0,
    });
    machine.long.active = Some(ActiveTransition {
        direction: Direction::Long,
        qualified_ts: 0,
        breakout_ts: 0,
        activated_idx: 0,
        activated_ts: 0,
    });

    for idx in 0..bars.len() {
        machine.step(&bars, idx);
    }

    assert!(machine.long.qualified.is_some());
    assert!(machine.long.active.is_some());
    assert!(machine.long.retest_arm.is_some());
}

#[test]
fn v6_keeps_an_armed_long_retest_through_an_interim_ema576_recross() {
    let mut bars = vec![bar(0, 104.0, 98.0, 100.0)];
    let mut interim_recross = bar(1, 99.5, 98.0, 100.0);
    interim_recross.low = 99.2;
    bars.push(interim_recross);
    let mut touch = bar(2, 99.0, 98.0, 100.0);
    touch.low = 98.5;
    bars.push(touch);

    let mut machine = IndependentActiveMachine::new_with_config(None, false, false);
    machine.long.qualified = Some(QualifiedState {
        direction: Qualification::Long,
        qualified_idx: 0,
        qualified_ts: 0,
    });
    machine.long.active = Some(ActiveTransition {
        direction: Direction::Long,
        qualified_ts: 0,
        breakout_ts: 0,
        activated_idx: 0,
        activated_ts: 0,
    });

    machine.step(&bars, 0);
    assert!(machine.long.retest_arm.is_some());
    let interim_step = machine.step(&bars, 1);
    assert!(interim_step.candidates.is_empty());
    assert!(machine.long.retest_arm.is_some());

    let touch_step = machine.step(&bars, 2);
    assert_eq!(touch_step.candidates.len(), 1);
    assert_eq!(touch_step.candidates[0].direction, Direction::Long);
    assert!(machine.long.retest_arm.is_none());
}

#[test]
fn v6_fails_closed_and_rebuilds_after_a_required_indicator_gap() {
    let mut machine = IndependentActiveMachine::new_with_config(None, false, false);
    machine.long.qualified = Some(QualifiedState {
        direction: Qualification::Long,
        qualified_idx: 0,
        qualified_ts: 0,
    });
    machine.long.active = Some(ActiveTransition {
        direction: Direction::Long,
        qualified_ts: 0,
        breakout_ts: 0,
        activated_idx: 0,
        activated_ts: 0,
    });
    let bars = vec![PatternBar {
        ts: 0,
        high: 101.0,
        low: 99.0,
        close: 100.0,
        ema144: None,
        ema576: Some(100.0),
        atr14: Some(2.0),
    }];

    machine.step(&bars, 0);

    assert!(machine.long.qualified.is_none());
    assert!(machine.long.active.is_none());
    assert!(machine.long.retest_arm.is_none());
}

#[test]
fn v8_preserves_an_armed_order_after_episode_close_then_requires_a_fresh_breakout() {
    let mut bars = vec![bar(0, 104.0, 90.0, 100.0)];
    let mut first_below = bar(1, 99.0, 90.0, 100.0);
    first_below.low = 98.5;
    bars.push(first_below);
    let mut second_below = bar(2, 98.5, 90.0, 100.0);
    second_below.low = 98.0;
    bars.push(second_below);
    let mut touch = bar(3, 91.0, 90.0, 100.0);
    touch.low = 90.5;
    bars.push(touch);
    bars.push(bar(4, 104.0, 90.0, 100.0));
    bars.push(bar(5, 104.0, 90.0, 100.0));

    let mut machine = IndependentActiveMachine::new_with_config(None, false, true);
    machine.long.qualified = Some(QualifiedState {
        direction: Qualification::Long,
        qualified_idx: 0,
        qualified_ts: 0,
    });
    machine.long.active = Some(ActiveTransition {
        direction: Direction::Long,
        qualified_ts: 0,
        breakout_ts: 0,
        activated_idx: 0,
        activated_ts: 0,
    });
    machine.long.episode_open = true;

    machine.step(&bars, 0);
    assert!(machine.long.retest_arm.is_some());
    machine.step(&bars, 1);
    machine.step(&bars, 2);
    assert!(!machine.long.episode_open);
    assert!(machine.long.active.is_some());
    assert!(machine.long.retest_arm.is_some());

    let touch_step = machine.step(&bars, 3);
    assert_eq!(touch_step.candidates.len(), 1);
    assert!(machine.long.active.is_none());
    assert!(machine.long.retest_arm.is_none());

    machine.step(&bars, 4);
    assert!(machine.long.active.is_none());
    machine.step(&bars, 5);
    assert!(machine.long.episode_open);
    assert_eq!(
        machine
            .long
            .active
            .expect("fresh long episode")
            .activated_idx,
        5
    );
    assert!(machine.long.retest_arm.is_some());
}
