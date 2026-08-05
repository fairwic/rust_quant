use crate::app::market_velocity_event_backtest::{
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION,
    MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION,
    MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION,
};

/// 追加独立 v1 的冻结研究参数；Top60 币池与时间窗口仍由单次回测显式传入。
pub(in super::super) fn append_filtered_volume_rsi_ema_macd_v1_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_ENTRY_RULE_VERSION,
        true,
        3.0,
        false,
    );
}

/// 追加 MACD 枢轴背离 v2 的研究参数；Z 与 D_min 必须由单次实验显式传入。
pub(in super::super) fn append_filtered_volume_rsi_ema_macd_v2_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION,
        true,
        3.0,
        false,
    );
}

/// 追加同币种周 `vol_ccy` P90 与形态风险契约 v3；当前规范明确不设置最长持仓。
pub(in super::super) fn append_filtered_volume_rsi_ema_macd_v3_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_ENTRY_RULE_VERSION,
        false,
        3.0,
        false,
    );
}

/// 追加 BB(12, 2.6) 反向冲突缓冲 v4；其余成交量和风险参数完全沿用 v3。
pub(in super::super) fn append_filtered_volume_rsi_ema_macd_v4_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_ENTRY_RULE_VERSION,
        false,
        3.0,
        false,
    );
}

/// 追加 EMA144 一 ATR 距离门禁 v5；不启用 v4 的布林冲突缓冲。
pub(in super::super) fn append_filtered_volume_rsi_ema_macd_v5_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_ENTRY_RULE_VERSION,
        false,
        3.0,
        false,
    );
}

/// 追加双端量比 2.5 与周 P90 放量锚点 RSI 背离 v9；其余风险合同沿用 v3。
pub(in super::super) fn append_filtered_volume_rsi_ema_macd_v9_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION,
        false,
        2.5,
        false,
    );
}

/// 追加 v9 锚点背离的下一根收盘确认 v10；其他量比、风险与成本合同保持不变。
pub(in super::super) fn append_filtered_volume_rsi_ema_macd_v10_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION,
        false,
        2.5,
        false,
    );
}

/// 追加 v11 影线下一开盘/非影线下一根盘中触价成交；其他合同保持 v9 不变。
pub(in super::super) fn append_filtered_volume_rsi_ema_macd_v11_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION,
        false,
        2.5,
        false,
    );
}

/// 追加 v12 趋势止盈与持仓放量阶梯保护；入场参数严格复用 v11。
pub(in super::super) fn append_filtered_volume_rsi_ema_macd_v12_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION,
        false,
        2.5,
        false,
    );
}

/// 追加 V13 普通逆势 1.5 ATR 目标；其余参数严格复用 V12。
pub(in super::super) fn append_filtered_volume_rsi_ema_macd_v13_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION,
        false,
        2.5,
        false,
    );
}

/// 追加只含 96 根净移动、异常量与价格拒绝的动量衰竭家族参数。
pub(in super::super) fn append_momentum_exhaustion_reversal_v1_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION,
        true,
        2.5,
        true,
    );
}

/// 追加方向性影线极值挂单 12 根与量比分档 ATR 目标的动量衰竭 V2 参数。
pub(in super::super) fn append_momentum_exhaustion_reversal_v2_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION,
        true,
        2.5,
        false,
    );
}

/// 追加 55% 方向影线门槛的动量衰竭 V3 参数，其余参数严格复用 V2。
pub(in super::super) fn append_momentum_exhaustion_reversal_v3_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION,
        true,
        2.5,
        false,
    );
}

/// 追加只含 q/p 放量锚点 RSI 背离与价格拒绝的独立家族参数。
pub(in super::super) fn append_volume_anchor_rsi_divergence_reversal_v1_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION,
        true,
        2.5,
        true,
    );
}

/// 追加四根锚点间隔与 60/40 摆动重置门禁的独立 V2 参数。
pub(in super::super) fn append_volume_anchor_rsi_divergence_reversal_v2_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION,
        true,
        2.5,
        true,
    );
}

/// 追加只含放量平台破位、两根接受确认与长期 EMA 确认的趋势家族参数。
pub(in super::super) fn append_volume_platform_break_trend_v1_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION,
        true,
        2.5,
        true,
    );
}

/// 追加破位前 ATR、水平性与分散触碰平台门禁的趋势 V2 参数。
pub(in super::super) fn append_volume_platform_break_trend_v2_research_args(
    args: &mut Vec<String>,
) {
    append_filtered_volume_rsi_ema_macd_research_args(
        args,
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION,
        true,
        2.5,
        true,
    );
}

fn append_filtered_volume_rsi_ema_macd_research_args(
    args: &mut Vec<String>,
    entry_rule_version: &'static str,
    max_holding_48h: bool,
    entry_min_volume_ratio: f64,
    fixed_one_r_exit: bool,
) {
    let entry_min_volume_ratio = if (entry_min_volume_ratio - 2.5).abs() <= f64::EPSILON {
        "2.5"
    } else {
        "3.0"
    };
    args.extend(
        [
            "--paper-outcome-entry-rule-version",
            entry_rule_version,
            "--event-source",
            "kline_15m",
            "--kline-current-live-only",
            "--trade-direction",
            "both",
            "--entry-filtered-volume-rsi-ema-macd",
            "--stop-loss-pct",
            "0.03",
            "--stop-loss-mode",
            "structure_or_fixed",
            "--target-rs",
            "1.0",
            "--entry-period",
            "20",
            "--entry-min-volume-ratio",
            entry_min_volume_ratio,
            "--backtest-fee-bps-per-side",
            "5.0",
            "--backtest-slippage-bps-per-side",
            "3.0",
            "--trend-timeframe",
            "off",
            "--min-delta-rank",
            "0",
            "--entry-trigger-allowlist",
            "all",
            "--ignore-entry-signal-updates-while-open",
        ]
        .map(str::to_string),
    );
    if !fixed_one_r_exit {
        args.extend(
            [
                "--volume-atr-take-profit",
                "--volume-atr-target-scale",
                "1.0",
                "--volume-atr-min-target-r",
                "1.8",
                "--volume-atr-max-target-r",
                "3.0",
            ]
            .map(str::to_string),
        );
    }
    if max_holding_48h {
        args.extend(["--equity-max-holding-hours", "48"].map(str::to_string));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::parse_cli_args_from;

    #[test]
    fn v2_cli_requires_z_and_d_min_as_a_valid_pair() {
        let mut valid = Vec::new();
        append_filtered_volume_rsi_ema_macd_v2_research_args(&mut valid);
        valid.extend(
            [
                "--entry-filtered-volume-macd-zero-band-atr-multiplier",
                "0.1",
                "--entry-filtered-volume-macd-min-normalized-dif-improvement",
                "0.005",
            ]
            .map(str::to_string),
        );
        let parsed = parse_cli_args_from(valid).unwrap();
        assert_eq!(
            parsed.entry_filtered_volume_macd_zero_band_atr_multiplier,
            Some(0.1)
        );
        assert_eq!(
            parsed.entry_filtered_volume_macd_min_normalized_dif_improvement,
            Some(0.005)
        );

        let mut partial = Vec::new();
        append_filtered_volume_rsi_ema_macd_v2_research_args(&mut partial);
        partial.extend(
            [
                "--entry-filtered-volume-macd-zero-band-atr-multiplier",
                "0.1",
            ]
            .map(str::to_string),
        );
        assert!(parse_cli_args_from(partial)
            .unwrap_err()
            .to_string()
            .contains("requires Z and D_min together"));

        let mut non_positive = Vec::new();
        append_filtered_volume_rsi_ema_macd_v2_research_args(&mut non_positive);
        non_positive.extend(
            [
                "--entry-filtered-volume-macd-zero-band-atr-multiplier",
                "0",
                "--entry-filtered-volume-macd-min-normalized-dif-improvement",
                "0.005",
            ]
            .map(str::to_string),
        );
        assert!(parse_cli_args_from(non_positive)
            .unwrap_err()
            .to_string()
            .contains("must both be finite and greater than 0"));
    }

    #[test]
    fn v9_freezes_two_and_a_half_volume_ratio() {
        let mut args = Vec::new();
        append_filtered_volume_rsi_ema_macd_v9_research_args(&mut args);
        let parsed = parse_cli_args_from(args.clone()).unwrap();
        assert_eq!(parsed.entry_min_volume_ratio, 2.5);

        let ratio_idx = args
            .iter()
            .position(|item| item == "--entry-min-volume-ratio")
            .unwrap()
            + 1;
        args[ratio_idx] = "3.0".to_string();
        assert!(parse_cli_args_from(args)
            .unwrap_err()
            .to_string()
            .contains("requires --entry-min-volume-ratio 2.5"));
    }

    #[test]
    fn v10_freezes_two_and_a_half_volume_ratio() {
        let mut args = Vec::new();
        append_filtered_volume_rsi_ema_macd_v10_research_args(&mut args);
        let parsed = parse_cli_args_from(args.clone()).unwrap();
        assert_eq!(parsed.entry_min_volume_ratio, 2.5);

        let ratio_idx = args
            .iter()
            .position(|item| item == "--entry-min-volume-ratio")
            .unwrap()
            + 1;
        args[ratio_idx] = "3.0".to_string();
        assert!(parse_cli_args_from(args)
            .unwrap_err()
            .to_string()
            .contains("requires --entry-min-volume-ratio 2.5"));
    }

    #[test]
    fn v11_freezes_two_and_a_half_volume_ratio() {
        let mut args = Vec::new();
        append_filtered_volume_rsi_ema_macd_v11_research_args(&mut args);
        let parsed = parse_cli_args_from(args.clone()).unwrap();
        assert_eq!(parsed.entry_min_volume_ratio, 2.5);

        let ratio_idx = args
            .iter()
            .position(|item| item == "--entry-min-volume-ratio")
            .unwrap()
            + 1;
        args[ratio_idx] = "3.0".to_string();
        assert!(parse_cli_args_from(args)
            .unwrap_err()
            .to_string()
            .contains("requires --entry-min-volume-ratio 2.5"));
    }

    #[test]
    fn v12_freezes_two_and_a_half_volume_ratio_without_a_max_holding_window() {
        let mut args = Vec::new();
        append_filtered_volume_rsi_ema_macd_v12_research_args(&mut args);
        let parsed = parse_cli_args_from(args).unwrap();

        assert_eq!(parsed.entry_min_volume_ratio, 2.5);
        assert_eq!(parsed.equity_max_holding_hours, None);
        assert_eq!(
            parsed.paper_outcome_entry_rule_version,
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION
        );
    }

    #[test]
    fn v13_changes_only_the_versioned_countertrend_target_policy() {
        let mut v12_args = Vec::new();
        append_filtered_volume_rsi_ema_macd_v12_research_args(&mut v12_args);
        let mut v13_args = Vec::new();
        append_filtered_volume_rsi_ema_macd_v13_research_args(&mut v13_args);

        let v12 = parse_cli_args_from(v12_args).unwrap();
        let v13 = parse_cli_args_from(v13_args).unwrap();
        assert_eq!(v13.entry_min_volume_ratio, 2.5);
        assert_eq!(v13.equity_max_holding_hours, None);
        assert_eq!(
            v13.paper_outcome_entry_rule_version,
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION
        );
        assert_ne!(
            v12.paper_outcome_entry_rule_version,
            v13.paper_outcome_entry_rule_version
        );
    }

    #[test]
    fn isolated_families_freeze_their_versioned_risk_contracts() {
        for (append, version) in [
            (
                append_momentum_exhaustion_reversal_v1_research_args as fn(&mut Vec<String>),
                MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION,
            ),
            (
                append_momentum_exhaustion_reversal_v2_research_args,
                MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION,
            ),
            (
                append_momentum_exhaustion_reversal_v3_research_args,
                MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION,
            ),
            (
                append_volume_anchor_rsi_divergence_reversal_v1_research_args,
                MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION,
            ),
            (
                append_volume_anchor_rsi_divergence_reversal_v2_research_args,
                MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION,
            ),
            (
                append_volume_platform_break_trend_v1_research_args,
                MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION,
            ),
            (
                append_volume_platform_break_trend_v2_research_args,
                MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION,
            ),
        ] {
            let mut raw = Vec::new();
            append(&mut raw);
            let parsed = parse_cli_args_from(raw).unwrap();

            assert_eq!(parsed.paper_outcome_entry_rule_version, version);
            assert_eq!(parsed.entry_min_volume_ratio, 2.5);
            assert_eq!(parsed.equity_max_holding_hours, Some(48));
            assert_eq!(parsed.target_rs, vec![1.0]);
            assert_eq!(
                parsed.volume_atr_take_profit,
                matches!(
                    version,
                    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION
                        | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION
                )
            );
            assert!(parsed.entry_filtered_volume_rsi_ema_macd);
        }
    }
}
