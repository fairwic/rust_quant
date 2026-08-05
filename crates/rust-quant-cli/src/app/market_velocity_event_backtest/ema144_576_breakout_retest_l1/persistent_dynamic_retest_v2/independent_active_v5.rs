use super::super::{breakout_at, Direction, PatternBar, ReadyBar};
use super::{
    candidate_from_core, departed_from_slow, reexpanded_from_fast, ActiveTransition, CandidateCore,
    PendingTransition, Qualification, QualifiedState, RetestArm, V2Candidate, V2StageCounts,
    EVALUATION_END_MS, EVALUATION_START_MS, REQUIRED_QUALIFICATION_BARS, RETEST_TOUCH_BUFFER_ATR,
    TRANSITION_WINDOW_BARS,
};
use anyhow::Result;

#[derive(Debug, Default)]
struct DirectionalState {
    qualified: Option<QualifiedState>,
    pending: Option<PendingTransition>,
    active: Option<ActiveTransition>,
    episode_open: bool,
    retest_arm: Option<RetestArm>,
}

#[derive(Debug, Default)]
struct IndependentStep {
    qualification_changes: usize,
    transition_breakouts: usize,
    active_transitions: usize,
    retest_arms: usize,
    candidates: Vec<CandidateCore>,
}

#[derive(Debug)]
pub(super) struct IndependentActiveMachine {
    qualification_max_age_bars: Option<usize>,
    clear_arm_off_price_side: bool,
    finite_price_episode: bool,
    long_age: usize,
    short_age: usize,
    long: DirectionalState,
    short: DirectionalState,
}

impl IndependentActiveMachine {
    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::new_with_config(Some(super::QUALIFICATION_MEMORY_BARS), true, false)
    }

    /// V5/V6 共用同一状态机；配置只控制资格过期和已武装订单的取消语义。
    pub(super) fn new_with_config(
        qualification_max_age_bars: Option<usize>,
        clear_arm_off_price_side: bool,
        finite_price_episode: bool,
    ) -> Self {
        Self {
            qualification_max_age_bars,
            clear_arm_off_price_side,
            finite_price_episode,
            long_age: 0,
            short_age: 0,
            long: DirectionalState::default(),
            short: DirectionalState::default(),
        }
    }

    fn step(&mut self, bars: &[PatternBar], idx: usize) -> IndependentStep {
        let Some(bar) = bars.get(idx).copied().and_then(PatternBar::ready) else {
            self.long_age = 0;
            self.short_age = 0;
            self.long = DirectionalState::default();
            self.short = DirectionalState::default();
            return IndependentStep::default();
        };
        let mut step = IndependentStep::default();

        // 先处理上一根收盘后已经可预置的订单，避免用当前收盘取消同一根内的触碰。
        for direction in [Direction::Long, Direction::Short] {
            if let Some(candidate) = self.try_touch(direction, bars, idx, bar) {
                step.candidates.push(candidate);
            }
        }
        self.update_qualifications(idx, bar, &mut step);
        for direction in [Direction::Long, Direction::Short] {
            self.expire_direction(direction, idx);
            self.close_price_episode_on_opposite_breakout(direction, bars, idx);
            self.update_transition(direction, bars, idx, bar, &mut step);
            self.update_retest_arm(direction, idx, bar, &mut step);
        }
        step
    }

    fn update_qualifications(&mut self, idx: usize, bar: ReadyBar, step: &mut IndependentStep) {
        if bar.ema144 < bar.ema576 {
            self.long_age = self.long_age.saturating_add(1);
            self.short_age = 0;
            if self.long_age >= REQUIRED_QUALIFICATION_BARS {
                self.refresh_qualification(Direction::Long, idx, bar.ts);
                step.qualification_changes +=
                    usize::from(self.long_age == REQUIRED_QUALIFICATION_BARS);
            }
        } else if bar.ema144 > bar.ema576 {
            self.short_age = self.short_age.saturating_add(1);
            self.long_age = 0;
            if self.short_age >= REQUIRED_QUALIFICATION_BARS {
                self.refresh_qualification(Direction::Short, idx, bar.ts);
                step.qualification_changes +=
                    usize::from(self.short_age == REQUIRED_QUALIFICATION_BARS);
            }
        } else {
            self.long_age = 0;
            self.short_age = 0;
        }
    }

    fn refresh_qualification(&mut self, direction: Direction, idx: usize, ts: i64) {
        let qualified = QualifiedState {
            direction: match direction {
                Direction::Long => Qualification::Long,
                Direction::Short => Qualification::Short,
            },
            qualified_idx: idx,
            qualified_ts: ts,
        };
        let state = self.state_mut(direction);
        state.qualified = Some(qualified);
        if let Some(active) = &mut state.active {
            active.qualified_ts = ts;
        }
        if let Some(pending) = &mut state.pending {
            pending.qualified_ts = ts;
        }
    }

    fn expire_direction(&mut self, direction: Direction, idx: usize) {
        let Some(max_age_bars) = self.qualification_max_age_bars else {
            return;
        };
        let state = self.state_mut(direction);
        let expired = state
            .qualified
            .is_none_or(|qualified| idx.saturating_sub(qualified.qualified_idx) > max_age_bars);
        if expired {
            state.qualified = None;
            state.pending = None;
            state.active = None;
            state.episode_open = false;
            state.retest_arm = None;
        }
    }

    fn try_touch(
        &mut self,
        direction: Direction,
        bars: &[PatternBar],
        idx: usize,
        bar: ReadyBar,
    ) -> Option<CandidateCore> {
        let previous_idx = idx.checked_sub(1)?;
        let anchor = bars.get(previous_idx).copied()?.ready()?;
        let finite_price_episode = self.finite_price_episode;
        let state = self.state_mut(direction);
        let (active, arm) = (state.active?, state.retest_arm?);
        if idx <= arm.armed_idx {
            return None;
        }
        let touched = match direction {
            Direction::Long => bar.low <= anchor.ema144 + RETEST_TOUCH_BUFFER_ATR * anchor.atr14,
            Direction::Short => bar.high >= anchor.ema144 - RETEST_TOUCH_BUFFER_ATR * anchor.atr14,
        };
        if !touched {
            return None;
        }
        state.retest_arm = None;
        if finite_price_episode && !state.episode_open {
            state.active = None;
        }
        Some(CandidateCore {
            direction,
            signal_idx: idx,
            signal_bar: bar,
            active,
            arm,
            anchor,
        })
    }

    fn update_transition(
        &mut self,
        direction: Direction,
        bars: &[PatternBar],
        idx: usize,
        bar: ReadyBar,
        step: &mut IndependentStep,
    ) {
        let finite_price_episode = self.finite_price_episode;
        let same_direction_breakout = breakout_at(bars, idx, direction);
        let state = self.state_mut(direction);
        let Some(qualified) = state.qualified else {
            return;
        };
        if state.active.is_some() {
            if !finite_price_episode || state.episode_open || !same_direction_breakout {
                return;
            }
            // 新的同方向突破代表更新的价格上下文；此时替换 episode 结束后遗留的旧挂单。
            state.active = None;
            state.retest_arm = None;
            state.pending = None;
        }
        if let Some(mut pending) = state.pending {
            pending.qualified_ts = qualified.qualified_ts;
            let elapsed = idx.saturating_sub(pending.breakout_idx);
            if elapsed >= TRANSITION_WINDOW_BARS {
                state.pending = None;
            } else if departed_from_slow(direction, bar) {
                state.active = Some(active_from_pending(pending, idx, bar.ts));
                state.episode_open = true;
                state.pending = None;
                state.retest_arm = None;
                step.active_transitions += 1;
                return;
            } else {
                state.pending = Some(pending);
                return;
            }
        }
        if !same_direction_breakout {
            return;
        }
        let pending = PendingTransition {
            direction,
            breakout_idx: idx,
            breakout_ts: bar.ts,
            qualified_ts: qualified.qualified_ts,
        };
        step.transition_breakouts += 1;
        if departed_from_slow(direction, bar) {
            state.active = Some(active_from_pending(pending, idx, bar.ts));
            state.episode_open = true;
            state.retest_arm = None;
            step.active_transitions += 1;
        } else {
            state.pending = Some(pending);
        }
    }

    fn update_retest_arm(
        &mut self,
        direction: Direction,
        idx: usize,
        bar: ReadyBar,
        step: &mut IndependentStep,
    ) {
        let finite_price_episode = self.finite_price_episode;
        let on_price_side = match direction {
            Direction::Long => bar.close > bar.ema576,
            Direction::Short => bar.close < bar.ema576,
        };
        if !on_price_side {
            if self.clear_arm_off_price_side {
                self.state_mut(direction).retest_arm = None;
            }
            return;
        }
        let should_arm = {
            let state = self.state_mut(direction);
            state.active.is_some()
                && (!finite_price_episode || state.episode_open)
                && state.retest_arm.is_none()
                && reexpanded_from_fast(direction, bar)
        };
        if !should_arm {
            return;
        }

        // V6/V8 只允许最新完成重扩张的方向保留待触发订单，避免同一根 K 同时触发多空。
        if !self.clear_arm_off_price_side {
            self.clear_retest_arm(opposite(direction));
        }
        self.state_mut(direction).retest_arm = Some(RetestArm {
            direction,
            armed_idx: idx,
            armed_ts: bar.ts,
        });
        step.retest_arms += 1;
    }

    /// V8 只结束继续武装的价格 episode；已经存在的订单保留，避免当前收盘反向取消盘中机会。
    fn close_price_episode_on_opposite_breakout(
        &mut self,
        direction: Direction,
        bars: &[PatternBar],
        idx: usize,
    ) {
        if !self.finite_price_episode || !breakout_at(bars, idx, opposite(direction)) {
            return;
        }
        let state = self.state_mut(direction);
        if state.active.is_none() {
            return;
        }
        state.episode_open = false;
        state.pending = None;
        if state.retest_arm.is_none() {
            state.active = None;
        }
    }

    /// 清除被更新上下文替换的挂单；若旧 episode 已结束，同时移除只为该挂单保留的 active 快照。
    fn clear_retest_arm(&mut self, direction: Direction) {
        let finite_price_episode = self.finite_price_episode;
        let state = self.state_mut(direction);
        state.retest_arm = None;
        if finite_price_episode && !state.episode_open {
            state.active = None;
        }
    }

    fn state_mut(&mut self, direction: Direction) -> &mut DirectionalState {
        match direction {
            Direction::Long => &mut self.long,
            Direction::Short => &mut self.short,
        }
    }
}

fn opposite(direction: Direction) -> Direction {
    match direction {
        Direction::Long => Direction::Short,
        Direction::Short => Direction::Long,
    }
}

fn active_from_pending(pending: PendingTransition, idx: usize, ts: i64) -> ActiveTransition {
    ActiveTransition {
        direction: pending.direction,
        qualified_ts: pending.qualified_ts,
        breakout_ts: pending.breakout_ts,
        activated_idx: idx,
        activated_ts: ts,
    }
}

pub(super) fn scan_symbol(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    qualification_max_age_bars: Option<usize>,
    clear_arm_off_price_side: bool,
    finite_price_episode: bool,
    candidates: &mut Vec<V2Candidate>,
    stages: &mut V2StageCounts,
) -> Result<()> {
    let mut machine = IndependentActiveMachine::new_with_config(
        qualification_max_age_bars,
        clear_arm_off_price_side,
        finite_price_episode,
    );
    for idx in start_idx..=end_idx {
        let step = machine.step(bars, idx);
        let ts = bars[idx].ts;
        if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&ts) {
            continue;
        }
        stages.qualification_changes += step.qualification_changes;
        stages.transition_breakouts += step.transition_breakouts;
        stages.active_transitions += step.active_transitions;
        stages.retest_arms += step.retest_arms;
        stages.retest_touches += step.candidates.len();
        for core in step.candidates {
            candidates.push(candidate_from_core(symbol, core)?);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
