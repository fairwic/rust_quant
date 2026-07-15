use super::PerformanceMetrics;
use serde::{Deserialize, Serialize};

/// Challenger 生命周期；没有 live 自动状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengerStatus {
    /// 训练结果尚未验证。
    Draft,
    /// 已通过离线验证，尚未执行 shadow。
    Validated,
    /// 只读 shadow 观察中。
    Shadow,
    /// Paper 观察中的候选。
    PaperChallenger,
    /// 当前 Paper Champion。
    PaperChampion,
    /// 失败、回滚或主动停用的保留证据版本。
    Archived,
}

/// Paper Champion/Challenger 的不可变身份与当前状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengerRecord {
    /// 对应 RuntimeManifest 哈希。
    pub manifest_hash: String,
    /// 版本状态。
    pub status: ChallengerStatus,
}

/// 自动晋级的冻结阈值；live promotion 不在本策略内。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotionPolicy {
    /// 平均 R 置信下界的最小值。
    pub min_mean_r_lower_bound: f64,
    /// 相对 Champion 配对增量下界。
    pub min_paired_delta_lower_bound: f64,
    /// 成本后最小胜率。
    pub min_win_rate: f64,
    /// 共享组合最大回撤上限，比例口径。
    pub max_drawdown_ratio: f64,
    /// 最小 Profit Factor。
    pub min_profit_factor: f64,
    /// Paper 最少自然日数。
    pub min_paper_days: u32,
    /// Paper 最少已结算交易数。
    pub min_paper_trades: usize,
}

/// 一次离线或 Paper 晋级评估需要的不可变证据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotionEvidence {
    /// 成本后组合或策略指标。
    pub metrics: PerformanceMetrics,
    /// 成本后平均 R 的置信下界。
    pub mean_r_lower_bound: f64,
    /// 相对 Champion 的配对增量下界。
    pub paired_delta_lower_bound: f64,
    /// 两倍成本压力场景是否仍为正。
    pub two_x_cost_positive: bool,
    /// 去除最大五笔盈利后是否仍为正。
    pub without_top_five_positive: bool,
    /// 多数 walk-forward 窗口是否为正。
    pub majority_walk_forward_positive: bool,
    /// 参数邻域是否稳定。
    pub parameter_neighborhood_stable: bool,
    /// 是否不存在单币种或单季度依赖。
    pub diversified: bool,
    /// 共享组合最大回撤比例。
    pub portfolio_max_drawdown_ratio: f64,
    /// Paper 已运行自然日数。
    pub paper_days: u32,
    /// 最大 PSI；超过 0.25 必须暂停。
    pub psi: f64,
}

/// 不可变的 promotion 审计记录；生成不代表可以进入实盘。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotionRecord {
    /// 从哪个 manifest 晋级。
    pub manifest_hash: String,
    /// 仅允许的目标状态。
    pub target_status: ChallengerStatus,
    /// 评估通过时引用的证据。
    pub evidence: PromotionEvidence,
}

impl PromotionPolicy {
    /// 返回方案约定的 Paper 自动晋级门槛。
    pub fn v1() -> Self {
        Self {
            min_mean_r_lower_bound: 0.0,
            min_paired_delta_lower_bound: 0.0,
            min_win_rate: 0.60,
            max_drawdown_ratio: 0.15,
            min_profit_factor: 1.2,
            min_paper_days: 90,
            min_paper_trades: 100,
        }
    }

    /// 判断离线证据是否满足从 Draft 到 Shadow 的所有硬门槛。
    pub fn permits_shadow(&self, evidence: &PromotionEvidence) -> bool {
        evidence.mean_r_lower_bound > self.min_mean_r_lower_bound
            && evidence.paired_delta_lower_bound > self.min_paired_delta_lower_bound
            && evidence.metrics.win_rate > self.min_win_rate
            && evidence.metrics.profit_factor > self.min_profit_factor
            && evidence.portfolio_max_drawdown_ratio < self.max_drawdown_ratio
            && evidence.two_x_cost_positive
            && evidence.without_top_five_positive
            && evidence.majority_walk_forward_positive
            && evidence.parameter_neighborhood_stable
            && evidence.diversified
    }

    /// Paper Challenger 只有满足自然日、成交数、期望和漂移约束才可成为 Paper Champion。
    pub fn permits_paper_champion(&self, evidence: &PromotionEvidence) -> bool {
        self.permits_shadow(evidence)
            && evidence.paper_days >= self.min_paper_days
            && evidence.metrics.trade_count >= self.min_paper_trades
            && evidence.metrics.mean_net_r >= 0.0
            && evidence.psi <= 0.25
    }

    /// 用已验证离线证据将 Challenger 从 Validated 推进到 Shadow，并生成审计记录。
    pub fn promote_to_shadow(
        &self,
        challenger: &mut ChallengerRecord,
        evidence: PromotionEvidence,
    ) -> Result<PromotionRecord, String> {
        if challenger.status != ChallengerStatus::Validated {
            return Err("only a validated challenger can enter shadow".to_owned());
        }
        if !self.permits_shadow(&evidence) {
            return Err("promotion evidence does not meet shadow thresholds".to_owned());
        }
        challenger.transition(ChallengerStatus::Shadow)?;
        Ok(PromotionRecord {
            manifest_hash: challenger.manifest_hash.clone(),
            target_status: ChallengerStatus::Shadow,
            evidence,
        })
    }

    /// 用 Forward Paper 证据将 Paper Challenger 切换为 Paper Champion；不提供 live 分支。
    pub fn promote_to_paper_champion(
        &self,
        challenger: &mut ChallengerRecord,
        evidence: PromotionEvidence,
    ) -> Result<PromotionRecord, String> {
        if challenger.status != ChallengerStatus::PaperChallenger {
            return Err("only a paper challenger can become paper champion".to_owned());
        }
        if !self.permits_paper_champion(&evidence) {
            return Err("promotion evidence does not meet paper champion thresholds".to_owned());
        }
        challenger.transition(ChallengerStatus::PaperChampion)?;
        Ok(PromotionRecord {
            manifest_hash: challenger.manifest_hash.clone(),
            target_status: ChallengerStatus::PaperChampion,
            evidence,
        })
    }
}

impl ChallengerRecord {
    /// 只允许单向状态迁移，失败可归档，但永远不能自动进入 live。
    pub fn transition(&mut self, next: ChallengerStatus) -> Result<(), String> {
        let allowed = matches!(
            (self.status, next),
            (ChallengerStatus::Draft, ChallengerStatus::Validated)
                | (ChallengerStatus::Validated, ChallengerStatus::Shadow)
                | (ChallengerStatus::Shadow, ChallengerStatus::PaperChallenger)
                | (
                    ChallengerStatus::PaperChallenger,
                    ChallengerStatus::PaperChampion
                )
                | (_, ChallengerStatus::Archived)
        );
        if !allowed {
            return Err(format!(
                "invalid lifecycle transition {:?} -> {:?}",
                self.status, next
            ));
        }
        self.status = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passing_evidence() -> PromotionEvidence {
        PromotionEvidence {
            metrics: PerformanceMetrics {
                trade_count: 100,
                mean_net_r: 0.1,
                win_rate: 0.61,
                profit_factor: 1.3,
                max_drawdown_r: 1.0,
                total_net_r: 10.0,
            },
            mean_r_lower_bound: 0.01,
            paired_delta_lower_bound: 0.01,
            two_x_cost_positive: true,
            without_top_five_positive: true,
            majority_walk_forward_positive: true,
            parameter_neighborhood_stable: true,
            diversified: true,
            portfolio_max_drawdown_ratio: 0.1,
            paper_days: 90,
            psi: 0.1,
        }
    }

    #[test]
    fn lifecycle_cannot_skip_directly_to_paper_champion() {
        let mut record = ChallengerRecord {
            manifest_hash: "sha256:test".to_owned(),
            status: ChallengerStatus::Draft,
        };
        assert!(record.transition(ChallengerStatus::PaperChampion).is_err());
        assert_eq!(record.status, ChallengerStatus::Draft);
    }

    #[test]
    fn promotion_requires_evidence_and_never_creates_live_state() {
        let policy = PromotionPolicy::v1();
        let mut challenger = ChallengerRecord {
            manifest_hash: "sha256:test".to_owned(),
            status: ChallengerStatus::Validated,
        };
        let shadow = policy
            .promote_to_shadow(&mut challenger, passing_evidence())
            .unwrap();
        assert_eq!(shadow.target_status, ChallengerStatus::Shadow);
        challenger
            .transition(ChallengerStatus::PaperChallenger)
            .unwrap();
        let paper = policy
            .promote_to_paper_champion(&mut challenger, passing_evidence())
            .unwrap();
        assert_eq!(paper.target_status, ChallengerStatus::PaperChampion);
    }
}
