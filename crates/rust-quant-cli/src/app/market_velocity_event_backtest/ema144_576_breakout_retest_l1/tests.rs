use super::*;
use crate::app::market_velocity_event_backtest::BacktestCandle;

fn pattern_bar(idx: usize, close: f64, ema144: f64, ema576: f64) -> PatternBar {
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

fn armed_long_bars() -> Vec<PatternBar> {
    (0..REQUIRED_REGIME_BARS)
        .map(|idx| pattern_bar(idx, 99.0, 98.0, 100.0))
        .collect()
}

fn push_long_breakout_and_impulse(bars: &mut Vec<PatternBar>) -> (usize, usize) {
    let first = bars.len();
    bars.push(pattern_bar(first, 100.5, 98.5, 100.0));
    let confirmation = bars.len();
    bars.push(pattern_bar(confirmation, 101.5, 99.0, 100.0));
    let impulse = bars.len();
    bars.push(pattern_bar(impulse, 102.0, 99.5, 100.0));
    (confirmation, impulse)
}

fn scan(direction: Direction, bars: &[PatternBar]) -> Vec<CandidateCore> {
    let mut machine = DirectionMachine::new(direction);
    let mut candidates = Vec::new();
    for idx in 0..bars.len() {
        if let Some(candidate) = machine.step(bars, idx).candidate {
            candidates.push(candidate);
        }
    }
    candidates
}

#[test]
fn ema_uses_full_window_sma_seed_then_recursive_update() {
    let candles = (1..=577)
        .map(|value| ComputedCandle {
            candle: BacktestCandle {
                ts: value as i64 * MS_15M,
                open: value as f64,
                high: value as f64,
                low: value as f64,
                close: value as f64,
                volume: 1.0,
            },
            volume_ccy: None,
            sma: None,
            ema: None,
            ema12: None,
            ema144: None,
            ema169: None,
            ema696: None,
            previous_volume_avg: None,
            previous_range_avg: None,
            rsi14: None,
            atr14: None,
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: None,
            macd_signal_line: None,
            macd_histogram: None,
        })
        .collect::<Vec<_>>();
    let ema = ema_close_series(&candles, EMA_SLOW_PERIOD);
    let seed = (1.0 + 576.0) / 2.0;
    let expected_next = (577.0 - seed) * (2.0 / 577.0) + seed;

    assert!(ema[..575].iter().all(Option::is_none));
    assert_eq!(ema[575], Some(seed));
    assert!((ema[576].expect("recursive EMA") - expected_next).abs() < 1e-12);
}

#[test]
fn exactly_143_regime_bars_do_not_arm() {
    let mut bars = armed_long_bars();
    bars.remove(0);
    let first = bars.len();
    bars.push(pattern_bar(first, 100.5, 100.5, 100.0));
    let confirmation = bars.len();
    bars.push(pattern_bar(confirmation, 101.5, 101.0, 100.0));
    let impulse = bars.len();
    bars.push(pattern_bar(impulse, 102.0, 101.5, 100.0));
    let idx = bars.len();
    let mut retest = pattern_bar(idx, 101.5, 101.5, 100.0);
    retest.low = 101.0;
    bars.push(retest);

    assert!(scan(Direction::Long, &bars).is_empty());
}

#[test]
fn long_retest_before_ema_cross_is_valid() {
    let mut bars = armed_long_bars();
    push_long_breakout_and_impulse(&mut bars);
    let idx = bars.len();
    let mut retest = pattern_bar(idx, 100.0, 100.0, 100.5);
    retest.low = 99.5;
    bars.push(retest);

    let candidates = scan(Direction::Long, &bars);
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        Direction::Long.cross_phase(candidates[0].signal_bar),
        "pre_cross_retest"
    );
}

#[test]
fn long_retest_after_ema_cross_is_also_valid() {
    let mut bars = armed_long_bars();
    push_long_breakout_and_impulse(&mut bars);
    let idx = bars.len();
    let mut retest = pattern_bar(idx, 101.0, 100.5, 100.0);
    retest.low = 100.0;
    bars.push(retest);

    let candidates = scan(Direction::Long, &bars);
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        Direction::Long.cross_phase(candidates[0].signal_bar),
        "post_cross_retest"
    );
}

#[test]
fn short_is_a_true_mirror_of_long() {
    let mut bars = (0..REQUIRED_REGIME_BARS)
        .map(|idx| pattern_bar(idx, 101.0, 102.0, 100.0))
        .collect::<Vec<_>>();
    let first = bars.len();
    bars.push(pattern_bar(first, 99.5, 101.5, 100.0));
    let confirmation = bars.len();
    bars.push(pattern_bar(confirmation, 98.5, 101.0, 100.0));
    let impulse = bars.len();
    bars.push(pattern_bar(impulse, 98.0, 100.5, 100.0));
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 100.0, 100.0, 99.5);
    retest.high = 100.5;
    bars.push(retest);

    let candidates = scan(Direction::Short, &bars);
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        Direction::Short.cross_phase(candidates[0].signal_bar),
        "pre_cross_retest"
    );
}

#[test]
fn failed_first_retest_consumes_episode() {
    let mut bars = armed_long_bars();
    push_long_breakout_and_impulse(&mut bars);
    let failed_idx = bars.len();
    let mut failed = pattern_bar(failed_idx, 98.5, 100.0, 100.5);
    failed.low = 98.0;
    bars.push(failed);
    let second_idx = bars.len();
    let mut second = pattern_bar(second_idx, 100.0, 100.0, 100.5);
    second.low = 99.5;
    bars.push(second);

    assert!(scan(Direction::Long, &bars).is_empty());
}

#[test]
fn frozen_boundaries_are_inclusive() {
    let mut bars = armed_long_bars();
    let first = bars.len();
    bars.push(pattern_bar(first, 100.1, 98.5, 100.0));
    let confirmation = bars.len();
    bars.push(pattern_bar(confirmation, 100.2, 99.0, 100.0));
    let impulse_idx = bars.len();
    let mut impulse = pattern_bar(impulse_idx, 101.5, 99.5, 100.0);
    impulse.atr14 = Some(2.0);
    bars.push(impulse);
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 100.0, 100.0, 100.5);
    retest.low = 99.0;
    retest.atr14 = Some(2.0);
    bars.push(retest);

    assert_eq!(scan(Direction::Long, &bars).len(), 1);
}

#[test]
fn retest_on_the_96th_following_bar_is_still_valid() {
    let mut bars = armed_long_bars();
    let first = bars.len();
    bars.push(pattern_bar(first, 100.5, 98.5, 100.0));
    let confirmation = bars.len();
    bars.push(pattern_bar(confirmation, 101.5, 99.0, 100.0));
    for _ in 1..RETEST_WINDOW_BARS {
        let idx = bars.len();
        bars.push(pattern_bar(idx, 103.0, 100.0, 100.5));
    }
    let retest_idx = bars.len();
    let mut retest = pattern_bar(retest_idx, 100.0, 100.0, 100.5);
    retest.low = 99.5;
    bars.push(retest);

    let candidates = scan(Direction::Long, &bars);
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].signal_idx - candidates[0].impulse_idx,
        RETEST_WINDOW_BARS
    );
}

#[test]
fn decision_never_claims_outcome_evaluation() {
    let summary = L1Summary {
        candidate_count: 10,
        by_direction: BTreeMap::from([("long", 8), ("short", 2)]),
        by_cross_phase: BTreeMap::new(),
        by_symbol: (0..4).map(|idx| (idx.to_string(), 1)).collect(),
        by_month_utc: (0..3).map(|idx| (idx.to_string(), 1)).collect(),
        effective_market_events: 5,
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
            matched_signal_timestamps_ms: vec![target.start_ms],
            matched: true,
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
