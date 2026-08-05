use super::profit_observation::{
    ProfitObservationDecision, ProfitObservationEvidence, ProfitObservationPhase,
};
use super::*;

const TARGET_COMPLETION_STOP_SOURCE: &str = "MarketVelocityTargetCompletionProfitObservation";

impl MarketVelocityReplayStrategy {
    /// 在完成 K 线收盘时推进研究状态；生成的新保护位只能由下一根 K 线触发。
    pub(super) fn maybe_build_profit_observation_signal(
        &mut self,
        candle: &CandleItem,
    ) -> Option<SignalResult> {
        if !self.target_completion_profit_observation {
            return None;
        }
        let active = self.active_position.as_mut()?;
        // 入场棒的高低点在开盘成交时尚不可见，不能反过来决定该棒内已经存在保护止损。
        if candle.ts <= active.entry_ts || active.stop_loss_pct <= 0.0 {
            return None;
        }
        let favorable_price = match active.direction {
            MarketVelocityTradeDirection::Long => candle.h,
            MarketVelocityTradeDirection::Short => candle.l,
            MarketVelocityTradeDirection::Both => return None,
        };
        let favorable_r = r_for_price(
            active.entry_price,
            active.stop_loss_pct,
            favorable_price,
            active.direction,
        );
        let close_r = r_for_price(
            active.entry_price,
            active.stop_loss_pct,
            candle.c,
            active.direction,
        );
        let decision = active
            .profit_observation
            .as_mut()?
            .observe_completed_candle(candle.ts, favorable_r, close_r, active.target_r);
        match decision {
            ProfitObservationDecision::None => None,
            ProfitObservationDecision::ExitAtClose(evidence) => {
                Some(self.build_profit_observation_exit_signal(candle, evidence))
            }
            ProfitObservationDecision::UpdateLock(evidence) => {
                self.build_profit_observation_lock_signal(candle, evidence)
            }
        }
    }

    fn build_profit_observation_exit_signal(
        &mut self,
        candle: &CandleItem,
        evidence: ProfitObservationEvidence,
    ) -> SignalResult {
        let active = self
            .active_position
            .take()
            .expect("profit observation exit requires an active position");
        SignalResult {
            should_buy: active.direction == MarketVelocityTradeDirection::Short,
            should_sell: active.direction == MarketVelocityTradeDirection::Long,
            open_price: candle.c,
            ts: candle.ts,
            single_value: Some(profit_observation_signal_value(
                active.event_id,
                &active.trigger,
                evidence,
                evidence.exit_reason,
            )),
            single_result: Some("market_velocity_framework_replay".to_string()),
            filter_reasons: vec![exit_only_filter_reason(active.direction).to_string()],
            direction: opposite_signal_direction(active.direction),
            ..SignalResult::default()
        }
    }

    fn build_profit_observation_lock_signal(
        &mut self,
        candle: &CandleItem,
        evidence: ProfitObservationEvidence,
    ) -> Option<SignalResult> {
        let active = self.active_position.as_mut()?;
        let lock_r = evidence.active_lock_r?;
        let protected_stop_price = target_price_for(
            active.entry_price,
            active.stop_loss_pct,
            lock_r,
            active.direction,
        );
        if stop_already_crossed(candle.c, protected_stop_price, active.direction) {
            return Some(self.build_profit_observation_exit_signal(
                candle,
                ProfitObservationEvidence {
                    exit_reason: "profit_observation_close_crossed_new_lock",
                    ..evidence
                },
            ));
        }
        let entry_price = active.entry_price;
        let event_id = active.event_id;
        let trigger = active.trigger.clone();
        let direction = active.direction;
        let stop_loss_pct = active.stop_loss_pct;
        let target_r = active.target_r;
        active.stop_loss_price = protected_stop_price;
        active.stop_loss_source = TARGET_COMPLETION_STOP_SOURCE.to_string();
        active.profit_protected = true;

        let mut signal = self.build_entry_direction_signal(
            candle.ts,
            entry_price,
            protected_stop_price,
            stop_loss_pct,
            TARGET_COMPLETION_STOP_SOURCE,
            event_id,
            &trigger,
            direction,
            target_r,
            true,
        );
        signal.open_price = candle.c;
        signal.single_value = Some(profit_observation_signal_value(
            event_id,
            &trigger,
            evidence,
            evidence.exit_reason,
        ));
        Some(signal)
    }
}

fn phase_label(phase: ProfitObservationPhase) -> &'static str {
    match phase {
        ProfitObservationPhase::Waiting => "waiting",
        ProfitObservationPhase::HalfObserved => "half_observed",
        ProfitObservationPhase::PostOne => "post_one",
    }
}

fn profit_observation_signal_value(
    event_id: i64,
    trigger: &str,
    evidence: ProfitObservationEvidence,
    exit_reason: &str,
) -> String {
    json!({
        "source": "market_velocity_framework_replay",
        "rank_event_id": event_id,
        "entry_trigger": trigger,
        "exit_reason": exit_reason,
        "profit_observation": {
            "phase": phase_label(evidence.phase),
            "observed_at_ts": evidence.observed_at_ts,
            "peak_r": evidence.peak_r,
            "true_target_r": evidence.true_target_r,
            "target_completion": evidence.target_completion,
            "active_lock_r": evidence.active_lock_r,
            "close_r": evidence.close_r,
            "decision_ts": evidence.decision_ts,
        }
    })
    .to_string()
}

fn exit_only_filter_reason(direction: MarketVelocityTradeDirection) -> &'static str {
    match direction {
        MarketVelocityTradeDirection::Long => "FIB_STRICT_MAJOR_BULL_BLOCK_SHORT",
        MarketVelocityTradeDirection::Short => "FIB_STRICT_MAJOR_BEAR_BLOCK_LONG",
        MarketVelocityTradeDirection::Both => "MARKET_VELOCITY_PROFIT_OBSERVATION_EXIT",
    }
}

fn opposite_signal_direction(direction: MarketVelocityTradeDirection) -> SignalDirection {
    match direction {
        MarketVelocityTradeDirection::Long => SignalDirection::Short,
        MarketVelocityTradeDirection::Short => SignalDirection::Long,
        MarketVelocityTradeDirection::Both => SignalDirection::None,
    }
}
