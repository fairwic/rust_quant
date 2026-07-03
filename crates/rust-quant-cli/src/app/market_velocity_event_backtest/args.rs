use anyhow::{bail, Context, Result};
pub use rust_quant_services::market::MarketVelocityStopLossMode;
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
    "--event-start-ms",
    "--event-end-ms",
    "--save-backtest-detail",
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
    /// 提供标签的集中实现，避免回测策略调用方重复处理相同细节。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Episodes => "episodes",
            Self::RawEvents => "raw_events",
            Self::RawState => "raw_state",
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaperStrategyPreset {
    Momentum03Sl20R,
    Momentum0375Sl17RReclaimMaPullbackDelta18To42,
    ResearchMomentum0375Sl27RReclaim13To22,
    ResearchMomentum0375Sl26RGap05Retest03Reclaim13To22,
    ResearchMomentum0375Sl15RSignalRetest2Delta24To34,
    ResearchMomentum0375Sl20RReclaimFvgWait5Delta20To40,
    ResearchMomentum0375Sl20RReclaimOnlyDelta13To72,
    ResearchMomentum0375Sl20RBreakoutReclaimFvgWait10Delta20To40,
    ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta20To40,
    ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta15To40,
    ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta15To40Runner6R20Stop1,
    ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta15To40Runner8R20Stop1,
    ResearchMomentum04Sl20RReclaimFvgWait10Delta15To40,
    ResearchMomentum04Sl18RReclaimFvgWait10Delta15To40,
    ResearchMomentum04Sl18RReclaimFvgWait10Delta20To40,
    ResearchMomentum04Sl18RReclaimFvgWait12Delta20To40,
    ResearchMomentum04Sl18RReclaimFvgWait14Pullback3Delta20To40,
    ResearchMomentum04Sl18RReclaimFvgWait14Retest1Pullback3Delta20To40,
    ResearchMomentum04Sl18RReclaimFvgWait14Retest1Gap0Pullback3Delta20To40,
    ResearchMomentum04Sl18RReclaimFvgWait14Retest1Gap0OpenFadeVol2Pullback3Delta20To40,
    ResearchMomentum04Sl18RReclaimRetest1Pullback3Delta20To40,
    ResearchMomentum04Sl20RReclaimRetest1Pullback3Delta20To40,
    ResearchMomentum04Sl18RBreakoutReclaimRetest1Delta20To40,
    ResearchMomentum04Sl18RBreakoutReclaimFvgRetest1Delta20To40Pchg5To8,
    ResearchMomentum04Sl20RBreakoutReclaimFvgWait10MinWait1Delta15To40,
    ResearchEpisodeMomentum03Sl24RRank5To30,
    ResearchEpisodeMomentum05Sl20RRank5,
    ResearchEpisodeMomentum05Sl30RRank5,
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
            MOMENTUM_STABLE_RECLAIM_MA_PULLBACK_PRESET => {
                Ok(Self::Momentum0375Sl17RReclaimMaPullbackDelta18To42)
            }
            MOMENTUM_RECLAIM_MIDRANK_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum0375Sl27RReclaim13To22)
            }
            MOMENTUM_RECLAIM_GAP_RETEST_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum0375Sl26RGap05Retest03Reclaim13To22)
            }
            MOMENTUM_SIGNAL_RETEST_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum0375Sl15RSignalRetest2Delta24To34)
            }
            MOMENTUM_RECLAIM_FVG_WAIT5_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum0375Sl20RReclaimFvgWait5Delta20To40)
            }
            MOMENTUM_RECLAIM_ONLY_0375SL_20R_DELTA13_72_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum0375Sl20RReclaimOnlyDelta13To72)
            }
            MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum0375Sl20RBreakoutReclaimFvgWait10Delta20To40)
            }
            MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta20To40)
            }
            MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta15To40)
            }
            MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RUNNER6R20_STOP1_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta15To40Runner6R20Stop1)
            }
            MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RUNNER8R20_STOP1_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta15To40Runner8R20Stop1)
            }
            MOMENTUM_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl20RReclaimFvgWait10Delta15To40)
            }
            MOMENTUM_RECLAIM_FVG_WAIT10_04SL_18R_DELTA15_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl18RReclaimFvgWait10Delta15To40)
            }
            MOMENTUM_RECLAIM_FVG_WAIT10_04SL_18R_DELTA20_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl18RReclaimFvgWait10Delta20To40)
            }
            MOMENTUM_RECLAIM_FVG_WAIT12_04SL_18R_DELTA20_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl18RReclaimFvgWait12Delta20To40)
            }
            MOMENTUM_RECLAIM_FVG_WAIT14_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl18RReclaimFvgWait14Pullback3Delta20To40)
            }
            MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl18RReclaimFvgWait14Retest1Pullback3Delta20To40)
            }
            MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_GAP0_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl18RReclaimFvgWait14Retest1Gap0Pullback3Delta20To40)
            }
            MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_GAP0_OPEN_FADE_VOL2_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl18RReclaimFvgWait14Retest1Gap0OpenFadeVol2Pullback3Delta20To40)
            }
            MOMENTUM_RECLAIM_RETEST1_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl18RReclaimRetest1Pullback3Delta20To40)
            }
            MOMENTUM_RECLAIM_RETEST1_04SL_20R_PULLBACK3_DELTA20_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl20RReclaimRetest1Pullback3Delta20To40)
            }
            MOMENTUM_BREAKOUT_RECLAIM_RETEST1_04SL_18R_DELTA20_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl18RBreakoutReclaimRetest1Delta20To40)
            }
            MOMENTUM_BREAKOUT_RECLAIM_FVG_RETEST1_04SL_18R_DELTA20_40_PCHG5_8_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl18RBreakoutReclaimFvgRetest1Delta20To40Pchg5To8)
            }
            MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_MINWAIT1_04SL_DELTA15_40_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum04Sl20RBreakoutReclaimFvgWait10MinWait1Delta15To40)
            }
            EPISODE_MOMENTUM_RESEARCH_PRESET => Ok(Self::ResearchEpisodeMomentum03Sl24RRank5To30),
            EPISODE_MOMENTUM_05SL_20R_RESEARCH_PRESET => {
                Ok(Self::ResearchEpisodeMomentum05Sl20RRank5)
            }
            EPISODE_MOMENTUM_05SL_30R_RESEARCH_PRESET => {
                Ok(Self::ResearchEpisodeMomentum05Sl30RRank5)
            }
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
            Self::Momentum0375Sl17RReclaimMaPullbackDelta18To42 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_STABLE_RECLAIM_MA_PULLBACK_ENTRY_RULE_VERSION.to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.0375".to_string(),
                    "--target-rs".to_string(),
                    "1.7".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.5".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "18".to_string(),
                    "--max-delta-rank".to_string(),
                    "42".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "10.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema,reclaim_ma,pullback_hold_ema".to_string(),
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
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                ]);
            }
            Self::ResearchMomentum0375Sl26RGap05Retest03Reclaim13To22 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_GAP_RETEST_RESEARCH_ENTRY_RULE_VERSION.to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.0375".to_string(),
                    "--target-rs".to_string(),
                    "2.6".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.5".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--entry-max-gap-without-retest-pct".to_string(),
                    "0.5".to_string(),
                    "--entry-retest-tolerance-pct".to_string(),
                    "0.3".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "13".to_string(),
                    "--max-delta-rank".to_string(),
                    "75".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                ]);
            }
            Self::ResearchMomentum0375Sl15RSignalRetest2Delta24To34 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_SIGNAL_RETEST_RESEARCH_ENTRY_RULE_VERSION.to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.0375".to_string(),
                    "--target-rs".to_string(),
                    "1.5".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--entry-retest-after-signal".to_string(),
                    "--entry-retest-max-wait-candles".to_string(),
                    "2".to_string(),
                    "--entry-retest-tolerance-pct".to_string(),
                    "0.3".to_string(),
                    "--entry-retest-min-entry-open-gap-pct".to_string(),
                    "0.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "24".to_string(),
                    "--max-delta-rank".to_string(),
                    "34".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "10.0".to_string(),
                ]);
            }
            Self::ResearchMomentum0375Sl20RReclaimFvgWait5Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_FVG_WAIT5_RESEARCH_ENTRY_RULE_VERSION.to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.0375".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "15m_impulse_retrace".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "5".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum0375Sl20RReclaimOnlyDelta13To72 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_ONLY_0375SL_20R_DELTA13_72_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.0375".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
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
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                ]);
            }
            Self::ResearchMomentum0375Sl20RBreakoutReclaimFvgWait10Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_RESEARCH_ENTRY_RULE_VERSION.to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.0375".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "breakout_previous_high,reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "15m_impulse_retrace".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "10".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "breakout_previous_high,reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "15m_impulse_retrace".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "10".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta15To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "15".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "breakout_previous_high,reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "15m_impulse_retrace".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "10".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta15To40Runner6R20Stop1 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RUNNER6R20_STOP1_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "15".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "15m_impulse_retrace".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "10".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                    "--runner-target-r".to_string(),
                    "6.0".to_string(),
                    "--runner-fraction".to_string(),
                    "0.2".to_string(),
                    "--runner-stop-r".to_string(),
                    "1.0".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl20RBreakoutReclaimFvgWait10Delta15To40Runner8R20Stop1 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RUNNER8R20_STOP1_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "15".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "15m_impulse_retrace".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "10".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                    "--runner-target-r".to_string(),
                    "8.0".to_string(),
                    "--runner-fraction".to_string(),
                    "0.2".to_string(),
                    "--runner-stop-r".to_string(),
                    "1.0".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl20RReclaimFvgWait10Delta15To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "15".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "15m_impulse_retrace".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "10".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl18RReclaimFvgWait10Delta15To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_FVG_WAIT10_04SL_18R_DELTA15_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "1.8".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "15".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "15m_impulse_retrace".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "10".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl18RReclaimFvgWait10Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_FVG_WAIT10_04SL_18R_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "1.8".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "10.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "15m_impulse_retrace".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "10".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl18RReclaimFvgWait12Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_FVG_WAIT12_04SL_18R_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "1.8".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "10.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "15m_impulse_retrace".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "12".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl18RReclaimFvgWait14Pullback3Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_FVG_WAIT14_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "1.8".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "3.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.1".to_string(),
                    "--entry-max-signal-pullback-pct".to_string(),
                    "3.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "10.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "m15_impulse_retrace".to_string(),
                    "--fvg-impulse-retrace-fill-pct".to_string(),
                    "10.0".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "14".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl18RReclaimFvgWait14Retest1Pullback3Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "1.8".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.1".to_string(),
                    "--entry-max-signal-pullback-pct".to_string(),
                    "3.0".to_string(),
                    "--entry-retest-after-signal".to_string(),
                    "--entry-retest-max-wait-candles".to_string(),
                    "1".to_string(),
                    "--entry-retest-tolerance-pct".to_string(),
                    "0.3".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "10.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "m15_impulse_retrace".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl18RReclaimFvgWait14Retest1Gap0Pullback3Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_GAP0_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "1.8".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.1".to_string(),
                    "--entry-max-signal-pullback-pct".to_string(),
                    "3.0".to_string(),
                    "--entry-retest-after-signal".to_string(),
                    "--entry-retest-max-wait-candles".to_string(),
                    "1".to_string(),
                    "--entry-retest-min-entry-open-gap-pct".to_string(),
                    "0.0".to_string(),
                    "--entry-retest-tolerance-pct".to_string(),
                    "2.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "10.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "m15_impulse_retrace".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl18RReclaimFvgWait14Retest1Gap0OpenFadeVol2Pullback3Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_GAP0_OPEN_FADE_VOL2_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "1.8".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.1".to_string(),
                    "--entry-max-signal-pullback-pct".to_string(),
                    "3.0".to_string(),
                    "--entry-retest-after-signal".to_string(),
                    "--entry-retest-max-wait-candles".to_string(),
                    "1".to_string(),
                    "--entry-retest-min-entry-open-gap-pct".to_string(),
                    "0.0".to_string(),
                    "--entry-retest-open-fade-min-volume-ratio".to_string(),
                    "2.0".to_string(),
                    "--entry-retest-tolerance-pct".to_string(),
                    "2.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "10.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "m15_impulse_retrace".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl18RReclaimRetest1Pullback3Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_RETEST1_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "1.8".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "3.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.1".to_string(),
                    "--entry-max-signal-pullback-pct".to_string(),
                    "3.0".to_string(),
                    "--entry-retest-after-signal".to_string(),
                    "--entry-retest-max-wait-candles".to_string(),
                    "1".to_string(),
                    "--entry-retest-tolerance-pct".to_string(),
                    "0.3".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "10.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl20RReclaimRetest1Pullback3Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_RECLAIM_RETEST1_04SL_20R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "3.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.1".to_string(),
                    "--entry-max-signal-pullback-pct".to_string(),
                    "3.0".to_string(),
                    "--entry-retest-after-signal".to_string(),
                    "--entry-retest-max-wait-candles".to_string(),
                    "1".to_string(),
                    "--entry-retest-tolerance-pct".to_string(),
                    "0.3".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "10.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "reclaim_ema".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl18RBreakoutReclaimRetest1Delta20To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_BREAKOUT_RECLAIM_RETEST1_04SL_18R_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "1.8".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--entry-max-signal-pullback-pct".to_string(),
                    "3.0".to_string(),
                    "--entry-retest-after-signal".to_string(),
                    "--entry-retest-max-wait-candles".to_string(),
                    "1".to_string(),
                    "--entry-retest-tolerance-pct".to_string(),
                    "0.3".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "10.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "breakout_previous_high,reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "off".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl18RBreakoutReclaimFvgRetest1Delta20To40Pchg5To8 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_BREAKOUT_RECLAIM_FVG_RETEST1_04SL_18R_DELTA20_40_PCHG5_8_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "1.8".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--entry-max-signal-pullback-pct".to_string(),
                    "3.0".to_string(),
                    "--entry-retest-after-signal".to_string(),
                    "--entry-retest-max-wait-candles".to_string(),
                    "1".to_string(),
                    "--entry-retest-tolerance-pct".to_string(),
                    "0.3".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "20".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "8.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "breakout_previous_high,reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "m15_impulse_retrace".to_string(),
                    "--fvg-lookback-candles".to_string(),
                    "40".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "24".to_string(),
                    "--fvg-impulse-retrace-fill-pct".to_string(),
                    "20.0".to_string(),
                    "--fvg-impulse-retrace-min-wait-candles".to_string(),
                    "0".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentum04Sl20RBreakoutReclaimFvgWait10MinWait1Delta15To40 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_MINWAIT1_04SL_DELTA15_40_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "5.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.0".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "15".to_string(),
                    "--max-delta-rank".to_string(),
                    "40".to_string(),
                    "--min-price-change-pct".to_string(),
                    "5.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "breakout_previous_high,reclaim_ema".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "15m_impulse_retrace".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "10".to_string(),
                    "--fvg-impulse-retrace-min-wait-candles".to_string(),
                    "1".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
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
                    "--entry-trigger-allowlist".to_string(),
                    "all".to_string(),
                ]);
            }
            Self::ResearchEpisodeMomentum05Sl20RRank5 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    EPISODE_MOMENTUM_05SL_20R_RESEARCH_ENTRY_RULE_VERSION.to_string(),
                    "--event-source".to_string(),
                    "episodes".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.05".to_string(),
                    "--target-rs".to_string(),
                    "2.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "7.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "0.8".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "5".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "all".to_string(),
                ]);
            }
            Self::ResearchEpisodeMomentum05Sl30RRank5 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    EPISODE_MOMENTUM_05SL_30R_RESEARCH_ENTRY_RULE_VERSION.to_string(),
                    "--event-source".to_string(),
                    "episodes".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.05".to_string(),
                    "--target-rs".to_string(),
                    "3.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "7.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "0.8".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "5".to_string(),
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
            entry_max_signal_pullback_pct: None,
            entry_max_gap_without_retest_pct: None,
            entry_retest_tolerance_pct: 0.3,
            entry_retest_after_signal: false,
            entry_retest_max_wait_candles: 8,
            entry_retest_min_entry_open_gap_pct: None,
            entry_retest_open_fade_min_volume_ratio: None,
            trend_min_average_distance_pct: 0.0,
            min_delta_rank: 10,
            max_delta_rank: None,
            min_price_change_pct: None,
            max_price_change_pct: None,
            event_start_ms: None,
            event_end_ms: None,
            max_15m_staleness_min: 30,
            max_4h_staleness_min: 240,
            sample_limit: 5,
            event_source: MarketVelocityEventSource::Episodes,
            trade_direction: MarketVelocityTradeDirection::Long,
            paper_outcome_sink: MarketVelocityPaperOutcomeSink::Off,
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
            ignore_entry_signal_updates_while_open: false,
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
                parsed.trend_min_average_distance_pct = parse_next(&mut args, &arg)?
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
            "--ignore-entry-signal-updates-while-open" => {
                parsed.ignore_entry_signal_updates_while_open = true
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
        "Usage: market_velocity_event_backtest [--event-source episodes|raw_events|raw_state] [--trade-direction long|short|both] [--target-rs 1.5,2.0] [--stop-loss-pct 0.02 --stop-loss-mode fixed_pct|structure_or_fixed|structure_with_cap --structure-stop-min-pct 0.01] [--entry-period 20] [--entry-max-signal-pullback-pct 3.0] [--entry-max-gap-without-retest-pct 0.8 --entry-retest-tolerance-pct 0.3 --entry-retest-after-signal --entry-retest-max-wait-candles 8 --entry-retest-min-entry-open-gap-pct 0.0 --entry-retest-open-fade-min-volume-ratio 2.0] [--min-delta-rank 15 --max-delta-rank 79] [--min-price-change-pct 5.0] [--event-start-ms 1717200000000 --event-end-ms 1719791999999] [--entry-trigger-allowlist breakout_previous_high,reclaim_ema] [--entry-trigger-blocklist pullback_hold_ema] [--stop-reentry-mode off|breakout_reclaim] [--profit-protect-after-r 1.0 --profit-protect-stop-r 0.0] [--runner-target-r 4.0 --runner-fraction 0.5 --runner-stop-r 0.0] [--early-exit-no-profit-candles 2] [--ignore-entry-signal-updates-while-open] [--fvg-entry-mode off|15m_to_1h|1h_to_4h|15m_self_after_signal|15m_impulse_retrace --fvg-impulse-retrace-fill-pct 20 --fvg-impulse-retrace-min-wait-candles 0] [--equity-report] [--equity-split-report] [--equity-quartile-report] [--equity-trigger-report] [--equity-concentration-report] [--equity-feature-report] [--equity-symbol-window-report] [--equity-trade-report --min-trades 30] [--save-backtest-detail] [--paper-outcome-sink off|jsonl|web]"
    );
}
/// 执行输出市场动量paperobservationusage步骤，串起回测策略需要的状态推进和错误处理。
pub fn print_market_velocity_paper_observation_usage() {
    println!(
        "Usage: market_velocity_paper_observation [--loop-interval-seconds 21600] [--paper-strategy-preset momentum_03sl_20r_v5|momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1|research_momentum_0375sl_27r_reclaim13_22_v1|research_momentum_0375sl_26r_gap05_retest03_reclaim13_22_v1|research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1|research_momentum_0375sl_20r_reclaim_fvgwait5_delta20_40_pchg5_12_v1|research_momentum_0375sl_20r_reclaim_delta13_72_pchg5_v1|research_momentum_0375sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1|research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1|research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_v1|research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner6r20_stop1_v1|research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner8r20_stop1_v1|research_momentum_04sl_20r_reclaim_fvgwait10_delta15_40_pchg5_12_v1|research_momentum_04sl_18r_reclaim_fvgwait10_delta15_40_pchg5_12_v1|research_momentum_04sl_18r_reclaim_fvgwait10_delta20_40_pchg5_10_v1|research_momentum_04sl_18r_reclaim_fvgwait12_delta20_40_pchg5_10_v1|research_momentum_04sl_18r_reclaim_fvgwait14_pullback3_delta20_40_pchg5_10_v1|research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2|research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_pullback3_delta20_40_pchg5_10_v3|research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_openfadevol2_pullback3_delta20_40_pchg5_10_v4|research_momentum_04sl_18r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1|research_momentum_04sl_20r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1|research_momentum_04sl_18r_breakout_reclaim_retest1_delta20_40_pchg5_10_v1|research_momentum_04sl_18r_breakout_reclaim_fvg_retest1_delta20_40_pchg5_8_v1|research_momentum_04sl_20r_breakout_reclaim_fvgwait10_minwait1_delta15_40_pchg5_12_v1|research_episode_momentum_03sl_24r_rank5_30_v1|research_episode_momentum_05sl_20r_rank5_v1|research_episode_momentum_05sl_30r_rank5_v1|research_episode_runner_03sl_24r_8r30_v1] [--target-rs 2.0] [--stop-loss-pct 0.03] [--entry-period 20]"
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
