use super::{CandidateTrade, SettledTrade};
use serde::Serialize;
use std::collections::BTreeMap;

/// V69 组合中互斥的机会来源；分类只读取信号时点已经冻结的审计标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum OpportunityFamily {
    /// Vegas 原始 Fib、加权条件与未被独立分支接管的信号。
    LegacyVegasCore,
    /// 压缩区间突破分支。
    CompressedRangeBreakout,
    /// 扫流动性后的即时反转或确认突破分支。
    LiquiditySweepImmediate,
    /// 扫流动性后紧邻首次回踩分支。
    LiquiditySweepFirstRetest,
    /// V70 之后的独立研究分支；V69 报告中出现该值即表示配置串线。
    OtherResearchBranch,
}

/// 成本质量门禁拒绝原因；与机会来源分开，避免把成本政策伪装成入场规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ProjectedCostRejectionReason {
    MissingProjectedCost,
    AboveMaximum,
}

/// 单个机会来源在某一管线阶段的交易级表现。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct OpportunityComponentReport {
    /// 互斥机会来源。
    pub(super) family: OpportunityFamily,
    /// 该阶段保留的交易数。
    pub(super) trades: usize,
    /// 其中多头交易数。
    pub(super) long_trades: usize,
    /// 其中空头交易数。
    pub(super) short_trades: usize,
    /// 成本后正收益交易数。
    pub(super) wins: usize,
    /// 成本后胜率，单位百分比。
    pub(super) win_rate_pct: f64,
    /// 该阶段收益绝对值口径的 Profit Factor；没有亏损样本时为空。
    pub(super) profit_factor: Option<f64>,
    /// 能还原入场初始风险的交易数。
    pub(super) initial_risk_covered_trades: usize,
    /// 全部样本风险证据完整时的成本后平均 R。
    pub(super) net_expectancy_r: Option<f64>,
}

/// 某个机会来源被成本质量门禁拒绝的数量。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct ProjectedCostRejectionReport {
    /// 被拒绝交易所属的机会来源。
    pub(super) family: OpportunityFamily,
    /// 成本证据缺失或超过冻结上限。
    pub(super) reason: ProjectedCostRejectionReason,
    /// 该来源、该原因对应的交易数。
    pub(super) trades: usize,
}

/// 只描述成本政策输入与决策，不包含信号阈值或组合容量结果。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct ProjectedCostGateReport {
    /// true 表示本轮实际应用了预计成本上限。
    pub(super) active: bool,
    /// 冻结的双倍往返成本上限，单位初始风险 R。
    pub(super) max_projected_double_execution_cost_r: Option<f64>,
    /// 进入成本门禁前的候选数。
    pub(super) candidates_before: usize,
    /// 通过成本门禁的候选数。
    pub(super) accepted: usize,
    /// 被成本门禁拒绝的候选数。
    pub(super) rejected: usize,
    /// 按机会来源和拒绝原因拆开的数量证据。
    pub(super) rejections: Vec<ProjectedCostRejectionReport>,
}

/// 以冻结样本外起点拆开的机会来源表现。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct OpportunityTimeSplitReport {
    /// 样本外起点，Unix 毫秒时间戳。
    pub(super) oos_start_ts: i64,
    /// 入场早于样本外起点的交易。
    pub(super) in_sample: Vec<OpportunityComponentReport>,
    /// 入场不早于样本外起点的交易。
    pub(super) out_of_sample: Vec<OpportunityComponentReport>,
}

/// V69 去耦后的三阶段审计：机会发现、质量门禁、组合容量。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct ComponentPipelineReport {
    /// 应用成本质量政策前的机会来源表现。
    pub(super) opportunity_candidates: Vec<OpportunityComponentReport>,
    /// 独立成本质量政策的决策摘要。
    pub(super) quality_gate: ProjectedCostGateReport,
    /// 通过成本门禁、尚未竞争组合容量的来源表现。
    pub(super) quality_accepted: Vec<OpportunityComponentReport>,
    /// 质量通过组的固定 IS/OOS 来源表现；未设置样本外起点时为空。
    pub(super) quality_accepted_time_split: Option<OpportunityTimeSplitReport>,
    /// 经过同交易对暴露和最大并发约束后的实际组合来源表现。
    pub(super) portfolio_accepted: Vec<OpportunityComponentReport>,
    /// 组合接纳组的固定 IS/OOS 来源表现；未设置样本外起点时为空。
    pub(super) portfolio_accepted_time_split: Option<OpportunityTimeSplitReport>,
}

/// 进入容量回放前冻结前两段结果，防止组合结果反向参与机会或质量判断。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ComponentPipelineSnapshot {
    /// 容量回放前已冻结的原始机会来源摘要。
    opportunity_candidates: Vec<OpportunityComponentReport>,
    /// 容量回放前已完成的质量门禁摘要。
    quality_gate: ProjectedCostGateReport,
    /// 容量回放前已冻结的质量通过组摘要。
    quality_accepted: Vec<OpportunityComponentReport>,
    /// 容量回放前已冻结的质量通过组时间切分。
    quality_accepted_time_split: Option<OpportunityTimeSplitReport>,
    /// 冻结样本外起点，供容量接纳组使用同一时间边界。
    oos_start_ts: Option<i64>,
}

impl ComponentPipelineSnapshot {
    /// 冻结容量选择之前的两段输入，确保后续盈亏不能回写前置分类。
    pub(super) fn new(
        opportunity_candidates: Vec<OpportunityComponentReport>,
        quality_accepted: &[CandidateTrade],
        quality_gate: ProjectedCostGateReport,
        oos_start_ts: Option<i64>,
    ) -> Self {
        Self {
            opportunity_candidates,
            quality_gate,
            quality_accepted: summarize_candidates(quality_accepted),
            quality_accepted_time_split: oos_start_ts
                .map(|split| summarize_candidate_time_split(quality_accepted, split)),
            oos_start_ts,
        }
    }

    /// 测试或时间子窗口已经接收质量过滤后的交易，因此使用无门禁身份快照。
    pub(super) fn without_quality_gate(candidates: &[CandidateTrade]) -> Self {
        let count = candidates.len();
        Self::new(
            summarize_candidates(candidates),
            candidates,
            ProjectedCostGateReport {
                active: false,
                max_projected_double_execution_cost_r: None,
                candidates_before: count,
                accepted: count,
                rejected: 0,
                rejections: Vec::new(),
            },
            None,
        )
    }

    /// 组合完成后只追加最终接纳组，不修改此前冻结的机会与质量摘要。
    pub(super) fn complete(self, settled: &[SettledTrade]) -> ComponentPipelineReport {
        ComponentPipelineReport {
            opportunity_candidates: self.opportunity_candidates,
            quality_gate: self.quality_gate,
            quality_accepted: self.quality_accepted,
            quality_accepted_time_split: self.quality_accepted_time_split,
            portfolio_accepted: summarize_settled(settled),
            portfolio_accepted_time_split: self
                .oos_start_ts
                .map(|split| summarize_settled_time_split(settled, split)),
        }
    }
}

/// 在质量政策执行前冻结机会来源摘要，避免为了审计复制完整 K 线路径。
pub(super) fn summarize_opportunity_candidates(
    trades: &[CandidateTrade],
) -> Vec<OpportunityComponentReport> {
    summarize_candidates(trades)
}

/// 按冻结审计标签识别机会来源；止损、止盈和动态风控标签不参与分类。
pub(super) fn classify_opportunity_family(adjustments: &[String]) -> OpportunityFamily {
    if has_prefix(adjustments, "LIQUIDITY_SWEEP_FIRST_RETEST_") {
        OpportunityFamily::LiquiditySweepFirstRetest
    } else if has_prefix(adjustments, "LIQUIDITY_SWEEP_REVERSAL_")
        || has_prefix(adjustments, "UPPER_SWEEP_CONFIRMATION_")
        || has_prefix(adjustments, "LOWER_SWEEP_CONFIRMATION_")
    {
        OpportunityFamily::LiquiditySweepImmediate
    } else if has_prefix(adjustments, "COMPRESSED_RANGE_BREAKOUT_") {
        OpportunityFamily::CompressedRangeBreakout
    } else if adjustments.iter().any(|adjustment| {
        [
            "EMA_TUNNEL_",
            "VOLUME_PROFILE_",
            "DONCHIAN_",
            "BOS_FVG_",
            "FAILED_BEARISH_FVG_",
            "MACD_DIVERGENCE_",
            "MACD_TREND_RESET_",
        ]
        .iter()
        .any(|prefix| adjustment.starts_with(prefix))
    }) {
        OpportunityFamily::OtherResearchBranch
    } else {
        OpportunityFamily::LegacyVegasCore
    }
}

/// 运行独立的预计成本质量政策，并保持候选原有顺序。
pub(super) fn apply_projected_cost_quality_gate(
    trades: &mut Vec<CandidateTrade>,
    max_cost_r: Option<f64>,
) -> ProjectedCostGateReport {
    let candidates_before = trades.len();
    let Some(max_cost_r) = max_cost_r else {
        return ProjectedCostGateReport {
            active: false,
            max_projected_double_execution_cost_r: None,
            candidates_before,
            accepted: candidates_before,
            rejected: 0,
            rejections: Vec::new(),
        };
    };

    let mut accepted = Vec::with_capacity(trades.len());
    let mut rejected = BTreeMap::<(OpportunityFamily, ProjectedCostRejectionReason), usize>::new();
    for trade in trades.drain(..) {
        match projected_cost_decision(trade.projected_double_execution_cost_r, max_cost_r) {
            Ok(()) => accepted.push(trade),
            Err(reason) => {
                *rejected
                    .entry((trade.opportunity_family, reason))
                    .or_default() += 1;
            }
        }
    }
    *trades = accepted;
    let rejected_count = candidates_before - trades.len();

    ProjectedCostGateReport {
        active: true,
        max_projected_double_execution_cost_r: Some(max_cost_r),
        candidates_before,
        accepted: trades.len(),
        rejected: rejected_count,
        rejections: rejected
            .into_iter()
            .map(|((family, reason), trades)| ProjectedCostRejectionReport {
                family,
                reason,
                trades,
            })
            .collect(),
    }
}

/// 对单笔候选执行纯成本判断；没有风险证据时按 fail-closed 拒绝。
fn projected_cost_decision(
    projected_cost_r: Option<f64>,
    max_cost_r: f64,
) -> Result<(), ProjectedCostRejectionReason> {
    match projected_cost_r {
        Some(cost_r) if cost_r <= max_cost_r => Ok(()),
        Some(_) => Err(ProjectedCostRejectionReason::AboveMaximum),
        None => Err(ProjectedCostRejectionReason::MissingProjectedCost),
    }
}

/// 审计标签允许携带冻结参数后缀，因此来源匹配使用前缀而非完整字符串。
fn has_prefix(adjustments: &[String], prefix: &str) -> bool {
    adjustments
        .iter()
        .any(|adjustment| adjustment.starts_with(prefix))
}

/// 构建交易级组件指标的内部累加器，不持有或修改策略状态。
#[derive(Default)]
struct ComponentAccumulator {
    /// 累计交易数。
    trades: usize,
    /// 累计多头数。
    long_trades: usize,
    /// 累计空头数。
    short_trades: usize,
    /// 累计正收益数。
    wins: usize,
    /// 正收益绝对和。
    gross_profit: f64,
    /// 负收益绝对和。
    gross_loss: f64,
    /// 初始风险证据完整的交易数。
    initial_risk_covered_trades: usize,
    /// 具备风险证据交易的成本后 R 之和。
    net_r_sum: f64,
}

impl ComponentAccumulator {
    /// 累加一笔已经完成结果计算的交易。
    fn add(&mut self, side: &str, profit: f64, normalized_return: f64, risk: Option<f64>) {
        self.trades += 1;
        self.long_trades += usize::from(side == "long");
        self.short_trades += usize::from(side == "short");
        self.wins += usize::from(profit > 0.0);
        if profit > 0.0 {
            self.gross_profit += profit;
        } else if profit < 0.0 {
            self.gross_loss += profit.abs();
        }
        if let Some(risk) = risk {
            self.initial_risk_covered_trades += 1;
            self.net_r_sum += normalized_return / risk;
        }
    }

    /// 只有风险覆盖完整时才发布 EV，避免用不完整分母美化组件。
    fn finish(self, family: OpportunityFamily) -> OpportunityComponentReport {
        OpportunityComponentReport {
            family,
            trades: self.trades,
            long_trades: self.long_trades,
            short_trades: self.short_trades,
            wins: self.wins,
            win_rate_pct: percentage(self.wins as f64, self.trades as f64),
            profit_factor: (self.gross_loss > 0.0).then_some(self.gross_profit / self.gross_loss),
            initial_risk_covered_trades: self.initial_risk_covered_trades,
            net_expectancy_r: (self.trades > 0 && self.initial_risk_covered_trades == self.trades)
                .then_some(self.net_r_sum / self.trades as f64),
        }
    }
}

/// 汇总容量选择前的独立交易结果，不引入共享账户结算顺序。
fn summarize_candidates(trades: &[CandidateTrade]) -> Vec<OpportunityComponentReport> {
    summarize_candidates_by(trades, |_| true)
}

/// 按时间条件汇总候选，闭包只读取已经冻结的入场时间。
fn summarize_candidates_by(
    trades: &[CandidateTrade],
    include: impl Fn(&CandidateTrade) -> bool,
) -> Vec<OpportunityComponentReport> {
    let mut groups = BTreeMap::<OpportunityFamily, ComponentAccumulator>::new();
    for trade in trades.iter().filter(|trade| include(trade)) {
        groups.entry(trade.opportunity_family).or_default().add(
            &trade.side,
            trade.normalized_return,
            trade.normalized_return,
            trade.initial_stop_risk_ratio,
        );
    }
    groups
        .into_iter()
        .map(|(family, accumulator)| accumulator.finish(family))
        .collect()
}

/// 用与主报告相同的入场时点边界拆分质量通过组。
fn summarize_candidate_time_split(
    trades: &[CandidateTrade],
    oos_start_ts: i64,
) -> OpportunityTimeSplitReport {
    OpportunityTimeSplitReport {
        oos_start_ts,
        in_sample: summarize_candidates_by(trades, |trade| trade.open_ts < oos_start_ts),
        out_of_sample: summarize_candidates_by(trades, |trade| trade.open_ts >= oos_start_ts),
    }
}

/// 汇总共享账户真正接纳并结算的组件结果。
fn summarize_settled(trades: &[SettledTrade]) -> Vec<OpportunityComponentReport> {
    summarize_settled_by(trades, |_| true)
}

/// 按入场时间条件汇总组合结果，避免用平仓时间重新分配样本。
fn summarize_settled_by(
    trades: &[SettledTrade],
    include: impl Fn(&SettledTrade) -> bool,
) -> Vec<OpportunityComponentReport> {
    let mut groups = BTreeMap::<OpportunityFamily, ComponentAccumulator>::new();
    for trade in trades.iter().filter(|trade| include(trade)) {
        groups.entry(trade.opportunity_family).or_default().add(
            &trade.side,
            trade.profit,
            trade.normalized_return,
            trade.initial_stop_risk_ratio,
        );
    }
    groups
        .into_iter()
        .map(|(family, accumulator)| accumulator.finish(family))
        .collect()
}

/// 用同一入场边界拆分组合接纳组，平仓时间不参与样本归属。
fn summarize_settled_time_split(
    trades: &[SettledTrade],
    oos_start_ts: i64,
) -> OpportunityTimeSplitReport {
    OpportunityTimeSplitReport {
        oos_start_ts,
        in_sample: summarize_settled_by(trades, |trade| trade.open_ts < oos_start_ts),
        out_of_sample: summarize_settled_by(trades, |trade| trade.open_ts >= oos_start_ts),
    }
}

/// 空样本返回零，避免审计 JSON 出现非有限数值。
fn percentage(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_retest_is_not_hidden_inside_the_generic_sweep_family() {
        let adjustments = vec![
            "LIQUIDITY_SWEEP_FIRST_RETEST_LONG".to_string(),
            "LIQUIDITY_SWEEP_FIRST_RETEST_TP_R:2".to_string(),
        ];

        assert_eq!(
            classify_opportunity_family(&adjustments),
            OpportunityFamily::LiquiditySweepFirstRetest
        );
    }

    #[test]
    fn projected_cost_policy_keeps_the_boundary_and_rejects_missing_evidence() {
        assert_eq!(projected_cost_decision(Some(0.20), 0.20), Ok(()));
        assert_eq!(
            projected_cost_decision(Some(0.21), 0.20),
            Err(ProjectedCostRejectionReason::AboveMaximum)
        );
        assert_eq!(
            projected_cost_decision(None, 0.20),
            Err(ProjectedCostRejectionReason::MissingProjectedCost)
        );
    }
}
