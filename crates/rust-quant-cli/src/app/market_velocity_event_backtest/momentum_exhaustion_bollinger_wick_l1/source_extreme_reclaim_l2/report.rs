//! 来源极值收回 L2 的机器报告合同。

use serde::Serialize;
use std::collections::BTreeMap;

/// L2 研究身份、风险公式与结果边界。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimL2Identity {
    /// 当前仅为本地多币种诊断。
    pub level: &'static str,
    /// 与 L1 相同的独立候选键。
    pub candidate_key: &'static str,
    /// 本批配对回放规则版本。
    pub rule_version: &'static str,
    /// 本批唯一变化的执行政策。
    pub only_variable: &'static str,
    /// 配对基线的成交政策。
    pub baseline_entry_policy: &'static str,
    /// 新候选的成交政策。
    pub variant_entry_policy: &'static str,
    /// 两侧共用的初始止损公式。
    pub initial_stop_policy: &'static str,
    /// 两侧共用的目标公式。
    pub target_policy: &'static str,
    /// 同一 K 同时触发止损与目标时的保守顺序。
    pub intrabar_conflict_policy: &'static str,
    /// 两侧共用的同币种冲突政策。
    pub paired_position_conflict_policy: &'static str,
    /// 单边手续费与滑点合计费率。
    pub per_side_cost_rate: f64,
    /// 最长持仓毫秒数。
    pub max_holding_ms: i64,
    /// L2 必须显式为 true。
    pub outcome_evaluation_performed: bool,
}

/// 从 L1 确认到账本回放的数量与阻塞证据。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimL2EntrySummary {
    /// L1 来源外轨 setup 数。
    pub base_touch_setups: usize,
    /// L1 来源极值确认数。
    pub l1_confirmed_setups: usize,
    /// 能在本地行情中重建两侧入场和风险的 pair 数。
    pub resolved_pairs: usize,
    /// 应用共同同币种冲突后实际执行的 pair 数。
    pub executed_pairs: usize,
    /// 两侧都有完整退出证据的 pair 数。
    pub completed_pairs: usize,
    /// 任一侧 forward 不完整的 pair 数。
    pub incomplete_pairs: usize,
    /// 完整 pair 的多空分布。
    pub completed_by_direction: BTreeMap<&'static str, usize>,
    /// 完整 pair 覆盖币种数。
    pub completed_symbol_count: usize,
    /// 完整 pair 覆盖候选入场 UTC 月份数。
    pub completed_month_count: usize,
    /// 按候选入场时间和方向一小时归并的有效事件数。
    pub completed_effective_market_events: usize,
    /// 数据、风险、forward 和共同冲突的逐项计数。
    pub blockers: BTreeMap<String, usize>,
}

/// 成本后逐笔 R 的交易级统计；L2 不冒充统一资金曲线。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimL2Performance {
    /// 纳入统计的完整交易数。
    pub trades: usize,
    /// 正向净 R 合计。
    pub positive_net_r: f64,
    /// 负向净 R 绝对值合计。
    pub negative_net_r_abs: f64,
    /// 全部成本后净 R 合计。
    pub net_sum_r: f64,
    /// 成本后净每笔期望。
    pub net_expectancy_r: f64,
    /// 成本后 Profit Factor；没有负 R 时为空。
    pub net_profit_factor: Option<f64>,
    /// 净 R 严格大于零的交易比例。
    pub win_rate_pct: f64,
    /// 逐笔 R 的交易级 Sharpe。
    pub trade_sharpe: Option<f64>,
    /// 按候选入场时间排序的累计 R 最大回撤。
    pub max_drawdown_r: f64,
}

/// 候选相对配对基线的净增量集中度。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimL2Concentration {
    /// 所有完整 pair 的候选减基线净 R。
    pub total_delta_net_r: f64,
    /// 移除净增量最高两笔后的剩余增量。
    pub delta_net_r_after_removing_top_two_trades: f64,
    /// 单一币种占全部正向增量的最大比例。
    pub max_symbol_positive_delta_share_pct: Option<f64>,
    /// 各币种净 R 增量。
    pub delta_net_r_by_symbol: BTreeMap<String, f64>,
    /// 多空方向净 R 增量。
    pub delta_net_r_by_direction: BTreeMap<&'static str, f64>,
}

/// 单侧入场、风险和退出路径证据。
#[derive(Debug, Clone, Serialize)]
pub struct EntryExitLegRecord {
    /// 实际入场 K 开始时间。
    pub entry_ts_ms: i64,
    /// 实际成交价格。
    pub entry_price: f64,
    /// 初始止损价格。
    pub initial_stop_price: f64,
    /// 入场到初始止损的价格风险。
    pub initial_risk_price: f64,
    /// 冻结原目标价格。
    pub target_price: f64,
    /// 是否有完整退出证据。
    pub complete: bool,
    /// 最终退出 K 开始时间。
    pub exit_ts_ms: i64,
    /// 最终退出价格。
    pub exit_price: f64,
    /// 止损、目标、超时或 forward 不完整。
    pub exit_reason: &'static str,
    /// 扣除开平成本后的净 R。
    pub net_r: f64,
}

/// 同一来源 setup 的被动极值基线与下一根开盘候选配对账本。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimL2TradeRecord {
    /// `symbol:setup_ts` 组成的稳定记录标识。
    pub candidate_id: String,
    /// OKX 永续合约标识。
    pub symbol: String,
    /// 来源 setup K 开始时间。
    pub setup_ts_ms: i64,
    /// 首次重测 K 开始时间。
    pub first_retest_ts_ms: i64,
    /// 来源方向。
    pub direction: &'static str,
    /// 来源长影触发标签。
    pub source_trigger: String,
    /// 来源 setup 极值。
    pub source_extreme_price: f64,
    /// 来源信号过滤量比。
    pub filtered_volume_ratio: f64,
    /// 来源 setup 的 ATR14。
    pub source_atr14: f64,
    /// 冻结量比分档对应的目标 ATR 倍数。
    pub target_atr_multiplier: f64,
    /// 来源 V2 被动极值入场侧。
    pub baseline: EntryExitLegRecord,
    /// 来源极值收回确认后下一根开盘侧。
    pub variant: EntryExitLegRecord,
    /// `variant.net_r - baseline.net_r`。
    pub delta_net_r: f64,
}

/// 查看结果前冻结的 L2 门禁结论。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimL2Decision {
    /// `stop` 或 `L2_pass_L3_required`。
    pub status: &'static str,
    /// 每项预注册门槛结果。
    pub gates: BTreeMap<&'static str, bool>,
    /// 停止或允许准备 L3 的主要依据。
    pub reason: String,
}

/// 来源极值确认入场的完整 L2 机器报告。
#[derive(Debug, Clone, Serialize)]
pub struct SourceExtremeReclaimL2Report {
    /// 报告 schema 版本。
    pub schema_version: &'static str,
    /// 生成时间不参与策略身份。
    pub generated_at_utc: String,
    /// 冻结 L2 身份与因果合同。
    pub identity: SourceExtremeReclaimL2Identity,
    /// 冻结 L1 报告 SHA-256。
    pub source_l1_report_sha256: String,
    /// 冻结 L1 候选账本 SHA-256。
    pub source_l1_candidate_ledger_sha256: String,
    /// 当前重载行情指纹。
    pub dataset_fingerprint_sha256: String,
    /// current-live Top60 返回成员数。
    pub returned_symbol_count: usize,
    /// 具备完整预热和评价窗口的成员数。
    pub eligible_symbol_count: usize,
    /// 因基础窗口缺口被排除的成员数。
    pub excluded_symbol_count: usize,
    /// 入场、冲突和完整退出覆盖。
    pub entry_summary: SourceExtremeReclaimL2EntrySummary,
    /// 来源 V2 被动极值配对基线指标。
    pub baseline: SourceExtremeReclaimL2Performance,
    /// 来源极值确认后下一根开盘候选指标。
    pub variant: SourceExtremeReclaimL2Performance,
    /// 基线多空分项指标。
    pub baseline_by_direction: BTreeMap<&'static str, SourceExtremeReclaimL2Performance>,
    /// 候选多空分项指标。
    pub variant_by_direction: BTreeMap<&'static str, SourceExtremeReclaimL2Performance>,
    /// 候选相对基线的净增量集中度。
    pub concentration: SourceExtremeReclaimL2Concentration,
    /// 两侧 setup、风险公式、冲突集合和退出合同是否一致。
    pub paired_contract_identity_verified: bool,
    /// L2 停止或准备 L3 的决定。
    pub decision: SourceExtremeReclaimL2Decision,
    /// 全部实际执行 pair；不完整 pair 保留但不进入绩效。
    pub trades: Vec<SourceExtremeReclaimL2TradeRecord>,
}
