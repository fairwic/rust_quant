//! V10 下一根开盘成交的 L2 机器报告合同。

use serde::Serialize;
use std::collections::BTreeMap;

/// V10 L2 的冻结策略、成交、风险和成本身份。
#[derive(Debug, Clone, Serialize)]
pub struct V10L2Identity {
    /// 当前只属于本地多币种诊断。
    pub level: &'static str,
    /// 与 L1 相同的独立候选键。
    pub candidate_key: &'static str,
    /// 冻结的 L1 形态规则。
    pub source_l1_rule_version: &'static str,
    /// L2 成交和退出规则版本。
    pub rule_version: &'static str,
    /// 本轮唯一接受 outcome 检验的假设。
    pub only_variable: &'static str,
    /// 回踩确认后的因果成交口径。
    pub entry_policy: &'static str,
    /// 初始止损口径。
    pub initial_stop_policy: &'static str,
    /// 固定目标口径。
    pub target_policy: &'static str,
    /// 同一 K 同时命中止损和目标时的顺序。
    pub intrabar_conflict_policy: &'static str,
    /// 同币种持仓冲突口径。
    pub symbol_position_policy: &'static str,
    /// 单边手续费与等价滑点合计费率。
    pub per_side_cost_rate: f64,
    /// 最长持仓毫秒数。
    pub max_holding_ms: i64,
    /// L2 是否建模资金费。
    pub funding_modeled: bool,
    /// L2 必须明确读取成交后结果。
    pub outcome_evaluation_performed: bool,
    /// 与运行态的隔离边界。
    pub runtime_boundary: &'static str,
}

/// L1 候选到完整交易的解析、冲突与分散性统计。
#[derive(Debug, Clone, Serialize)]
pub struct V10L2Coverage {
    /// 冻结 L1 候选数。
    pub l1_candidates: usize,
    /// 成功解析下一根连续开盘的候选数。
    pub resolved_candidates: usize,
    /// 应用同币种持仓锁后的交易数。
    pub executed_trades: usize,
    /// 具备完整退出证据的交易数。
    pub completed_trades: usize,
    /// forward 不完整的交易数。
    pub incomplete_trades: usize,
    /// 完整交易多空分布。
    pub completed_by_direction: BTreeMap<&'static str, usize>,
    /// 完整交易覆盖币种数。
    pub completed_symbol_count: usize,
    /// 完整交易覆盖 UTC 月份数。
    pub completed_month_count: usize,
    /// 按方向和一小时连续链归并的事件数。
    pub completed_effective_market_events: usize,
    /// 平均每个完整月份的组合交易数。
    pub completed_trades_per_month: f64,
    /// 本地 Top60 返回成员数。
    pub returned_symbol_count: usize,
    /// 完整预热且进入诊断的成员数。
    pub eligible_symbol_count: usize,
    /// 本地数据不完整而排除的成员数。
    pub excluded_symbol_count: usize,
    /// 解析、持仓冲突和 forward 阻塞计数。
    pub blockers: BTreeMap<String, usize>,
    /// 完整交易退出原因分布。
    pub exit_reasons: BTreeMap<&'static str, usize>,
}

/// 一组按初始风险单位归一化的交易级指标。
#[derive(Debug, Clone, Serialize)]
pub struct V10L2Performance {
    /// 交易数量。
    pub trades: usize,
    /// 正 R 合计。
    pub positive_r: f64,
    /// 负 R 绝对值合计。
    pub negative_r_abs: f64,
    /// 净 R 合计。
    pub sum_r: f64,
    /// 每笔平均 R。
    pub expectancy_r: f64,
    /// Profit Factor；没有负 R 时为空。
    pub profit_factor: Option<f64>,
    /// 严格正 R 交易比例。
    pub win_rate_pct: f64,
    /// 交易级 Sharpe，不代表组合年化 Sharpe。
    pub trade_sharpe: Option<f64>,
    /// 按时间排序累计 R 的最大回撤。
    pub max_drawdown_r: f64,
}

/// 成本后正收益对头部交易、币种和市场事件的依赖程度。
#[derive(Debug, Clone, Serialize)]
pub struct V10L2Concentration {
    /// 移除净 R 最高两笔后的剩余净 R。
    pub net_r_after_removing_top_two_trades: f64,
    /// 移除净收益最高事件簇后的剩余净 R。
    pub net_r_after_removing_top_event: f64,
    /// 单一币种占全部正净 R 的最大比例。
    pub max_symbol_positive_r_share_pct: Option<f64>,
    /// 单一事件簇占全部正净 R 的最大比例。
    pub max_event_positive_r_share_pct: Option<f64>,
    /// 各币种成本后净 R。
    pub net_r_by_symbol: BTreeMap<String, f64>,
    /// 各 UTC 月份成本后净 R。
    pub net_r_by_month: BTreeMap<String, f64>,
    /// 多空方向成本后净 R。
    pub net_r_by_direction: BTreeMap<&'static str, f64>,
    /// BTC、ETH 与其他币种的成本后净 R。
    pub net_r_by_asset_group: BTreeMap<&'static str, f64>,
}

/// 一笔 V10 回踩确认后下一根开盘交易的完整 L2 证据。
#[derive(Debug, Clone, Serialize)]
pub struct V10L2TradeRecord {
    /// `symbol:signal_ts:direction` 稳定身份。
    pub candidate_id: String,
    /// OKX USDT 永续合约。
    pub symbol: String,
    /// BTC、ETH 或其他币种分组。
    pub asset_group: &'static str,
    /// `long` 或 `short`。
    pub direction: &'static str,
    /// 长期资格完成时间。
    pub setup_ts_ms: i64,
    /// 价格 EMA576 突破确认时间。
    pub breakout_ts_ms: i64,
    /// EMA144 回踩再武装时间。
    pub rearmed_ts_ms: i64,
    /// 回踩守稳信号完成时间。
    pub signal_ts_ms: i64,
    /// 信号时 EMA144/576 所处交叉阶段。
    pub cross_phase: &'static str,
    /// 信号 K 完成后的 EMA144。
    pub signal_ema144: f64,
    /// 信号 K 完成后的 EMA576。
    pub signal_ema576: f64,
    /// 信号 K 完成后的 ATR14。
    pub signal_atr14: f64,
    /// 回踩极值到 EMA144 的方向归一化 ATR。
    pub retest_extreme_to_ema144_atr: f64,
    /// 收盘守稳到 EMA144 的方向归一化 ATR。
    pub close_to_ema144_directional_atr: f64,
    /// 下一根连续 15m K 的时间。
    pub entry_ts_ms: i64,
    /// 下一根连续 15m K 的开盘成交价。
    pub entry_price: f64,
    /// 入场时冻结的初始止损价；具体来源由报告 `initial_stop_policy` 标识。
    pub initial_stop_price: f64,
    /// 入场时冻结的目标价；具体毛/净 R 口径由报告 `target_policy` 标识。
    pub target_price: f64,
    /// 是否具有完整 24 小时 forward。
    pub complete: bool,
    /// 退出 K 时间。
    pub exit_ts_ms: i64,
    /// 退出价格。
    pub exit_price: f64,
    /// 止损、目标、超时或 forward 不完整。
    pub exit_reason: &'static str,
    /// 未扣成本的 R。
    pub gross_r: f64,
    /// 开平双边压力成本折算的 R。
    pub cost_r: f64,
    /// `gross_r-cost_r`。
    pub net_r: f64,
    /// 完整交易的一小时方向事件簇；不完整交易为空。
    pub event_cluster_id: Option<String>,
}

/// 预注册 L2 门禁的机器结论。
#[derive(Debug, Clone, Serialize)]
pub struct V10L2Decision {
    /// `stop` 或 `L2_pass_L3_required`。
    pub status: &'static str,
    /// 每项冻结门槛结果。
    pub gates: BTreeMap<&'static str, bool>,
    /// 停止或准备 L3 的原因。
    pub reason: String,
}

/// V10 的完整 L2 多币种本地诊断报告。
#[derive(Debug, Clone, Serialize)]
pub struct V10L2Report {
    /// 报告 schema 版本。
    pub schema_version: &'static str,
    /// 生成时间不参与策略身份。
    pub generated_at_utc: String,
    /// 冻结研究身份。
    pub identity: V10L2Identity,
    /// 冻结 L1 文件 SHA-256。
    pub source_l1_report_sha256: String,
    /// 重载行情指纹。
    pub dataset_fingerprint_sha256: String,
    /// 冻结 L1 候选与重建候选是否逐字段一致。
    pub source_candidate_ledger_verified: bool,
    /// 解析、冲突和样本覆盖。
    pub coverage: V10L2Coverage,
    /// 未扣成本绩效。
    pub gross: V10L2Performance,
    /// 8 bps/side 成本后绩效。
    pub net: V10L2Performance,
    /// 成本后多空分项。
    pub net_by_direction: BTreeMap<&'static str, V10L2Performance>,
    /// 成本后 BTC、ETH 与其他币种分项。
    pub net_by_asset_group: BTreeMap<&'static str, V10L2Performance>,
    /// 成本后集中度。
    pub concentration: V10L2Concentration,
    /// 成交、风险、退出、成本和同币种锁是否逐笔一致。
    pub contract_identity_verified: bool,
    /// 冻结门禁结论。
    pub decision: V10L2Decision,
    /// 全量实际执行交易账本。
    pub trades: Vec<V10L2TradeRecord>,
}
