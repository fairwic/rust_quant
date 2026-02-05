use serde::{Deserialize, Serialize};

// 信号类型枚举
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub enum SignalType {
    SimpleBreakEma2through, // 突破信号
    VolumeTrend,            // 成交量趋势
    EmaTrend,               // ema趋势
    Rsi,                    // RSI指标
    TrendStrength,          // 趋势强度
    EmaDivergence,          // 均线发散
    PriceLevel,             // 关键价位
    Bolling,                // 布林带
    Engulfing,              // 吞没形态
    KlineHammer,            // 锤子形态
    // 新增Smart Money Concepts相关信号类型
    LegDetection,    // 腿部识别
    FairValueGap,    // 公平价值缺口
    EqualHighLow,    // 等高/等低点
    PremiumDiscount, // 溢价/折扣区域
    // 新增第一性原理信号类型
    IctStructureBreakout, // ICT 结构突破信号
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub enum SignalDirect {
    IsLong,
    IsShort,
}

// 信号条件枚举
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub enum SignalCondition {
    PriceBreakout {
        price_above: bool,
        price_below: bool,
    },
    Volume {
        is_increasing: bool,
        ratio: f64,
    },
    RsiLevel {
        current: f64,
        oversold: f64,
        overbought: f64,
        is_valid: bool, //rsi是否有效
    },
    Trend {
        strength: f64,
        threshold: f64,
    },
    EmaStatus {
        is_diverging: bool,
    },
    EmaTouchTrend {
        is_long_signal: bool,
        is_short_signal: bool,
    },
    Bolling {
        is_long_signal: bool,
        is_short_signal: bool,
        is_close_signal: bool,
    },
    Engulfing {
        is_long_signal: bool,
        is_short_signal: bool,
    },
    KlineHammer {
        is_long_signal: bool,
        is_short_signal: bool,
    },
    // 新增Smart Money Concepts相关信号条件
    LegDetection {
        is_bullish_leg: bool,
        is_bearish_leg: bool,
        is_new_leg: bool,
    },
    FairValueGap {
        is_bullish_fvg: bool,
        is_bearish_fvg: bool,
    },
    EqualHighLow {
        is_equal_high: bool,
        is_equal_low: bool,
    },
    PremiumDiscount {
        in_premium_zone: bool,
        in_discount_zone: bool,
    },
}

// 权重配置结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalWeightsConfig {
    pub weights: Vec<(SignalType, f64)>,
    pub min_total_weight: f64,
}

// 信号评分结构体
#[derive(Debug)]
pub struct SignalScoreWithDirect {
    pub total_weight: f64,
    pub details: Vec<CheckConditionResult>,
    pub signal_result: Option<SignalDirect>,
}

impl Default for SignalWeightsConfig {
    fn default() -> Self {
        Self {
            weights: vec![
                (SignalType::SimpleBreakEma2through, 1.0),
                (SignalType::VolumeTrend, 1.0),
                (SignalType::Rsi, 1.0),
                (SignalType::EmaTrend, 1.0),
                (SignalType::Bolling, 1.0),
                (SignalType::Engulfing, 1.0),
                (SignalType::KlineHammer, 1.0),
                (SignalType::LegDetection, 1.2),
            ],
            min_total_weight: 2.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckConditionResult {
    pub signal_type: SignalType,
    pub score: f64,
    pub detail: SignalCondition,
    pub signal_result: Option<SignalDirect>,
}

impl SignalWeightsConfig {
    // 获取特定信号类型的权重
    fn get_weight(&self, signal_type: SignalType) -> f64 {
        self.weights
            .iter()
            .find(|(st, _)| st == &signal_type)
            .map(|(_, w)| *w)
            .unwrap_or(0.0)
    }

    // 评估单个信号条件
    fn evaluate_condition(
        &self,
        signal_type: SignalType,
        condition: SignalCondition,
    ) -> Option<CheckConditionResult> {
        // 获取权重
        let base_weight = self.get_weight(signal_type);

        match condition {
            // 新增Smart Money Concepts相关条件评估
            SignalCondition::LegDetection {
                is_bullish_leg,
                is_bearish_leg,
                is_new_leg,
            } => {
                if is_new_leg {
                    let score = base_weight * 1.2; // 新腿部形成权重更高
                    if is_bullish_leg {
                        Some(CheckConditionResult {
                            signal_type,
                            score,
                            detail: condition,
                            signal_result: Some(SignalDirect::IsLong),
                        })
                    } else if is_bearish_leg {
                        Some(CheckConditionResult {
                            signal_type,
                            score,
                            detail: condition,
                            signal_result: Some(SignalDirect::IsShort),
                        })
                    } else {
                        None
                    }
                } else if is_bullish_leg {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsLong),
                    })
                } else if is_bearish_leg {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsShort),
                    })
                } else {
                    None
                }
            }
            SignalCondition::FairValueGap {
                is_bullish_fvg,
                is_bearish_fvg,
            } => {
                if is_bullish_fvg {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsLong),
                    })
                } else if is_bearish_fvg {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsShort),
                    })
                } else {
                    None
                }
            }
            SignalCondition::EqualHighLow {
                is_equal_high,
                is_equal_low,
            } => {
                if is_equal_high {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsShort), // 等高点通常是卖出信号
                    })
                } else if is_equal_low {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsLong), // 等低点通常是买入信号
                    })
                } else {
                    None
                }
            }
            SignalCondition::PremiumDiscount {
                in_premium_zone,
                in_discount_zone,
            } => {
                if in_premium_zone {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsShort), // 溢价区域是卖出信号
                    })
                } else if in_discount_zone {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsLong), // 折扣区域是买入信号
                    })
                } else {
                    None
                }
            }
            SignalCondition::Engulfing {
                is_long_signal: is_long_engulfing,
                is_short_signal: is_short_engulfing,
            } => {
                if is_long_engulfing {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsLong),
                    })
                } else if is_short_engulfing {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsShort),
                    })
                } else {
                    None
                }
            }
            SignalCondition::PriceBreakout {
                price_above,
                price_below,
            } => {
                if price_above {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsLong),
                    })
                } else if price_below {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsShort),
                    })
                } else {
                    None
                }
            }
            SignalCondition::Volume {
                is_increasing,
                ratio: _,
            } => {
                if is_increasing {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: None,
                    })
                } else {
                    None
                }
            }
            SignalCondition::RsiLevel {
                current,
                oversold,
                overbought,
                is_valid: _,
            } => {
                if current < oversold {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsLong),
                    })
                } else if current > overbought {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsShort),
                    })
                } else {
                    None
                }
            }
            SignalCondition::Trend {
                strength,
                threshold,
            } => {
                if strength > threshold {
                    let score = base_weight * strength;
                    Some(CheckConditionResult {
                        signal_type,
                        score,
                        detail: condition,
                        signal_result: None,
                    })
                } else {
                    None
                }
            }
            SignalCondition::EmaStatus { is_diverging } => {
                if is_diverging {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: None,
                    })
                } else {
                    None
                }
            }
            SignalCondition::EmaTouchTrend {
                is_long_signal,
                is_short_signal,
            } => {
                if is_long_signal {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsLong),
                    })
                } else if is_short_signal {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsShort),
                    })
                } else {
                    None
                }
            }
            SignalCondition::Bolling {
                is_long_signal,
                is_short_signal,
                is_close_signal,
            } => {
                if is_long_signal {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsLong),
                    })
                } else if is_short_signal {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsShort),
                    })
                } else if is_close_signal {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: None,
                    })
                } else {
                    None
                }
            }
            SignalCondition::KlineHammer {
                is_long_signal,
                is_short_signal,
            } => {
                if is_long_signal {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsLong),
                    })
                } else if is_short_signal {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDirect::IsShort),
                    })
                } else {
                    None
                }
            }
        }
    }

    // 计算总分
    pub fn calculate_score(
        &self,
        conditions: Vec<(SignalType, SignalCondition)>,
    ) -> SignalScoreWithDirect {
        let mut total_weight = 0.0;
        let mut details = Vec::new();
        let mut is_long_nums = 0;
        let mut is_short_nums = 0;

        // println!("conditions: {:#?}", conditions);
        for (signal_type, condition) in conditions {
            if let Some(result) = self.evaluate_condition(signal_type, condition) {
                // println!("result: {:?}", result);
                total_weight += result.score;

                if let Some(signal_result) = result.signal_result {
                    match signal_result {
                        SignalDirect::IsLong => {
                            is_long_nums += 1;
                        }
                        SignalDirect::IsShort => {
                            is_short_nums += 1;
                        }
                    }
                }
                details.push(result);
            }
        }

        SignalScoreWithDirect {
            total_weight,
            details,
            signal_result: if is_long_nums > is_short_nums {
                Some(SignalDirect::IsLong)
            } else if is_long_nums < is_short_nums {
                Some(SignalDirect::IsShort)
            } else {
                None
            },
        }
    }

    pub fn is_signal_valid(&self, score: &SignalScoreWithDirect) -> Option<SignalDirect> {
        if score.total_weight >= self.min_total_weight {
            score.signal_result
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_weights_exclude_removed_signals() {
        let market_structure = serde_json::from_str::<SignalType>("\"MarketStructure\"");
        let fake_breakout = serde_json::from_str::<SignalType>("\"FakeBreakout\"");
        assert!(market_structure.is_err());
        assert!(fake_breakout.is_err());
    }
}
