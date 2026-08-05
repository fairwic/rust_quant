use super::super::{L1Coverage, L1Decision, TargetAudit};
use serde::Serialize;
use std::collections::BTreeMap;

/// V2 的独立研究身份，避免与已拒绝的 V1 或生产策略混用。
#[derive(Debug, Clone, Serialize)]
pub struct V2Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// 独立候选键。
    pub candidate_key: &'static str,
    /// 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V1 的唯一语义变量。
    pub only_variable: &'static str,
    /// 均线资格的方向与有效期策略。
    pub qualification_memory_policy: &'static str,
    /// 历史资格与 active 状态的失效方式。
    pub transition_latch_policy: &'static str,
    /// 可提前知道的回踩锚点。
    pub causal_order_anchor_policy: &'static str,
    /// L1 禁止读取的结果字段。
    pub label_boundary: &'static str,
    /// 运行隔离边界。
    pub runtime_boundary: &'static str,
}

/// 一条动态 EMA144 回踩触碰候选，只包含触碰时及此前可见字段。
#[derive(Debug, Clone, Serialize)]
pub struct V2Candidate {
    /// OKX 永续合约标识。
    pub symbol: String,
    /// `long` 或 `short`。
    pub direction: &'static str,
    /// 触碰 K 的开始时间。
    pub signal_ts_ms: i64,
    /// 触碰 K 的 UTC 月份。
    pub signal_month_utc: String,
    /// 最近历史均线资格完成时间。
    pub qualified_ts_ms: i64,
    /// 连续两根完成 EMA576 转换确认的时间。
    pub breakout_ts_ms: i64,
    /// 当前方向转换完成并锁存的时间。
    pub active_since_ts_ms: i64,
    /// 最近一次重新远离 EMA144、武装回踩的时间。
    pub reexpanded_ts_ms: i64,
    /// 转换锁存到本次触碰的根数。
    pub bars_since_activation: usize,
    /// 重新远离到本次触碰的根数。
    pub bars_since_reexpansion: usize,
    /// 触碰前一根已完成 K 的 EMA144，可用于预先挂单。
    pub anchor_ema144: f64,
    /// 触碰前一根已完成 K 的 ATR14。
    pub anchor_atr14: f64,
    /// 多头为 EMA144+0.30ATR，空头为 EMA144-0.30ATR。
    pub touch_zone_boundary: f64,
    /// 触碰 K 的方向极值。
    pub touch_extreme: f64,
    /// 极值相对锚点 EMA144 的方向归一化 ATR；负数表示穿越 EMA。
    pub touch_extreme_to_anchor_atr: f64,
    /// 触碰 K 收盘相对当前 EMA144 的方向归一化 ATR，只作诊断。
    pub close_to_current_ema144_atr: f64,
    /// 触碰 K 收盘是否仍守在当前 EMA144 的 active 一侧。
    pub close_holds_current_ema144: bool,
    /// `pre_cross_retest` 或 `post_cross_retest`，只分组不设门槛。
    pub cross_phase: &'static str,
    /// 触碰 K 的 EMA144。
    pub current_ema144: f64,
    /// 触碰 K 的 EMA576。
    pub current_ema576: f64,
    /// 触碰 K 的 ATR14。
    pub current_atr14: f64,
}

/// V2 因果状态机在评价窗口内的阶段计数。
#[derive(Debug, Clone, Default, Serialize)]
pub struct V2StageCounts {
    /// 最近历史均线资格发生改变。
    pub qualification_changes: usize,
    /// 满足两收盘 EMA576 突破或跌破。
    pub transition_breakouts: usize,
    /// 24 根内完成 0.75 ATR 离开并切换 active。
    pub active_transitions: usize,
    /// active 状态下重新远离 EMA144 并武装回踩。
    pub retest_arms: usize,
    /// 首次触碰后解除武装。
    pub retest_touches: usize,
}

/// V2 无标签候选的覆盖与信号时特征分布。
#[derive(Debug, Clone, Serialize)]
pub struct V2Summary {
    /// 候选总数。
    pub candidate_count: usize,
    /// 多空分布。
    pub by_direction: BTreeMap<&'static str, usize>,
    /// 金叉或死叉前后分布。
    pub by_cross_phase: BTreeMap<&'static str, usize>,
    /// 触碰 K 收盘是否守线的分布；不参与 L1 门禁。
    pub by_close_hold: BTreeMap<&'static str, usize>,
    /// 币种分布。
    pub by_symbol: BTreeMap<String, usize>,
    /// UTC 月份分布。
    pub by_month_utc: BTreeMap<String, usize>,
    /// 方向与 60 分钟单链聚类后的事件数。
    pub effective_market_events: usize,
    /// 因果阶段计数。
    pub stages: V2StageCounts,
}

/// V2 L1 完整机器产物。
#[derive(Debug, Clone, Serialize)]
pub struct V2Report {
    /// 报告 schema；字段语义变化必须升级。
    pub schema_version: &'static str,
    /// 生成时间不参与行情指纹。
    pub generated_at_utc: String,
    /// V2 冻结身份。
    pub identity: V2Identity,
    /// 与 V1 同口径的数据覆盖。
    pub coverage: L1Coverage,
    /// 无标签候选摘要。
    pub summary: V2Summary,
    /// 三张用户图定义审计。
    pub target_audits: Vec<TargetAudit>,
    /// L1 门禁结论。
    pub decision: L1Decision,
    /// 全量信号时点候选账本。
    pub candidates: Vec<V2Candidate>,
}
