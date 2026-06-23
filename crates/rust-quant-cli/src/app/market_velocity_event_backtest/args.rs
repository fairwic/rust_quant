use anyhow::{bail, Context, Result};
const DEFAULT_TARGET_RS: &[f64] = &[1.5, 2.0];
const DEFAULT_PAPER_OUTCOME_ENTRY_RULE_VERSION: &str = "rank_radar_4h_trend_15m_timing_v1";
const ENTRY_TRIGGER_ALLOWLIST_FILTER_VERSION: &str = "entry_trigger_allowlist_v1";
const ENTRY_TRIGGER_BLOCKLIST_FILTER_VERSION: &str = "entry_trigger_blocklist_v1";
const ENTRY_TRIGGER_RANK_BLOCKLIST_FILTER_VERSION: &str = "entry_trigger_rank_blocklist_v1";
const ENTRY_TRIGGER_UNFILTERED_VERSION: &str = "unfiltered_v1";
const DEFAULT_WEB_PAPER_OUTCOME_ENTRY_TRIGGER_ALLOWLIST: &[&str] =
    &["breakout_previous_high", "reclaim_ema"];
const DEFAULT_FVG_LOOKBACK_CANDLES: usize = 40;
const DEFAULT_FVG_MAX_WAIT_CANDLES: usize = 24;
const PAPER_OBSERVATION_LOOP_INTERVAL_FLAG: &str = "--loop-interval-seconds";
const PAPER_STRATEGY_PRESET_FLAG: &str = "--paper-strategy-preset";
const MOMENTUM_PROFIT_PRESET: &str = "momentum_03sl_20r_v5";
const MOMENTUM_PROFIT_ENTRY_RULE_VERSION: &str = "rank_radar_4h_trend_15m_momentum_03sl_20r_v5";
const MOMENTUM_RECLAIM_MIDRANK_RESEARCH_PRESET: &str =
    "research_momentum_0375sl_27r_reclaim13_22_v1";
const MOMENTUM_RECLAIM_MIDRANK_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_research_0375sl_27r_dist55_reclaim13_22_v1";
const EPISODE_MOMENTUM_RESEARCH_PRESET: &str = "research_episode_momentum_03sl_24r_rank5_30_v1";
const EPISODE_MOMENTUM_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1";
const EPISODE_RUNNER_RESEARCH_PRESET: &str = "research_episode_runner_03sl_24r_8r30_v1";
const EPISODE_RUNNER_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_episode_runner_03sl_24r_8r30_v1";
const PAPER_OBSERVATION_OWNED_FLAGS: &[&str] = &[
    "--paper-outcome-sink",
    "--paper-outcome-entry-rule-version",
    "--entry-trigger-allowlist",
    "--entry-trigger-blocklist",
    "--entry-trigger-rank-blocklist",
    "--symbol-blocklist",
    "--stop-reentry-mode",
    "--fvg-entry-mode",
    "--fvg-lookback-candles",
    "--fvg-max-wait-candles",
    "--profit-protect-after-r",
    "--profit-protect-stop-r",
    "--runner-target-r",
    "--runner-fraction",
    "--runner-stop-r",
    "--early-exit-no-profit-candles",
    "--save-backtest-detail",
];
const PAPER_STRATEGY_PRESET_LOCKED_FLAGS: &[&str] = &[
    "--event-source",
    "--target-rs",
    "--stop-loss-pct",
    "--entry-period",
    "--entry-max-distance-pct",
    "--entry-min-volume-ratio",
    "--trend-min-average-distance-pct",
    "--min-delta-rank",
    "--max-delta-rank",
    "--max-new-rank",
    "--min-price-change-pct",
    "--chase-top-rank",
    "--chase-price-change-pct",
    "--max-15m-staleness-min",
    "--max-4h-staleness-min",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketVelocityEventSource {
    Episodes,
    RawEvents,
    RawState,
}
impl MarketVelocityEventSource {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "episodes" | "episode" | "market_velocity_episodes" => Ok(Self::Episodes),
            "raw_events" | "raw" => Ok(Self::RawEvents),
            "raw_state" | "state" | "signal_state" => Ok(Self::RawState),
            other => bail!("unknown --event-source: {other}"),
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
            other => bail!("unknown --fvg-entry-mode: {other}"),
        }
    }
    /// 提供标签的集中实现，避免回测策略调用方重复处理相同细节。
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::M15To1h => "m15_to_1h",
            Self::H1To4h => "h1_to_4h",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaperStrategyPreset {
    Momentum03Sl20R,
    ResearchMomentum0375Sl27RReclaim13To22,
    ResearchEpisodeMomentum03Sl24RRank5To30,
    ResearchEpisodeRunner03Sl24R8R30,
}
impl PaperStrategyPreset {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            MOMENTUM_PROFIT_PRESET => Ok(Self::Momentum03Sl20R),
            MOMENTUM_RECLAIM_MIDRANK_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum0375Sl27RReclaim13To22)
            }
            EPISODE_MOMENTUM_RESEARCH_PRESET => Ok(Self::ResearchEpisodeMomentum03Sl24RRank5To30),
            EPISODE_RUNNER_RESEARCH_PRESET => Ok(Self::ResearchEpisodeRunner03Sl24R8R30),
            other => bail!("unknown {PAPER_STRATEGY_PRESET_FLAG}: {other}"),
        }
    }
    /// 把数据加入 回测与策略研究 聚合结果，保持集合构造逻辑集中。
    fn append_args(self, args: &mut Vec<String>) {
        match self {
            Self::Momentum03Sl20R => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_PROFIT_ENTRY_RULE_VERSION.to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.03".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "4.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "15".to_string(),
                ]);
            }
            Self::ResearchMomentum0375Sl27RReclaim13To22 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_MIDRANK_RESEARCH_ENTRY_RULE_VERSION.to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.0375".to_string(),
                    "--target-rs".to_string(),
                    "2.7".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.5".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "13".to_string(),
                    "--max-delta-rank".to_string(),
                    "72".to_string(),
                    "--max-new-rank".to_string(),
                    "30".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--chase-top-rank".to_string(),
                    "5".to_string(),
                    "--chase-price-change-pct".to_string(),
                    "80.0".to_string(),
                    "--entry-trigger-rank-blocklist".to_string(),
                    "reclaim_ema:13-22".to_string(),
                ]);
            }
            Self::ResearchEpisodeMomentum03Sl24RRank5To30 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    EPISODE_MOMENTUM_RESEARCH_ENTRY_RULE_VERSION.to_string(),
                    "--event-source".to_string(),
                    "episodes".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.03".to_string(),
                    "--target-rs".to_string(),
                    "2.4".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "7.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "0.8".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "5".to_string(),
                    "--max-new-rank".to_string(),
                    "30".to_string(),
                    "--chase-top-rank".to_string(),
                    "5".to_string(),
                    "--chase-price-change-pct".to_string(),
                    "80.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "all".to_string(),
                ]);
            }
            Self::ResearchEpisodeRunner03Sl24R8R30 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    EPISODE_RUNNER_RESEARCH_ENTRY_RULE_VERSION.to_string(),
                    "--event-source".to_string(),
                    "episodes".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.03".to_string(),
                    "--target-rs".to_string(),
                    "2.4".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "7.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "0.8".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "5".to_string(),
                    "--max-new-rank".to_string(),
                    "30".to_string(),
                    "--chase-top-rank".to_string(),
                    "5".to_string(),
                    "--chase-price-change-pct".to_string(),
                    "80.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "all".to_string(),
                    "--runner-target-r".to_string(),
                    "8.0".to_string(),
                    "--runner-fraction".to_string(),
                    "0.3".to_string(),
                    "--runner-stop-r".to_string(),
                    "0.0".to_string(),
                ]);
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryTriggerRankBlock {
    /// trigger，用于行情、K 线或市场扫描。
    pub trigger: String,
    /// 最小new排名，用于控制策略触发门槛。
    pub min_new_rank: i32,
    /// 最大new排名，用于控制策略触发门槛。
    pub max_new_rank: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityEventBacktestArgs {
    /// 止损百分比。
    pub stop_loss_pct: f64,
    /// 列表数据。
    pub target_rs: Vec<f64>,
    /// 入场周期，用于行情、K 线或市场扫描。
    pub entry_period: usize,
    /// 入场允许的最大距离百分比。
    pub entry_max_distance_pct: f64,
    /// 入场最小volume 比例。
    pub entry_min_volume_ratio: f64,
    /// 趋势过滤的最小平均距离百分比。
    pub trend_min_average_distance_pct: f64,
    /// 最小delta排名，用于控制策略触发门槛。
    pub min_delta_rank: i32,
    /// 最大排名变化；为空时不限制。
    pub max_delta_rank: Option<i32>,
    /// 最大new排名，用于控制策略触发门槛。
    pub max_new_rank: i32,
    /// 最小价格涨跌幅百分比。
    pub min_price_change_pct: Option<f64>,
    /// tailnewrank阈值；为空时使用默认值或表示不限制。
    pub tail_new_rank_threshold: Option<i32>,
    /// 尾部排名过滤的最小价格涨跌幅百分比。
    pub tail_rank_min_price_change_pct: Option<f64>,
    /// chasetop排名，用于行情、K 线或市场扫描。
    pub chase_top_rank: i32,
    /// 追价允许的价格变化百分比。
    pub chase_price_change_pct: f64,
    /// 最大15mstaleness最小，用于控制策略触发门槛。
    pub max_15m_staleness_min: i64,
    /// 最大4hstaleness最小，用于控制策略触发门槛。
    pub max_4h_staleness_min: i64,
    /// samplelimit，用于行情、K 线或市场扫描。
    pub sample_limit: usize,
    /// event来源，用于行情、K 线或市场扫描。
    pub event_source: MarketVelocityEventSource,
    /// tradedirection，用于行情、K 线或市场扫描。
    pub trade_direction: MarketVelocityTradeDirection,
    /// 模拟盘outcomesink，用于行情、K 线或市场扫描。
    pub paper_outcome_sink: MarketVelocityPaperOutcomeSink,
    /// 模拟盘outcome入场ruleversion，用于行情、K 线或市场扫描。
    pub paper_outcome_entry_rule_version: String,
    /// 列表数据。
    pub entry_trigger_allowlist: Vec<String>,
    /// 列表数据。
    pub entry_trigger_blocklist: Vec<String>,
    /// 列表数据。
    pub entry_trigger_rank_blocklist: Vec<EntryTriggerRankBlock>,
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
            target_rs: DEFAULT_TARGET_RS.to_vec(),
            entry_period: 20,
            entry_max_distance_pct: 3.0,
            entry_min_volume_ratio: 1.0,
            trend_min_average_distance_pct: 0.0,
            min_delta_rank: 10,
            max_delta_rank: None,
            max_new_rank: 30,
            min_price_change_pct: None,
            tail_new_rank_threshold: None,
            tail_rank_min_price_change_pct: None,
            chase_top_rank: 10,
            chase_price_change_pct: 8.0,
            max_15m_staleness_min: 30,
            max_4h_staleness_min: 240,
            sample_limit: 5,
            event_source: MarketVelocityEventSource::Episodes,
            trade_direction: MarketVelocityTradeDirection::Long,
            paper_outcome_sink: MarketVelocityPaperOutcomeSink::Off,
            paper_outcome_entry_rule_version: DEFAULT_PAPER_OUTCOME_ENTRY_RULE_VERSION.to_string(),
            entry_trigger_allowlist: Vec::new(),
            entry_trigger_blocklist: Vec::new(),
            entry_trigger_rank_blocklist: Vec::new(),
            symbol_blocklist: Vec::new(),
            stop_reentry_mode: StopReentryMode::Off,
            fvg_entry_mode: FvgEntryMode::Off,
            fvg_lookback_candles: DEFAULT_FVG_LOOKBACK_CANDLES,
            fvg_max_wait_candles: DEFAULT_FVG_MAX_WAIT_CANDLES,
            profit_protect_after_r: None,
            profit_protect_stop_r: 0.0,
            runner_target_r: None,
            runner_fraction: 0.0,
            runner_stop_r: 0.0,
            early_exit_no_profit_candles: None,
            equity_report: false,
            equity_split_report: false,
            equity_quartile_report: false,
            equity_trigger_report: false,
            equity_concentration_report: false,
            equity_feature_report: false,
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
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stop-loss-pct" => parsed.stop_loss_pct = parse_next(&mut args, &arg)?,
            "--target-rs" => parsed.target_rs = parse_target_rs(&next_arg(&mut args, &arg)?)?,
            "--entry-period" => parsed.entry_period = parse_next(&mut args, &arg)?,
            "--entry-max-distance-pct" => {
                parsed.entry_max_distance_pct = parse_next(&mut args, &arg)?
            }
            "--entry-min-volume-ratio" => {
                parsed.entry_min_volume_ratio = parse_next(&mut args, &arg)?
            }
            "--trend-min-average-distance-pct" => {
                parsed.trend_min_average_distance_pct = parse_next(&mut args, &arg)?
            }
            "--min-delta-rank" => parsed.min_delta_rank = parse_next(&mut args, &arg)?,
            "--max-delta-rank" => parsed.max_delta_rank = Some(parse_next(&mut args, &arg)?),
            "--max-new-rank" => parsed.max_new_rank = parse_next(&mut args, &arg)?,
            "--min-price-change-pct" => {
                parsed.min_price_change_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--tail-new-rank-threshold" => {
                parsed.tail_new_rank_threshold = Some(parse_next(&mut args, &arg)?)
            }
            "--tail-rank-min-price-change-pct" => {
                parsed.tail_rank_min_price_change_pct = Some(parse_next(&mut args, &arg)?)
            }
            "--chase-top-rank" => parsed.chase_top_rank = parse_next(&mut args, &arg)?,
            "--chase-price-change-pct" => {
                parsed.chase_price_change_pct = parse_next(&mut args, &arg)?
            }
            "--max-15m-staleness-min" => {
                parsed.max_15m_staleness_min = parse_next(&mut args, &arg)?
            }
            "--max-4h-staleness-min" => parsed.max_4h_staleness_min = parse_next(&mut args, &arg)?,
            "--sample-limit" => parsed.sample_limit = parse_next(&mut args, &arg)?,
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
            "--entry-trigger-rank-blocklist" => {
                parsed.entry_trigger_rank_blocklist =
                    parse_entry_trigger_rank_blocklist(&next_arg(&mut args, &arg)?)?
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
            "--equity-report" => parsed.equity_report = true,
            "--equity-split-report" => parsed.equity_split_report = true,
            "--equity-quartile-report" => parsed.equity_quartile_report = true,
            "--equity-trigger-report" => parsed.equity_trigger_report = true,
            "--equity-concentration-report" => parsed.equity_concentration_report = true,
            "--equity-feature-report" => parsed.equity_feature_report = true,
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
    if parsed.stop_loss_pct <= 0.0 {
        bail!("--stop-loss-pct must be greater than 0");
    }
    if parsed.trend_min_average_distance_pct < 0.0 {
        bail!("--trend-min-average-distance-pct must be zero or greater");
    }
    if parsed.min_trades == 0 {
        bail!("--min-trades must be greater than 0");
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
    match (
        parsed.tail_new_rank_threshold,
        parsed.tail_rank_min_price_change_pct,
    ) {
        (Some(threshold), Some(min_price_change_pct)) => {
            if threshold < 1 {
                bail!("--tail-new-rank-threshold must be greater than 0");
            }
            if min_price_change_pct < 0.0 {
                bail!("--tail-rank-min-price-change-pct must be zero or greater");
            }
        }
        (Some(_), None) => {
            bail!("--tail-new-rank-threshold requires --tail-rank-min-price-change-pct");
        }
        (None, Some(_)) => {
            bail!("--tail-rank-min-price-change-pct requires --tail-new-rank-threshold");
        }
        (None, None) => {}
    }
    if parsed.fvg_lookback_candles == 0 {
        bail!("--fvg-lookback-candles must be greater than 0");
    }
    if parsed.fvg_max_wait_candles == 0 {
        bail!("--fvg-max-wait-candles must be greater than 0");
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
    if parsed.paper_outcome_sink == MarketVelocityPaperOutcomeSink::Web
        && parsed.stop_reentry_mode != StopReentryMode::Off
        && !paper_outcome_entry_rule_version_explicit
    {
        bail!("--stop-reentry-mode with --paper-outcome-sink web requires explicit --paper-outcome-entry-rule-version");
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
    if preset.is_some() {
        reject_paper_strategy_preset_overrides(&user_args)?;
    }
    reject_paper_observation_owned_flags(&user_args)?;
    let mut parsed_args = Vec::with_capacity(user_args.len() + 10);
    parsed_args.push("--paper-outcome-sink".to_string());
    parsed_args.push("web".to_string());
    if let Some(preset) = preset {
        preset.append_args(&mut parsed_args);
    }
    parsed_args.extend(user_args);
    parse_cli_args_from(parsed_args)
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
            bail!("{PAPER_STRATEGY_PRESET_FLAG} locks {flag}; use market_velocity_event_backtest for parameter research");
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
        "Usage: market_velocity_event_backtest [--event-source episodes|raw_events|raw_state] [--trade-direction long|short|both] [--target-rs 1.5,2.0] [--stop-loss-pct 0.02] [--entry-period 20] [--min-delta-rank 15 --max-delta-rank 79] [--min-price-change-pct 5.0] [--tail-new-rank-threshold 21 --tail-rank-min-price-change-pct 10.0] [--entry-trigger-allowlist breakout_previous_high,reclaim_ema] [--entry-trigger-blocklist pullback_hold_ema] [--entry-trigger-rank-blocklist reclaim_ema:11-20] [--stop-reentry-mode off|breakout_reclaim] [--profit-protect-after-r 1.0 --profit-protect-stop-r 0.0] [--runner-target-r 4.0 --runner-fraction 0.5 --runner-stop-r 0.0] [--early-exit-no-profit-candles 2] [--fvg-entry-mode off|15m_to_1h|1h_to_4h] [--equity-report] [--equity-split-report] [--equity-quartile-report] [--equity-trigger-report] [--equity-concentration-report] [--equity-feature-report] [--equity-symbol-window-report] [--equity-trade-report --min-trades 30] [--save-backtest-detail] [--paper-outcome-sink off|jsonl|web]"
    );
}
/// 执行输出市场动量paperobservationusage步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_market_velocity_paper_observation_usage() {
    println!(
        "Usage: market_velocity_paper_observation [--loop-interval-seconds 21600] [--paper-strategy-preset momentum_03sl_20r_v5|research_momentum_0375sl_27r_reclaim13_22_v1|research_episode_momentum_03sl_24r_rank5_30_v1|research_episode_runner_03sl_24r_8r30_v1] [--target-rs 2.0] [--stop-loss-pct 0.03] [--entry-period 20]"
    );
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
fn parse_entry_trigger_rank_blocklist(value: &str) -> Result<Vec<EntryTriggerRankBlock>> {
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "all" | "*" | "none") {
        return Ok(Vec::new());
    }
    let blocks = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_entry_trigger_rank_block)
        .collect::<Result<Vec<_>>>()?;
    if blocks.is_empty() {
        bail!("entry trigger rank blocklist must not be empty");
    }
    Ok(blocks)
}
/// 解析输入参数并收敛为 回测与策略研究 可使用的结构化值。
fn parse_entry_trigger_rank_block(value: &str) -> Result<EntryTriggerRankBlock> {
    let (trigger, rank_range) = value
        .split_once(':')
        .with_context(|| format!("entry trigger rank block must use trigger:min-max: {value}"))?;
    let (min_new_rank, max_new_rank) = rank_range
        .split_once('-')
        .with_context(|| format!("entry trigger rank block must use trigger:min-max: {value}"))?;
    let min_new_rank = min_new_rank
        .trim()
        .parse::<i32>()
        .with_context(|| format!("parse min new rank in {value}"))?;
    let max_new_rank = max_new_rank
        .trim()
        .parse::<i32>()
        .with_context(|| format!("parse max new rank in {value}"))?;
    if min_new_rank < 1 {
        bail!("entry trigger rank block min rank must be greater than 0");
    }
    if max_new_rank < min_new_rank {
        bail!("entry trigger rank block max rank must be >= min rank");
    }
    let trigger = normalize_entry_trigger(trigger);
    if trigger.is_empty() {
        bail!("entry trigger rank block trigger must not be empty");
    }
    Ok(EntryTriggerRankBlock {
        trigger,
        min_new_rank,
        max_new_rank,
    })
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
pub(super) fn format_entry_trigger_rank_blocklist(values: &[EntryTriggerRankBlock]) -> String {
    if values.is_empty() {
        return "all".to_string();
    }
    values
        .iter()
        .map(|block| {
            format!(
                "{}:{}-{}",
                block.trigger, block.min_new_rank, block.max_new_rank
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}
/// 提供入场触发过滤version标签的集中实现，避免回测策略调用方重复处理相同细节。
pub(super) fn entry_trigger_filter_version_label(
    has_allowlist: bool,
    has_blocklist: bool,
    has_rank_blocklist: bool,
) -> &'static str {
    if has_rank_blocklist {
        ENTRY_TRIGGER_RANK_BLOCKLIST_FILTER_VERSION
    } else if has_allowlist {
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
