use super::PaFeatureSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 已批准的运行时特征名称；模型不能引用注册表外的值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaFeatureId {
    /// EMA20 五棒 ATR 归一化斜率。
    EmaSlopeAtr20x5,
    /// 最近八棒平均重叠比例。
    MeanOverlapRatio8,
    /// 最近三棒的方向化回撤深度。
    PullbackDepthAtr3,
    /// 当前收盘在 K 线中的位置。
    ClosePosition,
    /// 最近二十棒的方向效率。
    RangeEfficiency20,
    /// 最近十棒的 Always In 同侧比例。
    AlwaysInScore,
    /// 信号棒实体占比。
    SignalBodyRatio,
    /// 当前收盘在 20 棒区间的位置。
    RangePosition20,
    /// 信号收盘越过 EMA20 的方向化 ATR 距离。
    DirectionalReclaimAtr,
    /// 信号收盘靠近趋势方向端点的比例。
    DirectionalCloseStrength,
    /// 信号棒全长除以 ATR14。
    SignalRangeAtr,
    /// 最近三棒中收盘位于趋势反侧的比例。
    PullbackCloseFraction3,
}

impl PaFeatureId {
    /// 读取特征快照中的确定性数值。
    pub fn value(self, features: &PaFeatureSnapshot) -> f64 {
        match self {
            Self::EmaSlopeAtr20x5 => features.ema_slope_atr_20_5,
            Self::MeanOverlapRatio8 => features.mean_overlap_ratio_8,
            Self::PullbackDepthAtr3 => features.pullback_depth_atr_3,
            Self::ClosePosition => features.close_position,
            Self::RangeEfficiency20 => features.range_efficiency_20,
            Self::AlwaysInScore => features.always_in_score,
            Self::SignalBodyRatio => features.signal_body_ratio,
            Self::RangePosition20 => features.range_position_20,
            Self::DirectionalReclaimAtr => features.directional_reclaim_atr,
            Self::DirectionalCloseStrength => features.directional_close_strength,
            Self::SignalRangeAtr => features.signal_range_atr,
            Self::PullbackCloseFraction3 => features.pullback_close_fraction_3,
        }
    }
}

/// 运行时模型的二元输出，评分只用于审计与过滤。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelDecision {
    /// 候选被模型保留时为 true。
    pub keep: bool,
    /// 冻结模型的概率或规则置信度。
    pub score: f64,
}

/// 不可变模型的受限表达，不支持任意脚本或在线训练。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RuntimeModel {
    /// 固定规则基线，分数为通过条件的比例。
    FixedRules { rules: Vec<FeatureThreshold> },
    /// 正则化逻辑回归的冻结参数。
    LogisticRegression {
        intercept: f64,
        weights: BTreeMap<PaFeatureId, f64>,
        threshold: f64,
    },
    /// 代价复杂度剪枝后冻结的 CART。
    Cart { root: CartNode },
    /// 小型树集成挑战者，使用平均叶节点概率，不支持在线增量学习。
    Forest {
        trees: Vec<CartNode>,
        threshold: f64,
    },
}

/// 一个单变量比较条件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureThreshold {
    /// 被比较的已批准特征。
    pub feature: PaFeatureId,
    /// 阈值。
    pub threshold: f64,
    /// true 表示特征必须大于等于阈值，false 表示必须小于等于阈值。
    pub greater_or_equal: bool,
}

/// CART 的递归节点；树只读取已冻结特征并无副作用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "node")]
pub enum CartNode {
    /// 叶节点输出保留决定和校准概率。
    Leaf { keep: bool, probability: f64 },
    /// 按特征阈值分裂，左侧代表小于等于阈值。
    Split {
        feature: PaFeatureId,
        threshold: f64,
        less_or_equal: Box<CartNode>,
        greater: Box<CartNode>,
    },
}

impl RuntimeModel {
    /// 对一个特征快照做纯函数推理，结果由 manifest 完全确定。
    pub fn evaluate(&self, features: &PaFeatureSnapshot) -> ModelDecision {
        match self {
            Self::FixedRules { rules } => {
                let passed = rules.iter().filter(|rule| rule.matches(features)).count();
                let score = if rules.is_empty() {
                    1.0
                } else {
                    passed as f64 / rules.len() as f64
                };
                ModelDecision {
                    keep: passed == rules.len(),
                    score,
                }
            }
            Self::LogisticRegression {
                intercept,
                weights,
                threshold,
            } => {
                let linear = weights.iter().fold(*intercept, |sum, (feature, weight)| {
                    sum + feature.value(features) * weight
                });
                let score = 1.0 / (1.0 + (-linear).exp());
                ModelDecision {
                    keep: score >= *threshold,
                    score,
                }
            }
            Self::Cart { root } => root.evaluate(features),
            Self::Forest { trees, threshold } => {
                let score = trees
                    .iter()
                    .map(|tree| tree.evaluate(features).score)
                    .sum::<f64>()
                    / trees.len().max(1) as f64;
                ModelDecision {
                    keep: score >= *threshold,
                    score,
                }
            }
        }
    }

    /// 校验模型数值、树深度和叶子数，使不受控复杂度无法进入运行时。
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::FixedRules { rules } => {
                if rules.iter().any(|rule| !rule.threshold.is_finite()) {
                    Err("fixed rule threshold is not finite".to_owned())
                } else {
                    Ok(())
                }
            }
            Self::LogisticRegression {
                intercept,
                weights,
                threshold,
            } => {
                if !intercept.is_finite()
                    || !threshold.is_finite()
                    || !(0.0..=1.0).contains(threshold)
                    || weights.values().any(|weight| !weight.is_finite())
                {
                    Err("invalid logistic regression parameters".to_owned())
                } else {
                    Ok(())
                }
            }
            Self::Cart { root } => {
                if root.depth() > 6 || root.leaf_count() > 64 || !root.is_valid() {
                    Err("invalid CART shape or probability".to_owned())
                } else {
                    Ok(())
                }
            }
            Self::Forest { trees, threshold } => {
                if trees.is_empty()
                    || trees.len() > 16
                    || !threshold.is_finite()
                    || !(0.0..=1.0).contains(threshold)
                    || trees
                        .iter()
                        .any(|tree| tree.depth() > 6 || tree.leaf_count() > 64 || !tree.is_valid())
                {
                    Err("invalid forest shape or threshold".to_owned())
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl FeatureThreshold {
    /// 判断特征是否满足冻结比较。
    pub fn matches(&self, features: &PaFeatureSnapshot) -> bool {
        let value = self.feature.value(features);
        if self.greater_or_equal {
            value >= self.threshold
        } else {
            value <= self.threshold
        }
    }
}

impl CartNode {
    /// 递归计算 CART 的叶节点输出。
    pub fn evaluate(&self, features: &PaFeatureSnapshot) -> ModelDecision {
        match self {
            Self::Leaf { keep, probability } => ModelDecision {
                keep: *keep,
                score: *probability,
            },
            Self::Split {
                feature,
                threshold,
                less_or_equal,
                greater,
            } => {
                if feature.value(features) <= *threshold {
                    less_or_equal.evaluate(features)
                } else {
                    greater.evaluate(features)
                }
            }
        }
    }

    /// 返回树深度，叶节点深度为零。
    pub fn depth(&self) -> usize {
        match self {
            Self::Leaf { .. } => 0,
            Self::Split {
                less_or_equal,
                greater,
                ..
            } => 1 + less_or_equal.depth().max(greater.depth()),
        }
    }

    /// 返回叶节点数量，用于运行时复杂度限制。
    pub fn leaf_count(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Split {
                less_or_equal,
                greater,
                ..
            } => less_or_equal.leaf_count() + greater.leaf_count(),
        }
    }

    fn is_valid(&self) -> bool {
        match self {
            Self::Leaf { probability, .. } => {
                probability.is_finite() && (0.0..=1.0).contains(probability)
            }
            Self::Split {
                threshold,
                less_or_equal,
                greater,
                ..
            } => threshold.is_finite() && less_or_equal.is_valid() && greater.is_valid(),
        }
    }
}
