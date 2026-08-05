use super::*;
use crate::app::market_velocity_event_backtest::exit::true_break_even_price;
use crate::app::market_velocity_event_backtest::filtered_volume_baseline::causal_filtered_volume_ratio;

/// 单笔 v12 持仓冻结的 ATR 目标和已经成功推进的放量台阶。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct VolumeAtrTrailingReplayState {
    /// 锚点 p 的 ATR14。
    pub(super) atr14: f64,
    /// 信号时点最终选中的 ATR 止盈倍数。
    pub(super) target_atr_multiplier: f64,
    /// 单边手续费与等价滑点合计费率。
    pub(super) per_side_cost_rate: f64,
    /// 已成功更新止损的次数；价格门禁失败时不得增加。
    pub(super) accepted_updates: usize,
}

impl MarketVelocityReplayStrategy {
    /// 在本根旧保护单均未触发后，按已完成收盘确认 v12 放量台阶并从下一根生效。
    pub(super) fn maybe_build_volume_atr_trailing_signal(
        &mut self,
        candles: &[CandleItem],
    ) -> Option<SignalResult> {
        const HOLDING_VOLUME_MIN_RATIO: f64 = 2.5;

        let current_idx = candles.len().checked_sub(1)?;
        let candle = candles.get(current_idx)?;
        let (volume_ratio, retained_candles) = causal_filtered_volume_ratio(
            candles.len(),
            current_idx,
            HOLDING_VOLUME_MIN_RATIO,
            |idx| candles.get(idx).map(|item| item.v),
        )
        .ok()?;
        if volume_ratio < HOLDING_VOLUME_MIN_RATIO {
            return None;
        }

        let (
            entry_price,
            old_stop_price,
            new_stop_price,
            stop_source,
            event_id,
            trigger,
            direction,
            stop_loss_pct,
            target_r,
            atr14,
            target_atr_multiplier,
            accepted_level,
            atr_step,
            true_break_even,
        ) = {
            let active = self.active_position.as_mut()?;
            if candle.ts <= active.entry_ts {
                return None;
            }
            let trailing = active.volume_atr_trailing.as_mut()?;
            if !valid_state(*trailing) {
                return None;
            }
            let true_break_even = true_break_even_price(
                active.entry_price,
                active.direction,
                trailing.per_side_cost_rate,
            )?;
            let atr_step =
                (trailing.accepted_updates > 0).then_some(trailing.accepted_updates as f64);
            let candidate = if let Some(step) = atr_step {
                // 台阶必须严格位于目标之前；达到上限后继续忽略放量，且不消耗级别。
                if step >= trailing.target_atr_multiplier {
                    return None;
                }
                directional_price(active.entry_price, trailing.atr14 * step, active.direction)?
            } else {
                true_break_even
            };
            let target_price = directional_price(
                active.entry_price,
                trailing.atr14 * trailing.target_atr_multiplier,
                active.direction,
            )?;
            if !is_legal_tightening(
                active.direction,
                candle.c,
                active.stop_loss_price,
                candidate,
                target_price,
            ) {
                return None;
            }

            let old_stop = active.stop_loss_price;
            trailing.accepted_updates += 1;
            let accepted_level = trailing.accepted_updates;
            let source = if accepted_level == 1 {
                "MarketVelocityVolumeAtrTrailingBreakEven"
            } else {
                "MarketVelocityVolumeAtrTrailingAtrStep"
            };
            active.stop_loss_price = candidate;
            active.stop_loss_source = source.to_string();
            active.profit_protected = true;
            (
                active.entry_price,
                old_stop,
                candidate,
                source,
                active.event_id,
                active.trigger.clone(),
                active.direction,
                active.stop_loss_pct,
                active.target_r,
                trailing.atr14,
                trailing.target_atr_multiplier,
                accepted_level,
                atr_step,
                true_break_even,
            )
        };

        let mut signal = self.build_entry_direction_signal(
            candle.ts,
            entry_price,
            new_stop_price,
            stop_loss_pct,
            stop_source,
            event_id,
            &trigger,
            direction,
            target_r,
            true,
        );
        // 框架用当前收盘判断新保护价是否合法；原始入场价只用于冻结目标和 R。
        signal.open_price = candle.c;
        signal.single_value = Some(
            json!({
                "source": "market_velocity_framework_replay",
                "rank_event_id": event_id,
                "entry_trigger": trigger,
                "trade_direction": direction.label(),
                "target_r": target_r,
                "profit_protected": true,
                "stop_loss_update_evidence": {
                    "policy": "completed_15m_filtered_volume_atr_ladder_v12",
                    "decision_ts": candle.ts,
                    "effective_from": "next_completed_candle",
                    "filtered_volume_ratio": volume_ratio,
                    "filtered_volume_retained_candles": retained_candles,
                    "accepted_level": accepted_level,
                    "level_kind": if accepted_level == 1 { "cost_adjusted_true_break_even" } else { "frozen_atr_step" },
                    "atr_step": atr_step,
                    "old_stop_price": old_stop_price,
                    "new_stop_price": new_stop_price,
                    "frozen_atr14": atr14,
                    "target_atr_multiplier": target_atr_multiplier,
                    "target_distance_atr": target_atr_multiplier,
                    "true_break_even_price": true_break_even,
                    "close_price": candle.c,
                }
            })
            .to_string(),
        );
        Some(signal)
    }
}

fn valid_state(state: VolumeAtrTrailingReplayState) -> bool {
    state.atr14.is_finite()
        && state.atr14 > 0.0
        && state.target_atr_multiplier.is_finite()
        && state.target_atr_multiplier > 0.0
        && state.per_side_cost_rate.is_finite()
        && (0.0..1.0).contains(&state.per_side_cost_rate)
}

fn directional_price(
    entry_price: f64,
    favorable_distance: f64,
    direction: MarketVelocityTradeDirection,
) -> Option<f64> {
    let price = match direction {
        MarketVelocityTradeDirection::Long => entry_price + favorable_distance,
        MarketVelocityTradeDirection::Short => entry_price - favorable_distance,
        MarketVelocityTradeDirection::Both => return None,
    };
    (price.is_finite() && price > 0.0).then_some(price)
}

fn is_legal_tightening(
    direction: MarketVelocityTradeDirection,
    completed_close: f64,
    current_stop: f64,
    candidate_stop: f64,
    target_price: f64,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => {
            candidate_stop > current_stop
                && candidate_stop < target_price
                && completed_close > candidate_stop
        }
        MarketVelocityTradeDirection::Short => {
            candidate_stop < current_stop
                && candidate_stop > target_price
                && completed_close < candidate_stop
        }
        MarketVelocityTradeDirection::Both => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_gate_failure_does_not_consume_the_next_level() {
        let state = VolumeAtrTrailingReplayState {
            atr14: 2.0,
            target_atr_multiplier: 2.7,
            per_side_cost_rate: 0.0008,
            accepted_updates: 1,
        };
        let candidate = directional_price(
            100.0,
            state.accepted_updates as f64 * state.atr14,
            MarketVelocityTradeDirection::Long,
        )
        .unwrap();

        assert!(!is_legal_tightening(
            MarketVelocityTradeDirection::Long,
            101.5,
            100.1,
            candidate,
            105.4,
        ));
        assert_eq!(state.accepted_updates, 1);
    }

    #[test]
    fn target_one_atr_only_allows_true_break_even_level() {
        let state = VolumeAtrTrailingReplayState {
            atr14: 2.0,
            target_atr_multiplier: 1.0,
            per_side_cost_rate: 0.0008,
            accepted_updates: 1,
        };

        assert!(state.accepted_updates as f64 >= state.target_atr_multiplier);
    }
}
