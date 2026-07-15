use super::{PaFeatureId, PaStrategyKey, RuntimeModel};
use rust_quant_common::utils::function::sha256;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Feature Registry 允许的原子特征及其参数边界。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureRegistry {
    /// Registry 的人工审批版本。
    pub version: String,
    /// 每一个 primitive 的确定性参数约束。
    pub primitives: BTreeMap<String, FeaturePrimitiveSpec>,
}

/// 一个可供 DSL 使用的 primitive 声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeaturePrimitiveSpec {
    /// primitive 的最大回看长度，编译器以此阻止无界窗口。
    pub max_lookback: usize,
    /// 允许的精确参数组合；空映射代表此 primitive 没有参数。
    pub allowed_parameters: BTreeMap<String, Vec<usize>>,
    /// primitive 对应的运行时特征编号。
    pub feature: PaFeatureId,
}

/// 无字符串表达式执行能力的受限 DSL 调用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureCall {
    /// 经 registry 审批的 primitive 名称。
    pub primitive: String,
    /// 固定的正整数参数，例如 ema=20、lookback=5。
    pub parameters: BTreeMap<String, usize>,
}

/// DSL 的单个阈值条件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslCondition {
    /// 指向一个受限 primitive 调用。
    pub call: FeatureCall,
    /// 比较阈值。
    pub threshold: f64,
    /// true 表示 >=，false 表示 <=。
    pub greater_or_equal: bool,
}

/// v1 DSL 仅支持显式 AND，不支持脚本、函数调用或未来索引。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestrictedDslRule {
    /// 必须全部成立的条件。
    pub all: Vec<DslCondition>,
}

/// DSL 编译后的运行时条件，已经消除了字符串 primitive。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledDslRule {
    /// 从 registry 解析的特征条件。
    pub conditions: Vec<CompiledDslCondition>,
}

/// 一条编译完成的特征阈值条件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledDslCondition {
    /// 已批准的特征编号。
    pub feature: PaFeatureId,
    /// 冻结比较阈值。
    pub threshold: f64,
    /// true 表示 >=，false 表示 <=。
    pub greater_or_equal: bool,
}

/// 可部署的不可变运行时 manifest；它不保存训练中的可变状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeManifest {
    /// 策略标识，例如 pa_trend_15m 或 vegas_pa_meta_filter。
    pub strategy_key: String,
    /// 只增不改的策略版本。
    pub version: String,
    /// 已审批 Feature Registry 版本。
    pub feature_registry_version: String,
    /// 训练数据的指纹，用于证据回溯。
    pub dataset_fingerprint: String,
    /// 生成该 manifest 的代码 revision。
    pub code_revision: String,
    /// 冻结的运行时模型。
    pub model: RuntimeModel,
}

impl FeatureRegistry {
    /// 返回 v1 内置、人工审批的 primitives 与有限参数范围。
    pub fn v1() -> Self {
        let mut primitives = BTreeMap::new();
        primitives.insert(
            "ema_slope_atr".to_owned(),
            FeaturePrimitiveSpec {
                max_lookback: 5,
                allowed_parameters: BTreeMap::from([
                    ("ema".to_owned(), vec![20]),
                    ("lookback".to_owned(), vec![5]),
                ]),
                feature: PaFeatureId::EmaSlopeAtr20x5,
            },
        );
        primitives.insert(
            "mean_overlap_ratio".to_owned(),
            FeaturePrimitiveSpec {
                max_lookback: 8,
                allowed_parameters: BTreeMap::from([("window".to_owned(), vec![8])]),
                feature: PaFeatureId::MeanOverlapRatio8,
            },
        );
        primitives.insert(
            "pullback_depth_atr".to_owned(),
            FeaturePrimitiveSpec {
                max_lookback: 3,
                allowed_parameters: BTreeMap::from([("window".to_owned(), vec![3])]),
                feature: PaFeatureId::PullbackDepthAtr3,
            },
        );
        primitives.insert(
            "close_position".to_owned(),
            FeaturePrimitiveSpec {
                max_lookback: 1,
                allowed_parameters: BTreeMap::new(),
                feature: PaFeatureId::ClosePosition,
            },
        );
        primitives.insert(
            "range_efficiency".to_owned(),
            FeaturePrimitiveSpec {
                max_lookback: 20,
                allowed_parameters: BTreeMap::from([("window".to_owned(), vec![20])]),
                feature: PaFeatureId::RangeEfficiency20,
            },
        );
        Self {
            version: "pa-feature-registry-v1".to_owned(),
            primitives,
        }
    }

    /// 返回 v2 Registry；保留全部 v1 primitive，并加入预注册的趋势质量特征。
    pub fn v2() -> Self {
        let mut registry = Self::v1();
        registry.version = "pa-feature-registry-v2".to_owned();
        registry.primitives.insert(
            "directional_reclaim_atr".to_owned(),
            FeaturePrimitiveSpec {
                max_lookback: 1,
                allowed_parameters: BTreeMap::new(),
                feature: PaFeatureId::DirectionalReclaimAtr,
            },
        );
        registry.primitives.insert(
            "directional_close_strength".to_owned(),
            FeaturePrimitiveSpec {
                max_lookback: 1,
                allowed_parameters: BTreeMap::new(),
                feature: PaFeatureId::DirectionalCloseStrength,
            },
        );
        registry.primitives.insert(
            "signal_range_atr".to_owned(),
            FeaturePrimitiveSpec {
                max_lookback: 1,
                allowed_parameters: BTreeMap::new(),
                feature: PaFeatureId::SignalRangeAtr,
            },
        );
        registry.primitives.insert(
            "pullback_close_fraction".to_owned(),
            FeaturePrimitiveSpec {
                max_lookback: 3,
                allowed_parameters: BTreeMap::from([("window".to_owned(), vec![3])]),
                feature: PaFeatureId::PullbackCloseFraction3,
            },
        );
        registry
    }

    /// 编译 typed DSL，并拒绝未知 primitive、额外参数、未来索引和无效阈值。
    pub fn compile(&self, rule: &RestrictedDslRule) -> Result<CompiledDslRule, String> {
        if rule.all.is_empty() {
            return Err("DSL rule must contain at least one condition".to_owned());
        }
        let mut conditions = Vec::with_capacity(rule.all.len());
        for condition in &rule.all {
            if !condition.threshold.is_finite() {
                return Err("DSL threshold must be finite".to_owned());
            }
            let spec = self
                .primitives
                .get(&condition.call.primitive)
                .ok_or_else(|| {
                    format!("unknown feature primitive: {}", condition.call.primitive)
                })?;
            if condition.call.parameters.len() != spec.allowed_parameters.len() {
                return Err(format!(
                    "invalid parameter set for {}",
                    condition.call.primitive
                ));
            }
            for (name, value) in &condition.call.parameters {
                if name.contains("future") || name.contains("offset") {
                    return Err("future indexes are not allowed".to_owned());
                }
                let allowed = spec
                    .allowed_parameters
                    .get(name)
                    .ok_or_else(|| format!("unknown parameter {name}"))?;
                if !allowed.contains(value) || *value > spec.max_lookback {
                    return Err(format!("parameter {name} is outside approved range"));
                }
            }
            conditions.push(CompiledDslCondition {
                feature: spec.feature,
                threshold: condition.threshold,
                greater_or_equal: condition.greater_or_equal,
            });
        }
        Ok(CompiledDslRule { conditions })
    }
}

impl RuntimeManifest {
    /// 校验 manifest 的非空审计字段和运行时模型复杂度。
    pub fn validate(&self) -> Result<(), String> {
        if self.strategy_key.is_empty()
            || self.version.is_empty()
            || self.feature_registry_version.is_empty()
            || self.dataset_fingerprint.is_empty()
            || self.code_revision.is_empty()
        {
            return Err("manifest audit fields must not be empty".to_owned());
        }
        PaStrategyKey::parse(&self.strategy_key)?;
        self.model.validate()
    }

    /// 以规范 JSON 序列化并生成稳定的 sha256 前缀哈希。
    pub fn manifest_hash(&self) -> Result<String, String> {
        self.validate()?;
        let canonical = serde_json::to_string(self).map_err(|error| error.to_string())?;
        Ok(format!("sha256:{}", sha256(&canonical)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::implementations::pa_quant_tree::{FeatureThreshold, RuntimeModel};

    fn manifest() -> RuntimeManifest {
        RuntimeManifest {
            strategy_key: "pa_trend_15m".to_owned(),
            version: "1.0.0".to_owned(),
            feature_registry_version: "pa-feature-registry-v1".to_owned(),
            dataset_fingerprint: "dataset".to_owned(),
            code_revision: "revision".to_owned(),
            model: RuntimeModel::FixedRules {
                rules: vec![FeatureThreshold {
                    feature: PaFeatureId::SignalBodyRatio,
                    threshold: 0.2,
                    greater_or_equal: true,
                }],
            },
        }
    }

    #[test]
    fn manifest_hash_is_stable() {
        assert_eq!(
            manifest().manifest_hash().unwrap(),
            manifest().manifest_hash().unwrap()
        );
    }

    #[test]
    fn compiler_rejects_unknown_primitive_and_unapproved_parameters() {
        let registry = FeatureRegistry::v1();
        let unknown = RestrictedDslRule {
            all: vec![DslCondition {
                call: FeatureCall {
                    primitive: "future_close".to_owned(),
                    parameters: BTreeMap::new(),
                },
                threshold: 1.0,
                greater_or_equal: true,
            }],
        };
        assert!(registry.compile(&unknown).is_err());
        let invalid = RestrictedDslRule {
            all: vec![DslCondition {
                call: FeatureCall {
                    primitive: "ema_slope_atr".to_owned(),
                    parameters: BTreeMap::from([
                        ("ema".to_owned(), 20),
                        ("lookback".to_owned(), 6),
                    ]),
                },
                threshold: 0.0,
                greater_or_equal: true,
            }],
        };
        assert!(registry.compile(&invalid).is_err());
    }

    #[test]
    fn v2_registry_compiles_preregistered_trend_quality_features() {
        let registry = FeatureRegistry::v2();
        let compiled = registry
            .compile(&RestrictedDslRule {
                all: vec![DslCondition {
                    call: FeatureCall {
                        primitive: "pullback_close_fraction".to_owned(),
                        parameters: BTreeMap::from([("window".to_owned(), 3)]),
                    },
                    threshold: 1.0 / 3.0,
                    greater_or_equal: true,
                }],
            })
            .unwrap();
        assert_eq!(registry.version, "pa-feature-registry-v2");
        assert_eq!(
            compiled.conditions[0].feature,
            PaFeatureId::PullbackCloseFraction3
        );
    }

    #[test]
    fn manifest_rejects_unapproved_strategy_key() {
        let mut invalid = manifest();
        invalid.strategy_key = "pa_unreviewed_15m".to_owned();
        assert!(invalid.validate().is_err());
    }
}
