//! 来源极值确认后重挂限价研究的机器报告合同。

use serde::Serialize;
use std::collections::BTreeMap;

/// 本批唯一变量、时序边界和冻结风险合同。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitIdentity {
    /// 当前批次覆盖 L1，并且只有 L1 通过才包含 L2。
    pub level: &'static str,
    /// 新执行语义的独立候选键。
    pub candidate_key: &'static str,
    /// 无结果成交覆盖规则版本。
    pub l1_rule_version: &'static str,
    /// 条件成本后配对回放规则版本。
    pub l2_rule_version: &'static str,
    /// 本批只允许变化的一项执行政策。
    pub only_variable: &'static str,
    /// 确认后挂单的生效时点。
    pub activation_policy: &'static str,
    /// 原 setup 的固定有效期边界。
    pub expiry_policy: &'static str,
    /// 同一根同时触价和出现新 setup 时的顺序。
    pub replacement_policy: &'static str,
    /// L1 禁止读取的后验结果。
    pub l1_label_boundary: &'static str,
}

/// 冻结来源报告、候选账本与重新加载行情的身份。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitSourceEvidence {
    /// 来源 L1 原始文件 SHA-256。
    pub source_l1_report_sha256: String,
    /// 来源 L1 内记录的候选账本 SHA-256。
    pub source_l1_candidate_ledger_sha256: String,
    /// 来源 L1 行情指纹。
    pub source_dataset_fingerprint_sha256: String,
    /// 当前进程重建后的行情指纹。
    pub reloaded_dataset_fingerprint_sha256: String,
    /// 来源候选字段已经检查且没有交易结果标签。
    pub source_candidate_schema_no_outcome_fields: bool,
}

/// 当前本地数据成员覆盖；仍只是 current-live Top60 诊断。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitCoverage {
    /// 数据加载器返回成员数。
    pub returned_symbol_count: usize,
    /// 完成预热与评价窗口的成员数。
    pub eligible_symbol_count: usize,
    /// 因数据缺口排除的成员数。
    pub excluded_symbol_count: usize,
    /// 评价窗口起点。
    pub evaluation_start_ms: i64,
    /// 评价窗口终点。
    pub evaluation_end_ms: i64,
    /// 幸存者偏差与非 OOS 边界。
    pub universe_limitation: &'static str,
}

/// 一条来源确认在重挂政策下的因果成交或 blocker 终态。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitCandidate {
    /// `symbol:setup_ts` 稳定标识。
    pub candidate_id: String,
    /// OKX 永续合约标识。
    pub symbol: String,
    /// 来源 setup K 开始时间。
    pub setup_ts_ms: i64,
    /// 来源 setup 的 UTC 月份。
    pub setup_month_utc: String,
    /// `long` 或 `short`。
    pub direction: String,
    /// 来源 V2 长影线触发标签。
    pub source_trigger: String,
    /// 冻结重挂价格；做空为 setup high，做多为 setup low。
    pub source_extreme_price: f64,
    /// 来源 setup 的过滤量比。
    pub filtered_volume_ratio: f64,
    /// 来源 setup 前 96 根有符号净移动。
    pub prior_96_net_move_pct: f64,
    /// 来源方向影线占完整振幅比例。
    pub directional_wick_range_ratio: f64,
    /// 严格收回来源极值的确认 K 时间。
    pub confirmation_signal_ts_ms: i64,
    /// 来源 setup 到确认 K 的根数。
    pub first_retest_offset_bars: usize,
    /// 最早允许重挂成交的下一根 K 时间。
    pub activation_ts_ms: i64,
    /// 原 setup 第 12 根 K 的最终有效时间。
    pub original_expiry_ts_ms: i64,
    /// 重挂实际成交 K；未成交时为空。
    pub relimit_entry_ts_ms: Option<i64>,
    /// 重挂成交距来源 setup 的根数。
    pub relimit_entry_offset_bars: Option<usize>,
    /// 确认完成后等待的完整 K 根数；未成交时为空。
    pub wait_bars_after_confirmation: Option<usize>,
    /// 若被新 setup 替换，记录替换 K 时间。
    pub replaced_by_setup_ts_ms: Option<i64>,
    /// 成交、原有效期耗尽、替换或 forward 不完整。
    pub terminal_status: &'static str,
}

/// 固定最近十笔止损样本在新成交政策下的终态核对。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitTargetAudit {
    /// 固定目标交易对。
    pub symbol: &'static str,
    /// 固定来源 setup 时间。
    pub setup_ts_ms: i64,
    /// 是否找到来源确认候选。
    pub source_found: bool,
    /// 新政策是否得到唯一终态。
    pub terminal_resolved: bool,
    /// 新成交政策的终态。
    pub terminal_status: Option<&'static str>,
    /// 若成交，记录成交 K 时间。
    pub relimit_entry_ts_ms: Option<i64>,
}

/// L1 成交覆盖、方向与分散性统计；不含入场后的任何路径。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitL1Summary {
    /// 来源外轨长影 setup 总数。
    pub source_base_touch_setups: usize,
    /// 来源极值严格收回确认数。
    pub source_confirmed_setups: usize,
    /// 获得唯一重挂终态的确认数。
    pub terminal_setups: usize,
    /// 剩余原有效期内再次触及来源极值的成交数。
    pub relimit_filled_setups: usize,
    /// 成交数占来源确认数的比例。
    pub fill_retention_pct: f64,
    /// 成交多空分布。
    pub filled_by_direction: BTreeMap<&'static str, usize>,
    /// 成交覆盖币种数。
    pub filled_symbol_count: usize,
    /// 成交覆盖 UTC 月份数。
    pub filled_month_count: usize,
    /// 按成交时间和方向一小时归并的有效事件数。
    pub filled_effective_market_events: usize,
    /// 未成交终态的逐项计数。
    pub blockers: BTreeMap<&'static str, usize>,
    /// 固定十笔中得到唯一终态的数量。
    pub target_terminal_count: usize,
}

/// L1 查看结果前冻结的逐项停止门禁。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitL1Decision {
    /// `coverage_pass_l2_ready` 或 `stop`。
    pub status: &'static str,
    /// 每项预注册覆盖门槛。
    pub gates: BTreeMap<&'static str, bool>,
    /// 停止或允许 L2 的直接理由。
    pub reason: String,
    /// L1 必须保持 false。
    pub outcome_evaluation_performed: bool,
}

/// 完整 L1 无结果成交覆盖账本。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitL1Report {
    /// 143 个确认候选序列化后的 SHA-256。
    pub candidate_ledger_sha256: String,
    /// 无结果成交覆盖统计。
    pub summary: RelimitL1Summary,
    /// 固定十笔止损样本终态核对。
    pub target_sample_audit: Vec<RelimitTargetAudit>,
    /// 是否允许同进程进入 L2。
    pub decision: RelimitL1Decision,
    /// 全部来源确认候选的成交或 blocker 终态。
    pub candidates: Vec<RelimitCandidate>,
}

/// 单侧入场、止损、目标、退出与成本后 R 证据。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitLegRecord {
    /// 实际入场 K 开始时间。
    pub entry_ts_ms: i64,
    /// 实际成交价格。
    pub entry_price: f64,
    /// 初始止损价格。
    pub initial_stop_price: f64,
    /// 入场到初始止损的价格风险。
    pub initial_risk_price: f64,
    /// 冻结目标价格。
    pub target_price: f64,
    /// 是否具备完整 forward 退出证据。
    pub complete: bool,
    /// 退出 K 开始时间。
    pub exit_ts_ms: i64,
    /// 退出价格。
    pub exit_price: f64,
    /// 止损、目标、超时或 forward 不完整。
    pub exit_reason: &'static str,
    /// 扣除开平名义成本后的净 R。
    pub net_r: f64,
}

/// 同一成交 cohort 的下一根开盘基线与来源极值重挂候选。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitTradeRecord {
    /// 稳定候选标识。
    pub candidate_id: String,
    /// OKX 永续合约标识。
    pub symbol: String,
    /// 来源 setup K 时间。
    pub setup_ts_ms: i64,
    /// 来源确认 K 时间。
    pub confirmation_signal_ts_ms: i64,
    /// 来源 setup 第 12 根 K 的最终有效时间。
    pub original_expiry_ts_ms: i64,
    /// `long` 或 `short`。
    pub direction: &'static str,
    /// 来源长影线触发标签。
    pub source_trigger: String,
    /// 来源 setup 极值。
    pub source_extreme_price: f64,
    /// 来源过滤量比。
    pub filtered_volume_ratio: f64,
    /// 来源 setup ATR14。
    pub source_atr14: f64,
    /// 冻结目标 ATR 倍数。
    pub target_atr_multiplier: f64,
    /// 同 cohort 确认后下一根开盘基线。
    pub baseline_next_open: RelimitLegRecord,
    /// 确认后来源极值重挂候选。
    pub candidate_relimit: RelimitLegRecord,
    /// `candidate_relimit.net_r - baseline_next_open.net_r`。
    pub delta_net_r: f64,
}

/// 成本后逐笔 R 统计；不是统一资金曲线。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitPerformance {
    /// 完整交易数。
    pub trades: usize,
    /// 正净 R 合计。
    pub positive_net_r: f64,
    /// 负净 R 绝对值合计。
    pub negative_net_r_abs: f64,
    /// 成本后净 R 合计。
    pub net_sum_r: f64,
    /// 成本后净每笔期望。
    pub net_expectancy_r: f64,
    /// 成本后 Profit Factor；无负交易时为空。
    pub net_profit_factor: Option<f64>,
    /// 净 R 严格大于零的比例。
    pub win_rate_pct: f64,
    /// 逐笔 R 的交易级 Sharpe。
    pub trade_sharpe: Option<f64>,
    /// 按候选入场顺序累计净 R 的最大回撤。
    pub max_drawdown_r: f64,
}

/// L2 完整 pair 的覆盖与共同冲突证据。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitL2EntrySummary {
    /// L1 重挂成交 cohort 数。
    pub l1_filled_setups: usize,
    /// 能重建两侧入场与风险的 pair 数。
    pub resolved_pairs: usize,
    /// 应用共同币种锁后的 pair 数。
    pub executed_pairs: usize,
    /// 两侧都有完整退出证据的 pair 数。
    pub completed_pairs: usize,
    /// forward 不完整 pair 数。
    pub incomplete_pairs: usize,
    /// 完整 pair 多空分布。
    pub completed_by_direction: BTreeMap<&'static str, usize>,
    /// 完整 pair 覆盖币种数。
    pub completed_symbol_count: usize,
    /// 完整 pair 覆盖月份数。
    pub completed_month_count: usize,
    /// 按候选成交时间和方向一小时归并的有效事件数。
    pub completed_effective_market_events: usize,
    /// 风险、数据、forward 与共同冲突 blocker。
    pub blockers: BTreeMap<String, usize>,
}

/// 候选相对同 cohort 下一根开盘基线的净增量集中度。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitConcentration {
    /// 所有完整 pair 的净 R 增量。
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

/// 条件 L2 的冻结合同与结果边界。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitL2Identity {
    /// 当前仅为本地多币种诊断。
    pub level: &'static str,
    /// 条件 L2 配对回放规则版本。
    pub rule_version: &'static str,
    /// 同一成交 cohort 的基线执行政策。
    pub baseline_entry_policy: &'static str,
    /// 本版本候选执行政策。
    pub candidate_entry_policy: &'static str,
    /// 两侧初始止损公式。
    pub initial_stop_policy: &'static str,
    /// 两侧目标公式。
    pub target_policy: &'static str,
    /// 同棒止损和目标冲突顺序。
    pub intrabar_conflict_policy: &'static str,
    /// 两侧共同同币种冲突政策。
    pub paired_position_conflict_policy: &'static str,
    /// 单边手续费与滑点合计费率。
    pub per_side_cost_rate: f64,
    /// 最长持仓时间。
    pub max_holding_ms: i64,
    /// L2 必须为 true。
    pub outcome_evaluation_performed: bool,
}

/// L2 查看结果前冻结的联合门禁结论。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitL2Decision {
    /// `stop` 或 `L2_pass_L3_required`。
    pub status: &'static str,
    /// 每项预注册 L2 门槛。
    pub gates: BTreeMap<&'static str, bool>,
    /// 停止或允许准备 L3 的直接理由。
    pub reason: String,
}

/// L1 通过后同进程生成的唯一 L2 结果。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitL2Report {
    /// 冻结 L2 合同。
    pub identity: RelimitL2Identity,
    /// 成交、共同冲突与完整退出覆盖。
    pub entry_summary: RelimitL2EntrySummary,
    /// 同 cohort 下一根开盘基线。
    pub baseline_next_open: RelimitPerformance,
    /// 来源极值重挂候选。
    pub candidate_relimit: RelimitPerformance,
    /// 基线多空分项。
    pub baseline_by_direction: BTreeMap<&'static str, RelimitPerformance>,
    /// 候选多空分项。
    pub candidate_by_direction: BTreeMap<&'static str, RelimitPerformance>,
    /// 候选相对基线的增量集中度。
    pub concentration: RelimitConcentration,
    /// 两侧 setup、风险、冲突和退出公式是否一致。
    pub paired_contract_identity_verified: bool,
    /// 条件 L2 结论。
    pub decision: RelimitL2Decision,
    /// 全部共同执行 pair。
    pub trades: Vec<RelimitTradeRecord>,
}

/// 本研究批次唯一机器 JSON；L1 未通过时 `l2` 必须为空。
#[derive(Debug, Clone, Serialize)]
pub struct RelimitResearchReport {
    /// 报告 schema 版本。
    pub schema_version: &'static str,
    /// 生成时间不参与研究身份。
    pub generated_at_utc: String,
    /// 唯一变量和时序边界。
    pub identity: RelimitIdentity,
    /// 输入与重载行情身份。
    pub source_evidence: RelimitSourceEvidence,
    /// 当前本地覆盖边界。
    pub coverage: RelimitCoverage,
    /// 无结果成交覆盖账本。
    pub l1: RelimitL1Report,
    /// 仅在 L1 门禁全部通过时存在。
    pub l2: Option<RelimitL2Report>,
}
