use serde::Serialize;
use std::collections::BTreeMap;

/// 冻结研究身份，明确本报告不能被当作可交易版本。
#[derive(Debug, Clone, Serialize)]
pub struct L1Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// 独立候选键。
    pub candidate_key: &'static str,
    /// 精确规则版本。
    pub rule_version: &'static str,
    /// 本轮唯一形态变量。
    pub only_variable: &'static str,
    /// 均线计算口径。
    pub ema_policy: &'static str,
    /// 信号时序口径。
    pub signal_time_policy: &'static str,
    /// 禁止读取的结果字段。
    pub label_boundary: &'static str,
    /// 运行隔离边界。
    pub runtime_boundary: &'static str,
}

/// 单个币种因行情缺口或指标缺失而被排除的证据。
#[derive(Debug, Clone, Serialize)]
pub struct ExcludedSymbol {
    /// OKX 永续合约标识。
    pub symbol: String,
    /// 预热加评价窗口应有根数。
    pub expected_candles: usize,
    /// 实际落在窗口内的根数。
    pub loaded_candles: usize,
    /// 净缺口根数。
    pub missing_candles: usize,
    /// 排除原因。
    pub reason: &'static str,
}

/// 目标币种的输入完整性，独立于目标是否形成候选。
#[derive(Debug, Clone, Serialize)]
pub struct TargetInputCoverage {
    /// 目标币种。
    pub symbol: &'static str,
    /// 目标窗口是否每根 15m K 都存在且指标可判定。
    pub ready: bool,
    /// 预期目标 K 数。
    pub expected_candles: usize,
    /// 实际可判定 K 数。
    pub ready_candles: usize,
}

/// 冻结币池、时间窗及信号可见输入的数据身份。
#[derive(Debug, Clone, Serialize)]
pub struct L1Coverage {
    /// 冻结币池预期成员数。
    pub expected_symbol_count: usize,
    /// 数据加载器实际返回成员数。
    pub returned_symbol_count: usize,
    /// 具备完整预热及评价窗口的成员数。
    pub eligible_symbol_count: usize,
    /// 被排除成员及原因。
    pub excluded_symbols: Vec<ExcludedSymbol>,
    /// 评价起点。
    pub evaluation_start_ms: i64,
    /// 评价终点。
    pub evaluation_end_ms: i64,
    /// 最少历史预热根数。
    pub required_pre_evaluation_bars: usize,
    /// 目标币种输入完整性。
    pub target_inputs: Vec<TargetInputCoverage>,
    /// OHLC 与三项指标的稳定指纹。
    pub dataset_fingerprint_sha256: String,
    /// 当前币池限制。
    pub universe_limitation: &'static str,
}

/// 一条仅含信号收盘时可见数据的候选记录。
#[derive(Debug, Clone, Serialize)]
pub struct L1Candidate {
    /// OKX 永续合约标识。
    pub symbol: String,
    /// `long` 或 `short`。
    pub direction: &'static str,
    /// 信号 K 开始时间。
    pub signal_ts_ms: i64,
    /// 信号 UTC 月份。
    pub signal_month_utc: String,
    /// 连续两根站上或跌破 EMA576 的确认时间。
    pub breakout_ts_ms: i64,
    /// 首次达到 0.75 ATR 离开幅度的时间。
    pub impulse_ts_ms: i64,
    /// 突破前已完成的反向均线状态根数。
    pub prior_regime_bars: usize,
    /// 突破确认到信号的根数。
    pub bars_since_breakout: usize,
    /// 离开确认到首次回踩的根数。
    pub bars_since_impulse: usize,
    /// `pre_cross_retest` 或 `post_cross_retest`，只分组不设门槛。
    pub cross_phase: &'static str,
    /// 信号收盘后的 EMA144。
    pub ema144: f64,
    /// 信号收盘后的 EMA576。
    pub ema576: f64,
    /// 信号收盘后的 ATR14。
    pub atr14: f64,
    /// 回踩极值相对 EMA144 的方向归一化 ATR；负数表示刺穿。
    pub retest_extreme_to_ema144_atr: f64,
    /// 收盘相对 EMA144 的方向归一化 ATR；有效候选不小于零。
    pub close_to_ema144_directional_atr: f64,
    /// EMA144 相对 EMA576 的方向归一化 ATR；正数表示已完成金叉或死叉。
    pub ema_cross_progress_atr: f64,
}

/// 因果状态机在评价窗口内的阶段计数。
#[derive(Debug, Clone, Default, Serialize)]
pub struct L1StageCounts {
    /// 满足连续 144 根反向均线状态并完成武装。
    pub armed_episodes: usize,
    /// 连续两根完成有效方向突破的尝试。
    pub confirmed_breakouts: usize,
    /// 24 根内达到 0.75 ATR 的有效离开。
    pub effective_departures: usize,
    /// 第一次回踩刺穿或收盘失守，机会被消耗。
    pub failed_first_retests: usize,
    /// 有效离开后 96 根没有回踩，机会被消耗。
    pub retest_timeouts: usize,
}

/// L1 候选的覆盖与分散性摘要。
#[derive(Debug, Clone, Serialize)]
pub struct L1Summary {
    /// 候选总数。
    pub candidate_count: usize,
    /// 多空候选分布。
    pub by_direction: BTreeMap<&'static str, usize>,
    /// 金叉或死叉前后分布。
    pub by_cross_phase: BTreeMap<&'static str, usize>,
    /// 币种分布。
    pub by_symbol: BTreeMap<String, usize>,
    /// UTC 月份分布。
    pub by_month_utc: BTreeMap<String, usize>,
    /// 按方向与 60 分钟单链归并后的有效事件数。
    pub effective_market_events: usize,
    /// 状态机阶段计数。
    pub stages: L1StageCounts,
}

/// 用户截图目标的定义匹配，不读取目标窗口之后的价格。
#[derive(Debug, Clone, Serialize)]
pub struct TargetAudit {
    /// 预注册目标名。
    pub name: &'static str,
    /// 目标币种。
    pub symbol: &'static str,
    /// 目标方向。
    pub direction: &'static str,
    /// 目标窗口起点。
    pub start_ms: i64,
    /// 目标窗口终点。
    pub end_ms: i64,
    /// 窗口内匹配的候选时间。
    pub matched_signal_timestamps_ms: Vec<i64>,
    /// 是否至少匹配一条候选。
    pub matched: bool,
}

/// 用户目标窗口内逐根因果状态，专门解释定义在哪一阶段停止。
#[derive(Debug, Clone, Serialize)]
pub struct TargetBarTrace {
    /// 已完成 K 的开始时间。
    pub ts_ms: i64,
    /// true 表示冻结目标窗口；false 只是最多 96 根的前置因果上下文。
    pub in_target_window: bool,
    /// 推进当前 K 前的状态。
    pub phase_before: &'static str,
    /// 推进当前 K 后的状态。
    pub phase_after: &'static str,
    /// 当前方向的 EMA144/576 连续状态年龄。
    pub relation_age_bars: usize,
    /// 当前方向反面的 EMA144/576 连续状态年龄。
    pub opposite_relation_age_bars: usize,
    /// 当前 K 是否仍满足历史均线方向。
    pub regime_holds: bool,
    /// 当前三根是否构成两收盘突破。
    pub breakout_condition: bool,
    /// 当前收盘是否达到 0.75 ATR 有效离开。
    pub departure_condition: bool,
    /// 当前 K 是否进入 EMA144 回踩区。
    pub retest_zone_touched: bool,
    /// 当前 K 是否守住 EMA144 允许范围。
    pub retest_holds: bool,
    /// 当前收盘价。
    pub close: Option<f64>,
    /// 当前 EMA144。
    pub ema144: Option<f64>,
    /// 当前 EMA576。
    pub ema576: Option<f64>,
    /// 当前 ATR14。
    pub atr14: Option<f64>,
    /// 回踩极值相对 EMA144 的方向归一化 ATR。
    pub retest_extreme_to_ema144_atr: Option<f64>,
    /// 当前 K 触发的状态机事件，空数组表示只推进状态。
    pub events: Vec<&'static str>,
}

/// 一张用户目标图在窗口内的无标签逐根状态轨迹。
#[derive(Debug, Clone, Serialize)]
pub struct TargetTrace {
    /// 预注册目标名。
    pub name: &'static str,
    /// 目标币种。
    pub symbol: &'static str,
    /// 目标方向。
    pub direction: &'static str,
    /// 目标窗口内逐根状态。
    pub bars: Vec<TargetBarTrace>,
}

/// L1 预注册门禁；通过也只能进入新的 L2 预注册。
#[derive(Debug, Clone, Serialize)]
pub struct L1Decision {
    /// 停止原因或下一等级准备状态。
    pub status: &'static str,
    /// 每项冻结门槛的结果。
    pub gates: BTreeMap<&'static str, bool>,
    /// 人类可读的停止边界。
    pub reason: String,
    /// L1 必须恒为 false。
    pub outcome_evaluation_performed: bool,
    /// 三张用户目标图是否完成机器定义审计。
    pub target_chart_audit_completed: bool,
}

/// EMA144/576 首次回踩 L1 的完整机器产物。
#[derive(Debug, Clone, Serialize)]
pub struct L1Report {
    /// 报告 schema；字段语义变化必须升级。
    pub schema_version: &'static str,
    /// 生成时间不参与数据指纹。
    pub generated_at_utc: String,
    /// 冻结研究身份。
    pub identity: L1Identity,
    /// 行情和目标输入覆盖。
    pub coverage: L1Coverage,
    /// 无标签候选汇总。
    pub summary: L1Summary,
    /// 用户目标图定义审计。
    pub target_audits: Vec<TargetAudit>,
    /// 目标窗口逐根状态，仅含信号时可见字段。
    pub target_traces: Vec<TargetTrace>,
    /// 预注册门禁结论。
    pub decision: L1Decision,
    /// 全量信号时点候选账本。
    pub candidates: Vec<L1Candidate>,
}
