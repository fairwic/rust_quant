use super::super::model::{Candle, IndicatorSeries};
use super::prior_extremes;

const EMA_TREND_LONG_BREAKOUT_LOOKBACK: usize = 20;
const EMA_TREND_LONG_ACCEPTANCE_WINDOW: usize = 3;

/// V6 接受棒携带的来源棒冻结合同；确认棒只能验证接受，不能改写风险参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct EmaTrendLongAcceptanceV6 {
    pub(super) source_index: usize,
    pub(super) breakout_line: f64,
    pub(super) source_close: f64,
    pub(super) source_atr: f64,
    pub(super) source_volume_ratio: f64,
    pub(super) source_take_profit_atr: f64,
}

#[derive(Debug, Clone, Copy)]
struct EmaTrendLongPendingV6 {
    source_index: usize,
    breakout_line: f64,
    source_close: f64,
    source_atr: f64,
    source_volume_ratio: f64,
    source_take_profit_atr: f64,
}

/// V6 只保留一个尚未关闭的 EMA 趋势多来源，防止后续 K 线重算突破线。
#[derive(Debug, Default)]
pub(super) struct EmaTrendLongStateV6 {
    pending: Option<EmaTrendLongPendingV6>,
}

impl EmaTrendLongStateV6 {
    /// 先推进旧来源，再决定是否冻结新来源；关闭状态的当根不得重新武装。
    pub(super) fn evaluate(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        source_base_ready: bool,
        source_take_profit_atr: Option<f64>,
    ) -> Option<EmaTrendLongAcceptanceV6> {
        let candle = *candles.get(index)?;
        let point = indicators.get(index)?;
        let mut state_closed = false;
        let mut accepted = None;

        if let Some(pending) = self.pending {
            let age = index.saturating_sub(pending.source_index);
            let invalidated = candle.close <= pending.breakout_line;
            let accepted_now = (1..=EMA_TREND_LONG_ACCEPTANCE_WINDOW).contains(&age)
                && !invalidated
                && index > 0
                && candles[index - 1].close > pending.breakout_line
                && candle.close > pending.breakout_line
                && candle.close >= pending.source_close
                && point.ema12.zip(point.ema144).zip(point.ema696).is_some_and(
                    |((ema12, ema144), ema696)| {
                        ema12 > ema144 && ema144 > ema696 && candle.close > ema12
                    },
                );
            if accepted_now {
                accepted = Some(pending.accept());
                state_closed = true;
                self.pending = None;
            } else if invalidated || age >= EMA_TREND_LONG_ACCEPTANCE_WINDOW {
                state_closed = true;
                self.pending = None;
            }
        }

        if !state_closed && self.pending.is_none() && source_base_ready {
            let breakout_line = prior_extremes(candles, index, EMA_TREND_LONG_BREAKOUT_LOOKBACK)?.0;
            if candle.close > breakout_line {
                self.pending = Some(EmaTrendLongPendingV6 {
                    source_index: index,
                    breakout_line,
                    source_close: candle.close,
                    source_atr: point.atr14?,
                    source_volume_ratio: point.filtered_volume_ratio?,
                    source_take_profit_atr: source_take_profit_atr?,
                });
            }
        }
        accepted
    }
}

impl EmaTrendLongPendingV6 {
    fn accept(self) -> EmaTrendLongAcceptanceV6 {
        EmaTrendLongAcceptanceV6 {
            source_index: self.source_index,
            breakout_line: self.breakout_line,
            source_close: self.source_close,
            source_atr: self.source_atr,
            source_volume_ratio: self.source_volume_ratio,
            source_take_profit_atr: self.source_take_profit_atr,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tradingview_velocity_parity::model::IndicatorPoint;
    use crate::app::tradingview_velocity_parity::signals::{
        build_intent, Direction, IntentContext, ParityRuleVersion, SignalFamily, SignalState,
    };

    fn fixture() -> (Vec<Candle>, IndicatorSeries) {
        let mut candles = (0..24)
            .map(|index| Candle {
                timestamp_ms: index as i64 * 900_000,
                open: 95.0,
                high: 100.0,
                low: 94.0,
                close: 99.0,
                volume: 10.0,
            })
            .collect::<Vec<_>>();
        candles[20] = Candle {
            timestamp_ms: 18_000_000,
            open: 100.1,
            high: 104.0,
            low: 99.5,
            close: 103.0,
            volume: 60.0,
        };
        candles[21] = Candle {
            timestamp_ms: 18_900_000,
            open: 102.5,
            high: 105.0,
            low: 102.0,
            close: 104.0,
            volume: 5.0,
        };
        let mut points = vec![IndicatorPoint::default(); candles.len()];
        points[20] = IndicatorPoint {
            filtered_volume_ratio: Some(6.5),
            volume_event: true,
            rsi14: Some(65.0),
            ema12: Some(101.0),
            ema144: Some(98.0),
            ema696: Some(96.0),
            atr14: Some(2.0),
            ..IndicatorPoint::default()
        };
        points[21] = IndicatorPoint {
            filtered_volume_ratio: Some(1.0),
            volume_event: false,
            rsi14: Some(66.0),
            ema12: Some(102.0),
            ema144: Some(99.0),
            ema696: Some(97.0),
            atr14: Some(9.0),
            ..IndicatorPoint::default()
        };
        (candles, IndicatorSeries { points })
    }

    #[test]
    fn source_bar_only_arms_and_next_bar_can_accept_without_new_volume() {
        let (candles, indicators) = fixture();
        let mut state = EmaTrendLongStateV6::default();

        assert_eq!(
            state.evaluate(&candles, &indicators, 20, true, Some(4.5)),
            None
        );
        let accepted = state
            .evaluate(&candles, &indicators, 21, false, None)
            .expect("first completed bar should accept the frozen source");

        assert_eq!(accepted.source_index, 20);
        assert_eq!(accepted.breakout_line, 100.0);
        assert_eq!(accepted.source_close, 103.0);
        assert_eq!(accepted.source_atr, 2.0);
        assert_eq!(accepted.source_volume_ratio, 6.5);
        assert_eq!(accepted.source_take_profit_atr, 4.5);
    }

    #[test]
    fn close_at_breakout_line_invalidates_and_cannot_rearm_on_same_bar() {
        let (mut candles, mut indicators) = fixture();
        let mut state = EmaTrendLongStateV6::default();
        state.evaluate(&candles, &indicators, 20, true, Some(4.5));
        candles[21].close = 100.0;
        indicators.points[21].volume_event = true;
        indicators.points[21].filtered_volume_ratio = Some(7.0);

        assert_eq!(
            state.evaluate(&candles, &indicators, 21, true, Some(4.5)),
            None
        );
        candles[22].close = 104.0;
        assert_eq!(
            state.evaluate(&candles, &indicators, 22, false, None),
            None,
            "invalidating bar must close the state instead of silently rearming it"
        );
    }

    #[test]
    fn third_bar_expires_when_source_close_is_not_recovered() {
        let (mut candles, mut indicators) = fixture();
        let mut state = EmaTrendLongStateV6::default();
        state.evaluate(&candles, &indicators, 20, true, Some(4.5));
        for index in 21..=23 {
            candles[index].close = 102.0;
            indicators.points[index] = indicators.points[21].clone();
            assert_eq!(
                state.evaluate(&candles, &indicators, index, false, None),
                None
            );
        }
        assert!(state.pending.is_none());
    }

    #[test]
    fn accepted_intent_uses_source_risk_while_v5_keeps_signal_bar_values() {
        let (candles, indicators) = fixture();
        let accepted = EmaTrendLongAcceptanceV6 {
            source_index: 20,
            breakout_line: 100.0,
            source_close: 103.0,
            source_atr: 2.0,
            source_volume_ratio: 6.5,
            source_take_profit_atr: 4.5,
        };
        let v6 = build_intent(
            &candles,
            &indicators,
            21,
            0.1,
            Direction::Long,
            IntentContext {
                ema_long: true,
                ema_trend_long_v6: Some(accepted),
                ..IntentContext::default()
            },
            ParityRuleVersion::CandidateV6,
        )
        .expect("accepted V6 EMA long should freeze one intent");
        let v5 = build_intent(
            &candles,
            &indicators,
            21,
            0.1,
            Direction::Long,
            IntentContext {
                ema_long: true,
                take_profit_atr: Some(2.7),
                ..IntentContext::default()
            },
            ParityRuleVersion::CandidateV5,
        )
        .expect("V5 immediate EMA long contract must remain unchanged");

        assert!(v6.families.contains(&SignalFamily::EmaTrendLong));
        assert_eq!(v6.signal_atr, 2.0);
        assert_eq!(v6.stop_ticks, Some(30));
        assert_eq!(v6.target_ticks, Some(90));
        assert_eq!(v6.volume_ratio, Some(6.5));
        assert_eq!(v5.signal_atr, 9.0);
        assert_eq!(v5.stop_ticks, Some(135));
        assert_eq!(v5.target_ticks, Some(243));
    }

    #[test]
    fn candidate_v6_delays_only_ema_long_and_emits_on_non_volume_acceptance_bar() {
        let (candles, indicators) = fixture();
        let v5_source = SignalState::default().evaluate(
            &candles,
            &indicators,
            20,
            0.1,
            None,
            true,
            ParityRuleVersion::CandidateV5,
        );
        let mut v6_state = SignalState::default();
        let v6_source = v6_state.evaluate(
            &candles,
            &indicators,
            20,
            0.1,
            None,
            true,
            ParityRuleVersion::CandidateV6,
        );
        let v6_acceptance = v6_state.evaluate(
            &candles,
            &indicators,
            21,
            0.1,
            None,
            true,
            ParityRuleVersion::CandidateV6,
        );

        assert!(v5_source.intent.is_some());
        assert!(v6_source.intent.is_none());
        let intent = v6_acceptance
            .intent
            .expect("V6 must emit on the completed acceptance bar without new volume");
        assert_eq!(intent.families, vec![SignalFamily::EmaTrendLong]);
        assert_eq!(intent.signal_index, 21);
        assert_eq!(intent.signal_atr, 2.0);
        assert_eq!(intent.target_ticks, Some(90));
    }

    #[test]
    fn unqualified_rsi_pattern_on_acceptance_bar_cannot_relabel_or_move_stop() {
        let (mut candles, mut indicators) = fixture();
        candles[21] = Candle {
            timestamp_ms: 18_900_000,
            open: 102.5,
            high: 104.5,
            low: 96.0,
            close: 104.0,
            volume: 20.0,
        };
        indicators.points[21].volume_event = true;
        indicators.points[21].filtered_volume_ratio = Some(2.6);
        indicators.points[21].rsi14 = Some(25.0);

        let mut state = SignalState::default();
        assert!(state
            .evaluate(
                &candles,
                &indicators,
                20,
                0.1,
                None,
                true,
                ParityRuleVersion::CandidateV6,
            )
            .intent
            .is_none());
        let intent = state
            .evaluate(
                &candles,
                &indicators,
                21,
                0.1,
                None,
                true,
                ParityRuleVersion::CandidateV6,
            )
            .intent
            .expect("EMA 接受棒应继续入场，但低量 RSI 形态不能成为独立家族");

        assert_eq!(intent.families, vec![SignalFamily::EmaTrendLong]);
        assert_eq!(intent.stop_price, None);
        assert_eq!(intent.stop_ticks, Some(30));
        assert_eq!(intent.signal_atr, 2.0);
    }
}
