use super::super::model::{Candle, Direction, IndicatorSeries};
use super::prior_extremes;
use super::v6::EmaTrendLongAcceptanceV6;
use super::CandlePatterns;

const STRUCTURE_LOOKBACK: usize = 20;
const ACCEPTANCE_WINDOW: usize = 3;
const V13_IMPULSE_WINDOW: usize = 2;
const V14_IMPULSE_WINDOW: usize = 8;
const EMA_TREND_RETEST_BAND_ATR: f64 = 0.35;
const EMA_EXPANSION_RETEST_BAND_ATR: f64 = 0.25;
const EMA_SOURCE_DISTANCE_ATR_MAX: f64 = 1.25;
const EMA_TREND_BREAK_DISTANCE_ATR_MAX: f64 = 1.50;
const EMA_EXPANSION_LONG_RSI_MIN: f64 = 40.0;
const EMA_EXPANSION_LONG_RSI_MAX: f64 = 68.0;
const EMA_EXPANSION_SHORT_RSI_MIN: f64 = 35.0;
const EMA_EXPANSION_SHORT_RSI_MAX: f64 = 65.0;
const RSI_RECLAIM_LONG_MAX: f64 = 50.0;
const RSI_RECLAIM_SHORT_MIN: f64 = 50.0;

/// V10 普通 RSI 反转门禁结果；背离家族不经过这里，避免改变独立锚点合同。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RsiPatternDecisionV10 {
    pub(super) long: bool,
    pub(super) short: bool,
}

/// V10 压缩扩张的接受确认结果；来源棒只负责冻结结构，确认棒才生成订单。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct EmaExpansionDecisionV10 {
    pub(super) long: bool,
    pub(super) short: bool,
}

/// 选择冻结版本对应的压缩扩张接受合同，避免为每个 Research 版本复制状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EmaExpansionPolicyV10 {
    V10,
    V11,
    V13,
}

impl EmaExpansionPolicyV10 {
    const fn keeps_v11_residual_guards(self) -> bool {
        matches!(self, Self::V11 | Self::V13)
    }
}

#[derive(Debug, Clone, Copy)]
struct EmaTrendLongPendingV10 {
    source_index: usize,
    breakout_line: f64,
    source_close: f64,
    source_atr: f64,
    source_volume_ratio: f64,
    source_take_profit_atr: f64,
    /// true 时只有阳线才能确认该补充来源；在来源棒冻结，避免后续调用漂移。
    require_bullish_acceptance: bool,
}

#[derive(Debug, Clone, Copy)]
struct EmaExpansionPendingV10 {
    source_index: usize,
    direction: Direction,
    structure_line: f64,
    source_atr: f64,
}

#[derive(Debug, Clone, Copy)]
struct EmaExpansionSetupV13 {
    setup_index: usize,
    direction: Direction,
    structure_line: f64,
    source_atr: f64,
}

/// V14 的压缩 setup 不提前选择方向，避免一个早期弱方向占用整个观察窗口。
#[derive(Debug, Clone, Copy)]
struct EmaCompressionSetupV14 {
    setup_index: usize,
    structure_high: f64,
    structure_low: f64,
    source_atr: f64,
}

/// V10 只保存尚未接受的有限窗口 setup，所有结构线都在来源棒收盘冻结。
#[derive(Debug, Default)]
pub(super) struct SignalStateV10 {
    ema_trend_long_pending: Option<EmaTrendLongPendingV10>,
    ema_expansion_pending: Option<EmaExpansionPendingV10>,
    ema_expansion_setup_v13: Option<EmaExpansionSetupV13>,
    ema_compression_setup_v14: Option<EmaCompressionSetupV14>,
}

impl SignalStateV10 {
    /// 把 EMA 趋势多从单棒追入改成有限窗口回踩接受，避免在来源棒极端延伸后追价。
    /// 距离上限由调用方显式传入，使 Research 邻域可验证且默认 V19 仍固定为 1.25 ATR。
    pub(super) fn evaluate_ema_trend_long(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        source_base_ready: bool,
        source_take_profit_atr: Option<f64>,
        source_distance_atr_max: f64,
        source_requires_bullish_acceptance: bool,
    ) -> Option<EmaTrendLongAcceptanceV6> {
        let candle = *candles.get(index)?;
        let point = indicators.get(index)?;
        let mut state_closed = false;

        if let Some(pending) = self.ema_trend_long_pending {
            let age = index.saturating_sub(pending.source_index);
            let invalidated = candle.close <= pending.breakout_line;
            let trend_held = point.ema12.zip(point.ema144).zip(point.ema696).is_some_and(
                |((ema12, ema144), ema696)| {
                    ema12 > ema144 && ema144 > ema696 && candle.close > ema12
                },
            );
            let retest_accepted = (1..=ACCEPTANCE_WINDOW).contains(&age)
                && !invalidated
                && candle.low
                    <= pending.breakout_line + EMA_TREND_RETEST_BAND_ATR * pending.source_atr
                && trend_held
                && (!pending.require_bullish_acceptance || candle.close > candle.open);
            if retest_accepted {
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
            if invalidated || age >= ACCEPTANCE_WINDOW {
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
                self.ema_trend_long_pending = Some(EmaTrendLongPendingV10 {
                    source_index: index,
                    breakout_line,
                    source_close: candle.close,
                    source_atr: atr,
                    source_volume_ratio: point.filtered_volume_ratio?,
                    source_take_profit_atr: source_take_profit_atr?,
                    require_bullish_acceptance: source_requires_bullish_acceptance,
                });
            }
        }
        None
    }

    /// 要求压缩扩张先放量穿越冻结边界，再在三根内回踩失败后确认方向。
    pub(super) fn evaluate_ema_expansion(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        raw_long_state: bool,
        raw_short_state: bool,
        cooldown_ready: bool,
        policy: EmaExpansionPolicyV10,
    ) -> EmaExpansionDecisionV10 {
        if policy == EmaExpansionPolicyV10::V13 {
            return self.evaluate_ema_expansion_v13(
                candles,
                indicators,
                index,
                raw_long_state,
                raw_short_state,
                cooldown_ready,
            );
        }
        let Some(candle) = candles.get(index).copied() else {
            return EmaExpansionDecisionV10::default();
        };
        let Some(point) = indicators.get(index) else {
            return EmaExpansionDecisionV10::default();
        };
        let mut state_closed = false;

        if let Some(pending) = self.ema_expansion_pending {
            let age = index.saturating_sub(pending.source_index);
            let (invalidated, accepted) = match pending.direction {
                Direction::Long => {
                    let invalidated = candle.close <= pending.structure_line;
                    let accepted = (1..=ACCEPTANCE_WINDOW).contains(&age)
                        && !invalidated
                        && candle.low
                            <= pending.structure_line
                                + EMA_EXPANSION_RETEST_BAND_ATR * pending.source_atr
                        && point.ema12.is_some_and(|ema12| candle.close > ema12);
                    (invalidated, accepted)
                }
                Direction::Short => {
                    let invalidated = candle.close >= pending.structure_line;
                    let accepted = (1..=ACCEPTANCE_WINDOW).contains(&age)
                        && !invalidated
                        && candle.high
                            >= pending.structure_line
                                - EMA_EXPANSION_RETEST_BAND_ATR * pending.source_atr
                        && point.ema12.is_some_and(|ema12| candle.close < ema12)
                        && (!policy.keeps_v11_residual_guards()
                            || point
                                .rsi14
                                .is_some_and(|rsi| rsi >= EMA_EXPANSION_SHORT_RSI_MIN));
                    (invalidated, accepted)
                }
            };
            if accepted {
                self.ema_expansion_pending = None;
                return match pending.direction {
                    Direction::Long => EmaExpansionDecisionV10 {
                        long: true,
                        short: false,
                    },
                    Direction::Short => EmaExpansionDecisionV10 {
                        long: false,
                        short: true,
                    },
                };
            }
            if invalidated || age >= ACCEPTANCE_WINDOW {
                self.ema_expansion_pending = None;
                state_closed = true;
            }
        }

        if state_closed || self.ema_expansion_pending.is_some() || !cooldown_ready {
            return EmaExpansionDecisionV10::default();
        }
        let Some(atr) = point.atr14.filter(|value| *value > 0.0) else {
            return EmaExpansionDecisionV10::default();
        };
        let Some(ema12) = point.ema12 else {
            return EmaExpansionDecisionV10::default();
        };
        let Some(rsi) = point.rsi14 else {
            return EmaExpansionDecisionV10::default();
        };
        let Some((structure_high, structure_low)) =
            prior_extremes(candles, index, STRUCTURE_LOOKBACK)
        else {
            return EmaExpansionDecisionV10::default();
        };

        let long_source = raw_long_state
            && point.volume_event
            && (!policy.keeps_v11_residual_guards()
                || point
                    .filtered_volume_ratio
                    .is_some_and(|ratio| ratio >= 3.0))
            && (EMA_EXPANSION_LONG_RSI_MIN..=EMA_EXPANSION_LONG_RSI_MAX).contains(&rsi)
            && candle.open <= structure_high
            && candle.close > structure_high
            && (candle.close - ema12) / atr <= EMA_SOURCE_DISTANCE_ATR_MAX;
        let short_source = raw_short_state
            && point.volume_event
            && (EMA_EXPANSION_SHORT_RSI_MIN..=EMA_EXPANSION_SHORT_RSI_MAX).contains(&rsi)
            && candle.open >= structure_low
            && candle.close < structure_low
            && (ema12 - candle.close) / atr <= EMA_SOURCE_DISTANCE_ATR_MAX;
        if long_source || short_source {
            self.ema_expansion_pending = Some(EmaExpansionPendingV10 {
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
                source_atr: atr,
            });
        }
        EmaExpansionDecisionV10::default()
    }

    /// V13 把压缩释放、放量突破与回踩接受拆成有序阶段，但始终使用 setup 冻结边界。
    fn evaluate_ema_expansion_v13(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        raw_long_state: bool,
        raw_short_state: bool,
        cooldown_ready: bool,
    ) -> EmaExpansionDecisionV10 {
        let Some(candle) = candles.get(index).copied() else {
            return EmaExpansionDecisionV10::default();
        };
        let Some(point) = indicators.get(index) else {
            return EmaExpansionDecisionV10::default();
        };
        let mut state_closed = false;

        if let Some(pending) = self.ema_expansion_pending {
            let age = index.saturating_sub(pending.source_index);
            let (invalidated, accepted) = match pending.direction {
                Direction::Long => {
                    let invalidated = candle.close <= pending.structure_line;
                    let accepted = (1..=ACCEPTANCE_WINDOW).contains(&age)
                        && !invalidated
                        && candle.low
                            <= pending.structure_line
                                + EMA_EXPANSION_RETEST_BAND_ATR * pending.source_atr
                        && point.ema12.is_some_and(|ema12| candle.close > ema12);
                    (invalidated, accepted)
                }
                Direction::Short => {
                    let invalidated = candle.close >= pending.structure_line;
                    let accepted = (1..=ACCEPTANCE_WINDOW).contains(&age)
                        && !invalidated
                        && candle.high
                            >= pending.structure_line
                                - EMA_EXPANSION_RETEST_BAND_ATR * pending.source_atr
                        && point.ema12.is_some_and(|ema12| candle.close < ema12)
                        && point
                            .rsi14
                            .is_some_and(|rsi| rsi >= EMA_EXPANSION_SHORT_RSI_MIN);
                    (invalidated, accepted)
                }
            };
            if accepted {
                self.ema_expansion_pending = None;
                return match pending.direction {
                    Direction::Long => EmaExpansionDecisionV10 {
                        long: true,
                        short: false,
                    },
                    Direction::Short => EmaExpansionDecisionV10 {
                        long: false,
                        short: true,
                    },
                };
            }
            if invalidated || age >= ACCEPTANCE_WINDOW {
                self.ema_expansion_pending = None;
                state_closed = true;
            }
        }

        if state_closed || self.ema_expansion_pending.is_some() || !cooldown_ready {
            return EmaExpansionDecisionV10::default();
        }

        if let Some(setup) = self.ema_expansion_setup_v13 {
            let age = index.saturating_sub(setup.setup_index);
            if age > V13_IMPULSE_WINDOW {
                self.ema_expansion_setup_v13 = None;
                state_closed = true;
            }
        }

        if !state_closed
            && self.ema_expansion_setup_v13.is_none()
            && (raw_long_state || raw_short_state)
        {
            let Some(source_atr) = point.atr14.filter(|value| *value > 0.0) else {
                return EmaExpansionDecisionV10::default();
            };
            let Some((structure_high, structure_low)) =
                prior_extremes(candles, index, STRUCTURE_LOOKBACK)
            else {
                return EmaExpansionDecisionV10::default();
            };
            let direction = if raw_long_state {
                Direction::Long
            } else {
                Direction::Short
            };
            self.ema_expansion_setup_v13 = Some(EmaExpansionSetupV13 {
                setup_index: index,
                direction,
                structure_line: if direction == Direction::Long {
                    structure_high
                } else {
                    structure_low
                },
                source_atr,
            });
        }

        let Some(setup) = self.ema_expansion_setup_v13 else {
            return EmaExpansionDecisionV10::default();
        };
        let Some(ema12) = point.ema12 else {
            return EmaExpansionDecisionV10::default();
        };
        let Some(rsi) = point.rsi14 else {
            return EmaExpansionDecisionV10::default();
        };
        let impulse = match setup.direction {
            Direction::Long => {
                point.volume_event
                    && point
                        .filtered_volume_ratio
                        .is_some_and(|ratio| ratio >= 3.0)
                    && (EMA_EXPANSION_LONG_RSI_MIN..=EMA_EXPANSION_LONG_RSI_MAX).contains(&rsi)
                    && candle.open <= setup.structure_line
                    && candle.close > setup.structure_line
                    && (candle.close - ema12) / setup.source_atr <= EMA_SOURCE_DISTANCE_ATR_MAX
            }
            Direction::Short => {
                point.volume_event
                    && (EMA_EXPANSION_SHORT_RSI_MIN..=EMA_EXPANSION_SHORT_RSI_MAX).contains(&rsi)
                    && candle.open >= setup.structure_line
                    && candle.close < setup.structure_line
                    && (ema12 - candle.close) / setup.source_atr <= EMA_SOURCE_DISTANCE_ATR_MAX
            }
        };
        if impulse {
            self.ema_expansion_setup_v13 = None;
            self.ema_expansion_pending = Some(EmaExpansionPendingV10 {
                source_index: index,
                direction: setup.direction,
                structure_line: setup.structure_line,
                source_atr: setup.source_atr,
            });
        }
        EmaExpansionDecisionV10::default()
    }

    /// V14 先冻结无方向压缩区间，随后只在有限窗口内由真实破位脉冲决定方向。
    pub(super) fn evaluate_ema_expansion_v14(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        compression_ready: bool,
        long_impulse_ready: bool,
        short_impulse_ready: bool,
        cooldown_ready: bool,
    ) -> EmaExpansionDecisionV10 {
        let Some(candle) = candles.get(index).copied() else {
            return EmaExpansionDecisionV10::default();
        };
        let Some(point) = indicators.get(index) else {
            return EmaExpansionDecisionV10::default();
        };
        let mut state_closed = false;

        // 接受阶段与 V11 保持一致；V14 只改变 setup 和 impulse 的时序归属。
        if let Some(pending) = self.ema_expansion_pending {
            let age = index.saturating_sub(pending.source_index);
            let (invalidated, accepted) = match pending.direction {
                Direction::Long => {
                    let invalidated = candle.close <= pending.structure_line;
                    let accepted = (1..=ACCEPTANCE_WINDOW).contains(&age)
                        && !invalidated
                        && candle.low
                            <= pending.structure_line
                                + EMA_EXPANSION_RETEST_BAND_ATR * pending.source_atr
                        && point.ema12.is_some_and(|ema12| candle.close > ema12);
                    (invalidated, accepted)
                }
                Direction::Short => {
                    let invalidated = candle.close >= pending.structure_line;
                    let accepted = (1..=ACCEPTANCE_WINDOW).contains(&age)
                        && !invalidated
                        && candle.high
                            >= pending.structure_line
                                - EMA_EXPANSION_RETEST_BAND_ATR * pending.source_atr
                        && point.ema12.is_some_and(|ema12| candle.close < ema12)
                        && point
                            .rsi14
                            .is_some_and(|rsi| rsi >= EMA_EXPANSION_SHORT_RSI_MIN);
                    (invalidated, accepted)
                }
            };
            if accepted {
                self.ema_expansion_pending = None;
                return match pending.direction {
                    Direction::Long => EmaExpansionDecisionV10 {
                        long: true,
                        short: false,
                    },
                    Direction::Short => EmaExpansionDecisionV10 {
                        long: false,
                        short: true,
                    },
                };
            }
            if invalidated || age >= ACCEPTANCE_WINDOW {
                self.ema_expansion_pending = None;
                state_closed = true;
            }
        }

        if state_closed || self.ema_expansion_pending.is_some() || !cooldown_ready {
            return EmaExpansionDecisionV10::default();
        }

        if let Some(setup) = self.ema_compression_setup_v14 {
            let age = index.saturating_sub(setup.setup_index);
            if age > V14_IMPULSE_WINDOW {
                self.ema_compression_setup_v14 = None;
                state_closed = true;
            }
        }

        // 过期棒不立即重建 setup，避免同一根 K 线移动冻结边界。
        if !state_closed && self.ema_compression_setup_v14.is_none() && compression_ready {
            let Some(source_atr) = point.atr14.filter(|value| *value > 0.0) else {
                return EmaExpansionDecisionV10::default();
            };
            let Some((structure_high, structure_low)) =
                prior_extremes(candles, index, STRUCTURE_LOOKBACK)
            else {
                return EmaExpansionDecisionV10::default();
            };
            self.ema_compression_setup_v14 = Some(EmaCompressionSetupV14 {
                setup_index: index,
                structure_high,
                structure_low,
                source_atr,
            });
        }

        let Some(setup) = self.ema_compression_setup_v14 else {
            return EmaExpansionDecisionV10::default();
        };
        let Some(ema12) = point.ema12 else {
            return EmaExpansionDecisionV10::default();
        };
        let Some(rsi) = point.rsi14 else {
            return EmaExpansionDecisionV10::default();
        };
        let long_impulse = long_impulse_ready
            && point.volume_event
            && point
                .filtered_volume_ratio
                .is_some_and(|ratio| ratio >= 3.0)
            && (EMA_EXPANSION_LONG_RSI_MIN..=EMA_EXPANSION_LONG_RSI_MAX).contains(&rsi)
            && candle.open <= setup.structure_high
            && candle.close > setup.structure_high
            && (candle.close - ema12) / setup.source_atr <= EMA_SOURCE_DISTANCE_ATR_MAX;
        let short_impulse = short_impulse_ready
            && point.volume_event
            && (EMA_EXPANSION_SHORT_RSI_MIN..=EMA_EXPANSION_SHORT_RSI_MAX).contains(&rsi)
            && candle.open >= setup.structure_low
            && candle.close < setup.structure_low
            && (ema12 - candle.close) / setup.source_atr <= EMA_SOURCE_DISTANCE_ATR_MAX;
        if long_impulse || short_impulse {
            let direction = if long_impulse {
                Direction::Long
            } else {
                Direction::Short
            };
            self.ema_compression_setup_v14 = None;
            self.ema_expansion_pending = Some(EmaExpansionPendingV10 {
                source_index: index,
                direction,
                structure_line: if direction == Direction::Long {
                    setup.structure_high
                } else {
                    setup.structure_low
                },
                source_atr: setup.source_atr,
            });
        }
        EmaExpansionDecisionV10::default()
    }
}

/// 普通 RSI 形态必须先离开极值区并突破上一棒结构；强逆势时还需收复 EMA12。
pub(super) fn evaluate_rsi_patterns(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    patterns: CandlePatterns,
    bullish_engulfing_accepted: bool,
    bearish_engulfing_accepted: bool,
    divergence_present: bool,
    residual_v11: bool,
) -> RsiPatternDecisionV10 {
    let Some(previous_index) = index.checked_sub(1) else {
        return RsiPatternDecisionV10::default();
    };
    let Some(current) = candles.get(index).copied() else {
        return RsiPatternDecisionV10::default();
    };
    let Some(previous_candle) = candles.get(previous_index).copied() else {
        return RsiPatternDecisionV10::default();
    };
    let Some(point) = indicators.get(index) else {
        return RsiPatternDecisionV10::default();
    };
    let Some(previous) = indicators.get(previous_index) else {
        return RsiPatternDecisionV10::default();
    };
    let Some((rsi, previous_rsi, ema12, ema144, ema696)) = point
        .rsi14
        .zip(previous.rsi14)
        .zip(point.ema12)
        .zip(point.ema144)
        .zip(point.ema696)
        .map(|((((rsi, previous_rsi), ema12), ema144), ema696)| {
            (rsi, previous_rsi, ema12, ema144, ema696)
        })
    else {
        return RsiPatternDecisionV10::default();
    };

    let long_shape =
        (patterns.bullish_engulfing && bullish_engulfing_accepted) || patterns.long_lower_shadow;
    let short_shape =
        (patterns.bearish_engulfing && bearish_engulfing_accepted) || patterns.long_upper_shadow;
    let strict_bear_regime = ema12 < ema144 && ema144 < ema696;
    let strict_bull_regime = ema12 > ema144 && ema144 > ema696;
    RsiPatternDecisionV10 {
        long: point.volume_event
            && !divergence_present
            && previous_rsi <= 30.0
            && rsi > 30.0
            && rsi <= RSI_RECLAIM_LONG_MAX
            && long_shape
            && current.close > previous_candle.high
            && (if residual_v11 {
                current.close > ema12
            } else {
                !strict_bear_regime || current.close > ema12
            }),
        short: point.volume_event
            && !divergence_present
            && previous_rsi >= 70.0
            && rsi < 70.0
            && rsi >= RSI_RECLAIM_SHORT_MIN
            && short_shape
            && current.close < previous_candle.low
            && (if residual_v11 {
                current.close < ema12
            } else {
                !strict_bull_regime || current.close < ema12
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tradingview_velocity_parity::model::IndicatorPoint;

    fn trend_fixture() -> (Vec<Candle>, IndicatorSeries) {
        let mut candles = (0..23)
            .map(|index| Candle {
                timestamp_ms: index as i64 * 900_000,
                open: 98.0,
                high: 100.0,
                low: 97.0,
                close: 99.0,
                volume: 10.0,
            })
            .collect::<Vec<_>>();
        candles[20] = Candle {
            timestamp_ms: 18_000_000,
            open: 99.5,
            high: 102.0,
            low: 99.4,
            close: 101.0,
            volume: 60.0,
        };
        candles[21] = Candle {
            timestamp_ms: 18_900_000,
            open: 101.0,
            high: 101.5,
            low: 100.2,
            close: 100.8,
            volume: 15.0,
        };
        let mut points = vec![IndicatorPoint::default(); candles.len()];
        for index in 20..=21 {
            points[index] = IndicatorPoint {
                filtered_volume_ratio: Some(if index == 20 { 6.0 } else { 1.0 }),
                volume_event: index == 20,
                rsi14: Some(60.0),
                ema12: Some(100.1),
                ema144: Some(98.0),
                ema696: Some(96.0),
                atr14: Some(2.0),
                ..IndicatorPoint::default()
            };
        }
        (candles, IndicatorSeries { points })
    }

    #[test]
    fn ema_trend_long_requires_a_retest_but_not_a_new_high() {
        let (candles, indicators) = trend_fixture();
        let mut state = SignalStateV10::default();

        assert_eq!(
            state.evaluate_ema_trend_long(&candles, &indicators, 20, true, Some(4.5), 1.25, false,),
            None
        );
        let accepted = state
            .evaluate_ema_trend_long(&candles, &indicators, 21, false, None, 1.25, false)
            .expect("the first completed retest should accept the frozen breakout");

        assert_eq!(accepted.breakout_line, 100.0);
        assert!(candles[21].close < accepted.source_close);
    }

    #[test]
    fn ema_trend_long_source_distance_uses_the_research_parameter() {
        let (mut candles, indicators) = trend_fixture();
        candles[20].high = 102.8;
        candles[20].close = 102.68;

        let mut strict = SignalStateV10::default();
        assert_eq!(
            strict
                .evaluate_ema_trend_long(&candles, &indicators, 20, true, Some(4.5), 1.25, false,),
            None
        );
        assert_eq!(
            strict.evaluate_ema_trend_long(&candles, &indicators, 21, false, None, 1.25, false),
            None
        );

        let mut relaxed = SignalStateV10::default();
        assert_eq!(
            relaxed.evaluate_ema_trend_long(
                &candles,
                &indicators,
                20,
                true,
                Some(4.5),
                1.35,
                false,
            ),
            None
        );
        assert!(
            relaxed
                .evaluate_ema_trend_long(&candles, &indicators, 21, false, None, 1.35, false)
                .is_some(),
            "1.29 ATR source distance should be accepted only by the relaxed contract"
        );
    }

    #[test]
    fn ema_trend_long_freezes_bullish_acceptance_on_the_research_source() {
        let (mut candles, indicators) = trend_fixture();
        let mut strict = SignalStateV10::default();

        assert_eq!(
            strict.evaluate_ema_trend_long(&candles, &indicators, 20, true, Some(4.5), 1.25, true,),
            None
        );
        assert_eq!(
            strict.evaluate_ema_trend_long(&candles, &indicators, 21, false, None, 1.25, false,),
            None,
            "the bearish retest must remain blocked by the source-time contract"
        );

        candles[21].open = 100.6;
        let mut bullish = SignalStateV10::default();
        assert_eq!(
            bullish
                .evaluate_ema_trend_long(&candles, &indicators, 20, true, Some(4.5), 1.25, true,),
            None
        );
        assert!(
            bullish
                .evaluate_ema_trend_long(&candles, &indicators, 21, false, None, 1.25, false)
                .is_some(),
            "a bullish retest should accept the same frozen source"
        );
    }

    #[test]
    fn ema_expansion_waits_for_a_failed_retest() {
        let (candles, indicators) = trend_fixture();
        let mut state = SignalStateV10::default();

        assert_eq!(
            state.evaluate_ema_expansion(
                &candles,
                &indicators,
                20,
                true,
                false,
                true,
                EmaExpansionPolicyV10::V10,
            ),
            EmaExpansionDecisionV10::default()
        );
        assert_eq!(
            state.evaluate_ema_expansion(
                &candles,
                &indicators,
                21,
                false,
                false,
                true,
                EmaExpansionPolicyV10::V10,
            ),
            EmaExpansionDecisionV10 {
                long: true,
                short: false
            }
        );
    }

    #[test]
    fn v11_ema_expansion_rejects_sub_three_volume_source() {
        let (candles, mut indicators) = trend_fixture();
        indicators.points[20].filtered_volume_ratio = Some(2.9);
        let mut state = SignalStateV10::default();

        assert_eq!(
            state.evaluate_ema_expansion(
                &candles,
                &indicators,
                20,
                true,
                false,
                true,
                EmaExpansionPolicyV10::V11,
            ),
            EmaExpansionDecisionV10::default()
        );
        assert_eq!(
            state.evaluate_ema_expansion(
                &candles,
                &indicators,
                21,
                false,
                false,
                true,
                EmaExpansionPolicyV10::V11,
            ),
            EmaExpansionDecisionV10::default()
        );
    }

    #[test]
    fn v13_ema_expansion_accepts_a_later_impulse_against_the_frozen_line() {
        let (mut candles, mut indicators) = trend_fixture();
        candles[20].open = 99.0;
        candles[20].close = 99.5;
        candles[20].high = 99.8;
        indicators.points[20].volume_event = false;
        candles[21] = Candle {
            timestamp_ms: 18_900_000,
            open: 99.8,
            high: 101.8,
            low: 99.7,
            close: 101.2,
            volume: 60.0,
        };
        indicators.points[21].volume_event = true;
        indicators.points[21].filtered_volume_ratio = Some(3.2);
        candles[22] = Candle {
            timestamp_ms: 19_800_000,
            open: 101.1,
            high: 101.4,
            low: 100.2,
            close: 100.8,
            volume: 15.0,
        };
        indicators.points[22] = IndicatorPoint {
            filtered_volume_ratio: Some(1.0),
            rsi14: Some(58.0),
            ema12: Some(100.1),
            ema144: Some(98.0),
            ema696: Some(96.0),
            atr14: Some(2.0),
            ..IndicatorPoint::default()
        };
        let mut state = SignalStateV10::default();

        assert_eq!(
            state.evaluate_ema_expansion(
                &candles,
                &indicators,
                20,
                true,
                false,
                true,
                EmaExpansionPolicyV10::V13,
            ),
            EmaExpansionDecisionV10::default()
        );
        assert_eq!(
            state.evaluate_ema_expansion(
                &candles,
                &indicators,
                21,
                false,
                false,
                true,
                EmaExpansionPolicyV10::V13,
            ),
            EmaExpansionDecisionV10::default()
        );
        assert_eq!(
            state.evaluate_ema_expansion(
                &candles,
                &indicators,
                22,
                false,
                false,
                true,
                EmaExpansionPolicyV10::V13,
            ),
            EmaExpansionDecisionV10 {
                long: true,
                short: false,
            }
        );
    }

    #[test]
    fn v14_directionless_compression_waits_for_a_later_directional_impulse() {
        let (mut candles, mut indicators) = trend_fixture();
        candles[20].open = 99.0;
        candles[20].high = 99.8;
        candles[20].close = 99.5;
        indicators.points[20].volume_event = false;
        candles[21] = Candle {
            timestamp_ms: 18_900_000,
            open: 99.8,
            high: 101.8,
            low: 99.7,
            close: 101.2,
            volume: 60.0,
        };
        indicators.points[21].volume_event = true;
        indicators.points[21].filtered_volume_ratio = Some(3.2);
        candles[22] = Candle {
            timestamp_ms: 19_800_000,
            open: 101.1,
            high: 101.4,
            low: 100.2,
            close: 100.8,
            volume: 15.0,
        };
        indicators.points[22] = IndicatorPoint {
            filtered_volume_ratio: Some(1.0),
            rsi14: Some(58.0),
            ema12: Some(100.1),
            ema144: Some(98.0),
            ema696: Some(96.0),
            atr14: Some(2.0),
            ..IndicatorPoint::default()
        };
        let mut state = SignalStateV10::default();

        assert_eq!(
            state.evaluate_ema_expansion_v14(&candles, &indicators, 20, true, false, false, true,),
            EmaExpansionDecisionV10::default()
        );
        assert_eq!(
            state.evaluate_ema_expansion_v14(&candles, &indicators, 21, false, true, false, true,),
            EmaExpansionDecisionV10::default()
        );
        assert_eq!(
            state.evaluate_ema_expansion_v14(&candles, &indicators, 22, false, false, false, true,),
            EmaExpansionDecisionV10 {
                long: true,
                short: false,
            }
        );
    }

    #[test]
    fn rsi_pattern_requires_midline_reclaim_and_previous_high_break() {
        let candles = vec![
            Candle {
                timestamp_ms: 0,
                open: 100.0,
                high: 101.0,
                low: 98.0,
                close: 99.0,
                volume: 10.0,
            },
            Candle {
                timestamp_ms: 900_000,
                open: 99.0,
                high: 103.0,
                low: 98.5,
                close: 102.0,
                volume: 50.0,
            },
        ];
        let indicators = IndicatorSeries {
            points: vec![
                IndicatorPoint {
                    rsi14: Some(29.0),
                    ..IndicatorPoint::default()
                },
                IndicatorPoint {
                    volume_event: true,
                    rsi14: Some(35.0),
                    ema12: Some(100.0),
                    ema144: Some(99.0),
                    ema696: Some(98.0),
                    ..IndicatorPoint::default()
                },
            ],
        };

        assert_eq!(
            evaluate_rsi_patterns(
                &candles,
                &indicators,
                1,
                CandlePatterns {
                    bullish_engulfing: true,
                    ..CandlePatterns::default()
                },
                true,
                true,
                false,
                false,
            ),
            RsiPatternDecisionV10 {
                long: true,
                short: false
            }
        );
    }

    #[test]
    fn v11_rsi_reversal_requires_ema12_reclaim_even_outside_strict_bear_regime() {
        let candles = vec![
            Candle {
                timestamp_ms: 0,
                open: 100.0,
                high: 101.0,
                low: 98.0,
                close: 99.0,
                volume: 10.0,
            },
            Candle {
                timestamp_ms: 900_000,
                open: 99.0,
                high: 103.0,
                low: 98.5,
                close: 102.0,
                volume: 50.0,
            },
        ];
        let indicators = IndicatorSeries {
            points: vec![
                IndicatorPoint {
                    rsi14: Some(29.0),
                    ..IndicatorPoint::default()
                },
                IndicatorPoint {
                    volume_event: true,
                    rsi14: Some(35.0),
                    ema12: Some(102.5),
                    ema144: Some(99.0),
                    ema696: Some(98.0),
                    ..IndicatorPoint::default()
                },
            ],
        };
        let patterns = CandlePatterns {
            bullish_engulfing: true,
            ..CandlePatterns::default()
        };

        assert!(
            evaluate_rsi_patterns(&candles, &indicators, 1, patterns, true, true, false, false,)
                .long
        );
        assert!(
            !evaluate_rsi_patterns(&candles, &indicators, 1, patterns, true, true, false, true,)
                .long
        );
    }
}
