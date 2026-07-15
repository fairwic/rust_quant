use super::{
    PairedPathDelta, PerformanceMetrics, PortfolioReplay, ResearchDataset, WalkForwardPlan,
};
use rust_quant_strategies::implementations::pa_quant_tree::RuntimeManifest;
use serde::{Deserialize, Serialize};

/// 研究结果所属阶段；密封 OOS 与 Forward Paper 不能被混同为训练数据。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStage {
    /// 训练或 walk-forward 期间的候选比较。
    WalkForward,
    /// 只允许正式打开一次的密封样本。
    SealedOos,
    /// manifest 冻结后的未来 Paper 观察。
    ForwardPaper,
}

/// 一次研究评估的不可变证据清单，供 Promote、回滚和审计引用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchEvidenceManifest {
    /// 本次证据所属阶段，决定数据可否再次进入训练。
    pub stage: EvidenceStage,
    /// 运行时 manifest 的稳定哈希。
    pub runtime_manifest_hash: String,
    /// 训练或评估数据的稳定指纹。
    pub dataset_fingerprint: String,
    /// 生成证据的代码 revision。
    pub code_revision: String,
    /// 完整保留的实验 ID，包含失败实验。
    pub experiment_ids: Vec<String>,
    /// 使用的 Purged walk-forward 配置；Paper/OOS 也记录来源计划。
    pub walk_forward_plan: WalkForwardPlan,
    /// 成本后策略或过滤路径指标。
    pub performance: PerformanceMetrics,
    /// Vegas Meta-filter 的同候选配对增量；独立 PA 策略为空。
    pub paired_vegas_delta: Option<PairedPathDelta>,
    /// 按共享权益回放得到的组合结果。
    pub portfolio: PortfolioReplay,
}

impl ResearchEvidenceManifest {
    /// 从冻结 runtime manifest、研究数据与组合回放创建可审计证据，并验证关联一致性。
    pub fn new(
        stage: EvidenceStage,
        runtime_manifest: &RuntimeManifest,
        dataset: &ResearchDataset,
        code_revision: String,
        experiment_ids: Vec<String>,
        walk_forward_plan: WalkForwardPlan,
        performance: PerformanceMetrics,
        paired_vegas_delta: Option<PairedPathDelta>,
        portfolio: PortfolioReplay,
    ) -> Result<Self, String> {
        let evidence = Self {
            stage,
            runtime_manifest_hash: runtime_manifest.manifest_hash()?,
            dataset_fingerprint: dataset.fingerprint()?,
            code_revision,
            experiment_ids,
            walk_forward_plan,
            performance,
            paired_vegas_delta,
            portfolio,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    /// 验证证据身份、样本计数和组合权益的基本可审计约束。
    pub fn validate(&self) -> Result<(), String> {
        if self.runtime_manifest_hash.is_empty()
            || self.dataset_fingerprint.is_empty()
            || self.code_revision.is_empty()
            || self.experiment_ids.is_empty()
            || self.performance.trade_count != self.portfolio.settled_trades.len()
            || !self.portfolio.final_equity.is_finite()
            || !self.portfolio.max_drawdown_ratio.is_finite()
        {
            return Err("research evidence manifest is incomplete or inconsistent".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pa_quant_tree::{
        default_portfolio_risk_policy, replay_shared_portfolio, PortfolioTradeCandidate,
    };
    use rust_quant_strategies::implementations::pa_quant_tree::RuntimeModel;

    #[test]
    fn evidence_requires_matching_portfolio_trade_count() {
        let manifest = RuntimeManifest {
            strategy_key: "pa_trend_15m".to_owned(),
            version: "1.0.0".to_owned(),
            feature_registry_version: "pa-feature-registry-v1".to_owned(),
            dataset_fingerprint: "dataset".to_owned(),
            code_revision: "revision".to_owned(),
            model: RuntimeModel::FixedRules { rules: vec![] },
        };
        let dataset = ResearchDataset::new(vec![]).unwrap();
        let portfolio = replay_shared_portfolio(
            &[PortfolioTradeCandidate {
                candidate_id: "candidate".to_owned(),
                symbol: "BTC-USDT-SWAP".to_owned(),
                entry_ts: 1,
                exit_ts: 2,
                entry_price: 100.0,
                stop_price: 99.0,
                net_r: 0.1,
            }],
            &default_portfolio_risk_policy(),
        )
        .unwrap();
        let performance = PerformanceMetrics {
            trade_count: 1,
            mean_net_r: 0.1,
            win_rate: 1.0,
            profit_factor: f64::INFINITY,
            max_drawdown_r: 0.0,
            total_net_r: 0.1,
        };
        let evidence = ResearchEvidenceManifest::new(
            EvidenceStage::WalkForward,
            &manifest,
            &dataset,
            "revision".to_owned(),
            vec!["experiment-1".to_owned()],
            WalkForwardPlan {
                min_train_size: 50,
                validation_size: 20,
                purge_size: 5,
                max_windows: 3,
            },
            performance,
            None,
            portfolio,
        )
        .unwrap();
        assert_eq!(evidence.stage, EvidenceStage::WalkForward);
    }
}
