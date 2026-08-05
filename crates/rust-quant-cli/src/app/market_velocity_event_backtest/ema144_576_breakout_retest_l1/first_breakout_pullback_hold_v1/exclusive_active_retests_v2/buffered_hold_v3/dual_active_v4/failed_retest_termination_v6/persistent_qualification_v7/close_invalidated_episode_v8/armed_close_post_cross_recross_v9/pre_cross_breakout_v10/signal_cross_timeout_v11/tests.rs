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
            * super::super::super::super::super::super::super::super::super::super::super::MS_15M,
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

fn activate_long(machine: &mut V9Machine, bars: &mut Vec<PatternBar>) {
    for _ in 0..REGIME_WINDOW_BARS {
        step_with_bar(machine, bars, 99.2, 98.8, 99.0, 98.0, 100.0);
    }
    step_with_bar(machine, bars, 100.7, 100.3, 100.5, 98.5, 100.0);
    let activation = step_with_bar(machine, bars, 101.2, 100.8, 101.0, 99.0, 100.0);
    assert!(activation.episode_started.is_some());
}

fn activate_short(machine: &mut V9Machine, bars: &mut Vec<PatternBar>) {
    for _ in 0..REGIME_WINDOW_BARS {
        step_with_bar(machine, bars, 101.2, 100.8, 101.0, 102.0, 100.0);
    }
    step_with_bar(machine, bars, 99.7, 99.3, 99.5, 101.5, 100.0);
    let activation = step_with_bar(machine, bars, 99.2, 98.8, 99.0, 101.0, 100.0);
    assert!(activation.episode_started.is_some());
}

fn first_long_signal(machine: &mut V9Machine, bars: &mut Vec<PatternBar>) -> usize {
    let signal_idx = bars.len();
    let signal = step_with_bar(machine, bars, 100.0, 98.8, 99.2, 99.0, 100.0);
    assert_eq!(signal.candidates.len(), 1);
    assert_eq!(signal.signal_cross_deadline_starts.len(), 1);
    signal_idx
}

#[test]
fn later_pre_cross_signal_does_not_reset_first_signal_deadline() {
    let mut machine = V9Machine::with_lifecycle_policy(true, Some(SIGNAL_CROSS_TIMEOUT_BARS));
    let mut bars = Vec::new();
    activate_long(&mut machine, &mut bars);
    let first_signal_idx = first_long_signal(&mut machine, &mut bars);

    step_with_bar(&mut machine, &mut bars, 102.0, 101.0, 101.5, 99.0, 100.0);
    let second_signal = step_with_bar(&mut machine, &mut bars, 100.0, 98.8, 99.2, 99.0, 100.0);
    assert_eq!(second_signal.candidates.len(), 1);
    assert!(second_signal.signal_cross_deadline_starts.is_empty());
    assert_eq!(
        machine
            .long_signal_cross_deadline
            .expect("long deadline should remain")
            .first_signal_idx,
        first_signal_idx
    );

    while bars.len() <= first_signal_idx + SIGNAL_CROSS_TIMEOUT_BARS - 1 {
        step_with_bar(&mut machine, &mut bars, 102.0, 101.0, 101.5, 99.0, 100.0);
    }
    let timeout = step_with_bar(&mut machine, &mut bars, 102.0, 101.0, 101.5, 99.0, 100.0);
    assert_eq!(timeout.signal_cross_deadline_timeouts.len(), 1);
    assert_eq!(
        timeout.signal_cross_deadline_timeouts[0]
            .deadline
            .first_signal_idx,
        first_signal_idx
    );
    assert!(machine.long_active.is_none());
    assert!(machine.long_qualification.latched.is_none());
}

#[test]
fn directional_cross_on_deadline_bar_confirms_before_timeout() {
    let mut machine = V9Machine::with_lifecycle_policy(true, Some(SIGNAL_CROSS_TIMEOUT_BARS));
    let mut bars = Vec::new();
    activate_long(&mut machine, &mut bars);
    let first_signal_idx = first_long_signal(&mut machine, &mut bars);

    while bars.len() <= first_signal_idx + SIGNAL_CROSS_TIMEOUT_BARS - 1 {
        step_with_bar(&mut machine, &mut bars, 102.0, 101.0, 101.5, 99.0, 100.0);
    }
    let deadline_cross = step_with_bar(&mut machine, &mut bars, 102.0, 101.0, 101.5, 101.0, 100.0);
    assert_eq!(deadline_cross.signal_cross_deadline_confirmations.len(), 1);
    assert!(deadline_cross.signal_cross_deadline_timeouts.is_empty());
    assert!(machine.long_signal_cross_deadline.is_none());
    assert!(machine
        .long_active
        .is_some_and(|active| active.post_cross_seen));
}

#[test]
fn short_timeout_is_mirrored_and_consumes_old_qualification() {
    let mut machine = V9Machine::with_lifecycle_policy(true, Some(SIGNAL_CROSS_TIMEOUT_BARS));
    let mut bars = Vec::new();
    activate_short(&mut machine, &mut bars);
    let signal_idx = bars.len();
    let signal = step_with_bar(&mut machine, &mut bars, 101.2, 99.0, 100.8, 101.0, 100.0);
    assert_eq!(signal.candidates.len(), 1);
    assert_eq!(signal.signal_cross_deadline_starts.len(), 1);

    while bars.len() <= signal_idx + SIGNAL_CROSS_TIMEOUT_BARS - 1 {
        step_with_bar(&mut machine, &mut bars, 99.0, 98.0, 98.5, 101.0, 100.0);
    }
    let timeout = step_with_bar(&mut machine, &mut bars, 99.0, 98.0, 98.5, 101.0, 100.0);
    assert_eq!(timeout.signal_cross_deadline_timeouts.len(), 1);
    assert_eq!(
        timeout.signal_cross_deadline_timeouts[0]
            .deadline
            .origin_active
            .core
            .direction,
        Direction::Short
    );
    assert!(machine.short_active.is_none());
    assert!(machine.short_qualification.latched.is_none());
}
