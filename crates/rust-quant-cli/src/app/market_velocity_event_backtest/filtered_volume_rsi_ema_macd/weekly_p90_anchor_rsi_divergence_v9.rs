use super::super::{
    ComputedCandle, MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
    RsiDivergenceSignalEvidence,
};
use super::weekly_base_volume_v3::{
    filtered_volume_evidence, signal_with_weekly_p90_anchor_rsi_divergence,
    weekly_volume_ccy_evidence,
};
use super::{
    BranchCandidate, FilteredVolumeRsiEmaMacdSignal, DIVERGENCE_LOOKBACK_CANDLES, RSI_OVERBOUGHT,
    RSI_OVERSOLD,
};

/// v9 对历史异常量标记、锚点 q 和当前比较点 p 统一使用的过滤量比门槛。
pub(in super::super) const FILTERED_VOLUME_V9_MIN_RATIO: f64 = 2.5;
/// 逐笔证据使用稳定模式名区分即时放量锚点与 v3 的右侧确认价格枢轴。
pub(super) const RSI_DIVERGENCE_COMPARISON_MODE: &str = "weekly_p90_filtered_volume_anchors";
/// v9 底背离的稳定触发标签，供确认版在不改变 q/p 语义时识别候选。
pub(super) const BULLISH_ANCHOR_TRIGGER: &str = "rsi_volume_anchor_bullish_divergence_long";
/// v9 顶背离的稳定触发标签，供确认版在不改变 q/p 语义时识别候选。
pub(super) const BEARISH_ANCHOR_TRIGGER: &str = "rsi_volume_anchor_bearish_divergence_short";

/// 运行独立 v9；参数不等于冻结的 2.5 时失败关闭，避免同一版本名产生不同结果。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    if (args.entry_min_volume_ratio - FILTERED_VOLUME_V9_MIN_RATIO).abs() > f64::EPSILON {
        return Err("filtered_volume_v9_ratio_policy_mismatch");
    }
    signal_with_weekly_p90_anchor_rsi_divergence(
        candles,
        completed_count,
        args,
        FILTERED_VOLUME_V9_MIN_RATIO,
    )
}

/// 只比较当前完成 K 与最近方向性放量锚；先定最近锚再验背离，禁止向前挑有利样本。
pub(super) fn rsi_divergence_candidates(
    candles: &[ComputedCandle],
    latest_idx: usize,
    current_rsi: f64,
    filtered_volume_min_ratio: f64,
) -> (Vec<BranchCandidate>, Vec<RsiDivergenceSignalEvidence>) {
    let direction = if current_rsi <= RSI_OVERSOLD {
        MarketVelocityTradeDirection::Long
    } else if current_rsi >= RSI_OVERBOUGHT {
        MarketVelocityTradeDirection::Short
    } else {
        return (Vec::new(), Vec::new());
    };
    let Some(current) = candles.get(latest_idx) else {
        return (Vec::new(), Vec::new());
    };
    let Ok(current_volume) =
        filtered_volume_evidence(candles, latest_idx, filtered_volume_min_ratio)
    else {
        return (Vec::new(), Vec::new());
    };
    if current_volume.ratio < filtered_volume_min_ratio {
        return (Vec::new(), Vec::new());
    }
    let Ok(current_weekly_volume) = weekly_volume_ccy_evidence(candles, latest_idx) else {
        return (Vec::new(), Vec::new());
    };

    let start = latest_idx.saturating_sub(DIVERGENCE_LOOKBACK_CANDLES);
    let reference = (start..latest_idx).rev().find_map(|reference_idx| {
        let candle = candles.get(reference_idx)?;
        let reference_rsi = candle.rsi14.filter(|value| value.is_finite())?;
        let directionally_extreme = match direction {
            MarketVelocityTradeDirection::Long => reference_rsi <= RSI_OVERSOLD,
            MarketVelocityTradeDirection::Short => reference_rsi >= RSI_OVERBOUGHT,
            MarketVelocityTradeDirection::Both => false,
        };
        if !directionally_extreme {
            return None;
        }
        let volume =
            filtered_volume_evidence(candles, reference_idx, filtered_volume_min_ratio).ok()?;
        if volume.ratio < filtered_volume_min_ratio {
            return None;
        }
        let weekly_volume = weekly_volume_ccy_evidence(candles, reference_idx).ok()?;
        Some((reference_idx, reference_rsi, volume.ratio, weekly_volume))
    });
    let Some((reference_idx, reference_rsi, reference_ratio, reference_weekly_volume)) = reference
    else {
        return (Vec::new(), Vec::new());
    };
    let Some(reference) = candles.get(reference_idx) else {
        return (Vec::new(), Vec::new());
    };

    let (trigger, current_price, reference_price, confirmed) = match direction {
        MarketVelocityTradeDirection::Long => (
            BULLISH_ANCHOR_TRIGGER,
            current.candle.low,
            reference.candle.low,
            current.candle.low < reference.candle.low && current_rsi >= reference_rsi,
        ),
        MarketVelocityTradeDirection::Short => (
            BEARISH_ANCHOR_TRIGGER,
            current.candle.high,
            reference.candle.high,
            current.candle.high > reference.candle.high && current_rsi <= reference_rsi,
        ),
        MarketVelocityTradeDirection::Both => return (Vec::new(), Vec::new()),
    };
    if !current_price.is_finite() || !reference_price.is_finite() || !confirmed {
        return (Vec::new(), Vec::new());
    }

    let candidate = BranchCandidate { direction, trigger };
    let evidence = RsiDivergenceSignalEvidence {
        comparison_mode: RSI_DIVERGENCE_COMPARISON_MODE,
        direction,
        pivot_ts_ms: current.candle.ts,
        reference_pivot_ts_ms: reference.candle.ts,
        pivot_price: current_price,
        reference_pivot_price: reference_price,
        pivot_rsi14: current_rsi,
        reference_pivot_rsi14: reference_rsi,
        pivot_filtered_volume_ratio: Some(current_volume.ratio),
        reference_filtered_volume_ratio: Some(reference_ratio),
        pivot_volume_ccy: Some(current_weekly_volume.current),
        reference_volume_ccy: Some(reference_weekly_volume.current),
        pivot_weekly_volume_ccy_p90: Some(current_weekly_volume.p90),
        reference_weekly_volume_ccy_p90: Some(reference_weekly_volume.p90),
        confirmation_ts_ms: None,
        confirmation_close: None,
        confirmation_break_price: None,
    };
    (vec![candidate], vec![evidence])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{
        market_filtered_volume_rsi_ema_macd_v3_research_args,
        market_filtered_volume_rsi_ema_macd_v9_research_args,
        market_velocity_paper_strategy_preset_manifest, market_velocity_risk_config_detail,
        market_velocity_strategy_detail, market_velocity_strategy_type, BacktestCandle,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRODUCT_SLUG,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_STRATEGY_KEY, MS_15M,
    };

    fn candle(idx: usize) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: idx as i64 * MS_15M,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
                volume: 10.0,
            },
            volume_ccy: Some(100.0),
            sma: Some(100.0),
            ema: Some(100.0),
            ema12: Some(100.0),
            ema144: Some(100.0),
            ema169: Some(100.0),
            ema696: Some(100.0),
            previous_volume_avg: Some(10.0),
            previous_range_avg: Some(2.0),
            rsi14: Some(50.0),
            atr14: Some(2.0),
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: Some(0.0),
            macd_signal_line: Some(0.0),
            macd_histogram: Some(0.0),
        }
    }

    fn candles() -> Vec<ComputedCandle> {
        (0..750).map(candle).collect()
    }

    fn qualify_anchor(candles: &mut [ComputedCandle], idx: usize, rsi: f64) {
        candles[idx].candle.volume = 25.0;
        candles[idx].volume_ccy = Some(200.0);
        candles[idx].rsi14 = Some(rsi);
    }

    fn v9_signal(
        candles: &[ComputedCandle],
        completed_count: usize,
    ) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
        let mut args = market_filtered_volume_rsi_ema_macd_v3_research_args().unwrap();
        args.paper_outcome_entry_rule_version =
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION.to_string();
        args.entry_min_volume_ratio = FILTERED_VOLUME_V9_MIN_RATIO;
        signal(candles, completed_count, &args)
    }

    #[test]
    fn current_lower_low_with_equal_rsi_uses_nearest_qualified_anchor_immediately() {
        let mut candles = candles();
        let latest_idx = candles.len() - 1;
        let reference_idx = latest_idx - 27;
        qualify_anchor(&mut candles, reference_idx, 27.0);
        candles[reference_idx].candle.low = 97.0;
        qualify_anchor(&mut candles, latest_idx, 27.0);
        candles[latest_idx].candle.low = 96.0;

        let before = v9_signal(&candles, candles.len()).unwrap();
        let completed_count = candles.len();
        candles.push(candle(completed_count));
        candles.last_mut().unwrap().candle.low = 1.0;
        candles.last_mut().unwrap().rsi14 = Some(99.0);
        let after = v9_signal(&candles, completed_count).unwrap();

        assert_eq!(before, after);
        assert_eq!(before.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(before.trigger, "rsi_volume_anchor_bullish_divergence_long");
        let evidence = &before.evidence.rsi_divergences[0];
        assert_eq!(evidence.pivot_ts_ms, candles[latest_idx].candle.ts);
        assert_eq!(
            evidence.reference_pivot_ts_ms,
            candles[reference_idx].candle.ts
        );
        assert_eq!(evidence.reference_filtered_volume_ratio, Some(2.5));
    }

    #[test]
    fn neutral_volume_bar_does_not_replace_directional_anchor() {
        let mut candles = candles();
        let latest_idx = candles.len() - 1;
        let reference_idx = latest_idx - 20;
        qualify_anchor(&mut candles, reference_idx, 25.0);
        candles[reference_idx].candle.low = 97.0;
        qualify_anchor(&mut candles, latest_idx - 5, 50.0);
        qualify_anchor(&mut candles, latest_idx, 28.0);
        candles[latest_idx].candle.low = 96.0;

        let signal = v9_signal(&candles, candles.len()).unwrap();

        assert_eq!(
            signal.evidence.rsi_divergences[0].reference_pivot_ts_ms,
            candles[reference_idx].candle.ts
        );
    }

    #[test]
    fn nearer_directional_anchor_blocks_cherry_picking_an_older_pair() {
        let mut candles = candles();
        let latest_idx = candles.len() - 1;
        qualify_anchor(&mut candles, latest_idx - 30, 25.0);
        candles[latest_idx - 30].candle.low = 97.0;
        qualify_anchor(&mut candles, latest_idx - 8, 26.0);
        candles[latest_idx - 8].candle.low = 95.0;
        qualify_anchor(&mut candles, latest_idx, 28.0);
        candles[latest_idx].candle.low = 96.0;

        assert_eq!(
            v9_signal(&candles, candles.len()),
            Err("filtered_volume_v9_no_branch_signal")
        );
    }

    #[test]
    fn lookback_includes_48_bars_and_excludes_49() {
        let mut included = candles();
        let latest_idx = included.len() - 1;
        qualify_anchor(&mut included, latest_idx - 48, 25.0);
        included[latest_idx - 48].candle.low = 97.0;
        qualify_anchor(&mut included, latest_idx, 28.0);
        included[latest_idx].candle.low = 96.0;
        assert!(v9_signal(&included, included.len()).is_ok());

        let mut excluded = candles();
        qualify_anchor(&mut excluded, latest_idx - 49, 25.0);
        excluded[latest_idx - 49].candle.low = 97.0;
        qualify_anchor(&mut excluded, latest_idx, 28.0);
        excluded[latest_idx].candle.low = 96.0;
        assert_eq!(
            v9_signal(&excluded, excluded.len()),
            Err("filtered_volume_v9_no_branch_signal")
        );
    }

    #[test]
    fn bearish_divergence_is_the_exact_mirror() {
        let mut candles = candles();
        let latest_idx = candles.len() - 1;
        qualify_anchor(&mut candles, latest_idx - 24, 75.0);
        candles[latest_idx - 24].candle.high = 103.0;
        qualify_anchor(&mut candles, latest_idx, 72.0);
        candles[latest_idx].candle.high = 104.0;

        let signal = v9_signal(&candles, candles.len()).unwrap();

        assert_eq!(signal.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(signal.trigger, "rsi_volume_anchor_bearish_divergence_short");
    }

    #[test]
    fn ratio_below_two_and_a_half_fails_the_current_gate() {
        let mut candles = candles();
        let latest_idx = candles.len() - 1;
        qualify_anchor(&mut candles, latest_idx - 20, 25.0);
        candles[latest_idx - 20].candle.low = 97.0;
        qualify_anchor(&mut candles, latest_idx, 28.0);
        candles[latest_idx].candle.volume = 24.999;
        candles[latest_idx].candle.low = 96.0;

        assert_eq!(
            v9_signal(&candles, candles.len()),
            Err("filtered_volume_v3_volume_not_confirmed")
        );
    }

    #[test]
    fn v9_identity_and_persisted_contract_are_research_only_and_auditable() {
        let args = market_filtered_volume_rsi_ema_macd_v9_research_args().unwrap();
        let detail = market_velocity_strategy_detail(&args);
        let risk = market_velocity_risk_config_detail(&args, 1.0);
        let manifest = market_velocity_paper_strategy_preset_manifest(
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET,
        )
        .unwrap();

        assert_eq!(args.entry_min_volume_ratio, FILTERED_VOLUME_V9_MIN_RATIO);
        assert_eq!(
            market_velocity_strategy_type(&args),
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_STRATEGY_KEY
        );
        assert_eq!(detail["paper_live_eligible"], false);
        assert_eq!(
            detail["entry_rsi_divergence_mode"],
            "nearest_directional_weekly_p90_filtered_volume_anchor"
        );
        assert_eq!(
            detail["entry_rsi_divergence_anchor_gate"]["right_confirmation_candles"],
            0
        );
        assert_eq!(
            risk["filtered_volume_target_tiers"][0]["min_ratio"],
            FILTERED_VOLUME_V9_MIN_RATIO
        );
        assert_eq!(
            manifest.strategy_key,
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_STRATEGY_KEY
        );
        assert_eq!(
            manifest.manifest_json["product"]["slug"],
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRODUCT_SLUG
        );
        assert_eq!(
            manifest.manifest_json["rule_version"],
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION
        );
        assert_eq!(
            manifest.manifest_json["parameters"]["fast_momentum_filters"]
                ["filtered_volume_rsi_ema_macd"]["volume_baseline"]
                ["historical_and_current_min_ratio"],
            FILTERED_VOLUME_V9_MIN_RATIO
        );
        assert_eq!(
            manifest.manifest_json["parameters"]["take_profit"]["tiers"][0]["min_volume_ratio"],
            FILTERED_VOLUME_V9_MIN_RATIO
        );
    }
}
