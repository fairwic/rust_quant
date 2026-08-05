use super::super::{
    AnchorEntrySignalEvidence, ComputedCandle, MarketVelocityEventBacktestArgs,
    MarketVelocityTradeDirection,
};
use super::weekly_p90_anchor_rsi_divergence_v9::{
    signal as v9_signal, BEARISH_ANCHOR_TRIGGER, BULLISH_ANCHOR_TRIGGER,
    FILTERED_VOLUME_V9_MIN_RATIO, RSI_DIVERGENCE_COMPARISON_MODE as V9_COMPARISON_MODE,
};
use super::{
    FilteredVolumeRsiEmaMacdSignal, DOJI_MAX_BODY_RANGE_RATIO, REVERSAL_WICK_MIN_RANGE_RATIO,
};

/// v11 逐笔证据的稳定模式名，说明 q/p 仍是 v9 锚点，而成交不再等待收盘确认。
const RSI_DIVERGENCE_COMPARISON_MODE: &str =
    "weekly_p90_filtered_volume_anchors_wick_or_next_touch";
/// 底背离 p 为方向性长下影线时，下一根开盘直接做多。
pub(super) const BULLISH_WICK_TRIGGER: &str =
    "rsi_volume_anchor_bullish_divergence_lower_wick_next_open_long";
/// 顶背离 p 为方向性长上影线时，下一根开盘直接做空。
pub(super) const BEARISH_WICK_TRIGGER: &str =
    "rsi_volume_anchor_bearish_divergence_upper_wick_next_open_short";
/// 非方向性长影线底背离只在紧邻下一根盘中越过 p.high 时做多。
pub(super) const BULLISH_TOUCH_TRIGGER: &str =
    "rsi_volume_anchor_bullish_divergence_next_high_touch_long";
/// 非方向性长影线顶背离只在紧邻下一根盘中跌破 p.low 时做空。
pub(super) const BEARISH_TOUCH_TRIGGER: &str =
    "rsi_volume_anchor_bearish_divergence_next_low_touch_short";

/// 在 p 完成时冻结 v9 的 q/p 背离和触发线，不读取尚未完成的下一根 K 线。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    if (args.entry_min_volume_ratio - FILTERED_VOLUME_V9_MIN_RATIO).abs() > f64::EPSILON {
        return Err("filtered_volume_v11_ratio_policy_mismatch");
    }
    let pivot_idx = completed_count
        .checked_sub(1)
        .ok_or("filtered_volume_v11_anchor_setup_not_found")?;
    let pivot = candles
        .get(pivot_idx)
        .ok_or("filtered_volume_v11_anchor_setup_not_found")?;
    let mut setup = v9_signal(candles, completed_count, args)
        .map_err(|_| "filtered_volume_v11_anchor_setup_not_found")?;
    let anchor_trigger = match setup.direction {
        MarketVelocityTradeDirection::Long => BULLISH_ANCHOR_TRIGGER,
        MarketVelocityTradeDirection::Short => BEARISH_ANCHOR_TRIGGER,
        MarketVelocityTradeDirection::Both => {
            return Err("filtered_volume_v11_anchor_setup_not_found");
        }
    };
    if !setup
        .trigger
        .split('+')
        .any(|trigger| trigger == anchor_trigger)
    {
        return Err("filtered_volume_v11_anchor_setup_not_found");
    }

    let range = pivot.candle.high - pivot.candle.low;
    if !range.is_finite() || range <= 0.0 {
        return Err("filtered_volume_v11_pivot_range_invalid");
    }
    let body = (pivot.candle.close - pivot.candle.open).abs();
    let upper_wick = pivot.candle.high - pivot.candle.open.max(pivot.candle.close);
    let lower_wick = pivot.candle.open.min(pivot.candle.close) - pivot.candle.low;
    let body_ratio = body / range;
    let upper_wick_ratio = upper_wick.max(0.0) / range;
    let lower_wick_ratio = lower_wick.max(0.0) / range;
    let is_not_doji = body_ratio > DOJI_MAX_BODY_RANGE_RATIO;

    let (activation_price, directional_wick_ratio, opposite_wick_ratio, direct_wick_entry, trigger) =
        match setup.direction {
            MarketVelocityTradeDirection::Long => {
                let direct = is_not_doji
                    && lower_wick_ratio >= REVERSAL_WICK_MIN_RANGE_RATIO
                    && lower_wick_ratio > upper_wick_ratio;
                (
                    pivot.candle.high,
                    lower_wick_ratio,
                    upper_wick_ratio,
                    direct,
                    if direct {
                        BULLISH_WICK_TRIGGER
                    } else {
                        BULLISH_TOUCH_TRIGGER
                    },
                )
            }
            MarketVelocityTradeDirection::Short => {
                let direct = is_not_doji
                    && upper_wick_ratio >= REVERSAL_WICK_MIN_RANGE_RATIO
                    && upper_wick_ratio > lower_wick_ratio;
                (
                    pivot.candle.low,
                    upper_wick_ratio,
                    lower_wick_ratio,
                    direct,
                    if direct {
                        BEARISH_WICK_TRIGGER
                    } else {
                        BEARISH_TOUCH_TRIGGER
                    },
                )
            }
            MarketVelocityTradeDirection::Both => unreachable!(),
        };
    if !activation_price.is_finite() || activation_price <= 0.0 {
        return Err("filtered_volume_v11_activation_price_invalid");
    }

    let divergence = setup
        .evidence
        .rsi_divergences
        .iter_mut()
        .find(|evidence| {
            evidence.comparison_mode == V9_COMPARISON_MODE
                && evidence.direction == setup.direction
                && evidence.pivot_ts_ms == pivot.candle.ts
        })
        .ok_or("filtered_volume_v11_anchor_evidence_missing")?;
    divergence.comparison_mode = RSI_DIVERGENCE_COMPARISON_MODE;
    setup.evidence.anchor_entry = Some(AnchorEntrySignalEvidence {
        activation_mode: if direct_wick_entry {
            "pivot_directional_wick_next_open"
        } else {
            "next_candle_intrabar_break"
        },
        pivot_body_range_ratio: body_ratio,
        pivot_directional_wick_range_ratio: directional_wick_ratio,
        pivot_opposite_wick_range_ratio: opposite_wick_ratio,
        activation_price,
        activation_candle_ts_ms: None,
        fill_price: None,
        fill_price_source: None,
        intrabar_path_policy: (!direct_wick_entry)
            .then_some("full_15m_bar_conservative_stop_first"),
    });
    setup.trigger = setup.trigger.replacen(anchor_trigger, trigger, 1);
    Ok(setup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::immediate_entry::immediate_entry_from_signal;
    use crate::app::market_velocity_event_backtest::{
        completed_candle_entry_signal, market_filtered_volume_rsi_ema_macd_v11_research_args,
        market_velocity_paper_strategy_preset_manifest, market_velocity_strategy_detail,
        market_velocity_strategy_type, BacktestCandle, RadarEvent,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRODUCT_SLUG,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_STRATEGY_KEY, MS_15M,
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

    fn long_setup(directional_lower_wick: bool) -> Vec<ComputedCandle> {
        let mut candles = (0..751).map(candle).collect::<Vec<_>>();
        let pivot_idx = 749;
        let reference_idx = pivot_idx - 24;
        qualify_anchor(&mut candles, reference_idx, 25.0);
        candles[reference_idx].candle.low = 97.0;
        qualify_anchor(&mut candles, pivot_idx, 28.0);
        candles[pivot_idx].candle.high = 101.5;
        candles[pivot_idx].candle.low = 96.0;
        if directional_lower_wick {
            candles[pivot_idx].candle.open = 100.0;
            candles[pivot_idx].candle.close = 100.8;
        }
        candles
    }

    fn v11_args() -> MarketVelocityEventBacktestArgs {
        let args = market_filtered_volume_rsi_ema_macd_v11_research_args().unwrap();
        assert_eq!(
            args.paper_outcome_entry_rule_version,
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION
        );
        args
    }

    fn entry_event(candles: &[ComputedCandle]) -> RadarEvent {
        RadarEvent {
            id: 11,
            exchange: "okx".to_string(),
            symbol: "TEST-USDT-SWAP".to_string(),
            ts: 750_i64 * MS_15M,
            detected_at: "2026-07-23T00:00:00Z".to_string(),
            new_rank: 0,
            delta_rank: 0,
            current_price: candles[749].candle.close,
            price_change_pct: 0.0,
        }
    }

    #[test]
    fn directional_lower_wick_enters_at_immediate_next_open_without_future_confirmation() {
        let mut candles = long_setup(true);
        candles[750].candle.open = 102.0;
        candles[750].candle.high = 103.0;
        candles[750].candle.low = 99.0;
        let args = v11_args();
        let before = signal(&candles, 750, &args).unwrap();
        candles[750].candle.close = 1.0;
        let after = signal(&candles, 750, &args).unwrap();
        assert_eq!(before, after);
        assert_eq!(before.trigger, BULLISH_WICK_TRIGGER);

        let confirmed = immediate_entry_from_signal(
            &entry_event(&candles),
            &candles,
            before.direction,
            before.trigger,
            &args,
        )
        .unwrap();
        let anchor = confirmed
            .entry_signal_evidence
            .as_ref()
            .and_then(|evidence| evidence.anchor_entry.as_ref())
            .unwrap();

        assert_eq!(confirmed.entry_idx, 750);
        assert_eq!(confirmed.entry_price, 102.0);
        assert_eq!(anchor.activation_mode, "pivot_directional_wick_next_open");
        assert_eq!(
            anchor.fill_price_source,
            Some("next_open_after_directional_wick")
        );
    }

    #[test]
    fn doji_lower_wick_uses_next_candle_high_touch_price_instead_of_close() {
        let mut candles = long_setup(false);
        candles[750].candle.open = 100.5;
        candles[750].candle.high = 101.6;
        candles[750].candle.low = 99.0;
        candles[750].candle.close = 100.6;
        let args = v11_args();
        let setup = completed_candle_entry_signal(&candles, 750, &args).unwrap();
        assert_eq!(setup.trigger, BULLISH_TOUCH_TRIGGER);

        let confirmed = immediate_entry_from_signal(
            &entry_event(&candles),
            &candles,
            setup.direction,
            setup.trigger.clone(),
            &args,
        )
        .unwrap();
        let anchor = confirmed
            .entry_signal_evidence
            .as_ref()
            .and_then(|evidence| evidence.anchor_entry.as_ref())
            .unwrap();

        assert_eq!(confirmed.entry_price, 101.5);
        assert_eq!(anchor.activation_mode, "next_candle_intrabar_break");
        assert_eq!(anchor.fill_price_source, Some("intrabar_activation_price"));
        assert_eq!(
            anchor.intrabar_path_policy,
            Some("full_15m_bar_conservative_stop_first")
        );
    }

    #[test]
    fn non_wick_setup_expires_when_immediate_next_candle_does_not_strictly_break() {
        let mut candles = long_setup(false);
        candles[750].candle.open = 100.5;
        candles[750].candle.high = 101.5;
        candles[750].candle.low = 99.0;
        let args = v11_args();
        let setup = completed_candle_entry_signal(&candles, 750, &args).unwrap();

        assert_eq!(
            immediate_entry_from_signal(
                &entry_event(&candles),
                &candles,
                setup.direction,
                setup.trigger,
                &args,
            ),
            Err("filtered_volume_v11_next_candle_activation_not_touched".to_string())
        );
    }

    #[test]
    fn non_wick_gap_through_activation_fills_at_next_open() {
        let mut candles = long_setup(false);
        candles[750].candle.open = 102.0;
        candles[750].candle.high = 103.0;
        candles[750].candle.low = 100.0;
        let args = v11_args();
        let setup = completed_candle_entry_signal(&candles, 750, &args).unwrap();

        let confirmed = immediate_entry_from_signal(
            &entry_event(&candles),
            &candles,
            setup.direction,
            setup.trigger.clone(),
            &args,
        )
        .unwrap();
        let anchor = confirmed
            .entry_signal_evidence
            .as_ref()
            .and_then(|evidence| evidence.anchor_entry.as_ref())
            .unwrap();

        assert_eq!(confirmed.entry_price, 102.0);
        assert_eq!(
            anchor.fill_price_source,
            Some("next_open_gap_through_activation")
        );
    }

    #[test]
    fn directional_upper_wick_is_the_exact_short_mirror() {
        let mut candles = (0..751).map(candle).collect::<Vec<_>>();
        let pivot_idx = 749;
        let reference_idx = pivot_idx - 20;
        qualify_anchor(&mut candles, reference_idx, 75.0);
        candles[reference_idx].candle.high = 103.0;
        qualify_anchor(&mut candles, pivot_idx, 72.0);
        candles[pivot_idx].candle.open = 100.0;
        candles[pivot_idx].candle.close = 99.2;
        candles[pivot_idx].candle.high = 104.0;
        candles[pivot_idx].candle.low = 98.5;
        candles[750].candle.open = 98.0;
        let args = v11_args();
        let setup = completed_candle_entry_signal(&candles, 750, &args).unwrap();

        let confirmed = immediate_entry_from_signal(
            &entry_event(&candles),
            &candles,
            setup.direction,
            setup.trigger.clone(),
            &args,
        )
        .unwrap();

        assert_eq!(setup.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(setup.trigger, BEARISH_WICK_TRIGGER);
        assert_eq!(confirmed.entry_price, 98.0);
    }

    #[test]
    fn v11_identity_and_timing_contract_are_research_only_and_auditable() {
        let args = v11_args();
        let detail = market_velocity_strategy_detail(&args);
        let manifest = market_velocity_paper_strategy_preset_manifest(
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET,
        )
        .unwrap();

        assert_eq!(
            market_velocity_strategy_type(&args),
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_STRATEGY_KEY
        );
        assert_eq!(detail["paper_live_eligible"], false);
        assert_eq!(
            detail["entry_fill_mode"],
            "pivot_directional_wick_next_open_else_immediate_next_candle_intrabar_break"
        );
        assert_eq!(
            detail["entry_rsi_divergence_anchor_gate"]["setup_expiry_candles"],
            1
        );
        assert_eq!(
            manifest.manifest_json["product"]["slug"],
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRODUCT_SLUG
        );
        assert_eq!(
            manifest.manifest_json["parameters"]["entry_fill"],
            "pivot_directional_wick_next_open_else_immediate_next_candle_intrabar_break"
        );
        assert_eq!(manifest.channel, "research");
    }
}
