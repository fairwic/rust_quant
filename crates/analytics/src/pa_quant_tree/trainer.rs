use super::{calculate_metrics, validate_temporal_purge, ResearchDataset, WalkForwardPlan};
use rust_quant_strategies::implementations::pa_quant_tree::{
    CartNode, FeatureThreshold, PaFeatureId, RuntimeManifest, RuntimeModel,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MODEL_FEATURES: [PaFeatureId; 10] = [
    PaFeatureId::EmaSlopeAtr20x5,
    PaFeatureId::RangeEfficiency20,
    PaFeatureId::MeanOverlapRatio8,
    PaFeatureId::AlwaysInScore,
    PaFeatureId::SignalBodyRatio,
    PaFeatureId::PullbackDepthAtr3,
    PaFeatureId::DirectionalReclaimAtr,
    PaFeatureId::DirectionalCloseStrength,
    PaFeatureId::SignalRangeAtr,
    PaFeatureId::PullbackCloseFraction3,
];

/// 统一竞赛中的模型家族；所有候选都使用同一时间切分和净 R 标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFamily {
    /// M0：不做过滤。
    NoFilter,
    /// M1：人工冻结 PA 规则。
    FixedRules,
    /// M2：正则化逻辑回归。
    LogisticRegression,
    /// M3：代价复杂度剪枝 CART。
    Cart,
    /// M4：受限小型树集成。
    Forest,
}

/// 一个验证候选的折外决策；只用于研究统计，不进入运行时 Manifest。
#[derive(Debug, Clone, PartialEq)]
pub struct ModelOofDecision {
    /// 产生该决策的 walk-forward 验证折序号。
    pub fold_index: usize,
    /// 候选唯一审计 ID，用于回连两倍成本和组合路径。
    pub candidate_id: String,
    /// 候选信号时间，Unix 毫秒时间戳。
    pub signal_ts: i64,
    /// Core 统一交易对标识，用于分市场和共享时间块统计。
    pub symbol: String,
    /// true 表示仅由本折训练模型保留；false 表示折外拒绝。
    pub keep: bool,
    /// 候选基础成本后 R，只能在决策生成后用于验证统计。
    pub net_r: f64,
}

/// 一次模型竞赛的候选及其验证统计。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelTournamentEntry {
    /// 模型家族。
    pub family: ModelFamily,
    /// 可被冻结到 manifest 的运行时模型。
    pub model: RuntimeModel,
    /// 验证集成本后平均 R。
    pub mean_net_r: f64,
    /// 验证集均值的标准误。
    pub standard_error: f64,
    /// 模型复杂度，one-standard-error 规则优先更低值。
    pub complexity: usize,
    /// 验证窗口中被模型保留的已结算候选数。
    pub kept_trade_count: usize,
    /// 验证窗口原始候选总数。
    pub validation_candidate_count: usize,
    /// 保留候选占验证候选的比例。
    pub coverage: f64,
    /// 是否满足至少 30 笔和 10% 覆盖率的模型竞赛门槛。
    pub eligible: bool,
    /// 每个验证候选的折外决策。
    ///
    /// 该字段不序列化，避免改变冻结的 legacy v4/v5 JSON；新评估协议只输出聚合证据。
    #[serde(skip)]
    pub oof_decisions: Vec<ModelOofDecision>,
}

/// 训练输出；只有此结果被显式冻结后才能构造 Challenger manifest。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengerTrainingResult {
    /// 统一竞赛的全部模型，失败或落后项也必须保留。
    pub entries: Vec<ModelTournamentEntry>,
    /// 根据 one-standard-error 规则选择的模型下标。
    pub selected_index: usize,
    /// 训练数据的可审计指纹。
    pub dataset_fingerprint: String,
}

/// 基于已结算样本构建 M0 到 M4 的确定性 Challenger；绝不读取密封 OOS。
pub fn train_challenger(
    dataset: &ResearchDataset,
    plan: &WalkForwardPlan,
) -> Result<ChallengerTrainingResult, String> {
    if dataset.sealed_oos_opened {
        return Err("opened sealed OOS cannot be reused for challenger training".to_owned());
    }
    let windows = plan.build(dataset)?;
    if windows.is_empty() {
        return Err("insufficient observations for walk-forward training".to_owned());
    }
    let entries = [
        ModelFamily::NoFilter,
        ModelFamily::FixedRules,
        ModelFamily::LogisticRegression,
        ModelFamily::Cart,
        ModelFamily::Forest,
    ]
    .into_iter()
    .map(|family| evaluate(family, dataset, &windows))
    .collect::<Result<Vec<_>, _>>()?;
    let selected_index = select_one_standard_error(&entries)?;
    Ok(ChallengerTrainingResult {
        entries,
        selected_index,
        dataset_fingerprint: dataset.fingerprint()?,
    })
}

/// 根据训练结果构造未晋级的不可变 Challenger manifest。
pub fn freeze_challenger_manifest(
    result: &ChallengerTrainingResult,
    strategy_key: String,
    version: String,
    code_revision: String,
) -> Result<RuntimeManifest, String> {
    let entry = result
        .entries
        .get(result.selected_index)
        .ok_or_else(|| "selected model index is invalid".to_owned())?;
    let manifest = RuntimeManifest {
        strategy_key,
        version,
        feature_registry_version: "pa-feature-registry-v2".to_owned(),
        dataset_fingerprint: result.dataset_fingerprint.clone(),
        code_revision,
        model: entry.model.clone(),
    };
    manifest.validate()?;
    Ok(manifest)
}

/// 实现 one-standard-error：先找最高均值，再在其一个标准误以内选择最简单模型。
pub fn select_one_standard_error(entries: &[ModelTournamentEntry]) -> Result<usize, String> {
    let best = entries
        .iter()
        .filter(|entry| entry.eligible)
        .max_by(|left, right| left.mean_net_r.total_cmp(&right.mean_net_r))
        .ok_or_else(|| "model tournament has no coverage-eligible model".to_owned())?;
    let floor = best.mean_net_r - best.standard_error;
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.eligible && entry.mean_net_r >= floor)
        .min_by(|(_, left), (_, right)| {
            left.complexity
                .cmp(&right.complexity)
                .then_with(|| right.mean_net_r.total_cmp(&left.mean_net_r))
        })
        .map(|(index, _)| index)
        .ok_or_else(|| "no eligible model".to_owned())
}

fn evaluate(
    family: ModelFamily,
    dataset: &ResearchDataset,
    windows: &[super::WalkForwardWindow],
) -> Result<ModelTournamentEntry, String> {
    let mut returns = Vec::new();
    let mut oof_decisions = Vec::new();
    for (fold_index, window) in windows.iter().enumerate() {
        validate_temporal_purge(dataset, window)?;
        // 每一折只用该折训练段推导阈值与模型参数，验证段完全不可见。
        let fold_model = build_model(
            family,
            &dataset.observations[window.train_start..window.train_end],
        );
        for item in &dataset.observations[window.validation_start..window.validation_end] {
            let keep = fold_model.evaluate(&item.features).keep;
            oof_decisions.push(ModelOofDecision {
                fold_index,
                candidate_id: item.candidate_id.clone(),
                signal_ts: item.signal_ts,
                symbol: item.symbol.clone(),
                keep,
                net_r: item.net_r,
            });
            if keep {
                returns.push(item.net_r);
            }
        }
    }
    let metrics = calculate_metrics(&returns);
    let validation_candidate_count = windows
        .iter()
        .map(|window| window.validation_end - window.validation_start)
        .sum::<usize>();
    let coverage = returns.len() as f64 / validation_candidate_count.max(1) as f64;
    let variance = if returns.len() < 2 {
        0.0
    } else {
        returns
            .iter()
            .map(|value| (value - metrics.mean_net_r).powi(2))
            .sum::<f64>()
            / (returns.len() - 1) as f64
    };
    Ok(ModelTournamentEntry {
        family,
        // 评估完成后，才在全部已授权训练数据上重新冻结入选家族。
        model: build_model(family, &dataset.observations),
        mean_net_r: metrics.mean_net_r,
        standard_error: (variance / returns.len().max(1) as f64).sqrt(),
        complexity: complexity(family),
        kept_trade_count: returns.len(),
        validation_candidate_count,
        coverage,
        eligible: returns.len() >= 30 && coverage >= 0.10,
        oof_decisions,
    })
}

fn build_model(family: ModelFamily, observations: &[super::ResearchObservation]) -> RuntimeModel {
    match family {
        ModelFamily::NoFilter => RuntimeModel::FixedRules { rules: vec![] },
        // M1 是人工固定基线，不能从本轮样本学习阈值。
        ModelFamily::FixedRules => RuntimeModel::FixedRules {
            rules: vec![FeatureThreshold {
                feature: PaFeatureId::SignalBodyRatio,
                threshold: 0.20,
                greater_or_equal: true,
            }],
        },
        ModelFamily::LogisticRegression => fit_logistic(observations),
        ModelFamily::Cart => RuntimeModel::Cart {
            root: best_stump(observations),
        },
        ModelFamily::Forest => RuntimeModel::Forest {
            trees: vec![
                best_stump(observations),
                best_stump_for_feature(observations, PaFeatureId::DirectionalReclaimAtr),
                best_stump_for_feature(observations, PaFeatureId::SignalRangeAtr),
            ],
            threshold: 0.5,
        },
    }
}

fn complexity(family: ModelFamily) -> usize {
    match family {
        ModelFamily::NoFilter => 0,
        ModelFamily::FixedRules => 1,
        ModelFamily::LogisticRegression | ModelFamily::Cart => 2,
        ModelFamily::Forest => 6,
    }
}

fn fit_logistic(observations: &[super::ResearchObservation]) -> RuntimeModel {
    let features = MODEL_FEATURES;
    let mut intercept = 0.0;
    let mut weights = vec![0.0; features.len()];
    // 固定迭代次数、步长与 L2 系数保证离线训练可复现。
    for _ in 0..200 {
        let mut intercept_gradient = 0.0;
        let mut gradients = vec![0.0; features.len()];
        for observation in observations {
            let linear = intercept
                + features
                    .iter()
                    .zip(weights.iter())
                    .map(|(feature, weight)| feature.value(&observation.features) * *weight)
                    .sum::<f64>();
            let probability = 1.0 / (1.0 + (-linear).exp());
            let label = if observation.net_r > 0.0 { 1.0 } else { 0.0 };
            let error = probability - label;
            intercept_gradient += error;
            for (index, feature) in features.iter().enumerate() {
                gradients[index] += error * feature.value(&observation.features);
            }
        }
        let sample_count = observations.len().max(1) as f64;
        intercept -= 0.1 * intercept_gradient / sample_count;
        for index in 0..weights.len() {
            weights[index] -= 0.1 * (gradients[index] / sample_count + 0.05 * weights[index]);
        }
    }
    let scored_returns = observations
        .iter()
        .map(|observation| {
            let linear = intercept
                + features
                    .iter()
                    .zip(weights.iter())
                    .map(|(feature, weight)| feature.value(&observation.features) * *weight)
                    .sum::<f64>();
            (1.0 / (1.0 + (-linear).exp()), observation.net_r)
        })
        .collect::<Vec<_>>();
    RuntimeModel::LogisticRegression {
        intercept,
        weights: features
            .into_iter()
            .zip(weights)
            .collect::<BTreeMap<_, _>>(),
        threshold: select_expected_r_threshold(&scored_returns),
    }
}

/// 仅用训练折选择成本后平均 R 最高的概率阈值，并限制最小有效样本量。
fn select_expected_r_threshold(scored_returns: &[(f64, f64)]) -> f64 {
    if scored_returns.is_empty() {
        return 0.5;
    }
    let minimum_kept = (scored_returns.len() / 10)
        .max(50)
        .min(scored_returns.len());
    let mut thresholds = scored_returns
        .iter()
        .map(|(score, _)| *score)
        .collect::<Vec<_>>();
    thresholds.sort_by(f64::total_cmp);
    thresholds.dedup_by(|left, right| left.total_cmp(right).is_eq());
    thresholds
        .into_iter()
        .filter_map(|threshold| {
            let kept = scored_returns
                .iter()
                .filter(|(score, _)| *score >= threshold)
                .collect::<Vec<_>>();
            (kept.len() >= minimum_kept).then(|| {
                let mean_net_r =
                    kept.iter().map(|(_, net_r)| *net_r).sum::<f64>() / kept.len() as f64;
                (threshold, mean_net_r, kept.len())
            })
        })
        .max_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| right.0.total_cmp(&left.0))
        })
        .map(|(threshold, _, _)| threshold)
        .unwrap_or(0.5)
}

fn best_stump(observations: &[super::ResearchObservation]) -> CartNode {
    MODEL_FEATURES
        .into_iter()
        .map(|feature| best_stump_for_feature(observations, feature))
        .max_by(|left, right| {
            stump_value(left, observations).total_cmp(&stump_value(right, observations))
        })
        .unwrap_or(CartNode::Leaf {
            keep: false,
            probability: 0.0,
        })
}

fn best_stump_for_feature(
    observations: &[super::ResearchObservation],
    feature: PaFeatureId,
) -> CartNode {
    let threshold = median(
        observations
            .iter()
            .map(|item| feature.value(&item.features))
            .collect(),
    );
    let minimum_leaf_size = (observations.len() / 20).max(50);
    let left: Vec<_> = observations
        .iter()
        .filter(|item| feature.value(&item.features) <= threshold)
        .collect();
    let right: Vec<_> = observations
        .iter()
        .filter(|item| feature.value(&item.features) > threshold)
        .collect();
    if left.len() < minimum_leaf_size || right.len() < minimum_leaf_size {
        return CartNode::Leaf {
            keep: mean_net_r(observations) > 0.0,
            probability: win_probability(observations),
        };
    }
    CartNode::Split {
        feature,
        threshold,
        less_or_equal: Box::new(CartNode::Leaf {
            keep: mean_net_r(&left) > 0.0,
            probability: win_probability(&left),
        }),
        greater: Box::new(CartNode::Leaf {
            keep: mean_net_r(&right) > 0.0,
            probability: win_probability(&right),
        }),
    }
}

fn stump_value(root: &CartNode, observations: &[super::ResearchObservation]) -> f64 {
    observations
        .iter()
        .filter(|item| root.evaluate(&item.features).keep)
        .map(|item| item.net_r)
        .sum()
}

fn mean_net_r(items: &[impl std::borrow::Borrow<super::ResearchObservation>]) -> f64 {
    if items.is_empty() {
        0.0
    } else {
        items.iter().map(|item| item.borrow().net_r).sum::<f64>() / items.len() as f64
    }
}

fn win_probability(items: &[impl std::borrow::Borrow<super::ResearchObservation>]) -> f64 {
    if items.is_empty() {
        0.0
    } else {
        items
            .iter()
            .filter(|item| item.borrow().net_r > 0.0)
            .count() as f64
            / items.len() as f64
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|left, right| left.total_cmp(right));
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pa_quant_tree::{ResearchObservation, WalkForwardWindow};
    use rust_quant_strategies::implementations::pa_quant_tree::{
        PaDirection, PaFeatureSnapshot, PaMarketRegime,
    };

    /// 构造覆盖全部 v2 特征的最小训练样本，防止模型维度与梯度维度再次脱节。
    fn observation(candidate_id: &str, net_r: f64) -> ResearchObservation {
        ResearchObservation {
            signal_ts: 1,
            settled_ts: 2,
            symbol: "BTC-USDT-SWAP".to_owned(),
            strategy_key: "pa_trend_15m".to_owned(),
            strategy_version: "1.0.0-research".to_owned(),
            manifest_hash: "sha256:test".to_owned(),
            candidate_id: candidate_id.to_owned(),
            features: PaFeatureSnapshot {
                signal_ts: 1,
                atr14: 1.0,
                ema20: 100.0,
                ema_slope_atr_20_5: 0.2,
                range_efficiency_20: 0.5,
                range_high_20: 102.0,
                range_low_20: 98.0,
                range_position_20: 0.6,
                mean_overlap_ratio_8: 0.3,
                always_in_score: 0.8,
                signal_body_ratio: 0.6,
                close_position: 0.8,
                pullback_depth_atr_3: 0.4,
                directional_reclaim_atr: 0.3,
                directional_close_strength: 0.8,
                signal_range_atr: 1.1,
                pullback_close_fraction_3: 1.0 / 3.0,
                recent_ema_touch: true,
                regime: PaMarketRegime::Trend,
                trend_direction: Some(PaDirection::Long),
            },
            net_r,
            vegas_baseline_net_r: None,
        }
    }

    #[test]
    fn one_standard_error_prefers_simpler_eligible_model() {
        let no_filter = ModelTournamentEntry {
            family: ModelFamily::NoFilter,
            model: RuntimeModel::FixedRules { rules: vec![] },
            mean_net_r: 0.10,
            standard_error: 0.01,
            complexity: 0,
            kept_trade_count: 100,
            validation_candidate_count: 100,
            coverage: 1.0,
            eligible: true,
            oof_decisions: vec![],
        };
        let forest = ModelTournamentEntry {
            family: ModelFamily::Forest,
            model: RuntimeModel::Forest {
                trees: vec![CartNode::Leaf {
                    keep: true,
                    probability: 0.8,
                }],
                threshold: 0.5,
            },
            mean_net_r: 0.105,
            standard_error: 0.01,
            complexity: 4,
            kept_trade_count: 50,
            validation_candidate_count: 100,
            coverage: 0.5,
            eligible: true,
            oof_decisions: vec![],
        };
        assert_eq!(select_one_standard_error(&[no_filter, forest]).unwrap(), 0);
    }

    #[test]
    fn model_that_rejects_every_candidate_is_not_selectable() {
        let all_rejected = ModelTournamentEntry {
            family: ModelFamily::Cart,
            model: RuntimeModel::Cart {
                root: CartNode::Leaf {
                    keep: false,
                    probability: 0.0,
                },
            },
            mean_net_r: 0.0,
            standard_error: 0.0,
            complexity: 1,
            kept_trade_count: 0,
            validation_candidate_count: 100,
            coverage: 0.0,
            eligible: false,
            oof_decisions: vec![],
        };
        assert!(select_one_standard_error(&[all_rejected]).is_err());
    }

    #[test]
    fn expected_r_threshold_can_keep_positive_payoff_below_fifty_percent_win_rate() {
        let mut scored_returns = vec![(0.20, -1.0); 40];
        scored_returns.extend((0..60).map(|index| {
            let net_r = if index < 25 { 1.5 } else { -1.0 };
            (0.42, net_r)
        }));

        let threshold = select_expected_r_threshold(&scored_returns);
        let kept = scored_returns
            .iter()
            .filter(|(score, _)| *score >= threshold)
            .collect::<Vec<_>>();
        let mean_net_r = kept.iter().map(|(_, net_r)| *net_r).sum::<f64>() / kept.len() as f64;
        let win_rate =
            kept.iter().filter(|(_, net_r)| *net_r > 0.0).count() as f64 / kept.len() as f64;

        assert_eq!(threshold, 0.42);
        assert_eq!(kept.len(), 60);
        assert!(mean_net_r > 0.0);
        assert!(win_rate < 0.5);
    }

    #[test]
    fn logistic_fit_uses_every_registered_model_feature_without_dimension_mismatch() {
        let model = fit_logistic(&[observation("winner", 1.5), observation("loser", -1.0)]);
        let RuntimeModel::LogisticRegression { weights, .. } = model else {
            panic!("fit_logistic must return logistic regression");
        };
        assert_eq!(weights.len(), MODEL_FEATURES.len());
        assert!(MODEL_FEATURES
            .iter()
            .all(|feature| weights.contains_key(feature)));
    }

    #[test]
    fn no_filter_records_every_validation_candidate_as_oof_keep() {
        let observations = (0..100)
            .map(|index| {
                let mut item = observation(&format!("candidate-{index}"), 0.2);
                item.signal_ts = (index as i64 + 1) * 10;
                item.settled_ts = item.signal_ts + 1;
                item.features.signal_ts = item.signal_ts;
                item
            })
            .collect::<Vec<_>>();
        let dataset = ResearchDataset::new(observations).unwrap();
        let entry = evaluate(
            ModelFamily::NoFilter,
            &dataset,
            &[WalkForwardWindow {
                train_start: 0,
                train_end: 60,
                validation_start: 65,
                validation_end: 95,
            }],
        )
        .unwrap();

        assert_eq!(entry.oof_decisions.len(), 30);
        assert_eq!(entry.kept_trade_count, 30);
        assert!(entry.oof_decisions.iter().all(|decision| decision.keep));
        assert_eq!(entry.oof_decisions[0].fold_index, 0);
        assert_eq!(entry.oof_decisions[0].candidate_id, "candidate-65");
        assert!(!serde_json::to_string(&entry)
            .unwrap()
            .contains("oof_decisions"));
    }
}
