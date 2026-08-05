/// 首次进入盈利观察所需的峰值收益。
pub(crate) const OBSERVATION_TRIGGER_R: f64 = 0.5;
/// 未达到 1R 前，只有后续完成 K 线收盘严格跌破该值才退出。
pub(crate) const PRE_ONE_EXIT_CLOSE_R: f64 = 0.25;
/// 达到该峰值后切换为真实目标完成比例保护。
pub(crate) const POST_ONE_TRIGGER_R: f64 = 1.0;
/// 目标完成比例先扣除该缓冲，再决定需要保留多少峰值利润。
pub(crate) const TARGET_COMPLETION_OFFSET: f64 = 0.25;
/// 达到 1R 后的最低保护收益。
pub(crate) const MINIMUM_LOCK_R: f64 = 0.25;

/// 盈利观察状态；只由已完成 K 线推进，不读取未来价格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfitObservationPhase {
    Waiting,
    HalfObserved,
    PostOne,
}

/// 单笔仓位的峰值收益和单调保护位置。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProfitObservationState {
    pub(crate) phase: ProfitObservationPhase,
    pub(crate) observed_at_ts: Option<i64>,
    pub(crate) peak_r: f64,
    pub(crate) active_lock_r: Option<f64>,
}

impl Default for ProfitObservationState {
    fn default() -> Self {
        Self {
            phase: ProfitObservationPhase::Waiting,
            observed_at_ts: None,
            peak_r: 0.0,
            active_lock_r: None,
        }
    }
}

/// 状态机在当前完成 K 线作出的唯一动作。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ProfitObservationDecision {
    None,
    ExitAtClose(ProfitObservationEvidence),
    UpdateLock(ProfitObservationEvidence),
}

/// 随退出或止损更新保存的目标完成比例证据。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProfitObservationEvidence {
    pub(crate) phase: ProfitObservationPhase,
    pub(crate) observed_at_ts: Option<i64>,
    pub(crate) peak_r: f64,
    pub(crate) true_target_r: f64,
    pub(crate) target_completion: f64,
    pub(crate) active_lock_r: Option<f64>,
    pub(crate) close_r: f64,
    pub(crate) decision_ts: i64,
    pub(crate) exit_reason: &'static str,
}

impl ProfitObservationState {
    /// 用当前完成 K 线更新峰值；首次观察 K 不触发 1R 前的回落退出。
    pub(crate) fn observe_completed_candle(
        &mut self,
        ts: i64,
        favorable_r: f64,
        close_r: f64,
        true_target_r: f64,
    ) -> ProfitObservationDecision {
        if !favorable_r.is_finite()
            || !close_r.is_finite()
            || !true_target_r.is_finite()
            || true_target_r <= POST_ONE_TRIGGER_R
        {
            return ProfitObservationDecision::None;
        }
        self.peak_r = self.peak_r.max(favorable_r);
        if self.phase == ProfitObservationPhase::Waiting && self.peak_r >= OBSERVATION_TRIGGER_R {
            self.phase = ProfitObservationPhase::HalfObserved;
            self.observed_at_ts = Some(ts);
        }
        if self.phase == ProfitObservationPhase::HalfObserved && self.peak_r >= POST_ONE_TRIGGER_R {
            self.phase = ProfitObservationPhase::PostOne;
        }
        if self.phase == ProfitObservationPhase::HalfObserved
            && self.observed_at_ts.is_some_and(|observed| ts > observed)
            && close_r < PRE_ONE_EXIT_CLOSE_R
        {
            return ProfitObservationDecision::ExitAtClose(self.evidence(
                ts,
                close_r,
                true_target_r,
                "profit_observation_pre_one_close_below_0_25r",
            ));
        }
        if self.phase != ProfitObservationPhase::PostOne {
            return ProfitObservationDecision::None;
        }

        let completion = (self.peak_r / true_target_r).clamp(0.0, 1.0);
        let candidate_lock =
            (self.peak_r * (completion - TARGET_COMPLETION_OFFSET).max(0.0)).max(MINIMUM_LOCK_R);
        if self
            .active_lock_r
            .is_some_and(|active| candidate_lock <= active)
        {
            return ProfitObservationDecision::None;
        }
        self.active_lock_r = Some(candidate_lock);
        ProfitObservationDecision::UpdateLock(self.evidence(
            ts,
            close_r,
            true_target_r,
            "profit_observation_target_completion_lock",
        ))
    }

    fn evidence(
        &self,
        ts: i64,
        close_r: f64,
        true_target_r: f64,
        exit_reason: &'static str,
    ) -> ProfitObservationEvidence {
        ProfitObservationEvidence {
            phase: self.phase,
            observed_at_ts: self.observed_at_ts,
            peak_r: self.peak_r,
            true_target_r,
            target_completion: (self.peak_r / true_target_r).clamp(0.0, 1.0),
            active_lock_r: self.active_lock_r,
            close_r,
            decision_ts: ts,
            exit_reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_r_only_observes_and_requires_a_later_close_below_quarter_r() {
        let mut state = ProfitObservationState::default();
        assert_eq!(
            state.observe_completed_candle(1, 0.5, 0.2, 2.4),
            ProfitObservationDecision::None
        );
        assert_eq!(state.phase, ProfitObservationPhase::HalfObserved);
        assert_eq!(
            state.observe_completed_candle(2, 0.6, 0.25, 2.4),
            ProfitObservationDecision::None
        );
        assert!(matches!(
            state.observe_completed_candle(3, 0.6, 0.249, 2.4),
            ProfitObservationDecision::ExitAtClose(_)
        ));
    }

    #[test]
    fn one_r_uses_target_completion_and_lock_never_moves_down() {
        let mut state = ProfitObservationState::default();
        let first = state.observe_completed_candle(1, 1.2, 1.0, 2.4);
        let ProfitObservationDecision::UpdateLock(first) = first else {
            panic!("1.2R should arm target-completion protection");
        };
        assert!((first.active_lock_r.unwrap() - 0.3).abs() < 1e-12);
        assert_eq!(
            state.observe_completed_candle(2, 1.1, 1.0, 2.4),
            ProfitObservationDecision::None
        );
        let next = state.observe_completed_candle(3, 1.8, 1.5, 2.4);
        let ProfitObservationDecision::UpdateLock(next) = next else {
            panic!("new peak should raise the lock");
        };
        assert!((next.active_lock_r.unwrap() - 0.9).abs() < 1e-12);
    }
}
