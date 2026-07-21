use anyhow::{bail, Context, Result};
pub use rust_quant_services::market::MarketVelocityStopLossMode;
use std::path::PathBuf;
mod paper_strategy_preset;
use paper_strategy_preset::*;
const DEFAULT_TARGET_RS: &[f64] = &[1.5, 2.0];
const DEFAULT_PAPER_OUTCOME_ENTRY_RULE_VERSION: &str = "rank_radar_4h_trend_15m_timing_v1";
const ENTRY_TRIGGER_ALLOWLIST_FILTER_VERSION: &str = "entry_trigger_allowlist_v1";
const ENTRY_TRIGGER_BLOCKLIST_FILTER_VERSION: &str = "entry_trigger_blocklist_v1";
const ENTRY_TRIGGER_UNFILTERED_VERSION: &str = "unfiltered_v1";
const DEFAULT_WEB_PAPER_OUTCOME_ENTRY_TRIGGER_ALLOWLIST: &[&str] =
    &["breakout_previous_high", "reclaim_ema"];
const DEFAULT_FVG_LOOKBACK_CANDLES: usize = 40;
const DEFAULT_FVG_MAX_WAIT_CANDLES: usize = 24;
const PAPER_OBSERVATION_LOOP_INTERVAL_FLAG: &str = "--loop-interval-seconds";
const PAPER_OBSERVATION_OWNED_FLAGS: &[&str] = &[
    "--paper-outcome-sink",
    "--paper-outcome-entry-rule-version",
    "--entry-trigger-allowlist",
    "--entry-trigger-blocklist",
    "--symbol-blocklist",
    "--stop-reentry-mode",
    "--fvg-entry-mode",
    "--fvg-lookback-candles",
    "--fvg-max-wait-candles",
    "--fvg-impulse-retrace-fill-pct",
    "--fvg-impulse-retrace-min-wait-candles",
    "--profit-protect-after-r",
    "--profit-protect-stop-r",
    "--runner-target-r",
    "--runner-fraction",
    "--runner-stop-r",
    "--early-exit-no-profit-candles",
    "--trend-timeframe",
    "--event-start-ms",
    "--event-end-ms",
    "--save-backtest-detail",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketVelocityEventSource {
    Episodes,
    RawEvents,
    RawState,
    Kline15m,
}
impl MarketVelocityEventSource {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "episodes" | "episode" | "market_velocity_episodes" => Ok(Self::Episodes),
            "raw_events" | "raw" => Ok(Self::RawEvents),
            "raw_state" | "state" | "signal_state" => Ok(Self::RawState),
            "kline_15m" | "klines_15m" | "candles_15m" | "15m_klines" => Ok(Self::Kline15m),
            other => bail!("unknown --event-source: {other}"),
        }
    }
    /// 提供标签的集中实现，避免回测策略调用方重复处理相同细节。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Episodes => "episodes",
            Self::RawEvents => "raw_events",
            Self::RawState => "raw_state",
            Self::Kline15m => "kline_15m",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketVelocityTradeDirection {
    Long,
    Short,
    Both,
}
impl MarketVelocityTradeDirection {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "long" | "up" => Ok(Self::Long),
            "short" | "down" => Ok(Self::Short),
            "both" | "long_short" | "long+short" => Ok(Self::Both),
            other => bail!("unknown --trade-direction: {other}"),
        }
    }
    /// 提供标签的集中实现，避免回测策略调用方重复处理相同细节。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
            Self::Both => "both",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketVelocityTrendTimeframe {
    FourHour,
    OneHour,
    Off,
}
impl MarketVelocityTrendTimeframe {
    /// 解析研究回测的趋势过滤周期，默认旧策略仍使用 4H。
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "4h" | "h4" | "four_hour" | "four-hour" => Ok(Self::FourHour),
            "1h" | "h1" | "one_hour" | "one-hour" => Ok(Self::OneHour),
            "off" | "none" | "disabled" | "0" | "false" => Ok(Self::Off),
            other => bail!("unknown --trend-timeframe: {other}"),
        }
    }
    /// 提供参数标签，便于 manifest 和回测报告审计不同趋势门槛。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::FourHour => "4h",
            Self::OneHour => "1h",
            Self::Off => "off",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketVelocityPaperOutcomeSink {
    Off,
    Jsonl,
    Web,
}
impl MarketVelocityPaperOutcomeSink {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "disabled" | "0" | "false" => Ok(Self::Off),
            "jsonl" | "stdout" | "print" => Ok(Self::Jsonl),
            "web" | "quant_web" | "submit" => Ok(Self::Web),
            other => bail!("unknown --paper-outcome-sink: {other}"),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketVelocityPaperStrategySignalSink {
    Off,
    Web,
}
impl MarketVelocityPaperStrategySignalSink {
    /// 解析 paper observation 的策略信号输出目标；默认关闭，避免观察任务误接实盘链路。
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "disabled" | "0" | "false" => Ok(Self::Off),
            "web" | "quant_web" | "submit" => Ok(Self::Web),
            other => bail!("unknown --paper-strategy-signal-sink: {other}"),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReentryMode {
    Off,
    BreakoutReclaim,
}
impl StopReentryMode {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "disabled" | "0" | "false" => Ok(Self::Off),
            "breakout_reclaim" | "reclaim_breakout" | "on" | "true" => Ok(Self::BreakoutReclaim),
            other => bail!("unknown --stop-reentry-mode: {other}"),
        }
    }
    /// 提供标签的集中实现，避免回测策略调用方重复处理相同细节。
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::BreakoutReclaim => "breakout_reclaim",
        }
    }
    /// 提供触发suffix的集中实现，避免回测策略调用方重复处理相同细节。
    pub(super) fn trigger_suffix(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::BreakoutReclaim => Some("stop_reentry_breakout_reclaim"),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FvgEntryMode {
    Off,
    M15To1h,
    H1To4h,
    M15SelfAfterSignal,
    M15ImpulseRetrace,
}
impl FvgEntryMode {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "disabled" | "0" | "false" => Ok(Self::Off),
            "m15_to_1h" | "15m_to_1h" | "15m-1h" => Ok(Self::M15To1h),
            "h1_to_4h" | "1h_to_4h" | "1h-4h" => Ok(Self::H1To4h),
            "m15_self_after_signal" | "15m_self_after_signal" | "15m-self-after-signal" => {
                Ok(Self::M15SelfAfterSignal)
            }
            "m15_impulse_retrace" | "15m_impulse_retrace" | "15m-impulse-retrace" => {
                Ok(Self::M15ImpulseRetrace)
            }
            other => bail!("unknown --fvg-entry-mode: {other}"),
        }
    }
    /// 提供标签的集中实现，避免回测策略调用方重复处理相同细节。
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::M15To1h => "m15_to_1h",
            Self::H1To4h => "h1_to_4h",
            Self::M15SelfAfterSignal => "m15_self_after_signal",
            Self::M15ImpulseRetrace => "m15_impulse_retrace",
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityEventBacktestArgs {
    /// 止损百分比；在 structure_with_cap 模式下也作为最大风险上限。
    pub stop_loss_pct: f64,
    /// 止损模式。
    pub stop_loss_mode: MarketVelocityStopLossMode,
    /// 结构止损最小百分比；0 表示不对结构锚点额外放宽。
    pub structure_stop_min_pct: f64,
    /// 列表数据。
    pub target_rs: Vec<f64>,
    /// 入场周期，用于行情、K 线或市场扫描。
    pub entry_period: usize,
    /// 入场允许的最大距离百分比。
    pub entry_max_distance_pct: f64,
    /// 入场最小volume 比例。
    pub entry_min_volume_ratio: f64,
    /// RSI14 下限；为空时不启用 RSI 下限过滤。
    pub entry_min_rsi: Option<f64>,
    /// RSI14 上限；为空时不启用 RSI 上限过滤。
    pub entry_max_rsi: Option<f64>,
    /// 当前 RSI14 相对回看 K 线的最小抬升幅度；为空时不启用。
    pub entry_min_rsi_delta: Option<f64>,
    /// RSI 抬升确认回看 15m K 线数量。
    pub entry_rsi_delta_lookback_candles: usize,
    /// 是否要求 15m 收盘价突破布林带外轨。
    pub entry_bollinger_breakout: bool,
    /// 布林带宽度相对上一根的最小扩张百分比；为空时不启用。
    pub entry_min_bollinger_bandwidth_expansion_pct: Option<f64>,
    /// 当前 15m K 线实体占全振幅的最小百分比；为空时不启用。
    pub entry_min_body_ratio_pct: Option<f64>,
    /// 当前 15m K 线收盘贴近方向极值的最小百分比；为空时不启用。
    pub entry_min_close_position_pct: Option<f64>,
    /// 当前 15m K 线振幅相对前置窗口平均振幅的最小倍数；为空时不启用。
    pub entry_min_range_expansion_ratio: Option<f64>,
    /// 是否把极端量 K 线按实体/主导影线反向解释；仅限独立反转研究。
    pub entry_extreme_volume_contrarian: bool,
    /// 是否把极端量大实体 K 线按历史趋势同向解释；仅限独立延续研究。
    pub entry_extreme_volume_continuation: bool,
    /// 是否用过去十个 UTC 日相同 15m 时点均量替代连续 K 线均量。
    pub entry_relative_volume_at_time_10d: bool,
    /// 入场前回看窗口内要求出现的最小回撤幅度；为空时不启用。
    pub entry_min_recent_drawdown_pct: Option<f64>,
    /// 近期回撤确认回看 15m K 线数量，不包含当前突破 K 线。
    pub entry_recent_drawdown_lookback_candles: usize,
    /// 触发 K 线之前用于判断反向净趋势的回看 K 线数量。
    pub entry_opposite_move_lookback_candles: usize,
    /// 反向窗口首根开盘到末根收盘的最小净幅度；为空时不启用。
    pub entry_min_opposite_net_move_pct: Option<f64>,
    /// 不要求固定幅度时，反向趋势至少持续的 15m K 线数量；为空时不启用时间分支。
    pub entry_min_opposite_duration_candles: Option<usize>,
    /// 反向持续趋势线性回归的最小 R²；默认保持历史版本 0.70。
    pub entry_opposite_duration_min_r_squared: f64,
    /// 当前反转簇相对历史最强已确认极值簇的最小成交量比例；为空时不启用。
    pub entry_min_exhaustion_volume_dominance_ratio: Option<f64>,
    /// 入场前 96 根 BTC 15m K 线允许的最大绝对净涨跌幅；为空时不启用基准震荡门禁。
    pub entry_btc_96_max_abs_net_move_pct: Option<f64>,
    /// 入场前 384 根 BTC 15m K 线沿交易方向要求的最小净幅度；0 表示只要求同向。
    pub entry_btc_384_min_directional_net_move_pct: Option<f64>,
    /// 是否要求入场前最后一根已完成 BTC 15m K 线与交易方向一致。
    pub entry_btc_require_current_directional_candle: bool,
    /// 是否按触发量比分层选择 ATR14 止盈距离。
    pub volume_atr_take_profit: bool,
    /// 原始 Volume-ATR 目标的风险归一化缩放倍数。
    pub volume_atr_target_scale: f64,
    /// Volume-ATR 目标下限；为空时保留旧版本原始目标。
    pub volume_atr_min_target_r: Option<f64>,
    /// Volume-ATR 目标上限；为空时保留旧版本原始目标。
    pub volume_atr_max_target_r: Option<f64>,
    /// 回测单边手续费 bps；为空时沿用框架旧默认值。
    pub backtest_fee_bps_per_side: Option<f64>,
    /// 回测单边滑点 bps，以等价比例交易成本计入。
    pub backtest_slippage_bps_per_side: f64,
    /// 放量大阴线是否只创建待确认做多 setup，而不立即入场。
    pub entry_defer_bearish_continuation: bool,
    /// 放量大阳线是否只创建待确认做空 setup，而不立即入场。
    pub entry_defer_bullish_continuation: bool,
    /// 是否启用仅限研究的做多长下引线灰区 setup；启用后只允许下一根确认、再下一根开盘入场。
    pub entry_defer_long_lower_wick_reversal: bool,
    /// 是否启用仅限研究的做多阳线锤头灰区反转；满足当前收盘确认后即时入场。
    pub entry_long_bullish_hammer_reversal: bool,
    /// 是否启用仅限研究的信号前两个连续 24 根窗口同向恢复门禁。
    pub entry_require_two_stage_recovery: bool,
    /// 是否要求收盘价 MACD(12,26,9) 前一根负柱、当前柱体回升；仅限反转研究。
    pub entry_require_macd_negative_histogram_improving: bool,
    /// 反向策略是否要求信号 K 线形成方向对称的实体突破确认。
    pub entry_require_opposite_reversal_confirmation: bool,
    /// 反转确认收盘是否必须回到 EMA20 与 SMA20 的目标方向一侧。
    pub entry_require_reversal_average_reclaim: bool,
    /// 是否要求做多信号收盘突破最近一个 5+5 已确认摆动高点；仅限反转研究。
    pub entry_require_bullish_structure_break: bool,
    /// 放量大阴线后等待止跌确认的最大 15m K 线数量。
    pub entry_defer_max_wait_candles: usize,
    /// 同一交易对再次入场前需要等待的 15m K 线数量；为空时不启用。
    pub entry_symbol_cooldown_candles: Option<usize>,
    /// 每次历史趋势从中性首次成立后，只消费第一个完整入场信号。
    pub entry_once_per_opposite_trend_state: bool,
    /// 每次历史趋势从中性首次成立后，只消费第一个同向延续信号。
    pub entry_once_per_historical_trend_state: bool,
    /// 极端量反转 setup 后，是否等待最多三根已完成 K 线收盘收回 setup 开盘价。
    pub entry_wait_setup_open_reclaim: bool,
    /// 已消费趋势重新武装前必须连续保持中性的 15m K 线数量；默认 1 保持 V1 行为。
    pub entry_opposite_trend_reset_confirm_candles: usize,
    /// signal 当前价到实际入场价允许的最大回撤百分比；为空时不启用。
    pub entry_max_signal_pullback_pct: Option<f64>,
    /// 未发生前高回踩时，允许的最大信号后跳空入场百分比；为空时不启用。
    pub entry_max_gap_without_retest_pct: Option<f64>,
    /// 前高回踩确认允许的价格容差百分比。
    pub entry_retest_tolerance_pct: f64,
    /// 是否等待原始入场信号后的结构回踩确认。
    pub entry_retest_after_signal: bool,
    /// 原始入场信号后等待结构回踩确认的最大 15m K 线数量。
    pub entry_retest_max_wait_candles: usize,
    /// 回踩确认后下一根入场开盘相对确认收盘的最小 gap 百分比；为空时不启用。
    pub entry_retest_min_entry_open_gap_pct: Option<f64>,
    /// 回踩确认后下一根开盘若弱于确认收盘，允许用更高确认量能作为例外通行；为空时不启用。
    pub entry_retest_open_fade_min_volume_ratio: Option<f64>,
    /// 趋势过滤的最小平均距离百分比。
    pub trend_min_average_distance_pct: f64,
    /// 趋势过滤周期；off 表示只用 15m 入场确认，不再要求更高周期趋势。
    pub trend_timeframe: MarketVelocityTrendTimeframe,
    /// 最小delta排名，用于控制策略触发门槛。
    pub min_delta_rank: i32,
    /// 最大排名变化；为空时不限制。
    pub max_delta_rank: Option<i32>,
    /// 最小价格涨跌幅百分比。
    pub min_price_change_pct: Option<f64>,
    /// 最大价格涨跌幅百分比；为空时不限制，用于避免追高。
    pub max_price_change_pct: Option<f64>,
    /// 事件开始时间毫秒时间戳；为空时不限制。
    pub event_start_ms: Option<i64>,
    /// 事件结束时间毫秒时间戳；为空时不限制。
    pub event_end_ms: Option<i64>,
    /// 最大15mstaleness最小，用于控制策略触发门槛。
    pub max_15m_staleness_min: i64,
    /// 最大4hstaleness最小，用于控制策略触发门槛。
    pub max_4h_staleness_min: i64,
    /// samplelimit，用于行情、K 线或市场扫描。
    pub sample_limit: usize,
    /// K 线样本抽样种子；仅对 kline_15m 数据源生效，保证随机样本可复现。
    pub sample_seed: String,
    /// 版本化历史币池 manifest；设置后替代随机 sample，并按事件时点使用对应月成员。
    pub historical_universe_manifest: Option<PathBuf>,
    /// 是否用 15m K 线重建滚动 24 小时成交额排名动量，而不是为每根 K 线生成事件。
    pub kline_volume_rank_velocity: bool,
    /// 排名上升时是否同时要求本标的滚动 24 小时成交额较前一快照增长。
    pub kline_volume_rank_require_turnover_growth: bool,
    /// 是否要求当前和前一根 15m 收盘的排名都连续改善。
    pub kline_volume_rank_require_consecutive_improvement: bool,
    /// K 线研究是否只加载当前仍为 live 的 OKX USDT 线性永续。
    pub kline_current_live_only: bool,
    /// event来源，用于行情、K 线或市场扫描。
    pub event_source: MarketVelocityEventSource,
    /// tradedirection，用于行情、K 线或市场扫描。
    pub trade_direction: MarketVelocityTradeDirection,
    /// 模拟盘outcomesink，用于行情、K 线或市场扫描。
    pub paper_outcome_sink: MarketVelocityPaperOutcomeSink,
    /// 观察任务是否把确认后的策略信号写入 Web signal-only 订阅日志。
    pub paper_strategy_signal_sink: MarketVelocityPaperStrategySignalSink,
    /// paper strategy preset 名称，用于提交给 Web 的可审计信号 payload。
    pub paper_strategy_preset: String,
    /// 模拟盘outcome入场ruleversion，用于行情、K 线或市场扫描。
    pub paper_outcome_entry_rule_version: String,
    /// 列表数据。
    pub entry_trigger_allowlist: Vec<String>,
    /// 列表数据。
    pub entry_trigger_blocklist: Vec<String>,
    /// 列表数据。
    pub symbol_blocklist: Vec<String>,
    /// 止损reentry模式，用于行情、K 线或市场扫描。
    pub stop_reentry_mode: StopReentryMode,
    /// fvg入场模式，用于行情、K 线或市场扫描。
    pub fvg_entry_mode: FvgEntryMode,
    /// fvglookbackK 线，用于行情、K 线或市场扫描。
    pub fvg_lookback_candles: usize,
    /// fvg最大waitK 线，用于行情、K 线或市场扫描。
    pub fvg_max_wait_candles: usize,
    /// m15 impulse retrace 挂单距离 FVG 下沿的百分比。
    pub fvg_impulse_retrace_fill_pct: f64,
    /// m15 impulse retrace 从 signal 到允许 fill 之间至少等待的完整 15m K 线数量。
    pub fvg_impulse_retrace_min_wait_candles: usize,
    /// 达到指定 R 倍数后启用利润保护；为空时不启用。
    pub profit_protect_after_r: Option<f64>,
    /// 收益protect止损r，用于行情、K 线或市场扫描。
    pub profit_protect_stop_r: f64,
    /// runnertargetR 倍数；为空时表示该条件不启用。
    pub runner_target_r: Option<f64>,
    /// runnerfraction，用于行情、K 线或市场扫描。
    pub runner_fraction: f64,
    /// runner止损r，用于行情、K 线或市场扫描。
    pub runner_stop_r: f64,
    /// 无盈利时提前退出所需 K 线数量；为空时不启用。
    pub early_exit_no_profit_candles: Option<usize>,
    /// 框架权益回放的最大持仓小时数；为空时保留历史上的无限制行为。
    pub equity_max_holding_hours: Option<usize>,
    /// 持仓期间忽略同交易对重复入场信号对止损/止盈的更新。
    pub ignore_entry_signal_updates_while_open: bool,
    /// 权益报告，用于行情、K 线或市场扫描。
    pub equity_report: bool,
    /// 权益split报告，用于行情、K 线或市场扫描。
    pub equity_split_report: bool,
    /// 权益quartile报告，用于行情、K 线或市场扫描。
    pub equity_quartile_report: bool,
    /// 权益trigger报告，用于行情、K 线或市场扫描。
    pub equity_trigger_report: bool,
    /// 权益concentration报告，用于行情、K 线或市场扫描。
    pub equity_concentration_report: bool,
    /// 权益feature报告，用于行情、K 线或市场扫描。
    pub equity_feature_report: bool,
    /// 仅按实际成交交易输出 15m 量价与后续回收诊断，不重新回放筛选后的信号。
    pub equity_price_volume_diagnostic_report: bool,
    /// 权益交易对window报告，用于行情、K 线或市场扫描。
    pub equity_symbol_window_report: bool,
    /// 权益trade报告，用于行情、K 线或市场扫描。
    pub equity_trade_report: bool,
    /// savebacktest详情，用于行情、K 线或市场扫描。
    pub save_backtest_detail: bool,
    /// 最小trades，用于控制策略触发门槛。
    pub min_trades: usize,
}
impl Default for MarketVelocityEventBacktestArgs {
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
    fn default() -> Self {
        Self {
            stop_loss_pct: 0.03,
            stop_loss_mode: MarketVelocityStopLossMode::FixedPct,
            structure_stop_min_pct: 0.0,
            target_rs: DEFAULT_TARGET_RS.to_vec(),
            entry_period: 20,
            entry_max_distance_pct: 3.0,
            entry_min_volume_ratio: 1.0,
            entry_min_rsi: None,
            entry_max_rsi: None,
            entry_min_rsi_delta: None,
            entry_rsi_delta_lookback_candles: 3,
            entry_bollinger_breakout: false,
            entry_min_bollinger_bandwidth_expansion_pct: None,
            entry_min_body_ratio_pct: None,
            entry_min_close_position_pct: None,
            entry_min_range_expansion_ratio: None,
            entry_extreme_volume_contrarian: false,
            entry_extreme_volume_continuation: false,
            entry_relative_volume_at_time_10d: false,
            entry_min_recent_drawdown_pct: None,
            entry_recent_drawdown_lookback_candles: 12,
            entry_opposite_move_lookback_candles: 12,
            entry_min_opposite_net_move_pct: None,
            entry_min_opposite_duration_candles: None,
            entry_opposite_duration_min_r_squared: 0.70,
            entry_min_exhaustion_volume_dominance_ratio: None,
            entry_btc_96_max_abs_net_move_pct: None,
            entry_btc_384_min_directional_net_move_pct: None,
            entry_btc_require_current_directional_candle: false,
            volume_atr_take_profit: false,
            volume_atr_target_scale: 1.0,
            volume_atr_min_target_r: None,
            volume_atr_max_target_r: None,
            backtest_fee_bps_per_side: None,
            backtest_slippage_bps_per_side: 0.0,
            entry_defer_bearish_continuation: false,
            entry_defer_bullish_continuation: false,
            entry_defer_long_lower_wick_reversal: false,
            entry_long_bullish_hammer_reversal: false,
            entry_require_two_stage_recovery: false,
            entry_require_macd_negative_histogram_improving: false,
            entry_require_opposite_reversal_confirmation: false,
            entry_require_reversal_average_reclaim: false,
            entry_require_bullish_structure_break: false,
            entry_defer_max_wait_candles: 3,
            entry_symbol_cooldown_candles: None,
            entry_once_per_opposite_trend_state: false,
            entry_once_per_historical_trend_state: false,
            entry_wait_setup_open_reclaim: false,
            entry_opposite_trend_reset_confirm_candles: 1,
            entry_max_signal_pullback_pct: None,
            entry_max_gap_without_retest_pct: None,
            entry_retest_tolerance_pct: 0.3,
            entry_retest_after_signal: false,
            entry_retest_max_wait_candles: 8,
            entry_retest_min_entry_open_gap_pct: None,
            entry_retest_open_fade_min_volume_ratio: None,
            trend_min_average_distance_pct: 0.0,
            trend_timeframe: MarketVelocityTrendTimeframe::FourHour,
            min_delta_rank: 10,
            max_delta_rank: None,
            min_price_change_pct: None,
            max_price_change_pct: None,
            event_start_ms: None,
            event_end_ms: None,
            max_15m_staleness_min: 30,
            max_4h_staleness_min: 240,
            sample_limit: 5,
            sample_seed: "default".to_string(),
            historical_universe_manifest: None,
            kline_volume_rank_velocity: false,
            kline_volume_rank_require_turnover_growth: false,
            kline_volume_rank_require_consecutive_improvement: false,
            kline_current_live_only: false,
            event_source: MarketVelocityEventSource::Episodes,
            trade_direction: MarketVelocityTradeDirection::Long,
            paper_outcome_sink: MarketVelocityPaperOutcomeSink::Off,
            paper_strategy_signal_sink: MarketVelocityPaperStrategySignalSink::Off,
            paper_strategy_preset: String::new(),
            paper_outcome_entry_rule_version: DEFAULT_PAPER_OUTCOME_ENTRY_RULE_VERSION.to_string(),
            entry_trigger_allowlist: Vec::new(),
            entry_trigger_blocklist: Vec::new(),
            symbol_blocklist: Vec::new(),
            stop_reentry_mode: StopReentryMode::Off,
            fvg_entry_mode: FvgEntryMode::Off,
            fvg_lookback_candles: DEFAULT_FVG_LOOKBACK_CANDLES,
            fvg_max_wait_candles: DEFAULT_FVG_MAX_WAIT_CANDLES,
            fvg_impulse_retrace_fill_pct: 20.0,
            fvg_impulse_retrace_min_wait_candles: 0,
            profit_protect_after_r: None,
            profit_protect_stop_r: 0.0,
            runner_target_r: None,
            runner_fraction: 0.0,
            runner_stop_r: 0.0,
            early_exit_no_profit_candles: None,
            equity_max_holding_hours: None,
            ignore_entry_signal_updates_while_open: false,
            equity_report: false,
            equity_split_report: false,
            equity_quartile_report: false,
            equity_trigger_report: false,
            equity_concentration_report: false,
            equity_feature_report: false,
            equity_price_volume_diagnostic_report: false,
            equity_symbol_window_report: false,
            equity_trade_report: false,
            save_backtest_detail: false,
            min_trades: 30,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityPaperObservationCommand {
    /// backtestargs，用于行情、K 线或市场扫描。
    pub backtest_args: MarketVelocityEventBacktestArgs,
    /// 秒级时长。
    pub loop_interval_seconds: Option<u64>,
}
/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
pub fn parse_cli_args_from<I, S>(args: I) -> Result<MarketVelocityEventBacktestArgs>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut parsed = MarketVelocityEventBacktestArgs::default();
    let mut entry_trigger_allowlist_explicit = false;
    let mut entry_trigger_blocklist_explicit = false;
    let mut paper_outcome_entry_rule_version_explicit = false;
    let mut higher_timeframe_trend_control_explicit = false;
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stop-loss-pct" => parsed.stop_loss_pct = parse_next(&mut args, &arg)?,
            "--stop-loss-mode" => {
                parsed.stop_loss_mode =
                    MarketVelocityStopLossMode::from_str(&next_arg(&mut args, &arg)?)?
            }
            "--structure-stop-min-pct" => {
                parsed.structure_stop_min_pct = parse_next(&mut args, &arg)?
            }
            "--target-rs" => parsed.target_rs = parse_target_rs(&next_arg(&mut args, &arg)?)?,
            "--entry-period" => parsed.entry_period = parse_next(&mut args, &arg)?,
            "--entry-max-distance-pct" => {
                parsed.entry_max_distance_pct = parse_next(&mut args, &arg)?
            }
            "--entry-min-volume-ratio" => {
                parsed.entry_min_volume_ratio = parse_next(&mut args, &arg)?
            }
            "--entry-min-rsi" => parsed.entry_min_rsi = Some(parse_next(&mut args, &arg)?),
            "--entry-max-rsi" => parsed.entry_max_rsi = Some(parse_next(&mut args, &arg)?),
            "--entry-min-rsi-delta" => {
                parsed.entry_min_rsi_delta = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-rsi-delta-lookback-candles" => {
                parsed.entry_rsi_delta_lookback_candles = parse_next(&mut args, &arg)?
            }
            "--entry-bollinger-breakout" => parsed.entry_bollinger_breakout = true,
            "--entry-min-bollinger-bandwidth-expansion-pct" => {
                parsed.entry_min_bollinger_bandwidth_expansion_pct =
                    Some(parse_next(&mut args, &arg)?)
            }
            "--entry-min-body-ratio-pct" => {
                parsed.entry_min_body_ratio_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-min-close-position-pct" => {
                parsed.entry_min_close_position_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-min-range-expansion-ratio" => {
                parsed.entry_min_range_expansion_ratio = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-extreme-volume-contrarian" => parsed.entry_extreme_volume_contrarian = true,
            "--entry-extreme-volume-continuation" => {
                parsed.entry_extreme_volume_continuation = true
            }
            "--entry-relative-volume-at-time-10d" => {
                parsed.entry_relative_volume_at_time_10d = true
            }
            "--entry-min-recent-drawdown-pct" => {
                parsed.entry_min_recent_drawdown_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-recent-drawdown-lookback-candles" => {
                parsed.entry_recent_drawdown_lookback_candles = parse_next(&mut args, &arg)?
            }
            "--entry-opposite-move-lookback-candles" => {
                parsed.entry_opposite_move_lookback_candles = parse_next(&mut args, &arg)?
            }
            "--entry-min-opposite-net-move-pct" => {
                parsed.entry_min_opposite_net_move_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-min-opposite-duration-candles" => {
                parsed.entry_min_opposite_duration_candles = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-opposite-duration-min-r-squared" => {
                parsed.entry_opposite_duration_min_r_squared = parse_next(&mut args, &arg)?
            }
            "--entry-min-exhaustion-volume-dominance-ratio" => {
                parsed.entry_min_exhaustion_volume_dominance_ratio =
                    Some(parse_next(&mut args, &arg)?)
            }
            "--entry-btc-96-max-abs-net-move-pct" => {
                parsed.entry_btc_96_max_abs_net_move_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-btc-384-min-directional-net-move-pct" => {
                parsed.entry_btc_384_min_directional_net_move_pct =
                    Some(parse_next(&mut args, &arg)?)
            }
            "--entry-btc-require-current-directional-candle" => {
                parsed.entry_btc_require_current_directional_candle = true
            }
            "--volume-atr-take-profit" => parsed.volume_atr_take_profit = true,
            "--volume-atr-target-scale" => {
                parsed.volume_atr_target_scale = parse_next(&mut args, &arg)?
            }
            "--volume-atr-min-target-r" => {
                parsed.volume_atr_min_target_r = Some(parse_next(&mut args, &arg)?)
            }
            "--volume-atr-max-target-r" => {
                parsed.volume_atr_max_target_r = Some(parse_next(&mut args, &arg)?)
            }
            "--backtest-fee-bps-per-side" => {
                parsed.backtest_fee_bps_per_side = Some(parse_next(&mut args, &arg)?)
            }
            "--backtest-slippage-bps-per-side" => {
                parsed.backtest_slippage_bps_per_side = parse_next(&mut args, &arg)?
            }
            "--entry-defer-bearish-continuation" => parsed.entry_defer_bearish_continuation = true,
            "--entry-defer-bullish-continuation" => parsed.entry_defer_bullish_continuation = true,
            "--entry-defer-long-lower-wick-reversal" => {
                parsed.entry_defer_long_lower_wick_reversal = true
            }
            "--entry-long-bullish-hammer-reversal" => {
                parsed.entry_long_bullish_hammer_reversal = true
            }
            "--entry-require-two-stage-recovery" => parsed.entry_require_two_stage_recovery = true,
            "--entry-require-macd-negative-histogram-improving" => {
                parsed.entry_require_macd_negative_histogram_improving = true
            }
            "--entry-require-opposite-reversal-confirmation" => {
                parsed.entry_require_opposite_reversal_confirmation = true
            }
            "--entry-require-reversal-average-reclaim" => {
                parsed.entry_require_reversal_average_reclaim = true
            }
            "--entry-require-bullish-structure-break" => {
                parsed.entry_require_bullish_structure_break = true
            }
            "--entry-defer-max-wait-candles" => {
                parsed.entry_defer_max_wait_candles = parse_next(&mut args, &arg)?
            }
            "--entry-symbol-cooldown-candles" => {
                parsed.entry_symbol_cooldown_candles = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-once-per-opposite-trend-state" => {
                parsed.entry_once_per_opposite_trend_state = true
            }
            "--entry-once-per-historical-trend-state" => {
                parsed.entry_once_per_historical_trend_state = true
            }
            "--entry-wait-setup-open-reclaim" => parsed.entry_wait_setup_open_reclaim = true,
            "--entry-opposite-trend-reset-confirm-candles" => {
                parsed.entry_opposite_trend_reset_confirm_candles = parse_next(&mut args, &arg)?
            }
            "--entry-max-signal-pullback-pct" => {
                parsed.entry_max_signal_pullback_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-max-gap-without-retest-pct" => {
                parsed.entry_max_gap_without_retest_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-retest-tolerance-pct" => {
                parsed.entry_retest_tolerance_pct = parse_next(&mut args, &arg)?
            }
            "--entry-retest-after-signal" => parsed.entry_retest_after_signal = true,
            "--entry-retest-max-wait-candles" => {
                parsed.entry_retest_max_wait_candles = parse_next(&mut args, &arg)?
            }
            "--entry-retest-min-entry-open-gap-pct" => {
                parsed.entry_retest_min_entry_open_gap_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--entry-retest-open-fade-min-volume-ratio" => {
                parsed.entry_retest_open_fade_min_volume_ratio = Some(parse_next(&mut args, &arg)?)
            }
            "--trend-min-average-distance-pct" => {
                higher_timeframe_trend_control_explicit = true;
                parsed.trend_min_average_distance_pct = parse_next(&mut args, &arg)?
            }
            "--trend-timeframe" => {
                higher_timeframe_trend_control_explicit = true;
                parsed.trend_timeframe =
                    MarketVelocityTrendTimeframe::from_str(&next_arg(&mut args, &arg)?)?
            }
            "--min-delta-rank" => parsed.min_delta_rank = parse_next(&mut args, &arg)?,
            "--max-delta-rank" => parsed.max_delta_rank = Some(parse_next(&mut args, &arg)?),
            "--min-price-change-pct" => {
                parsed.min_price_change_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--max-price-change-pct" => {
                parsed.max_price_change_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--event-start-ms" => parsed.event_start_ms = Some(parse_next(&mut args, &arg)?),
            "--event-end-ms" => parsed.event_end_ms = Some(parse_next(&mut args, &arg)?),
            "--max-15m-staleness-min" => {
                parsed.max_15m_staleness_min = parse_next(&mut args, &arg)?
            }
            "--max-4h-staleness-min" => parsed.max_4h_staleness_min = parse_next(&mut args, &arg)?,
            "--sample-limit" => parsed.sample_limit = parse_next(&mut args, &arg)?,
            "--sample-seed" => parsed.sample_seed = next_arg(&mut args, &arg)?,
            "--historical-universe-manifest" => {
                parsed.historical_universe_manifest =
                    Some(PathBuf::from(next_arg(&mut args, &arg)?))
            }
            "--kline-volume-rank-velocity" => parsed.kline_volume_rank_velocity = true,
            "--kline-volume-rank-require-turnover-growth" => {
                parsed.kline_volume_rank_require_turnover_growth = true
            }
            "--kline-volume-rank-require-consecutive-improvement" => {
                parsed.kline_volume_rank_require_consecutive_improvement = true
            }
            "--kline-current-live-only" => parsed.kline_current_live_only = true,
            "--event-source" => {
                parsed.event_source =
                    MarketVelocityEventSource::from_str(&next_arg(&mut args, &arg)?)?
            }
            "--trade-direction" => {
                parsed.trade_direction =
                    MarketVelocityTradeDirection::from_str(&next_arg(&mut args, &arg)?)?
            }
            "--paper-outcome-sink" => {
                parsed.paper_outcome_sink =
                    MarketVelocityPaperOutcomeSink::from_str(&next_arg(&mut args, &arg)?)?
            }
            "--paper-strategy-signal-sink" => {
                parsed.paper_strategy_signal_sink =
                    MarketVelocityPaperStrategySignalSink::from_str(&next_arg(&mut args, &arg)?)?
            }
            "--paper-outcome-entry-rule-version" => {
                paper_outcome_entry_rule_version_explicit = true;
                parsed.paper_outcome_entry_rule_version = next_arg(&mut args, &arg)?
            }
            "--entry-trigger-allowlist" => {
                entry_trigger_allowlist_explicit = true;
                parsed.entry_trigger_allowlist =
                    parse_entry_trigger_list(&next_arg(&mut args, &arg)?)?
            }
            "--entry-trigger-blocklist" => {
                entry_trigger_blocklist_explicit = true;
                parsed.entry_trigger_blocklist =
                    parse_entry_trigger_list(&next_arg(&mut args, &arg)?)?
            }
            "--symbol-blocklist" => {
                parsed.symbol_blocklist = parse_symbol_list(&next_arg(&mut args, &arg)?)?
            }
            "--stop-reentry-mode" => {
                parsed.stop_reentry_mode = StopReentryMode::from_str(&next_arg(&mut args, &arg)?)?
            }
            "--fvg-entry-mode" => {
                parsed.fvg_entry_mode = FvgEntryMode::from_str(&next_arg(&mut args, &arg)?)?
            }
            "--fvg-lookback-candles" => parsed.fvg_lookback_candles = parse_next(&mut args, &arg)?,
            "--fvg-max-wait-candles" => parsed.fvg_max_wait_candles = parse_next(&mut args, &arg)?,
            "--fvg-impulse-retrace-fill-pct" => {
                parsed.fvg_impulse_retrace_fill_pct = parse_next(&mut args, &arg)?
            }
            "--fvg-impulse-retrace-min-wait-candles" => {
                parsed.fvg_impulse_retrace_min_wait_candles = parse_next(&mut args, &arg)?
            }
            "--profit-protect-after-r" => {
                parsed.profit_protect_after_r = Some(parse_next(&mut args, &arg)?)
            }
            "--profit-protect-stop-r" => {
                parsed.profit_protect_stop_r = parse_next(&mut args, &arg)?
            }
            "--runner-target-r" => parsed.runner_target_r = Some(parse_next(&mut args, &arg)?),
            "--runner-fraction" => parsed.runner_fraction = parse_next(&mut args, &arg)?,
            "--runner-stop-r" => parsed.runner_stop_r = parse_next(&mut args, &arg)?,
            "--early-exit-no-profit-candles" => {
                parsed.early_exit_no_profit_candles = Some(parse_next(&mut args, &arg)?)
            }
            "--equity-max-holding-hours" => {
                parsed.equity_max_holding_hours = Some(parse_next(&mut args, &arg)?)
            }
            "--ignore-entry-signal-updates-while-open" => {
                parsed.ignore_entry_signal_updates_while_open = true
            }
            "--equity-report" => parsed.equity_report = true,
            "--equity-split-report" => parsed.equity_split_report = true,
            "--equity-quartile-report" => parsed.equity_quartile_report = true,
            "--equity-trigger-report" => parsed.equity_trigger_report = true,
            "--equity-concentration-report" => parsed.equity_concentration_report = true,
            "--equity-feature-report" => parsed.equity_feature_report = true,
            "--equity-price-volume-diagnostic-report" => {
                parsed.equity_price_volume_diagnostic_report = true
            }
            "--equity-symbol-window-report" => parsed.equity_symbol_window_report = true,
            "--equity-trade-report" => parsed.equity_trade_report = true,
            "--save-backtest-detail" => parsed.save_backtest_detail = true,
            "--min-trades" => parsed.min_trades = parse_next(&mut args, &arg)?,
            "--help" | "-h" => {
                print_market_velocity_event_backtest_usage();
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    if parsed.event_source == MarketVelocityEventSource::Kline15m
        && !higher_timeframe_trend_control_explicit
    {
        parsed.trend_timeframe = MarketVelocityTrendTimeframe::Off;
    }
    if parsed.paper_outcome_sink == MarketVelocityPaperOutcomeSink::Web
        && !entry_trigger_allowlist_explicit
        && !entry_trigger_blocklist_explicit
    {
        parsed.entry_trigger_allowlist = DEFAULT_WEB_PAPER_OUTCOME_ENTRY_TRIGGER_ALLOWLIST
            .iter()
            .map(|value| (*value).to_string())
            .collect();
    }
    validate_args(&parsed, paper_outcome_entry_rule_version_explicit)?;
    Ok(parsed)
}
/// 校验输入和运行前置条件，提前暴露 回测与策略研究 的不可执行原因。
fn validate_args(
    parsed: &MarketVelocityEventBacktestArgs,
    paper_outcome_entry_rule_version_explicit: bool,
) -> Result<()> {
    if parsed.entry_period == 0 {
        bail!("--entry-period must be greater than 0");
    }
    if parsed.kline_volume_rank_velocity
        && parsed.event_source != MarketVelocityEventSource::Kline15m
    {
        bail!("--kline-volume-rank-velocity requires --event-source kline_15m");
    }
    if parsed.kline_volume_rank_require_turnover_growth && !parsed.kline_volume_rank_velocity {
        bail!("--kline-volume-rank-require-turnover-growth requires --kline-volume-rank-velocity");
    }
    if parsed.kline_volume_rank_require_consecutive_improvement
        && !parsed.kline_volume_rank_velocity
    {
        bail!(
            "--kline-volume-rank-require-consecutive-improvement requires --kline-volume-rank-velocity"
        );
    }
    if parsed.historical_universe_manifest.is_some() {
        if parsed.event_source != MarketVelocityEventSource::Kline15m {
            bail!("--historical-universe-manifest requires --event-source kline_15m");
        }
        if parsed.event_start_ms.is_none() || parsed.event_end_ms.is_none() {
            bail!(
                "--historical-universe-manifest requires explicit --event-start-ms and --event-end-ms"
            );
        }
        if parsed.paper_outcome_sink != MarketVelocityPaperOutcomeSink::Off
            || parsed.paper_strategy_signal_sink != MarketVelocityPaperStrategySignalSink::Off
        {
            bail!("--historical-universe-manifest is research-only");
        }
    }
    if parsed.stop_loss_pct <= 0.0 {
        bail!("--stop-loss-pct must be greater than 0");
    }
    if parsed.structure_stop_min_pct < 0.0 || parsed.structure_stop_min_pct >= 1.0 {
        bail!("--structure-stop-min-pct must be zero or greater and lower than 1");
    }
    if parsed.stop_loss_mode == MarketVelocityStopLossMode::StructureWithCap
        && parsed.structure_stop_min_pct > parsed.stop_loss_pct
    {
        bail!(
            "--structure-stop-min-pct must be lower than or equal to --stop-loss-pct when --stop-loss-mode=structure_with_cap"
        );
    }
    if parsed.trend_min_average_distance_pct < 0.0 {
        bail!("--trend-min-average-distance-pct must be zero or greater");
    }
    if let Some(max_pullback) = parsed.entry_max_signal_pullback_pct {
        if max_pullback < 0.0 {
            bail!("--entry-max-signal-pullback-pct must be zero or greater");
        }
    }
    if let Some(max_gap) = parsed.entry_max_gap_without_retest_pct {
        if max_gap < 0.0 {
            bail!("--entry-max-gap-without-retest-pct must be zero or greater");
        }
    }
    if parsed.entry_retest_tolerance_pct < 0.0 {
        bail!("--entry-retest-tolerance-pct must be zero or greater");
    }
    if let Some(min_rsi) = parsed.entry_min_rsi {
        if !(0.0..=100.0).contains(&min_rsi) {
            bail!("--entry-min-rsi must be between 0 and 100");
        }
    }
    if let Some(max_rsi) = parsed.entry_max_rsi {
        if !(0.0..=100.0).contains(&max_rsi) {
            bail!("--entry-max-rsi must be between 0 and 100");
        }
        if parsed
            .entry_min_rsi
            .is_some_and(|min_rsi| max_rsi < min_rsi)
        {
            bail!("--entry-max-rsi must be greater than or equal to --entry-min-rsi");
        }
    }
    if let Some(min_delta) = parsed.entry_min_rsi_delta {
        if min_delta < 0.0 {
            bail!("--entry-min-rsi-delta must be zero or greater");
        }
    }
    if parsed.entry_rsi_delta_lookback_candles == 0 {
        bail!("--entry-rsi-delta-lookback-candles must be greater than 0");
    }
    if let Some(min_expansion) = parsed.entry_min_bollinger_bandwidth_expansion_pct {
        if min_expansion < 0.0 {
            bail!("--entry-min-bollinger-bandwidth-expansion-pct must be zero or greater");
        }
    }
    if let Some(min_body_ratio) = parsed.entry_min_body_ratio_pct {
        if !(0.0..=100.0).contains(&min_body_ratio) {
            bail!("--entry-min-body-ratio-pct must be between 0 and 100");
        }
    }
    if let Some(min_close_position) = parsed.entry_min_close_position_pct {
        if !(0.0..=100.0).contains(&min_close_position) {
            bail!("--entry-min-close-position-pct must be between 0 and 100");
        }
    }
    if let Some(min_range_expansion) = parsed.entry_min_range_expansion_ratio {
        if min_range_expansion < 0.0 {
            bail!("--entry-min-range-expansion-ratio must be zero or greater");
        }
    }
    if let Some(min_drawdown) = parsed.entry_min_recent_drawdown_pct {
        if min_drawdown < 0.0 {
            bail!("--entry-min-recent-drawdown-pct must be zero or greater");
        }
    }
    if parsed.entry_recent_drawdown_lookback_candles == 0 {
        bail!("--entry-recent-drawdown-lookback-candles must be greater than 0");
    }
    if parsed.entry_opposite_move_lookback_candles == 0 {
        bail!("--entry-opposite-move-lookback-candles must be greater than 0");
    }
    if parsed
        .entry_min_opposite_net_move_pct
        .is_some_and(|min_move_pct| min_move_pct <= 0.0)
    {
        bail!("--entry-min-opposite-net-move-pct must be greater than 0");
    }
    if parsed
        .entry_min_opposite_duration_candles
        .is_some_and(|duration_candles| duration_candles < 4)
    {
        bail!("--entry-min-opposite-duration-candles must be at least 4");
    }
    if !parsed.entry_opposite_duration_min_r_squared.is_finite()
        || parsed.entry_opposite_duration_min_r_squared <= 0.0
        || parsed.entry_opposite_duration_min_r_squared > 1.0
    {
        bail!("--entry-opposite-duration-min-r-squared must be within (0, 1]");
    }
    if parsed
        .entry_min_exhaustion_volume_dominance_ratio
        .is_some_and(|dominance_ratio| dominance_ratio <= 0.0)
    {
        bail!("--entry-min-exhaustion-volume-dominance-ratio must be greater than 0");
    }
    if parsed
        .entry_btc_96_max_abs_net_move_pct
        .is_some_and(|max_move_pct| max_move_pct <= 0.0)
    {
        bail!("--entry-btc-96-max-abs-net-move-pct must be greater than 0");
    }
    if parsed
        .entry_btc_384_min_directional_net_move_pct
        .is_some_and(|min_move_pct| !min_move_pct.is_finite() || min_move_pct < 0.0)
    {
        bail!("--entry-btc-384-min-directional-net-move-pct must be finite and non-negative");
    }
    if parsed.volume_atr_target_scale <= 0.0 {
        bail!("--volume-atr-target-scale must be greater than 0");
    }
    if parsed
        .volume_atr_min_target_r
        .is_some_and(|target_r| target_r <= 0.0)
    {
        bail!("--volume-atr-min-target-r must be greater than 0");
    }
    if parsed
        .volume_atr_max_target_r
        .is_some_and(|target_r| target_r <= 0.0)
    {
        bail!("--volume-atr-max-target-r must be greater than 0");
    }
    if let (Some(min_target_r), Some(max_target_r)) = (
        parsed.volume_atr_min_target_r,
        parsed.volume_atr_max_target_r,
    ) {
        if max_target_r < min_target_r {
            bail!(
                "--volume-atr-max-target-r must be greater than or equal to --volume-atr-min-target-r"
            );
        }
    }
    if parsed
        .backtest_fee_bps_per_side
        .is_some_and(|fee_bps| fee_bps < 0.0)
    {
        bail!("--backtest-fee-bps-per-side must be zero or greater");
    }
    if parsed.backtest_slippage_bps_per_side < 0.0 {
        bail!("--backtest-slippage-bps-per-side must be zero or greater");
    }
    if parsed.backtest_slippage_bps_per_side > 0.0 && parsed.backtest_fee_bps_per_side.is_none() {
        bail!("--backtest-slippage-bps-per-side requires explicit --backtest-fee-bps-per-side");
    }
    if (parsed.entry_defer_bearish_continuation || parsed.entry_defer_bullish_continuation)
        && parsed.entry_defer_max_wait_candles == 0
    {
        bail!("--entry-defer-max-wait-candles must be greater than 0");
    }
    if parsed.entry_defer_long_lower_wick_reversal {
        if parsed.trade_direction != MarketVelocityTradeDirection::Long {
            bail!("--entry-defer-long-lower-wick-reversal requires --trade-direction long");
        }
        if parsed.entry_min_opposite_net_move_pct.is_none()
            || parsed.entry_min_opposite_duration_candles.is_none()
        {
            bail!(
                "--entry-defer-long-lower-wick-reversal requires both opposite-move history branches"
            );
        }
        if parsed.entry_defer_max_wait_candles != 1 {
            bail!(
                "--entry-defer-long-lower-wick-reversal requires --entry-defer-max-wait-candles 1"
            );
        }
        if parsed.paper_outcome_sink != MarketVelocityPaperOutcomeSink::Off
            || parsed.paper_strategy_signal_sink != MarketVelocityPaperStrategySignalSink::Off
        {
            bail!("--entry-defer-long-lower-wick-reversal is research-only");
        }
    }
    if parsed.entry_long_bullish_hammer_reversal {
        if parsed.entry_defer_long_lower_wick_reversal {
            bail!("lower-wick deferred and bullish-hammer immediate modes are mutually exclusive");
        }
        if parsed.trade_direction != MarketVelocityTradeDirection::Long {
            bail!("--entry-long-bullish-hammer-reversal requires --trade-direction long");
        }
        if parsed.entry_min_opposite_net_move_pct.is_none()
            || parsed.entry_min_opposite_duration_candles.is_none()
        {
            bail!(
                "--entry-long-bullish-hammer-reversal requires both opposite-move history branches"
            );
        }
        if parsed.paper_outcome_sink != MarketVelocityPaperOutcomeSink::Off
            || parsed.paper_strategy_signal_sink != MarketVelocityPaperStrategySignalSink::Off
        {
            bail!("--entry-long-bullish-hammer-reversal is research-only");
        }
    }
    if parsed.entry_require_two_stage_recovery {
        if parsed.entry_defer_long_lower_wick_reversal || parsed.entry_long_bullish_hammer_reversal
        {
            bail!("two-stage recovery and lower-wick buffer modes are mutually exclusive");
        }
        if parsed.trade_direction != MarketVelocityTradeDirection::Long {
            bail!("--entry-require-two-stage-recovery requires --trade-direction long");
        }
        if parsed.entry_min_opposite_net_move_pct.is_none()
            || parsed.entry_min_opposite_duration_candles.is_none()
        {
            bail!(
                "--entry-require-two-stage-recovery requires both opposite-move history branches"
            );
        }
        if parsed.paper_outcome_sink != MarketVelocityPaperOutcomeSink::Off
            || parsed.paper_strategy_signal_sink != MarketVelocityPaperStrategySignalSink::Off
        {
            bail!("--entry-require-two-stage-recovery is research-only");
        }
    }
    if parsed.entry_require_macd_negative_histogram_improving {
        if parsed.trade_direction != MarketVelocityTradeDirection::Long {
            bail!(
                "--entry-require-macd-negative-histogram-improving requires --trade-direction long"
            );
        }
        if parsed.entry_min_opposite_net_move_pct.is_none()
            || parsed.entry_min_opposite_duration_candles.is_none()
        {
            bail!(
                "--entry-require-macd-negative-histogram-improving requires both opposite-move history branches"
            );
        }
        if parsed.paper_outcome_sink != MarketVelocityPaperOutcomeSink::Off
            || parsed.paper_strategy_signal_sink != MarketVelocityPaperStrategySignalSink::Off
        {
            bail!("--entry-require-macd-negative-histogram-improving is research-only");
        }
    }
    if parsed.entry_require_bullish_structure_break {
        if parsed.trade_direction != MarketVelocityTradeDirection::Long {
            bail!("--entry-require-bullish-structure-break requires --trade-direction long");
        }
        if parsed.entry_min_opposite_net_move_pct.is_none()
            || parsed.entry_min_opposite_duration_candles.is_none()
        {
            bail!(
                "--entry-require-bullish-structure-break requires both opposite-move history branches"
            );
        }
        if parsed.paper_outcome_sink != MarketVelocityPaperOutcomeSink::Off
            || parsed.paper_strategy_signal_sink != MarketVelocityPaperStrategySignalSink::Off
        {
            bail!("--entry-require-bullish-structure-break is research-only");
        }
    }
    if parsed.entry_extreme_volume_contrarian && parsed.entry_extreme_volume_continuation {
        bail!("extreme-volume contrarian and continuation modes are mutually exclusive");
    }
    if parsed.entry_relative_volume_at_time_10d && !parsed.entry_extreme_volume_continuation {
        bail!("--entry-relative-volume-at-time-10d requires --entry-extreme-volume-continuation");
    }
    if parsed.entry_extreme_volume_contrarian {
        if parsed.event_source != MarketVelocityEventSource::Kline15m
            || parsed.kline_volume_rank_velocity
        {
            bail!(
                "--entry-extreme-volume-contrarian requires direct --event-source kline_15m without rank velocity"
            );
        }
        if parsed.trade_direction != MarketVelocityTradeDirection::Both {
            bail!("--entry-extreme-volume-contrarian requires --trade-direction both");
        }
        if parsed.entry_min_opposite_net_move_pct.is_none()
            || parsed.entry_min_opposite_duration_candles.is_none()
        {
            bail!("--entry-extreme-volume-contrarian requires both opposite-move history branches");
        }
        if parsed.entry_min_range_expansion_ratio.is_none() {
            bail!("--entry-extreme-volume-contrarian requires --entry-min-range-expansion-ratio");
        }
        if parsed.paper_outcome_sink != MarketVelocityPaperOutcomeSink::Off
            || parsed.paper_strategy_signal_sink != MarketVelocityPaperStrategySignalSink::Off
        {
            bail!("--entry-extreme-volume-contrarian is research-only");
        }
    }
    if parsed.entry_extreme_volume_continuation {
        if !parsed.entry_once_per_historical_trend_state {
            bail!(
                "--entry-extreme-volume-continuation requires --entry-once-per-historical-trend-state"
            );
        }
        if parsed.event_source != MarketVelocityEventSource::Kline15m
            || parsed.kline_volume_rank_velocity
        {
            bail!(
                "--entry-extreme-volume-continuation requires direct --event-source kline_15m without rank velocity"
            );
        }
        if parsed.trade_direction != MarketVelocityTradeDirection::Both {
            bail!("--entry-extreme-volume-continuation requires --trade-direction both");
        }
        if parsed.entry_min_opposite_net_move_pct.is_none()
            || parsed.entry_min_opposite_duration_candles.is_none()
        {
            bail!("--entry-extreme-volume-continuation requires both historical-trend branches");
        }
        if parsed.entry_min_body_ratio_pct.is_none()
            || parsed.entry_min_range_expansion_ratio.is_none()
        {
            bail!(
                "--entry-extreme-volume-continuation requires body-ratio and range-expansion filters"
            );
        }
        if parsed.paper_outcome_sink != MarketVelocityPaperOutcomeSink::Off
            || parsed.paper_strategy_signal_sink != MarketVelocityPaperStrategySignalSink::Off
        {
            bail!("--entry-extreme-volume-continuation is research-only");
        }
    }
    if parsed.entry_once_per_opposite_trend_state {
        if !parsed.entry_extreme_volume_contrarian {
            bail!(
                "--entry-once-per-opposite-trend-state requires --entry-extreme-volume-contrarian"
            );
        }
        if parsed.trend_timeframe != MarketVelocityTrendTimeframe::Off {
            bail!("--entry-once-per-opposite-trend-state requires --trend-timeframe off");
        }
        if parsed.event_start_ms.is_none() || parsed.event_end_ms.is_none() {
            bail!("--entry-once-per-opposite-trend-state requires explicit event start and end");
        }
        if !parsed.kline_current_live_only || parsed.historical_universe_manifest.is_some() {
            bail!(
                "--entry-once-per-opposite-trend-state requires current-live K-line universe without a historical manifest"
            );
        }
    }
    if parsed.entry_once_per_historical_trend_state {
        if !parsed.entry_extreme_volume_continuation {
            bail!(
                "--entry-once-per-historical-trend-state requires --entry-extreme-volume-continuation"
            );
        }
        if parsed.trend_timeframe != MarketVelocityTrendTimeframe::Off {
            bail!("--entry-once-per-historical-trend-state requires --trend-timeframe off");
        }
        if parsed.event_start_ms.is_none() || parsed.event_end_ms.is_none() {
            bail!("--entry-once-per-historical-trend-state requires explicit event start and end");
        }
        if !parsed.kline_current_live_only || parsed.historical_universe_manifest.is_some() {
            bail!(
                "--entry-once-per-historical-trend-state requires current-live K-line universe without a historical manifest"
            );
        }
    }
    if parsed.entry_opposite_trend_reset_confirm_candles == 0 {
        bail!("--entry-opposite-trend-reset-confirm-candles must be greater than 0");
    }
    if parsed.entry_wait_setup_open_reclaim && !parsed.entry_once_per_opposite_trend_state {
        bail!("--entry-wait-setup-open-reclaim requires --entry-once-per-opposite-trend-state");
    }
    if parsed.entry_wait_setup_open_reclaim && parsed.entry_defer_max_wait_candles != 3 {
        bail!("--entry-wait-setup-open-reclaim requires --entry-defer-max-wait-candles 3");
    }
    if parsed.entry_opposite_trend_reset_confirm_candles != 1
        && !parsed.entry_once_per_opposite_trend_state
        && !parsed.entry_once_per_historical_trend_state
    {
        bail!("--entry-opposite-trend-reset-confirm-candles requires a one-shot trend-state mode");
    }
    if parsed.kline_current_live_only && parsed.event_source != MarketVelocityEventSource::Kline15m
    {
        bail!("--kline-current-live-only requires --event-source kline_15m");
    }
    if parsed.entry_symbol_cooldown_candles == Some(0) {
        bail!("--entry-symbol-cooldown-candles must be greater than 0");
    }
    if let Some(min_volume_ratio) = parsed.entry_retest_open_fade_min_volume_ratio {
        if min_volume_ratio < 0.0 {
            bail!("--entry-retest-open-fade-min-volume-ratio must be zero or greater");
        }
    }
    if parsed.entry_retest_max_wait_candles == 0 {
        bail!("--entry-retest-max-wait-candles must be greater than 0");
    }
    if parsed.min_trades == 0 {
        bail!("--min-trades must be greater than 0");
    }
    if parsed.paper_strategy_signal_sink == MarketVelocityPaperStrategySignalSink::Web
        && parsed.trade_direction == MarketVelocityTradeDirection::Both
    {
        bail!("--paper-strategy-signal-sink web requires a single trade direction");
    }
    if let Some(max_delta_rank) = parsed.max_delta_rank {
        if max_delta_rank < parsed.min_delta_rank {
            bail!("--max-delta-rank must be greater than or equal to --min-delta-rank");
        }
    }
    if let Some(min_price_change_pct) = parsed.min_price_change_pct {
        if min_price_change_pct < 0.0 {
            bail!("--min-price-change-pct must be zero or greater");
        }
    }
    if let Some(max_price_change_pct) = parsed.max_price_change_pct {
        if max_price_change_pct < 0.0 {
            bail!("--max-price-change-pct must be zero or greater");
        }
        if parsed
            .min_price_change_pct
            .is_some_and(|min_price_change_pct| max_price_change_pct < min_price_change_pct)
        {
            bail!("--max-price-change-pct must be greater than or equal to --min-price-change-pct");
        }
    }
    if let (Some(event_start_ms), Some(event_end_ms)) = (parsed.event_start_ms, parsed.event_end_ms)
    {
        if event_end_ms < event_start_ms {
            bail!("--event-end-ms must be greater than or equal to --event-start-ms");
        }
    }
    if parsed.fvg_lookback_candles == 0 {
        bail!("--fvg-lookback-candles must be greater than 0");
    }
    if parsed.fvg_max_wait_candles == 0 {
        bail!("--fvg-max-wait-candles must be greater than 0");
    }
    if parsed.fvg_impulse_retrace_fill_pct <= 0.0 || parsed.fvg_impulse_retrace_fill_pct > 100.0 {
        bail!("--fvg-impulse-retrace-fill-pct must be greater than 0 and at most 100");
    }
    match parsed.profit_protect_after_r {
        Some(after_r) => {
            if after_r <= 0.0 {
                bail!("--profit-protect-after-r must be greater than 0");
            }
            if parsed.profit_protect_stop_r < 0.0 {
                bail!("--profit-protect-stop-r must be zero or greater");
            }
            if parsed.profit_protect_stop_r >= after_r {
                bail!("--profit-protect-stop-r must be lower than --profit-protect-after-r");
            }
        }
        None if parsed.profit_protect_stop_r != 0.0 => {
            bail!("--profit-protect-stop-r requires --profit-protect-after-r");
        }
        None => {}
    }
    match parsed.runner_target_r {
        Some(target_r) => {
            if target_r <= 0.0 {
                bail!("--runner-target-r must be greater than 0");
            }
            if parsed.runner_fraction <= 0.0 || parsed.runner_fraction >= 1.0 {
                bail!("--runner-fraction must be greater than 0 and lower than 1");
            }
            if parsed.runner_stop_r < 0.0 {
                bail!("--runner-stop-r must be zero or greater");
            }
            if parsed.runner_stop_r >= target_r {
                bail!("--runner-stop-r must be lower than --runner-target-r");
            }
        }
        None if parsed.runner_fraction != 0.0 || parsed.runner_stop_r != 0.0 => {
            bail!("--runner-fraction and --runner-stop-r require --runner-target-r");
        }
        None => {}
    }
    if parsed.early_exit_no_profit_candles == Some(0) {
        bail!("--early-exit-no-profit-candles must be greater than 0");
    }
    if parsed.equity_max_holding_hours == Some(0) {
        bail!("--equity-max-holding-hours must be greater than 0");
    }
    if parsed.equity_max_holding_hours.is_some_and(|hours| {
        i64::try_from(hours)
            .ok()
            .and_then(|hours| hours.checked_mul(60 * 60 * 1_000))
            .is_none()
    }) {
        bail!("--equity-max-holding-hours is too large");
    }
    if parsed.volume_atr_take_profit && parsed.target_rs.len() != 1 {
        bail!("--volume-atr-take-profit requires exactly one placeholder --target-rs value");
    }
    if parsed.paper_outcome_sink == MarketVelocityPaperOutcomeSink::Web
        && parsed.stop_reentry_mode != StopReentryMode::Off
        && !paper_outcome_entry_rule_version_explicit
    {
        bail!(
            "--stop-reentry-mode with --paper-outcome-sink web requires explicit --paper-outcome-entry-rule-version"
        );
    }
    Ok(())
}
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
pub fn parse_paper_observation_args_from<I, S>(args: I) -> Result<MarketVelocityEventBacktestArgs>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let user_args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let (preset, user_args) = extract_paper_strategy_preset(user_args)?;
    let effective_preset = if preset.is_none() && user_args.is_empty() {
        Some(PaperStrategyPreset::Momentum0375Sl17RReclaimMaPullbackDelta18To42)
    } else {
        preset
    };
    if effective_preset.is_some() {
        reject_paper_strategy_preset_overrides(&user_args)?;
    }
    reject_paper_observation_owned_flags(&user_args)?;
    let mut parsed_args = Vec::with_capacity(user_args.len() + 10);
    parsed_args.push("--paper-outcome-sink".to_string());
    parsed_args.push("web".to_string());
    if let Some(preset) = effective_preset {
        preset.append_args(&mut parsed_args);
    }
    parsed_args.extend(user_args);
    let mut parsed = parse_cli_args_from(parsed_args)?;
    if let Some(preset) = effective_preset {
        parsed.paper_strategy_preset = preset.name().to_string();
    }
    Ok(parsed)
}
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
pub fn parse_paper_observation_command_from<I, S>(
    args: I,
) -> Result<MarketVelocityPaperObservationCommand>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut backtest_args = Vec::new();
    let mut loop_interval_seconds = None;
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        if arg == PAPER_OBSERVATION_LOOP_INTERVAL_FLAG {
            set_paper_observation_loop_interval(
                &mut loop_interval_seconds,
                &next_arg(&mut args, PAPER_OBSERVATION_LOOP_INTERVAL_FLAG)?,
            )?;
        } else if let Some(value) = arg.strip_prefix("--loop-interval-seconds=") {
            set_paper_observation_loop_interval(&mut loop_interval_seconds, value)?;
        } else if arg == "--help" || arg == "-h" {
            print_market_velocity_paper_observation_usage();
            std::process::exit(0);
        } else {
            backtest_args.push(arg);
        }
    }
    Ok(MarketVelocityPaperObservationCommand {
        backtest_args: parse_paper_observation_args_from(backtest_args)?,
        loop_interval_seconds,
    })
}
/// 更新 回测与策略研究 状态，并保留调用方需要的结果或错误信息。
fn set_paper_observation_loop_interval(
    loop_interval_seconds: &mut Option<u64>,
    value: &str,
) -> Result<()> {
    if loop_interval_seconds.is_some() {
        bail!("{PAPER_OBSERVATION_LOOP_INTERVAL_FLAG} can only be provided once");
    }
    let seconds = value
        .parse::<u64>()
        .with_context(|| format!("invalid value for {PAPER_OBSERVATION_LOOP_INTERVAL_FLAG}"))?;
    if seconds == 0 {
        bail!("{PAPER_OBSERVATION_LOOP_INTERVAL_FLAG} must be greater than 0");
    }
    *loop_interval_seconds = Some(seconds);
    Ok(())
}
/// 提供rejectpaperobservationownedflags的集中实现，避免回测策略调用方重复处理相同细节。
fn reject_paper_observation_owned_flags(args: &[String]) -> Result<()> {
    for arg in args {
        let flag = normalized_arg_flag(arg);
        if PAPER_OBSERVATION_OWNED_FLAGS.contains(&flag) {
            bail!(
                "market_velocity_paper_observation owns {flag}; use market_velocity_event_backtest for experimental overrides"
            );
        }
    }
    Ok(())
}
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
fn extract_paper_strategy_preset(
    args: Vec<String>,
) -> Result<(Option<PaperStrategyPreset>, Vec<String>)> {
    let mut preset = None;
    let mut rest = Vec::with_capacity(args.len());
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == PAPER_STRATEGY_PRESET_FLAG {
            let value = iter
                .next()
                .with_context(|| format!("missing value for {PAPER_STRATEGY_PRESET_FLAG}"))?;
            set_paper_strategy_preset(&mut preset, &value)?;
        } else if let Some(value) = arg.strip_prefix("--paper-strategy-preset=") {
            set_paper_strategy_preset(&mut preset, value)?;
        } else {
            rest.push(arg);
        }
    }
    Ok((preset, rest))
}
/// 更新 回测与策略研究 状态，并保留调用方需要的结果或错误信息。
fn set_paper_strategy_preset(preset: &mut Option<PaperStrategyPreset>, value: &str) -> Result<()> {
    if preset.is_some() {
        bail!("{PAPER_STRATEGY_PRESET_FLAG} can only be provided once");
    }
    *preset = Some(PaperStrategyPreset::from_str(value)?);
    Ok(())
}
/// 提供rejectpaper策略presetoverrides的集中实现，避免回测策略调用方重复处理相同细节。
fn reject_paper_strategy_preset_overrides(args: &[String]) -> Result<()> {
    for arg in args {
        let flag = normalized_arg_flag(arg);
        if PAPER_STRATEGY_PRESET_LOCKED_FLAGS.contains(&flag) {
            bail!(
                "{PAPER_STRATEGY_PRESET_FLAG} locks {flag}; use market_velocity_event_backtest for parameter research"
            );
        }
    }
    Ok(())
}
fn normalized_arg_flag(arg: &str) -> &str {
    arg.split_once('=').map(|(flag, _)| flag).unwrap_or(arg)
}
/// 执行输出市场动量event回测usage步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_market_velocity_event_backtest_usage() {
    println!(
        "Usage: market_velocity_event_backtest [--event-source episodes|raw_events|raw_state|kline_15m --kline-current-live-only --kline-volume-rank-velocity --kline-volume-rank-require-turnover-growth --kline-volume-rank-require-consecutive-improvement] [--trade-direction long|short|both] [--sample-limit 20 --sample-seed batch_a | --historical-universe-manifest PATH --event-start-ms MS --event-end-ms MS] [--target-rs 1.5,2.0] [--stop-loss-pct 0.02 --stop-loss-mode fixed_pct|structure_or_fixed|structure_with_cap --structure-stop-min-pct 0.01] [--entry-period 20] [--entry-min-rsi 55 --entry-max-rsi 78 --entry-min-rsi-delta 3 --entry-rsi-delta-lookback-candles 3 --entry-bollinger-breakout --entry-min-bollinger-bandwidth-expansion-pct 12 --entry-min-body-ratio-pct 65 --entry-min-close-position-pct 80 --entry-min-range-expansion-ratio 1.5 --entry-extreme-volume-contrarian --entry-once-per-opposite-trend-state --entry-wait-setup-open-reclaim --entry-extreme-volume-continuation --entry-relative-volume-at-time-10d --entry-once-per-historical-trend-state --entry-opposite-trend-reset-confirm-candles 8 --entry-min-recent-drawdown-pct 3.5 --entry-recent-drawdown-lookback-candles 12 --entry-opposite-move-lookback-candles 192 --entry-min-opposite-net-move-pct 10 --entry-min-opposite-duration-candles 96 --entry-opposite-duration-min-r-squared 0.70 --entry-min-exhaustion-volume-dominance-ratio 1.0 --entry-btc-96-max-abs-net-move-pct 2.0 --entry-btc-384-min-directional-net-move-pct 0 --entry-btc-require-current-directional-candle --volume-atr-take-profit --volume-atr-target-scale 4 --volume-atr-min-target-r 1.8 --volume-atr-max-target-r 3.0 --backtest-fee-bps-per-side 5 --backtest-slippage-bps-per-side 3 --entry-defer-bearish-continuation --entry-defer-bullish-continuation --entry-defer-long-lower-wick-reversal --entry-long-bullish-hammer-reversal --entry-require-two-stage-recovery --entry-require-macd-negative-histogram-improving --entry-require-opposite-reversal-confirmation --entry-require-reversal-average-reclaim --entry-require-bullish-structure-break --entry-defer-max-wait-candles 3 --entry-symbol-cooldown-candles 8] [--entry-max-signal-pullback-pct 3.0] [--entry-max-gap-without-retest-pct 0.8 --entry-retest-tolerance-pct 0.3 --entry-retest-after-signal --entry-retest-max-wait-candles 8 --entry-retest-min-entry-open-gap-pct 0.0 --entry-retest-open-fade-min-volume-ratio 2.0] [--trend-timeframe 4h|1h|off] [--min-delta-rank 15 --max-delta-rank 79] [--min-price-change-pct 5.0] [--event-start-ms 1717200000000 --event-end-ms 1719791999999] [--entry-trigger-allowlist breakout_previous_high,reclaim_ema] [--entry-trigger-blocklist pullback_hold_ema] [--stop-reentry-mode off|breakout_reclaim] [--profit-protect-after-r 1.0 --profit-protect-stop-r 0.0] [--runner-target-r 4.0 --runner-fraction 0.5 --runner-stop-r 0.0] [--early-exit-no-profit-candles 2] [--equity-max-holding-hours 48] [--ignore-entry-signal-updates-while-open] [--fvg-entry-mode off|15m_to_1h|1h_to_4h|15m_self_after_signal|15m_impulse_retrace --fvg-impulse-retrace-fill-pct 20 --fvg-impulse-retrace-min-wait-candles 0] [--equity-report] [--equity-split-report] [--equity-quartile-report] [--equity-trigger-report] [--equity-concentration-report] [--equity-feature-report] [--equity-price-volume-diagnostic-report] [--equity-symbol-window-report] [--equity-trade-report --min-trades 30] [--save-backtest-detail] [--paper-outcome-sink off|jsonl|web]"
    );
}
/// 返回 paper observation CLI usage，并让 preset 列表复用解析常量，避免可运行 preset 漏出帮助文本。
pub(crate) fn market_velocity_paper_observation_usage() -> String {
    let presets = [
        MOMENTUM_PROFIT_PRESET,
        MOMENTUM_STABLE_RECLAIM_MA_PULLBACK_PRESET,
        MOMENTUM_RECLAIM_MIDRANK_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_GAP_RETEST_RESEARCH_PRESET,
        MOMENTUM_SIGNAL_RETEST_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_FVG_WAIT5_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_ONLY_0375SL_20R_DELTA13_72_RESEARCH_PRESET,
        MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_RESEARCH_PRESET,
        MOMENTUM_BREAKOUT_RECLAIM_LOW_TARGET_0375SL_DELTA11_72_RESEARCH_PRESET,
        MOMENTUM_BREAKOUT_RECLAIM_MA_IGNORE_LOW_TARGET_0375SL_DELTA11_72_RESEARCH_PRESET,
        MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_0375SL_DELTA5_72_RESEARCH_PRESET,
        MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_04SL_DELTA5_72_V2_RESEARCH_PRESET,
        MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_04SL_06R_DELTA5_72_V3_RESEARCH_PRESET,
        MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_04SL_06R_DELTA5_72_V4_RESEARCH_PRESET,
        MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_04SL_065R_DELTA1_100_V5_RESEARCH_PRESET,
        MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_04SL_10R_DELTA5_100_V6_RESEARCH_PRESET,
        MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_RESEARCH_PRESET,
        MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RESEARCH_PRESET,
        MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RUNNER6R20_STOP1_RESEARCH_PRESET,
        MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RUNNER8R20_STOP1_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_FVG_WAIT10_04SL_18R_DELTA15_40_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_FVG_WAIT10_04SL_18R_DELTA20_40_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_FVG_WAIT12_04SL_18R_DELTA20_40_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_FVG_WAIT14_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_GAP0_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_GAP0_OPEN_FADE_VOL2_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_RETEST1_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET,
        MOMENTUM_RECLAIM_RETEST1_04SL_20R_PULLBACK3_DELTA20_40_RESEARCH_PRESET,
        MOMENTUM_BREAKOUT_RECLAIM_RETEST1_04SL_18R_DELTA20_40_RESEARCH_PRESET,
        MOMENTUM_BREAKOUT_RECLAIM_FVG_RETEST1_04SL_18R_DELTA20_40_PCHG5_8_RESEARCH_PRESET,
        MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_MINWAIT1_04SL_DELTA15_40_RESEARCH_PRESET,
        MOMENTUM_KLINE15M_BREAKOUT_FVG20_04SL_10R_RESEARCH_PRESET,
        MOMENTUM_KLINE15M_BREAKOUT_FVG20_04SL_06R_RESEARCH_PRESET,
        MOMENTUM_KLINE15M_BREAKOUT_FVG30_04SL_05R_RESEARCH_PRESET,
        MOMENTUM_KLINE15M_BREAKOUT_FVG30_04SL_055R_RESEARCH_PRESET,
        MOMENTUM_KLINE15M_BREAKOUT_FVG50_04SL_052R_RESEARCH_PRESET,
        MOMENTUM_KLINE15M_DIRECT_SHAPE_04SL_10R_RESEARCH_PRESET,
        MARKET_MOMENTUM_OPPOSITE_MOVE_VOLUME_ATR_RESEARCH_PRESET,
        MARKET_MOMENTUM_OPPOSITE_MOVE_DEFERRED_LONG_RESEARCH_PRESET,
        MARKET_MOMENTUM_OPPOSITE_MOVE_DURATION_BOTH_RESEARCH_PRESET,
        MARKET_MOMENTUM_OPPOSITE_MOVE_EXHAUSTION_VOLUME_RESEARCH_PRESET,
        EPISODE_MOMENTUM_RESEARCH_PRESET,
        EPISODE_MOMENTUM_05SL_20R_RESEARCH_PRESET,
        EPISODE_MOMENTUM_05SL_30R_RESEARCH_PRESET,
        EPISODE_RUNNER_RESEARCH_PRESET,
    ];
    format!(
        "Usage: market_velocity_paper_observation [--loop-interval-seconds 21600] [--paper-strategy-preset {}] [--target-rs 2.0] [--stop-loss-pct 0.03] [--entry-period 20]",
        presets.join("|")
    )
}
/// 执行输出市场动量paperobservationusage步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_market_velocity_paper_observation_usage() {
    println!("{}", market_velocity_paper_observation_usage());
}
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
fn parse_target_rs(value: &str) -> Result<Vec<f64>> {
    let targets = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.parse::<f64>().context("parse --target-rs value"))
        .collect::<Result<Vec<_>>>()?;
    if targets.is_empty() {
        return Ok(DEFAULT_TARGET_RS.to_vec());
    }
    Ok(targets)
}
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
fn parse_entry_trigger_list(value: &str) -> Result<Vec<String>> {
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "all" | "*" | "none") {
        return Ok(Vec::new());
    }
    let triggers = value
        .split(',')
        .map(normalize_entry_trigger)
        .filter(|trigger| !trigger.is_empty())
        .collect::<Vec<_>>();
    if triggers.is_empty() {
        bail!("entry trigger list must not be empty");
    }
    Ok(triggers)
}
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
fn parse_symbol_list(value: &str) -> Result<Vec<String>> {
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "all" | "*" | "none") {
        return Ok(Vec::new());
    }
    let symbols = value
        .split(',')
        .map(normalize_symbol)
        .filter(|symbol| !symbol.is_empty())
        .collect::<Vec<_>>();
    if symbols.is_empty() {
        bail!("symbol list must not be empty");
    }
    Ok(symbols)
}
pub(super) fn normalize_entry_trigger(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
pub(super) fn normalize_symbol(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}
/// 生成 回测与策略研究 需要的派生数据，供后续执行、展示或审计使用。
pub(super) fn format_entry_trigger_filter_list(values: &[String]) -> String {
    if values.is_empty() {
        "all".to_string()
    } else {
        values.join(",")
    }
}
/// 生成 回测与策略研究 需要的派生数据，供后续执行、展示或审计使用。
/// 提供入场触发过滤version标签的集中实现，避免回测策略调用方重复处理相同细节。
pub(super) fn entry_trigger_filter_version_label(
    has_allowlist: bool,
    has_blocklist: bool,
) -> &'static str {
    if has_allowlist {
        ENTRY_TRIGGER_ALLOWLIST_FILTER_VERSION
    } else if has_blocklist {
        ENTRY_TRIGGER_BLOCKLIST_FILTER_VERSION
    } else {
        ENTRY_TRIGGER_UNFILTERED_VERSION
    }
}
/// 封装推进arg，减少回测策略调用方重复实现相同细节。
fn next_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("missing value for {flag}"))
}
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
fn parse_next<T>(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    next_arg(args, flag)?
        .parse::<T>()
        .with_context(|| format!("invalid value for {flag}"))
}
