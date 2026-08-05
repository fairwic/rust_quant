use super::super::{ComputedCandle, MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection};
use super::weekly_p90_anchor_rsi_divergence_v9::{
    signal as v9_signal, BEARISH_ANCHOR_TRIGGER, BULLISH_ANCHOR_TRIGGER,
    FILTERED_VOLUME_V9_MIN_RATIO, RSI_DIVERGENCE_COMPARISON_MODE as V9_COMPARISON_MODE,
};
use super::FilteredVolumeRsiEmaMacdSignal;

/// v10 逐笔证据的稳定模式名，明确确认 K 不属于 q/p 背离比较对。
const RSI_DIVERGENCE_COMPARISON_MODE: &str =
    "weekly_p90_filtered_volume_anchors_next_close_confirmed";
/// v10 底背离经下一根收盘确认后的稳定触发标签。
const BULLISH_CONFIRMED_TRIGGER: &str =
    "rsi_volume_anchor_bullish_divergence_next_close_confirmed_long";
/// v10 顶背离经下一根收盘确认后的稳定触发标签。
const BEARISH_CONFIRMED_TRIGGER: &str =
    "rsi_volume_anchor_bearish_divergence_next_close_confirmed_short";

/// 只确认紧邻上一根 p 的 v9 锚点背离；未在这一根突破即过期，不向后扫描补造入场。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    if (args.entry_min_volume_ratio - FILTERED_VOLUME_V9_MIN_RATIO).abs() > f64::EPSILON {
        return Err("filtered_volume_v10_ratio_policy_mismatch");
    }
    let confirmation_idx = completed_count
        .checked_sub(1)
        .ok_or("filtered_volume_v10_confirmation_not_ready")?;
    let setup_completed_count = confirmation_idx;
    let pivot_idx = setup_completed_count
        .checked_sub(1)
        .ok_or("filtered_volume_v10_confirmation_not_ready")?;
    let confirmation = candles
        .get(confirmation_idx)
        .ok_or("filtered_volume_v10_confirmation_not_ready")?;
    let pivot = candles
        .get(pivot_idx)
        .ok_or("filtered_volume_v10_confirmation_not_ready")?;

    // 复用 v9 在 p 完成时冻结的 q/p、量比、周 P90、ATR 与分层目标证据。
    // completed_count 故意少一根，保证确认 K 不会反向参与 p 的指标与成交量计算。
    let mut setup = v9_signal(candles, setup_completed_count, args)
        .map_err(|_| "filtered_volume_v10_anchor_setup_not_found")?;
    let anchor_trigger = match setup.direction {
        MarketVelocityTradeDirection::Long => BULLISH_ANCHOR_TRIGGER,
        MarketVelocityTradeDirection::Short => BEARISH_ANCHOR_TRIGGER,
        MarketVelocityTradeDirection::Both => {
            return Err("filtered_volume_v10_anchor_setup_not_found");
        }
    };
    if !setup
        .trigger
        .split('+')
        .any(|trigger| trigger == anchor_trigger)
    {
        return Err("filtered_volume_v10_anchor_setup_not_found");
    }

    let (break_price, confirmed_trigger, confirmed) = match setup.direction {
        MarketVelocityTradeDirection::Long => (
            pivot.candle.high,
            BULLISH_CONFIRMED_TRIGGER,
            confirmation.candle.close > pivot.candle.high,
        ),
        MarketVelocityTradeDirection::Short => (
            pivot.candle.low,
            BEARISH_CONFIRMED_TRIGGER,
            confirmation.candle.close < pivot.candle.low,
        ),
        MarketVelocityTradeDirection::Both => unreachable!(),
    };
    if !confirmation.candle.close.is_finite() || !break_price.is_finite() || !confirmed {
        return Err("filtered_volume_v10_next_close_not_confirmed");
    }

    let evidence = setup
        .evidence
        .rsi_divergences
        .iter_mut()
        .find(|evidence| {
            evidence.comparison_mode == V9_COMPARISON_MODE
                && evidence.direction == setup.direction
                && evidence.pivot_ts_ms == pivot.candle.ts
        })
        .ok_or("filtered_volume_v10_anchor_evidence_missing")?;
    evidence.comparison_mode = RSI_DIVERGENCE_COMPARISON_MODE;
    evidence.confirmation_ts_ms = Some(confirmation.candle.ts);
    evidence.confirmation_close = Some(confirmation.candle.close);
    evidence.confirmation_break_price = Some(break_price);
    setup.trigger = setup.trigger.replacen(anchor_trigger, confirmed_trigger, 1);
    Ok(setup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::immediate_entry::immediate_entry_from_signal;
    use crate::app::market_velocity_event_backtest::{
        completed_candle_entry_signal, market_filtered_volume_rsi_ema_macd_v10_research_args,
        market_velocity_paper_strategy_preset_manifest, market_velocity_risk_config_detail,
        market_velocity_strategy_detail, market_velocity_strategy_type, BacktestCandle, RadarEvent,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRODUCT_SLUG,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_STRATEGY_KEY, MS_15M,
    };

    fn candle(idx: usize) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: idx as i64 * MS_15M,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
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

    fn qualify_anchor(candles: &mut [ComputedCandle], idx: usize, rsi14: f64) {
        candles[idx].candle.volume = 25.0;
        candles[idx].volume_ccy = Some(200.0);
        candles[idx].rsi14 = Some(rsi14);
    }

    fn long_setup(confirmation_close: f64) -> Vec<ComputedCandle> {
        let mut candles = (0..751).map(candle).collect::<Vec<_>>();
        let pivot_idx = 749;
        let reference_idx = pivot_idx - 24;
        qualify_anchor(&mut candles, reference_idx, 25.0);
        candles[reference_idx].candle.low = 97.0;
        qualify_anchor(&mut candles, pivot_idx, 28.0);
        candles[pivot_idx].candle.high = 101.5;
        candles[pivot_idx].candle.low = 96.0;
        candles[750].candle.close = confirmation_close;
        candles
    }

    fn v10_signal(
        candles: &[ComputedCandle],
        completed_count: usize,
    ) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
        let args = market_filtered_volume_rsi_ema_macd_v10_research_args().unwrap();
        assert_eq!(
            args.paper_outcome_entry_rule_version,
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION
        );
        signal(candles, completed_count, &args)
    }

    #[test]
    fn bullish_anchor_requires_the_immediate_next_close_above_pivot_high() {
        let candles = long_setup(101.6);

        let confirmed = v10_signal(&candles, candles.len()).unwrap();
        let evidence = &confirmed.evidence.rsi_divergences[0];

        assert_eq!(confirmed.direction, MarketVelocityTradeDirection::Long);
        assert!(confirmed.trigger.contains(BULLISH_CONFIRMED_TRIGGER));
        assert_eq!(evidence.pivot_ts_ms, candles[749].candle.ts);
        assert_eq!(evidence.confirmation_ts_ms, Some(candles[750].candle.ts));
        assert_eq!(evidence.confirmation_close, Some(101.6));
        assert_eq!(evidence.confirmation_break_price, Some(101.5));
    }

    #[test]
    fn bullish_anchor_expires_when_the_immediate_next_close_does_not_break() {
        let candles = long_setup(101.5);

        assert_eq!(
            v10_signal(&candles, candles.len()),
            Err("filtered_volume_v10_next_close_not_confirmed")
        );
    }

    #[test]
    fn bearish_anchor_confirmation_is_the_exact_mirror() {
        let mut candles = (0..751).map(candle).collect::<Vec<_>>();
        let pivot_idx = 749;
        let reference_idx = pivot_idx - 20;
        qualify_anchor(&mut candles, reference_idx, 75.0);
        candles[reference_idx].candle.high = 103.0;
        qualify_anchor(&mut candles, pivot_idx, 72.0);
        candles[pivot_idx].candle.high = 104.0;
        candles[pivot_idx].candle.low = 98.5;
        candles[750].candle.close = 98.4;

        let confirmed = v10_signal(&candles, candles.len()).unwrap();
        let evidence = &confirmed.evidence.rsi_divergences[0];

        assert_eq!(confirmed.direction, MarketVelocityTradeDirection::Short);
        assert!(confirmed.trigger.contains(BEARISH_CONFIRMED_TRIGGER));
        assert_eq!(evidence.confirmation_break_price, Some(98.5));
    }

    #[test]
    fn pivot_completion_alone_cannot_emit_v10_and_future_data_is_not_read() {
        let mut candles = long_setup(101.6);
        assert_eq!(
            v10_signal(&candles, 750),
            Err("filtered_volume_v10_anchor_setup_not_found")
        );

        let before = v10_signal(&candles, 751).unwrap();
        candles.push(candle(751));
        candles[751].candle.close = 1.0;
        let after = v10_signal(&candles, 751).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn confirmed_setup_fills_only_at_the_open_after_confirmation() {
        let mut candles = long_setup(101.6);
        candles.push(candle(751));
        candles[751].candle.open = 103.0;
        let args = market_filtered_volume_rsi_ema_macd_v10_research_args().unwrap();
        let strategy_signal = completed_candle_entry_signal(&candles, 751, &args).unwrap();
        let event = RadarEvent {
            id: 10,
            exchange: "okx".to_string(),
            symbol: "TEST-USDT-SWAP".to_string(),
            ts: 751_i64 * MS_15M,
            detected_at: "2026-07-23T00:00:00Z".to_string(),
            new_rank: 0,
            delta_rank: 0,
            current_price: candles[750].candle.close,
            price_change_pct: 0.0,
        };

        let confirmed = immediate_entry_from_signal(
            &event,
            &candles,
            strategy_signal.direction,
            strategy_signal.trigger,
            &args,
        )
        .unwrap();

        assert_eq!(confirmed.entry_idx, 751);
        assert_eq!(confirmed.entry_price, 103.0);
        assert_eq!(confirmed.entry_ts, event.ts);
        assert_eq!(
            confirmed
                .entry_signal_evidence
                .as_ref()
                .unwrap()
                .rsi_divergences[0]
                .confirmation_ts_ms,
            Some(candles[750].candle.ts)
        );
    }

    #[test]
    fn v10_identity_and_timing_contract_are_research_only_and_auditable() {
        let args = market_filtered_volume_rsi_ema_macd_v10_research_args().unwrap();
        let detail = market_velocity_strategy_detail(&args);
        let risk = market_velocity_risk_config_detail(&args, 1.0);
        let manifest = market_velocity_paper_strategy_preset_manifest(
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET,
        )
        .unwrap();

        assert_eq!(
            market_velocity_strategy_type(&args),
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_STRATEGY_KEY
        );
        assert_eq!(detail["paper_live_eligible"], false);
        assert_eq!(
            detail["entry_fill_mode"],
            "next_open_after_one_completed_confirmation_candle"
        );
        assert_eq!(
            detail["entry_rsi_divergence_anchor_gate"]["entry_confirmation_candles"],
            1
        );
        assert_eq!(
            risk["filtered_volume_target_tiers"][0]["min_ratio"],
            FILTERED_VOLUME_V9_MIN_RATIO
        );
        assert_eq!(
            manifest.strategy_key,
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_STRATEGY_KEY
        );
        assert_eq!(
            manifest.manifest_json["product"]["slug"],
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRODUCT_SLUG
        );
        assert_eq!(
            manifest.manifest_json["parameters"]["entry_fill"],
            "next_open_after_one_completed_confirmation_candle"
        );
        assert_eq!(manifest.channel, "research");
    }
}
