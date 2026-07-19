use super::PaperStrategyPreset;

pub(in super::super) const MARKET_MOMENTUM_OPPOSITE_MOVE_VOLUME_ATR_RESEARCH_PRESET: &str =
    "research_market_momentum_opposite_move10_n192_volume_atr_both_15m_v1";
const MARKET_MOMENTUM_OPPOSITE_MOVE_VOLUME_ATR_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_market_momentum_opposite_net10_n192_volatr_both_v1";
pub(in super::super) const MARKET_MOMENTUM_OPPOSITE_MOVE_DEFERRED_LONG_RESEARCH_PRESET: &str =
    "research_market_momentum_opposite_move10_n192_volume_atr_long_defer3_15m_v2";
const MARKET_MOMENTUM_OPPOSITE_MOVE_DEFERRED_LONG_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_market_momentum_opposite_net10_n192_volatr_long_defer3_v2";
pub(in super::super) const MARKET_MOMENTUM_OPPOSITE_MOVE_DURATION_BOTH_RESEARCH_PRESET: &str =
    "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_both_deferlong3_15m_v3";
const MARKET_MOMENTUM_OPPOSITE_MOVE_DURATION_BOTH_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_market_momentum_opposite_net10_n192_or_dur96_volatr_both_deferlong3_v3";
pub(in super::super) const MARKET_MOMENTUM_OPPOSITE_MOVE_EXHAUSTION_VOLUME_RESEARCH_PRESET: &str =
    "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_both_deferlong3_exhaustionvol1_15m_v4";
const MARKET_MOMENTUM_OPPOSITE_MOVE_EXHAUSTION_VOLUME_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_market_momentum_opposite_net10_n192_or_dur96_volatr_both_deferlong3_exvol1_v4";
pub(in super::super) const MARKET_MOMENTUM_OPPOSITE_MOVE_RISK_REWARD_RESEARCH_PRESET: &str =
    "research_market_momentum_opposite_move10_n192_or_duration96_volume_atr_r18_30_scale4_both_deferlong3_exhaustionvol1_15m_v5";
const MARKET_MOMENTUM_OPPOSITE_MOVE_RISK_REWARD_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_market_momentum_opposite_volatr_r18_30_s4_exvol1_v5";
pub(in super::super) const MARKET_MOMENTUM_OPPOSITE_MOVE_CONFIRMED_REVERSAL_RESEARCH_PRESET: &str =
    "research_market_momentum_opposite_move_reversal_confirmed_both_defer3_volatr_r18_30_15m_v6";
const MARKET_MOMENTUM_OPPOSITE_MOVE_CONFIRMED_REVERSAL_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_market_momentum_opposite_reversal_confirmed_both_defer3_v6";
pub(in super::super) const MARKET_MOMENTUM_OPPOSITE_MOVE_MEAN_RECLAIM_RESEARCH_PRESET: &str =
    "research_market_momentum_opposite_move_reversal_mean_reclaim_both_defer3_volatr_r18_30_15m_v7";
const MARKET_MOMENTUM_OPPOSITE_MOVE_MEAN_RECLAIM_RESEARCH_ENTRY_RULE_VERSION: &str =
    "kline15m_market_momentum_opposite_reversal_mean_reclaim_both_v7";

/// 追加 Market Momentum Opposite Move v1-v6 的冻结参数。
///
/// 这些版本共用同一策略身份；独立放在子模块中是为了让大型 preset 注册表保持在
/// 主项目行数限制内，同时确保旧版本参数不会在 v4 迭代中被覆盖。
pub(super) fn append_market_momentum_reversal_args(
    preset: PaperStrategyPreset,
    args: &mut Vec<String>,
) -> bool {
    let (
        rule_version,
        direction,
        duration_candles,
        defer_long,
        dominance_ratio,
        target_policy,
        explicit_costs,
        require_reversal_confirmation,
        defer_short,
    ) = match preset {
        PaperStrategyPreset::ResearchMarketMomentumOppositeMoveVolumeAtrBoth15m => (
            MARKET_MOMENTUM_OPPOSITE_MOVE_VOLUME_ATR_RESEARCH_ENTRY_RULE_VERSION,
            "both",
            None,
            false,
            None,
            None,
            false,
            false,
            false,
        ),
        PaperStrategyPreset::ResearchMarketMomentumOppositeMoveDeferredLong15mV2 => (
            MARKET_MOMENTUM_OPPOSITE_MOVE_DEFERRED_LONG_RESEARCH_ENTRY_RULE_VERSION,
            "long",
            None,
            true,
            None,
            None,
            false,
            false,
            false,
        ),
        PaperStrategyPreset::ResearchMarketMomentumOppositeMoveDurationBoth15mV3 => (
            MARKET_MOMENTUM_OPPOSITE_MOVE_DURATION_BOTH_RESEARCH_ENTRY_RULE_VERSION,
            "both",
            Some("96"),
            true,
            None,
            None,
            false,
            false,
            false,
        ),
        PaperStrategyPreset::ResearchMarketMomentumOppositeMoveExhaustionVolume15mV4 => (
            MARKET_MOMENTUM_OPPOSITE_MOVE_EXHAUSTION_VOLUME_RESEARCH_ENTRY_RULE_VERSION,
            "both",
            Some("96"),
            true,
            Some("1.0"),
            None,
            false,
            false,
            false,
        ),
        PaperStrategyPreset::ResearchMarketMomentumOppositeMoveRiskReward15mV5 => (
            MARKET_MOMENTUM_OPPOSITE_MOVE_RISK_REWARD_RESEARCH_ENTRY_RULE_VERSION,
            "both",
            Some("96"),
            true,
            Some("1.0"),
            Some(("4.0", "1.8", "3.0")),
            true,
            false,
            false,
        ),
        PaperStrategyPreset::ResearchMarketMomentumOppositeMoveConfirmedReversal15mV6 => (
            MARKET_MOMENTUM_OPPOSITE_MOVE_CONFIRMED_REVERSAL_RESEARCH_ENTRY_RULE_VERSION,
            "both",
            Some("96"),
            true,
            Some("1.0"),
            Some(("4.0", "1.8", "3.0")),
            true,
            true,
            true,
        ),
        PaperStrategyPreset::ResearchMarketMomentumOppositeMoveMeanReclaim15mV7 => (
            MARKET_MOMENTUM_OPPOSITE_MOVE_MEAN_RECLAIM_RESEARCH_ENTRY_RULE_VERSION,
            "both",
            Some("96"),
            true,
            Some("1.0"),
            Some(("4.0", "1.8", "3.0")),
            true,
            true,
            true,
        ),
        _ => return false,
    };
    let require_average_reclaim =
        preset == PaperStrategyPreset::ResearchMarketMomentumOppositeMoveMeanReclaim15mV7;

    args.extend([
        "--paper-outcome-entry-rule-version".to_string(),
        rule_version.to_string(),
        "--event-source".to_string(),
        "kline_15m".to_string(),
        "--trade-direction".to_string(),
        direction.to_string(),
        "--stop-loss-pct".to_string(),
        "0.03".to_string(),
        "--target-rs".to_string(),
        "1.0".to_string(),
        "--entry-period".to_string(),
        "20".to_string(),
        "--entry-max-distance-pct".to_string(),
        "14.0".to_string(),
        "--entry-min-volume-ratio".to_string(),
        "1.5".to_string(),
        "--entry-opposite-move-lookback-candles".to_string(),
        "192".to_string(),
        "--entry-min-opposite-net-move-pct".to_string(),
        "10.0".to_string(),
    ]);
    if let Some(duration_candles) = duration_candles {
        args.extend([
            "--entry-min-opposite-duration-candles".to_string(),
            duration_candles.to_string(),
        ]);
    }
    if let Some(dominance_ratio) = dominance_ratio {
        args.extend([
            "--entry-min-exhaustion-volume-dominance-ratio".to_string(),
            dominance_ratio.to_string(),
        ]);
    }
    args.push("--volume-atr-take-profit".to_string());
    if let Some((scale, min_target_r, max_target_r)) = target_policy {
        args.extend([
            "--volume-atr-target-scale".to_string(),
            scale.to_string(),
            "--volume-atr-min-target-r".to_string(),
            min_target_r.to_string(),
            "--volume-atr-max-target-r".to_string(),
            max_target_r.to_string(),
        ]);
    }
    if explicit_costs {
        args.extend([
            "--backtest-fee-bps-per-side".to_string(),
            "5.0".to_string(),
            "--backtest-slippage-bps-per-side".to_string(),
            "3.0".to_string(),
        ]);
    }
    if defer_long {
        args.extend([
            "--entry-defer-bearish-continuation".to_string(),
            "--entry-defer-max-wait-candles".to_string(),
            "3".to_string(),
        ]);
    }
    if defer_short {
        args.push("--entry-defer-bullish-continuation".to_string());
    }
    if require_reversal_confirmation {
        args.push("--entry-require-opposite-reversal-confirmation".to_string());
    }
    if require_average_reclaim {
        args.push("--entry-require-reversal-average-reclaim".to_string());
    }
    args.extend([
        "--trend-timeframe".to_string(),
        "off".to_string(),
        "--trend-min-average-distance-pct".to_string(),
        "0.0".to_string(),
        "--min-delta-rank".to_string(),
        "0".to_string(),
        "--min-price-change-pct".to_string(),
        "0.8".to_string(),
        "--max-price-change-pct".to_string(),
        "8.0".to_string(),
        "--entry-trigger-allowlist".to_string(),
        "all".to_string(),
        "--ignore-entry-signal-updates-while-open".to_string(),
    ]);
    true
}
