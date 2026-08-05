use super::filtered_volume_rsi_ema_macd::{
    FILTERED_VOLUME_ATR_STOP_SOURCE, FILTERED_VOLUME_V3_ATR_STOP_SOURCE,
    FILTERED_VOLUME_V3_BEARISH_ENGULFING_STOP_SOURCE,
    FILTERED_VOLUME_V3_BULLISH_ENGULFING_STOP_SOURCE,
    FILTERED_VOLUME_V3_INVALID_AT_FILL_STOP_SOURCE, FILTERED_VOLUME_V3_LOWER_WICK_STOP_SOURCE,
    FILTERED_VOLUME_V3_UPPER_WICK_STOP_SOURCE,
};
use super::rsi_volume_regime::RSI_VOLUME_V3_ATR_STOP_SOURCE;
use super::{
    is_filtered_volume_weekly_base_version, ConfirmedEvent, MarketVelocityEventBacktestArgs,
    MarketVelocityStopLossMode, MarketVelocityTradeDirection,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_ENTRY_RULE_VERSION,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION,
    MARKET_RSI_VOLUME_REGIME_V3_ENTRY_RULE_VERSION, MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION,
    MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SelectedStopLossForSignal {
    pub(crate) price: f64,
    pub(crate) stop_loss_pct: f64,
    pub(crate) source: String,
}

/// 选择 paper/backtest 使用的止损价格，同时保留固定止损与结构止损来源。
pub(crate) fn select_stop_loss_for_confirmed_signal(
    signal: &ConfirmedEvent,
    args: &MarketVelocityEventBacktestArgs,
) -> SelectedStopLossForSignal {
    if is_filtered_volume_weekly_base_version(&args.paper_outcome_entry_rule_version)
        && signal.structure_stop_loss_source.as_deref()
            == Some(FILTERED_VOLUME_V3_INVALID_AT_FILL_STOP_SOURCE)
    {
        return SelectedStopLossForSignal {
            price: signal.entry_price,
            stop_loss_pct: 0.0,
            source: FILTERED_VOLUME_V3_INVALID_AT_FILL_STOP_SOURCE.to_string(),
        };
    }
    let direction = signal.direction;
    let fixed_price =
        stop_loss_price_for_direction(signal.entry_price, args.stop_loss_pct, direction);
    let fixed_source = fixed_stop_loss_source(args.stop_loss_pct);
    let structure = signal
        .structure_stop_loss_price
        .filter(|price| {
            price.is_finite()
                && *price > 0.0
                && is_loss_side_stop_price(signal.entry_price, *price, direction)
        })
        .zip(signal.structure_stop_loss_source.clone())
        .map(|(price, source)| {
            apply_structure_stop_min_pct_floor(
                signal.entry_price,
                price,
                source,
                args.structure_stop_min_pct,
                direction,
            )
        });
    // 这些研究版本的初始 R 已由信号时点 ATR14 冻结；若再比较固定百分比，会静默改写风险口径。
    if is_filtered_volume_weekly_base_version(&args.paper_outcome_entry_rule_version)
        || matches!(
            args.paper_outcome_entry_rule_version.as_str(),
            MARKET_RSI_VOLUME_REGIME_V3_ENTRY_RULE_VERSION
                | MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION
                | MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION
                | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_ENTRY_RULE_VERSION
                | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION
        )
    {
        if let Some((price, source)) = structure.as_ref().filter(|(_, source)| {
            source == RSI_VOLUME_V3_ATR_STOP_SOURCE
                || source == FILTERED_VOLUME_ATR_STOP_SOURCE
                || matches!(
                    source.as_str(),
                    FILTERED_VOLUME_V3_ATR_STOP_SOURCE
                        | FILTERED_VOLUME_V3_BULLISH_ENGULFING_STOP_SOURCE
                        | FILTERED_VOLUME_V3_LOWER_WICK_STOP_SOURCE
                        | FILTERED_VOLUME_V3_BEARISH_ENGULFING_STOP_SOURCE
                        | FILTERED_VOLUME_V3_UPPER_WICK_STOP_SOURCE
                )
        }) {
            return SelectedStopLossForSignal {
                price: *price,
                stop_loss_pct: (*price - signal.entry_price).abs() / signal.entry_price,
                source: source.clone(),
            };
        }
    }
    let (price, source) = match (args.stop_loss_mode, structure) {
        (
            MarketVelocityStopLossMode::StructureOrFixed,
            Some((structure_price, structure_source)),
        ) if should_use_structure_stop(
            signal.entry_price,
            structure_price,
            fixed_price,
            direction,
        ) =>
        {
            (structure_price, structure_source)
        }
        (
            MarketVelocityStopLossMode::StructureWithCap,
            Some((structure_price, structure_source)),
        ) => apply_structure_stop_max_pct_cap(
            signal.entry_price,
            structure_price,
            structure_source,
            args.stop_loss_pct,
            direction,
        ),
        _ => (fixed_price, fixed_source),
    };
    SelectedStopLossForSignal {
        price,
        stop_loss_pct: (price - signal.entry_price).abs() / signal.entry_price,
        source,
    }
}

fn stop_loss_price_for_direction(
    entry_price: f64,
    stop_loss_pct: f64,
    direction: MarketVelocityTradeDirection,
) -> f64 {
    match direction {
        MarketVelocityTradeDirection::Short => entry_price * (1.0 + stop_loss_pct),
        MarketVelocityTradeDirection::Long | MarketVelocityTradeDirection::Both => {
            entry_price * (1.0 - stop_loss_pct)
        }
    }
}

fn is_loss_side_stop_price(
    entry_price: f64,
    stop_price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Short => stop_price > entry_price,
        MarketVelocityTradeDirection::Long | MarketVelocityTradeDirection::Both => {
            stop_price < entry_price
        }
    }
}

fn should_use_structure_stop(
    entry_price: f64,
    structure_price: f64,
    fixed_price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Short => {
            structure_price > entry_price && structure_price < fixed_price
        }
        MarketVelocityTradeDirection::Long | MarketVelocityTradeDirection::Both => {
            structure_price < entry_price && structure_price > fixed_price
        }
    }
}

fn apply_structure_stop_min_pct_floor(
    entry_price: f64,
    structure_price: f64,
    structure_source: String,
    structure_stop_min_pct: f64,
    direction: MarketVelocityTradeDirection,
) -> (f64, String) {
    if structure_stop_min_pct <= 0.0 {
        return (structure_price, structure_source);
    }
    let floor_price = stop_loss_price_for_direction(entry_price, structure_stop_min_pct, direction);
    match direction {
        MarketVelocityTradeDirection::Short if structure_price < floor_price => {
            (floor_price, format!("{structure_source}+min_pct_floor"))
        }
        MarketVelocityTradeDirection::Long | MarketVelocityTradeDirection::Both
            if structure_price > floor_price =>
        {
            (floor_price, format!("{structure_source}+min_pct_floor"))
        }
        _ => (structure_price, structure_source),
    }
}

fn apply_structure_stop_max_pct_cap(
    entry_price: f64,
    structure_price: f64,
    structure_source: String,
    stop_loss_pct: f64,
    direction: MarketVelocityTradeDirection,
) -> (f64, String) {
    let cap_price = stop_loss_price_for_direction(entry_price, stop_loss_pct, direction);
    match direction {
        MarketVelocityTradeDirection::Short if structure_price > cap_price => {
            (cap_price, format!("{structure_source}+max_pct_cap"))
        }
        MarketVelocityTradeDirection::Long | MarketVelocityTradeDirection::Both
            if structure_price < cap_price =>
        {
            (cap_price, format!("{structure_source}+max_pct_cap"))
        }
        _ => (structure_price, structure_source),
    }
}

fn fixed_stop_loss_source(stop_loss_pct: f64) -> String {
    let basis_points = (stop_loss_pct * 10_000.0).round() as i64;
    let tag = format!("{basis_points:04}")
        .trim_end_matches('0')
        .to_string();
    format!("market_velocity_fixed_{tag}sl")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{
        RadarEvent, MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION,
    };

    #[test]
    fn v5_keeps_atr_stop_without_falling_back_to_fixed_three_percent() {
        let signal = ConfirmedEvent {
            event: RadarEvent {
                id: 1,
                exchange: "OKX".to_string(),
                symbol: "TEST-USDT-SWAP".to_string(),
                ts: 0,
                detected_at: "1970-01-01 08:00:00".to_string(),
                new_rank: 0,
                delta_rank: 0,
                current_price: 100.0,
                price_change_pct: 0.0,
            },
            direction: MarketVelocityTradeDirection::Long,
            entry_ts: 0,
            entry_price: 100.0,
            entry_idx: 0,
            trigger: "opposite_96_net_decline_volume_long".to_string(),
            structure_stop_loss_price: Some(95.0),
            structure_stop_loss_source: Some(RSI_VOLUME_V3_ATR_STOP_SOURCE.to_string()),
            entry_signal_evidence: None,
        };
        let args = MarketVelocityEventBacktestArgs {
            paper_outcome_entry_rule_version: MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION
                .to_string(),
            stop_loss_pct: 0.03,
            ..MarketVelocityEventBacktestArgs::default()
        };

        let selected = select_stop_loss_for_confirmed_signal(&signal, &args);

        assert_eq!(selected.price, 95.0);
        assert_eq!(selected.stop_loss_pct, 0.05);
        assert_eq!(selected.source, RSI_VOLUME_V3_ATR_STOP_SOURCE);
    }
}
