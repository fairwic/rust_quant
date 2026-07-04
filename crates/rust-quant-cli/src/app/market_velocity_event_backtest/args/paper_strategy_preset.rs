use anyhow::{bail, Result};

pub(super) const PAPER_STRATEGY_PRESET_FLAG: &str = "--paper-strategy-preset";
pub(super) const MOMENTUM_PROFIT_PRESET: &str = "momentum_03sl_20r_v5";
pub(super) const MOMENTUM_PROFIT_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_momentum_03sl_20r_v5";
pub(super) const MOMENTUM_STABLE_RECLAIM_MA_PULLBACK_PRESET: &str =
    "momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1";
pub(super) const MOMENTUM_STABLE_RECLAIM_MA_PULLBACK_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_mom0375_17r_rcm_ma_pb_d18_42_p5_10_v1";
pub(super) const MOMENTUM_RECLAIM_MIDRANK_RESEARCH_PRESET: &str =
    "research_momentum_0375sl_27r_reclaim13_22_v1";
pub(super) const MOMENTUM_RECLAIM_MIDRANK_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_research_0375sl_27r_dist55_reclaim13_22_v1";
pub(super) const MOMENTUM_RECLAIM_GAP_RETEST_RESEARCH_PRESET: &str =
    "research_momentum_0375sl_26r_gap05_retest03_reclaim13_22_v1";
pub(super) const MOMENTUM_RECLAIM_GAP_RETEST_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r0375_26r_gap05_rt03_rcm13_22_v1";
pub(super) const MOMENTUM_SIGNAL_RETEST_RESEARCH_PRESET: &str =
    "research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1";
pub(super) const MOMENTUM_SIGNAL_RETEST_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r0375_15r_sigrt2_d24_34_p5_10_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT5_RESEARCH_PRESET: &str =
    "research_momentum_0375sl_20r_reclaim_fvgwait5_delta20_40_pchg5_12_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT5_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r0375_20r_rcm_fvg5_d20_40_p5_12_v1";
pub(super) const MOMENTUM_RECLAIM_ONLY_0375SL_20R_DELTA13_72_RESEARCH_PRESET: &str =
    "research_momentum_0375sl_20r_reclaim_delta13_72_pchg5_v1";
pub(super) const MOMENTUM_RECLAIM_ONLY_0375SL_20R_DELTA13_72_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r0375_20r_rcm_d13_72_p5_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_RESEARCH_PRESET: &str =
    "research_momentum_0375sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r0375_20r_brk_rcm_fvg10_d20_40_p5_12_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_LOW_TARGET_0375SL_DELTA11_72_RESEARCH_PRESET: &str =
    "research_momentum_0375sl_10r_breakout_reclaim_delta11_72_pchg4_12_dist14_vol11_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_LOW_TARGET_0375SL_DELTA11_72_RESEARCH_ENTRY_RULE_VERSION:
    &str = "rank_radar_4h15m_r0375_10r_brk_rcm_d11_72_p4_12_dist14_vol11_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_MA_IGNORE_LOW_TARGET_0375SL_DELTA11_72_RESEARCH_PRESET: &str =
    "research_momentum_0375sl_10r_breakout_reclaim_ma_delta11_72_pchg4_12_dist14_vol11_ignore_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_MA_IGNORE_LOW_TARGET_0375SL_DELTA11_72_RESEARCH_ENTRY_RULE_VERSION:
    &str = "rank_radar_4h15m_r0375_10r_brk_rcm_ma_ign_d11_72_p4_12_dist14_vol11_v1";
pub(super) const MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_0375SL_DELTA5_72_RESEARCH_PRESET: &str =
    "research_momentum_short_0375sl_10r_15m_support_breakdown_delta5_72_pchg1p5_12_vol13_v1";
pub(super) const MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_0375SL_DELTA5_72_RESEARCH_ENTRY_RULE_VERSION:
    &str = "rank_radar_15m_short_r0375_10r_15msup_brkdn_d5_72_p1p5_12_v1";
pub(super) const MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_04SL_DELTA5_72_V2_RESEARCH_PRESET: &str =
    "research_momentum_short_04sl_10r_15m_support_breakdown_d5_72_pchg1p5_12_vol11_prevlow_v2";
pub(super) const MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_04SL_DELTA5_72_V2_RESEARCH_ENTRY_RULE_VERSION:
    &str = "rank_radar_15m_short_r04_10r_15msup_brkdn_d5_72_p1p5_12_vol11_prev_v2";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_RESEARCH_PRESET: &str =
    "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d20_40_p5_12_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d15_40_p5_12_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RUNNER6R20_STOP1_RESEARCH_PRESET: &str =
    "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner6r20_stop1_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RUNNER6R20_STOP1_RESEARCH_ENTRY_RULE_VERSION:
    &str = "rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d15_40_p5_12_r6f20_s1_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RUNNER8R20_STOP1_RESEARCH_PRESET: &str =
    "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner8r20_stop1_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RUNNER8R20_STOP1_RESEARCH_ENTRY_RULE_VERSION:
    &str = "rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d15_40_p5_12_r8f20_s1_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_20r_reclaim_fvgwait10_delta15_40_pchg5_12_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT10_04SL_DELTA15_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_20r_rcm_fvg10_d15_40_p5_12_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT10_04SL_18R_DELTA15_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_18r_reclaim_fvgwait10_delta15_40_pchg5_12_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT10_04SL_18R_DELTA15_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_18r_rcm_fvg10_d15_40_p5_12_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT10_04SL_18R_DELTA20_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_18r_reclaim_fvgwait10_delta20_40_pchg5_10_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT10_04SL_18R_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_18r_rcm_fvg10_d20_40_p5_10_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT12_04SL_18R_DELTA20_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_18r_reclaim_fvgwait12_delta20_40_pchg5_10_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT12_04SL_18R_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_18r_rcm_fvg12_d20_40_p5_10_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT14_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_18r_reclaim_fvgwait14_pullback3_delta20_40_pchg5_10_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT14_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_18r_rcm_fvg14_d3_pb3_vol11_fp10_d20_40_p5_10_v1";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_18r_rcm_fvg_rt1_pb3_vol11_d20_40_p5_10_v2";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_GAP0_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_pullback3_delta20_40_pchg5_10_v3";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_GAP0_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_18r_rcm_fvg_rt1_t2_gap0_pb3_vol11_d20_40_p5_10_v3";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_GAP0_OPEN_FADE_VOL2_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_18r_reclaim_fvg_retest1_gap0_openfadevol2_pullback3_delta20_40_pchg5_10_v4";
pub(super) const MOMENTUM_RECLAIM_FVG_WAIT14_RETEST1_GAP0_OPEN_FADE_VOL2_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_18r_rcm_fvg_rt1_t2_gap0_ofv2_pb3_v11_d20_40_p5_10_v4";
pub(super) const MOMENTUM_RECLAIM_RETEST1_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_18r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1";
pub(super) const MOMENTUM_RECLAIM_RETEST1_04SL_18R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_18r_rcm_rt1_d3_pb3_vol11_d20_40_p5_10_v1";
pub(super) const MOMENTUM_RECLAIM_RETEST1_04SL_20R_PULLBACK3_DELTA20_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_20r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1";
pub(super) const MOMENTUM_RECLAIM_RETEST1_04SL_20R_PULLBACK3_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_20r_rcm_rt1_d3_pb3_vol11_d20_40_p5_10_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_RETEST1_04SL_18R_DELTA20_40_RESEARCH_PRESET: &str =
    "research_momentum_04sl_18r_breakout_reclaim_retest1_delta20_40_pchg5_10_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_RETEST1_04SL_18R_DELTA20_40_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_18r_brk_rcm_rt1_vol10_d20_40_p5_10_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_RETEST1_04SL_18R_DELTA20_40_PCHG5_8_RESEARCH_PRESET: &str =
    "research_momentum_04sl_18r_breakout_reclaim_fvg_retest1_delta20_40_pchg5_8_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_RETEST1_04SL_18R_DELTA20_40_PCHG5_8_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h15m_r04_18r_brk_rcm_fvg_rt1_vol10_d20_40_p5_8_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_MINWAIT1_04SL_DELTA15_40_RESEARCH_PRESET:
    &str = "research_momentum_04sl_20r_breakout_reclaim_fvgwait10_minwait1_delta15_40_pchg5_12_v1";
pub(super) const MOMENTUM_BREAKOUT_RECLAIM_FVG_WAIT10_MINWAIT1_04SL_DELTA15_40_RESEARCH_ENTRY_RULE_VERSION:
    &str = "rank_radar_4h15m_r04_20r_brk_rcm_fvg10_mw1_d15_40_p5_12_v1";
pub(super) const MOMENTUM_KLINE15M_BREAKOUT_FVG20_04SL_10R_RESEARCH_PRESET: &str =
    "research_momentum_04sl_10r_kline15m_breakout_fvg20_vol13_dd35_v1";
pub(super) const MOMENTUM_KLINE15M_BREAKOUT_FVG20_04SL_10R_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_mom04_10r_brk_fvg20_vol13_dd35_v1";
pub(super) const MOMENTUM_KLINE15M_BREAKOUT_FVG20_04SL_06R_RESEARCH_PRESET: &str =
    "research_momentum_04sl_06r_kline15m_breakout_fvg20_vol13_dd35_v1";
pub(super) const MOMENTUM_KLINE15M_BREAKOUT_FVG20_04SL_06R_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_mom04_06r_brk_fvg20_vol13_dd35_v1";
pub(super) const MOMENTUM_KLINE15M_BREAKOUT_FVG30_04SL_05R_RESEARCH_PRESET: &str =
    "research_momentum_04sl_05r_kline15m_breakout_fvg30_vol13_dd35_v1";
pub(super) const MOMENTUM_KLINE15M_BREAKOUT_FVG30_04SL_05R_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_mom04_05r_brk_fvg30_vol13_dd35_v1";
pub(super) const MOMENTUM_KLINE15M_BREAKOUT_FVG30_04SL_055R_RESEARCH_PRESET: &str =
    "research_momentum_04sl_055r_kline15m_breakout_fvg30_vol13_dd35_v1";
pub(super) const MOMENTUM_KLINE15M_BREAKOUT_FVG30_04SL_055R_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_mom04_055r_brk_fvg30_vol13_dd35_v1";
pub(super) const MOMENTUM_KLINE15M_BREAKOUT_FVG50_04SL_052R_RESEARCH_PRESET: &str =
    "research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1";
pub(super) const MOMENTUM_KLINE15M_BREAKOUT_FVG50_04SL_052R_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_mom04_052r_brk_fvg50_vol13_dd35_v1";
pub(super) const EPISODE_MOMENTUM_RESEARCH_PRESET: &str =
    "research_episode_momentum_03sl_24r_rank5_30_v1";
pub(super) const EPISODE_MOMENTUM_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1";
pub(super) const EPISODE_MOMENTUM_05SL_20R_RESEARCH_PRESET: &str =
    "research_episode_momentum_05sl_20r_rank5_v1";
pub(super) const EPISODE_MOMENTUM_05SL_20R_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_episode_research_05sl_20r_rank5_v1";
pub(super) const EPISODE_MOMENTUM_05SL_30R_RESEARCH_PRESET: &str =
    "research_episode_momentum_05sl_30r_rank5_v1";
pub(super) const EPISODE_MOMENTUM_05SL_30R_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_episode_research_05sl_30r_rank5_v1";
pub(super) const EPISODE_RUNNER_RESEARCH_PRESET: &str = "research_episode_runner_03sl_24r_8r30_v1";
pub(super) const EPISODE_RUNNER_RESEARCH_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_episode_runner_03sl_24r_8r30_v1";

pub(super) const PAPER_STRATEGY_PRESET_LOCKED_FLAGS: &[&str] = &[
    "--event-source",
    "--trade-direction",
    "--target-rs",
    "--stop-loss-pct",
    "--entry-period",
    "--entry-max-distance-pct",
    "--entry-min-volume-ratio",
    "--entry-max-signal-pullback-pct",
    "--entry-max-gap-without-retest-pct",
    "--entry-retest-tolerance-pct",
    "--entry-retest-after-signal",
    "--entry-retest-max-wait-candles",
    "--entry-retest-min-entry-open-gap-pct",
    "--entry-retest-open-fade-min-volume-ratio",
    "--entry-min-rsi",
    "--entry-max-rsi",
    "--entry-min-rsi-delta",
    "--entry-rsi-delta-lookback-candles",
    "--entry-bollinger-breakout",
    "--entry-min-bollinger-bandwidth-expansion-pct",
    "--entry-min-recent-drawdown-pct",
    "--entry-recent-drawdown-lookback-candles",
    "--entry-symbol-cooldown-candles",
    "--trend-min-average-distance-pct",
    "--min-delta-rank",
    "--max-delta-rank",
    "--min-price-change-pct",
    "--max-price-change-pct",
    "--event-start-ms",
    "--event-end-ms",
    "--max-15m-staleness-min",
    "--max-4h-staleness-min",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PaperStrategyPreset {
    Momentum03Sl20R,
    Momentum0375Sl17RReclaimMaPullbackDelta18To42,
    ResearchMomentum0375Sl27RReclaim13To22,
    ResearchMomentum0375Sl26RGap05Retest03Reclaim13To22,
    ResearchMomentum0375Sl15RSignalRetest2Delta24To34,
    ResearchMomentum0375Sl20RReclaimFvgWait5Delta20To40,
    ResearchMomentum0375Sl20RReclaimOnlyDelta13To72,
    ResearchMomentum0375Sl20RBreakoutReclaimFvgWait10Delta20To40,
    ResearchMomentum0375Sl10RBreakoutReclaimDelta11To72Dist14,
    ResearchMomentum0375Sl10RBreakoutReclaimMaIgnoreDelta11To72Dist14,
    ResearchMomentumShort0375Sl10r15mSupportBreakdownDelta5To72,
    ResearchMomentumShort04Sl10r15mSupportBreakdownDelta5To72V2,
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
    ResearchMomentumKline15mBreakoutFvg20Sl04R10,
    ResearchMomentumKline15mBreakoutFvg20Sl04R06,
    ResearchMomentumKline15mBreakoutFvg30Sl04R05,
    ResearchMomentumKline15mBreakoutFvg30Sl04R055,
    ResearchMomentumKline15mBreakoutFvg50Sl04R052,
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
    pub(super) fn from_str(value: &str) -> Result<Self> {
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
            MOMENTUM_BREAKOUT_RECLAIM_LOW_TARGET_0375SL_DELTA11_72_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum0375Sl10RBreakoutReclaimDelta11To72Dist14)
            }
            MOMENTUM_BREAKOUT_RECLAIM_MA_IGNORE_LOW_TARGET_0375SL_DELTA11_72_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentum0375Sl10RBreakoutReclaimMaIgnoreDelta11To72Dist14)
            }
            MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_0375SL_DELTA5_72_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentumShort0375Sl10r15mSupportBreakdownDelta5To72)
            }
            MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_04SL_DELTA5_72_V2_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentumShort04Sl10r15mSupportBreakdownDelta5To72V2)
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
            MOMENTUM_KLINE15M_BREAKOUT_FVG20_04SL_10R_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentumKline15mBreakoutFvg20Sl04R10)
            }
            MOMENTUM_KLINE15M_BREAKOUT_FVG20_04SL_06R_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentumKline15mBreakoutFvg20Sl04R06)
            }
            MOMENTUM_KLINE15M_BREAKOUT_FVG30_04SL_05R_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentumKline15mBreakoutFvg30Sl04R05)
            }
            MOMENTUM_KLINE15M_BREAKOUT_FVG30_04SL_055R_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentumKline15mBreakoutFvg30Sl04R055)
            }
            MOMENTUM_KLINE15M_BREAKOUT_FVG50_04SL_052R_RESEARCH_PRESET => {
                Ok(Self::ResearchMomentumKline15mBreakoutFvg50Sl04R052)
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
    pub(super) fn append_args(self, args: &mut Vec<String>) {
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
            Self::ResearchMomentum0375Sl10RBreakoutReclaimDelta11To72Dist14 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_BREAKOUT_RECLAIM_LOW_TARGET_0375SL_DELTA11_72_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.0375".to_string(),
                    "--target-rs".to_string(),
                    "1.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "14.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.1".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "11".to_string(),
                    "--max-delta-rank".to_string(),
                    "72".to_string(),
                    "--min-price-change-pct".to_string(),
                    "4.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "breakout_previous_high,reclaim_ema".to_string(),
                ]);
            }
            Self::ResearchMomentum0375Sl10RBreakoutReclaimMaIgnoreDelta11To72Dist14 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_BREAKOUT_RECLAIM_MA_IGNORE_LOW_TARGET_0375SL_DELTA11_72_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.0375".to_string(),
                    "--target-rs".to_string(),
                    "1.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "14.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.1".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "11".to_string(),
                    "--max-delta-rank".to_string(),
                    "72".to_string(),
                    "--min-price-change-pct".to_string(),
                    "4.0".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "breakout_previous_high,reclaim_ema,reclaim_ma".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentumShort0375Sl10r15mSupportBreakdownDelta5To72 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_0375SL_DELTA5_72_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--trade-direction".to_string(),
                    "short".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.0375".to_string(),
                    "--target-rs".to_string(),
                    "1.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "8.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.3".to_string(),
                    "--trend-timeframe".to_string(),
                    "off".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "5".to_string(),
                    "--max-delta-rank".to_string(),
                    "72".to_string(),
                    "--min-price-change-pct".to_string(),
                    "1.5".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "breakdown_range_low".to_string(),
                    "--ignore-entry-signal-updates-while-open".to_string(),
                ]);
            }
            Self::ResearchMomentumShort04Sl10r15mSupportBreakdownDelta5To72V2 => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    MOMENTUM_SHORT_15M_SUPPORT_BREAKDOWN_04SL_DELTA5_72_V2_RESEARCH_ENTRY_RULE_VERSION
                        .to_string(),
                    "--event-source".to_string(),
                    "raw_state".to_string(),
                    "--trade-direction".to_string(),
                    "short".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    "1.0".to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "8.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.1".to_string(),
                    "--trend-timeframe".to_string(),
                    "off".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "5".to_string(),
                    "--max-delta-rank".to_string(),
                    "72".to_string(),
                    "--min-price-change-pct".to_string(),
                    "1.5".to_string(),
                    "--max-price-change-pct".to_string(),
                    "12.0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "breakdown_range_low,breakdown_previous_low".to_string(),
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
            Self::ResearchMomentumKline15mBreakoutFvg20Sl04R10
            | Self::ResearchMomentumKline15mBreakoutFvg20Sl04R06
            | Self::ResearchMomentumKline15mBreakoutFvg30Sl04R05
            | Self::ResearchMomentumKline15mBreakoutFvg30Sl04R055
            | Self::ResearchMomentumKline15mBreakoutFvg50Sl04R052 => {
                let (entry_rule_version, target_r, sample_seed, fvg_fill_pct) = match self {
                    Self::ResearchMomentumKline15mBreakoutFvg50Sl04R052 => (
                        MOMENTUM_KLINE15M_BREAKOUT_FVG50_04SL_052R_RESEARCH_ENTRY_RULE_VERSION,
                        "0.52",
                        "kline15m_fvg50_v1",
                        "50.0",
                    ),
                    Self::ResearchMomentumKline15mBreakoutFvg30Sl04R055 => (
                        MOMENTUM_KLINE15M_BREAKOUT_FVG30_04SL_055R_RESEARCH_ENTRY_RULE_VERSION,
                        "0.55",
                        "kline15m_fvg30_v1",
                        "30.0",
                    ),
                    Self::ResearchMomentumKline15mBreakoutFvg30Sl04R05 => (
                        MOMENTUM_KLINE15M_BREAKOUT_FVG30_04SL_05R_RESEARCH_ENTRY_RULE_VERSION,
                        "0.5",
                        "kline15m_fvg30_v1",
                        "30.0",
                    ),
                    Self::ResearchMomentumKline15mBreakoutFvg20Sl04R06 => (
                        MOMENTUM_KLINE15M_BREAKOUT_FVG20_04SL_06R_RESEARCH_ENTRY_RULE_VERSION,
                        "0.6",
                        "kline15m_fvg20_v1",
                        "20.0",
                    ),
                    _ => (
                        MOMENTUM_KLINE15M_BREAKOUT_FVG20_04SL_10R_RESEARCH_ENTRY_RULE_VERSION,
                        "1.0",
                        "kline15m_fvg20_v1",
                        "20.0",
                    ),
                };
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    entry_rule_version.to_string(),
                    "--event-source".to_string(),
                    "kline_15m".to_string(),
                    "--sample-limit".to_string(),
                    "20".to_string(),
                    "--sample-seed".to_string(),
                    sample_seed.to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.04".to_string(),
                    "--target-rs".to_string(),
                    target_r.to_string(),
                    "--entry-max-distance-pct".to_string(),
                    "14.0".to_string(),
                    "--entry-min-volume-ratio".to_string(),
                    "1.3".to_string(),
                    "--entry-min-rsi".to_string(),
                    "50.0".to_string(),
                    "--entry-max-rsi".to_string(),
                    "90.0".to_string(),
                    "--entry-bollinger-breakout".to_string(),
                    "--entry-min-recent-drawdown-pct".to_string(),
                    "3.5".to_string(),
                    "--entry-recent-drawdown-lookback-candles".to_string(),
                    "12".to_string(),
                    "--entry-symbol-cooldown-candles".to_string(),
                    "4".to_string(),
                    "--trend-timeframe".to_string(),
                    "off".to_string(),
                    "--trend-min-average-distance-pct".to_string(),
                    "0.0".to_string(),
                    "--min-delta-rank".to_string(),
                    "0".to_string(),
                    "--entry-trigger-allowlist".to_string(),
                    "breakout_previous_high".to_string(),
                    "--fvg-entry-mode".to_string(),
                    "m15_impulse_retrace".to_string(),
                    "--fvg-lookback-candles".to_string(),
                    "40".to_string(),
                    "--fvg-max-wait-candles".to_string(),
                    "24".to_string(),
                    "--fvg-impulse-retrace-fill-pct".to_string(),
                    fvg_fill_pct.to_string(),
                    "--fvg-impulse-retrace-min-wait-candles".to_string(),
                    "0".to_string(),
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
