use super::super::model::{Candle, Direction, IndicatorSeries};
use super::prior_extremes;
use super::v6::EmaTrendLongAcceptanceV6;
use super::CandlePatterns;

const STRUCTURE_LOOKBACK: usize = 20;
const CONFIRMATION_MIN_AGE: usize = 2;
const CONFIRMATION_MAX_AGE: usize = 4;
const EMA_TREND_RETEST_BAND_ATR: f64 = 0.45;
const EMA_EXPANSION_STRUCTURE_NEAR_ATR: f64 = 0.50;
const EMA_EXPANSION_SOURCE_DISTANCE_ATR_MAX: f64 = 1.75;
const EMA_TREND_BREAK_DISTANCE_ATR_MAX: f64 = 1.50;
const EMA_EXPANSION_LONG_RSI_MIN: f64 = 40.0;
const EMA_EXPANSION_LONG_RSI_MAX: f64 = 68.0;
const EMA_EXPANSION_SHORT_RSI_MIN: f64 = 35.0;
const EMA_EXPANSION_SHORT_RSI_MAX: f64 = 65.0;

/// V12 普通 RSI 反转的有限等待结果；背离家族仍保持独立锚点合同。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RsiPatternDecisionV12 {
    pub(super) long: bool,
    pub(super) short: bool,
}

/// V12 EMA 压缩扩张的确认结果；setup 与确认棒之间至少间隔两根。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct EmaExpansionDecisionV12 {
    pub(super) long: bool,
    pub(super) short: bool,
}

#[derive(Debug, Clone, Copy)]
struct EmaTrendLongPendingV12 {
    source_index: usize,
    breakout_line: f64,
    source_close: f64,
    source_atr: f64,
    source_volume_ratio: f64,
    source_take_profit_atr: f64,
}

#[derive(Debug, Clone, Copy)]
struct EmaExpansionPendingV12 {
    source_index: usize,
    direction: Direction,
    structure_line: f64,
}

#[derive(Debug, Clone, Copy)]
struct RsiPatternPendingV12 {
    source_index: usize,
    direction: Direction,
    source_high: f64,
    source_low: f64,
}

/// V12 仅保存来源棒已经冻结的 setup；后续确认不得重算来源结构。
#[derive(Debug, Default)]
pub(super) struct SignalStateV12 {
    ema_trend_long_pending: Option<EmaTrendLongPendingV12>,
    ema_expansion_pending: Option<EmaExpansionPendingV12>,
    rsi_pattern_pending: Option<RsiPatternPendingV12>,
}

impl SignalStateV12 {
    /// EMA 趋势多允许“回踩接受”或“继续放量站稳”两条确认路径，避免只剩回踩样本。
    pub(super) fn evaluate_ema_trend_long(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        source_base_ready: bool,
        source_take_profit_atr: Option<f64>,
        source_distance_atr_max: f64,
    ) -> Option<EmaTrendLongAcceptanceV6> {
        let candle = *candles.get(index)?;
        let point = indicators.get(index)?;
        let mut state_closed = false;

        if let Some(pending) = self.ema_trend_long_pending {
            let age = index.saturating_sub(pending.source_index);
            let ordering_held = point
                .ema12
                .zip(point.ema144)
                .zip(point.ema696)
                .is_some_and(|((ema12, ema144), ema696)| ema12 > ema144 && ema144 > ema696);
            let invalidated = candle.close <= pending.breakout_line || !ordering_held;
            let evidence = usize::from(
                candle.low
                    <= pending.breakout_line + EMA_TREND_RETEST_BAND_ATR * pending.source_atr,
            ) + usize::from(point.ema12.is_some_and(|ema12| candle.close > ema12))
                + usize::from(candle.close >= pending.source_close && candle.close > candle.open);
            let accepted = (CONFIRMATION_MIN_AGE..=CONFIRMATION_MAX_AGE).contains(&age)
                && !invalidated
                && evidence >= 2;
            if accepted {
                self.ema_trend_long_pending = None;
                return Some(EmaTrendLongAcceptanceV6 {
                    source_index: pending.source_index,
                    breakout_line: pending.breakout_line,
                    source_close: pending.source_close,
                    source_atr: pending.source_atr,
                    source_volume_ratio: pending.source_volume_ratio,
                    source_take_profit_atr: pending.source_take_profit_atr,
                });
            }
            if invalidated || age >= CONFIRMATION_MAX_AGE {
                self.ema_trend_long_pending = None;
                state_closed = true;
            }
        }

        if !state_closed && self.ema_trend_long_pending.is_none() && source_base_ready {
            let atr = point.atr14.filter(|value| *value > 0.0)?;
            let ema12 = point.ema12?;
            let breakout_line = prior_extremes(candles, index, STRUCTURE_LOOKBACK)?.0;
            let ema_distance_atr = (candle.close - ema12) / atr;
            let break_distance_atr = (candle.close - breakout_line) / atr;
            if candle.close > breakout_line
                && (0.0..=source_distance_atr_max).contains(&ema_distance_atr)
                && (0.0..=EMA_TREND_BREAK_DISTANCE_ATR_MAX).contains(&break_distance_atr)
            {
                self.ema_trend_long_pending = Some(EmaTrendLongPendingV12 {
                    source_index: index,
                    breakout_line,
                    source_close: candle.close,
                    source_atr: atr,
                    source_volume_ratio: point.filtered_volume_ratio?,
                    source_take_profit_atr: source_take_profit_atr?,
                });
            }
        }
        None
    }

    /// 压缩扩张来源只冻结方向与结构，RSI、离轨和结构接近度改为可替代证据。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_ema_expansion(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        raw_long_state: bool,
        raw_short_state: bool,
        cooldown_ready: bool,
    ) -> EmaExpansionDecisionV12 {
        let Some(candle) = candles.get(index).copied() else {
            return EmaExpansionDecisionV12::default();
        };
        let Some(point) = indicators.get(index) else {
            return EmaExpansionDecisionV12::default();
        };
        let mut state_closed = false;

        if let Some(pending) = self.ema_expansion_pending {
            let age = index.saturating_sub(pending.source_index);
            let ema_values = point.ema12.zip(point.ema144).zip(point.ema596);
            let (structure_confirmed, ordering_held, quality) = match pending.direction {
                Direction::Long => {
                    let ordering_held = ema_values
                        .is_some_and(|((ema12, ema144), ema596)| ema12 > ema144 && ema12 > ema596);
                    let rsi_ready = point.rsi14.is_some_and(|rsi| {
                        (EMA_EXPANSION_LONG_RSI_MIN..=EMA_EXPANSION_LONG_RSI_MAX).contains(&rsi)
                    });
                    (
                        candle.close > pending.structure_line,
                        ordering_held,
                        usize::from(candle.close > candle.open)
                            + usize::from(rsi_ready)
                            + usize::from(point.ema12.is_some_and(|ema12| candle.close > ema12)),
                    )
                }
                Direction::Short => {
                    let ordering_held = ema_values
                        .is_some_and(|((ema12, ema144), ema596)| ema12 < ema144 && ema12 < ema596);
                    let rsi_ready = point.rsi14.is_some_and(|rsi| {
                        (EMA_EXPANSION_SHORT_RSI_MIN..=EMA_EXPANSION_SHORT_RSI_MAX).contains(&rsi)
                    });
                    (
                        candle.close < pending.structure_line,
                        ordering_held,
                        usize::from(candle.close < candle.open)
                            + usize::from(rsi_ready)
                            + usize::from(point.ema12.is_some_and(|ema12| candle.close < ema12)),
                    )
                }
            };
            let accepted = (CONFIRMATION_MIN_AGE..=CONFIRMATION_MAX_AGE).contains(&age)
                && structure_confirmed
                && ordering_held
                && quality >= 1;
            if accepted {
                self.ema_expansion_pending = None;
                return match pending.direction {
                    Direction::Long => EmaExpansionDecisionV12 {
                        long: true,
                        short: false,
                    },
                    Direction::Short => EmaExpansionDecisionV12 {
                        long: false,
                        short: true,
                    },
                };
            }
            if !ordering_held || age >= CONFIRMATION_MAX_AGE {
                self.ema_expansion_pending = None;
                state_closed = true;
            }
        }

        if state_closed || self.ema_expansion_pending.is_some() || !cooldown_ready {
            return EmaExpansionDecisionV12::default();
        }
        let Some(atr) = point.atr14.filter(|value| *value > 0.0) else {
            return EmaExpansionDecisionV12::default();
        };
        let Some(ema12) = point.ema12 else {
            return EmaExpansionDecisionV12::default();
        };
        let Some(rsi) = point.rsi14 else {
            return EmaExpansionDecisionV12::default();
        };
        let Some((structure_high, structure_low)) =
            prior_extremes(candles, index, STRUCTURE_LOOKBACK)
        else {
            return EmaExpansionDecisionV12::default();
        };
        let long_quality =
            usize::from((EMA_EXPANSION_LONG_RSI_MIN..=EMA_EXPANSION_LONG_RSI_MAX).contains(&rsi))
                + usize::from(
                    (candle.close - ema12) / atr <= EMA_EXPANSION_SOURCE_DISTANCE_ATR_MAX,
                )
                + usize::from(
                    candle.close >= structure_high - EMA_EXPANSION_STRUCTURE_NEAR_ATR * atr,
                );
        let short_quality =
            usize::from((EMA_EXPANSION_SHORT_RSI_MIN..=EMA_EXPANSION_SHORT_RSI_MAX).contains(&rsi))
                + usize::from(
                    (ema12 - candle.close) / atr <= EMA_EXPANSION_SOURCE_DISTANCE_ATR_MAX,
                )
                + usize::from(
                    candle.close <= structure_low + EMA_EXPANSION_STRUCTURE_NEAR_ATR * atr,
                );
        let long_source = raw_long_state && point.volume_event && long_quality >= 2;
        let short_source = raw_short_state && point.volume_event && short_quality >= 2;
        if long_source || short_source {
            self.ema_expansion_pending = Some(EmaExpansionPendingV12 {
                source_index: index,
                direction: if long_source {
                    Direction::Long
                } else {
                    Direction::Short
                },
                structure_line: if long_source {
                    structure_high
                } else {
                    structure_low
                },
            });
        }
        EmaExpansionDecisionV12::default()
    }

    /// 普通 RSI 形态先冻结极值棒，再等待 2～4 根内离开极值并突破来源结构。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_rsi_patterns(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        patterns: CandlePatterns,
        bullish_engulfing_accepted: bool,
        bearish_engulfing_accepted: bool,
        divergence_present: bool,
    ) -> RsiPatternDecisionV12 {
        let Some(candle) = candles.get(index).copied() else {
            return RsiPatternDecisionV12::default();
        };
        let Some(point) = indicators.get(index) else {
            return RsiPatternDecisionV12::default();
        };
        let Some(rsi) = point.rsi14 else {
            return RsiPatternDecisionV12::default();
        };
        let mut state_closed = false;

        if let Some(pending) = self.rsi_pattern_pending {
            let age = index.saturating_sub(pending.source_index);
            let previous_macd = index
                .checked_sub(1)
                .and_then(|previous| indicators.get(previous))
                .and_then(|point| point.macd_histogram);
            let quality = match pending.direction {
                Direction::Long => {
                    usize::from(point.ema12.is_some_and(|ema12| candle.close > ema12))
                        + usize::from(candle.close > candle.open)
                        + usize::from(
                            point
                                .macd_histogram
                                .zip(previous_macd)
                                .is_some_and(|(current, previous)| current > previous),
                        )
                }
                Direction::Short => {
                    usize::from(point.ema12.is_some_and(|ema12| candle.close < ema12))
                        + usize::from(candle.close < candle.open)
                        + usize::from(
                            point
                                .macd_histogram
                                .zip(previous_macd)
                                .is_some_and(|(current, previous)| current < previous),
                        )
                }
            };
            let accepted = (CONFIRMATION_MIN_AGE..=CONFIRMATION_MAX_AGE).contains(&age)
                && quality >= 1
                && match pending.direction {
                    Direction::Long => {
                        rsi > 30.0 && rsi <= 55.0 && candle.close > pending.source_high
                    }
                    Direction::Short => {
                        rsi < 70.0 && rsi >= 45.0 && candle.close < pending.source_low
                    }
                };
            if accepted {
                self.rsi_pattern_pending = None;
                return match pending.direction {
                    Direction::Long => RsiPatternDecisionV12 {
                        long: true,
                        short: false,
                    },
                    Direction::Short => RsiPatternDecisionV12 {
                        long: false,
                        short: true,
                    },
                };
            }
            if age >= CONFIRMATION_MAX_AGE {
                self.rsi_pattern_pending = None;
                state_closed = true;
            }
        }

        if state_closed || self.rsi_pattern_pending.is_some() || divergence_present {
            return RsiPatternDecisionV12::default();
        }
        let long_shape = (patterns.bullish_engulfing && bullish_engulfing_accepted)
            || patterns.long_lower_shadow;
        let short_shape = (patterns.bearish_engulfing && bearish_engulfing_accepted)
            || patterns.long_upper_shadow;
        let direction = if point.volume_event && rsi <= 30.0 && long_shape {
            Some(Direction::Long)
        } else if point.volume_event && rsi >= 70.0 && short_shape {
            Some(Direction::Short)
        } else {
            None
        };
        if let Some(direction) = direction {
            self.rsi_pattern_pending = Some(RsiPatternPendingV12 {
                source_index: index,
                direction,
                source_high: candle.high,
                source_low: candle.low,
            });
        }
        RsiPatternDecisionV12::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tradingview_velocity_parity::model::IndicatorPoint;

    fn rsi_fixture() -> (Vec<Candle>, IndicatorSeries) {
        let mut candles = vec![
            Candle {
                timestamp_ms: 0,
                open: 100.0,
                high: 101.0,
                low: 98.0,
                close: 99.0,
                volume: 50.0,
            };
            5
        ];
        candles[2] = Candle {
            timestamp_ms: 1_800_000,
            open: 99.0,
            high: 101.5,
            low: 98.5,
            close: 101.2,
            volume: 20.0,
        };
        let mut points = vec![IndicatorPoint::default(); 5];
        points[0].volume_event = true;
        points[0].rsi14 = Some(28.0);
        points[0].ema12 = Some(100.0);
        points[0].macd_histogram = Some(-2.0);
        points[1].rsi14 = Some(29.0);
        points[1].ema12 = Some(100.0);
        points[1].macd_histogram = Some(-1.5);
        points[2].rsi14 = Some(35.0);
        points[2].ema12 = Some(100.0);
        points[2].macd_histogram = Some(-1.0);
        (candles, IndicatorSeries { points })
    }

    #[test]
    fn rsi_setup_never_enters_before_second_completed_bar() {
        let (candles, indicators) = rsi_fixture();
        let patterns = CandlePatterns {
            bullish_engulfing: false,
            bearish_engulfing: false,
            long_lower_shadow: true,
            long_upper_shadow: false,
        };
        let mut state = SignalStateV12::default();
        assert_eq!(
            state.evaluate_rsi_patterns(&candles, &indicators, 0, patterns, true, true, false,),
            RsiPatternDecisionV12::default()
        );
        assert_eq!(
            state.evaluate_rsi_patterns(
                &candles,
                &indicators,
                1,
                CandlePatterns::default(),
                true,
                true,
                false,
            ),
            RsiPatternDecisionV12::default()
        );
        assert!(
            state
                .evaluate_rsi_patterns(
                    &candles,
                    &indicators,
                    2,
                    CandlePatterns::default(),
                    true,
                    true,
                    false,
                )
                .long
        );
    }
}
