use super::{
    trade_direction_for_event, ConfirmedEvent, MarketVelocityEventBacktestArgs,
    MarketVelocityStopLossMode, MarketVelocityTradeDirection,
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
    let direction = trade_direction_for_event(&signal.event);
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
