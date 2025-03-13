use hmac::digest::typenum::Min;
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
    Bollinger,              // 布林带
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub enum SignalDeriect {
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
    Bollinger {
        is_long_signal: bool,
        is_short_signal: bool,
        is_close_signal: bool,
    },
}

// 权重配置结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalWeights {
    pub weights: Vec<(SignalType, f64)>,
    pub min_total_weight: f64,
}

// 信号评分结构体
#[derive(Debug)]
pub struct SignalScoreWithDeriact {
    pub total_weight: f64,
    pub details: Vec<CheckConditionResult>,
    pub signal_result: Option<SignalDeriect>,
}

impl Default for SignalWeights {
    fn default() -> Self {
        Self {
            weights: vec![
                (SignalType::SimpleBreakEma2through, 1.0),
                (SignalType::VolumeTrend, 1.0),
                (SignalType::Rsi, 1.0),
                (SignalType::TrendStrength, 1.0),
                (SignalType::EmaDivergence, 1.0),
                (SignalType::PriceLevel, 1.0),
                (SignalType::EmaTrend, 1.0),
                (SignalType::Bollinger, 1.0),
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
    pub signal_result: Option<SignalDeriect>,
}

impl SignalWeights {
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
            SignalCondition::PriceBreakout {
                price_above,
                price_below,
            } => {
                if price_above {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDeriect::IsLong),
                    })
                } else if price_below {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDeriect::IsShort),
                    })
                } else {
                    None
                }
            }
            SignalCondition::Volume {
                is_increasing,
                ratio,
            } => {
                if is_increasing {
                    let score = base_weight * (ratio / 2.0).min(1.0);
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
            SignalCondition::RsiLevel {
                current,
                oversold,
                overbought,
                is_valid,
            } => {
                if current < oversold {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDeriect::IsLong),
                    })
                } else if current > overbought {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDeriect::IsShort),
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
                        signal_result: Some(SignalDeriect::IsLong),
                    })
                } else if is_short_signal {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDeriect::IsShort),
                    })
                } else {
                    None
                }
            }
            SignalCondition::Bollinger {
                is_long_signal,
                is_short_signal,
                is_close_signal,
            } => {
                if is_long_signal {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDeriect::IsLong),
                    })
                } else if is_short_signal {
                    Some(CheckConditionResult {
                        signal_type,
                        score: base_weight,
                        detail: condition,
                        signal_result: Some(SignalDeriect::IsShort),
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
        }
    }

    // 计算总分
    pub fn calculate_score(
        &self,
        conditions: Vec<(SignalType, SignalCondition)>,
    ) -> SignalScoreWithDeriact {
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
                        SignalDeriect::IsLong => {
                            is_long_nums += 1;
                        }
                        SignalDeriect::IsShort => {
                            is_short_nums += 1;
                        }
                    }
                }
                details.push(result);
            }
        }

        SignalScoreWithDeriact {
            total_weight,
            details,
            signal_result: if is_long_nums > is_short_nums {
                Some(SignalDeriect::IsLong)
            } else if is_long_nums < is_short_nums {
                Some(SignalDeriect::IsShort)
            } else {
                None
            },
        }
    }

    pub fn is_signal_valid(&self, score: &SignalScoreWithDeriact) -> Option<SignalDeriect> {
        if score.total_weight >= self.min_total_weight {
            score.signal_result
        } else {
            None
        }
    }
}
