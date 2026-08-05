use super::{
    breakout_at, DirectionMachine, PatternBar, TargetBarTrace, TargetTrace, EVALUATION_END_MS,
    MS_15M, RETEST_WINDOW_BARS, TARGETS,
};

/// 重放每张目标图至窗口终点，逐根记录因果状态而不读取窗口后的行情。
pub(super) fn build_target_traces(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
) -> Vec<TargetTrace> {
    TARGETS
        .iter()
        .filter(|target| target.symbol == symbol)
        .map(|target| {
            let mut machine = DirectionMachine::new(target.direction);
            let mut opposite_relation_age = 0usize;
            let mut traced_bars = Vec::new();
            let trace_start_ms = target
                .start_ms
                .saturating_sub(RETEST_WINDOW_BARS as i64 * MS_15M);
            for idx in start_idx..bars.len() {
                let bar = bars[idx];
                if bar.ts > target.end_ms || bar.ts > EVALUATION_END_MS {
                    break;
                }
                let phase_before = machine.phase.label();
                let ready = bar.ready();
                opposite_relation_age =
                    if ready.is_some_and(|bar| !target.direction.regime_holds(bar)) {
                        opposite_relation_age.saturating_add(1)
                    } else {
                        0
                    };
                let breakout_condition = breakout_at(bars, idx, target.direction);
                let step = machine.step(bars, idx);
                if bar.ts < trace_start_ms {
                    continue;
                }
                let mut events = Vec::new();
                if step.armed {
                    events.push("armed");
                }
                if step.breakout {
                    events.push("confirmed_breakout");
                }
                if step.departure {
                    events.push("effective_departure");
                }
                if step.failed_first_retest {
                    events.push("failed_first_retest");
                }
                if step.retest_timeout {
                    events.push("retest_timeout");
                }
                if step.candidate.is_some() {
                    events.push("candidate");
                }
                traced_bars.push(TargetBarTrace {
                    ts_ms: bar.ts,
                    in_target_window: bar.ts >= target.start_ms,
                    phase_before,
                    phase_after: machine.phase.label(),
                    relation_age_bars: machine.relation_age,
                    opposite_relation_age_bars: opposite_relation_age,
                    regime_holds: ready.is_some_and(|bar| target.direction.regime_holds(bar)),
                    breakout_condition,
                    departure_condition: ready.is_some_and(|bar| target.direction.departed(bar)),
                    retest_zone_touched: ready
                        .is_some_and(|bar| target.direction.touches_retest_zone(bar)),
                    retest_holds: ready.is_some_and(|bar| target.direction.holds_retest(bar)),
                    close: ready.map(|bar| bar.close),
                    ema144: ready.map(|bar| bar.ema144),
                    ema576: ready.map(|bar| bar.ema576),
                    atr14: ready.map(|bar| bar.atr14),
                    retest_extreme_to_ema144_atr: ready
                        .map(|bar| target.direction.retest_extreme_atr(bar)),
                    events,
                });
            }
            TargetTrace {
                name: target.name,
                symbol: target.symbol,
                direction: target.direction.label(),
                bars: traced_bars,
            }
        })
        .collect()
}
