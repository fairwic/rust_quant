use crate::app::market_velocity_event_backtest::{
    MARKET_RSI_VOLUME_REGIME_V1_ENTRY_RULE_VERSION, MARKET_RSI_VOLUME_REGIME_V2_ENTRY_RULE_VERSION,
    MARKET_RSI_VOLUME_REGIME_V3_ENTRY_RULE_VERSION, MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION,
    MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION,
};

/// 追加 RSI 放量/横盘突破 v1 的冻结研究参数；该版本不进入 paper preset 注册表。
pub(in super::super) fn append_rsi_volume_regime_v1_research_args(args: &mut Vec<String>) {
    append_rsi_volume_regime_research_args(
        args,
        MARKET_RSI_VOLUME_REGIME_V1_ENTRY_RULE_VERSION,
        "2.0",
    );
}

/// 追加 RSI 放量/布林压缩突破/因果背离 v2 的冻结研究参数。
pub(in super::super) fn append_rsi_volume_regime_v2_research_args(args: &mut Vec<String>) {
    append_rsi_volume_regime_research_args(
        args,
        MARKET_RSI_VOLUME_REGIME_V2_ENTRY_RULE_VERSION,
        "2.0",
    );
}

/// 追加 v3 的四根 1.5 倍量比与 ATR 风险研究参数；不注册到 paper observation。
pub(in super::super) fn append_rsi_volume_regime_v3_research_args(args: &mut Vec<String>) {
    append_rsi_volume_regime_research_args(
        args,
        MARKET_RSI_VOLUME_REGIME_V3_ENTRY_RULE_VERSION,
        "1.5",
    );
}

/// 追加 v4 的背离与 96 根净幅研究参数；压缩突破分支已从该版本移除。
pub(in super::super) fn append_rsi_volume_regime_v4_research_args(args: &mut Vec<String>) {
    append_rsi_volume_regime_research_args(
        args,
        MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION,
        "1.5",
    );
}

/// 追加 v5 的十根异常量过滤基线；背离、净幅与 ATR 风险语义继承 v4。
pub(in super::super) fn append_rsi_volume_regime_v5_research_args(args: &mut Vec<String>) {
    append_rsi_volume_regime_research_args(
        args,
        MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION,
        "2.0",
    );
}

/// 各版本只共享执行与成本口径，入场语义和量比由不可变规则版本显式冻结。
fn append_rsi_volume_regime_research_args(
    args: &mut Vec<String>,
    entry_rule_version: &str,
    min_volume_ratio: &str,
) {
    args.extend(
        [
            "--paper-outcome-entry-rule-version",
            entry_rule_version,
            "--event-source",
            "kline_15m",
            "--kline-current-live-only",
            "--trade-direction",
            "both",
            "--entry-rsi-volume-regime",
            "--stop-loss-pct",
            "0.03",
            "--stop-loss-mode",
            "structure_or_fixed",
            "--target-rs",
            "1.0",
            "--entry-period",
            "20",
            "--entry-min-volume-ratio",
            min_volume_ratio,
            "--volume-atr-take-profit",
            "--volume-atr-target-scale",
            "4.0",
            "--volume-atr-min-target-r",
            "1.8",
            "--volume-atr-max-target-r",
            "3.0",
            "--backtest-fee-bps-per-side",
            "5.0",
            "--backtest-slippage-bps-per-side",
            "3.0",
            "--trend-timeframe",
            "off",
            "--min-delta-rank",
            "0",
            "--max-price-change-pct",
            "8.0",
            "--entry-trigger-allowlist",
            "all",
            "--equity-max-holding-hours",
            "48",
            "--ignore-entry-signal-updates-while-open",
        ]
        .map(str::to_string),
    );
}
