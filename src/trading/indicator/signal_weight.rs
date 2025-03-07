use serde::{Deserialize, Serialize};

// 信号类型枚举
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub enum SignalType {
    Breakthrough,    // 突破信号
    VolumeTrend,    // 成交量趋势
    EmaTrend,       // ema趋势
    Rsi,            // RSI指标
    TrendStrength,  // 趋势强度
    EmaDivergence,  // 均线发散
    PriceLevel,     // 关键价位
}

// 信号条件枚举
#[derive(Debug,Clone, Copy, Deserialize, Serialize)]
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
    },
    Trend {
        strength: f64,
        threshold: f64,
    },
    EmaStatus {
        is_diverging: bool,
    },
    EmaTrend {
        is_long_signal: bool,
        is_short_signal: bool,
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
pub struct SignalScore {
    pub total_weight: f64,
    pub details: Vec<String>,
}

impl Default for SignalWeights {
    fn default() -> Self {
        Self {
            weights: vec![
                (SignalType::Breakthrough, 1.0),
                (SignalType::VolumeTrend, 1.0),
                (SignalType::Rsi, 1.0),
                (SignalType::TrendStrength, 1.0),
                (SignalType::EmaDivergence, 1.0),
                (SignalType::PriceLevel, 1.0),
                (SignalType::EmaTrend, 1.0),
            ],
            min_total_weight: 2.0,
        }
    }
}

impl SignalWeights {
    // 获取特定信号类型的权重
    fn get_weight(&self, signal_type: SignalType) -> f64 {
        self.weights.iter()
            .find(|(st, _)| st == &signal_type)
            .map(|(_, w)| *w)
            .unwrap_or(0.0)
    }

    // 评估单个信号条件
    fn evaluate_condition(&self, signal_type: SignalType, condition: SignalCondition) -> Option<(f64, String)> {
        let base_weight = self.get_weight(signal_type);

        match condition {
            SignalCondition::PriceBreakout { price_above, price_below } => {
                if price_above || price_below {
                    Some((base_weight, format!("突破信号 +{:.1}", base_weight)))
                } else {
                    None
                }
            },
            SignalCondition::Volume { is_increasing, ratio } => {
                if is_increasing {
                    let score = base_weight * (ratio / 2.0).min(1.0);
                    Some((score, format!("成交量放大 +{:.1}", score)))
                } else {
                    None
                }
            },
            SignalCondition::RsiLevel { current, oversold, overbought } => {
                if current < oversold || current > overbought {
                    Some((base_weight, format!("RSI信号 +{:.1}", base_weight)))
                } else {
                    None
                }
            },
            SignalCondition::Trend { strength, threshold } => {
                if strength > threshold {
                    let score = base_weight * strength;
                    Some((score, format!("趋势强度 +{:.1}", score)))
                } else {
                    None
                }
            },
            SignalCondition::EmaStatus { is_diverging } => {
                if is_diverging {
                    Some((base_weight, format!("均线发散 +{:.1}", base_weight)))
                } else {
                    None
                }
            },
            SignalCondition::EmaTrend { is_long_signal, is_short_signal } => {
                if is_long_signal || is_short_signal {
                    Some((base_weight, format!("ema趋势 +{:.1}", base_weight)))
                } else {
                    None
                }
            },
        }
    }

    // 计算总分
    pub fn calculate_score(&self, conditions: Vec<(SignalType, SignalCondition)>) -> SignalScore {
        let mut total_weight = 0.0;
        let mut details = Vec::new();

        for (signal_type, condition) in conditions {
            if let Some((weight, detail)) = self.evaluate_condition(signal_type, condition) {
                total_weight += weight;
                details.push(detail);
            }
        }

        SignalScore {
            total_weight,
            details,
        }
    }

    pub fn is_signal_valid(&self, score: &SignalScore) -> bool {
        score.total_weight >= self.min_total_weight
    }
} 