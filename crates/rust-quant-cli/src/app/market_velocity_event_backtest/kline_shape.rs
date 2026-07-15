use super::{ComputedCandle, MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection};

/// 返回 direct-Kline 动量形态过滤的首个阻塞原因；所有阈值为空时不改变原入场语义。
pub(super) fn direct_kline_momentum_shape_filter_reason(
    latest: &ComputedCandle,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<&'static str> {
    let range = latest.candle.high - latest.candle.low;
    if args.entry_min_body_ratio_pct.is_some()
        || args.entry_min_close_position_pct.is_some()
        || args.entry_min_range_expansion_ratio.is_some()
    {
        if !valid_positive(range) {
            return Some("kline_range_not_ready");
        }
    }
    if let Some(min_body_ratio_pct) = args.entry_min_body_ratio_pct {
        let body_ratio_pct = (latest.candle.close - latest.candle.open).abs() / range * 100.0;
        if body_ratio_pct < min_body_ratio_pct {
            return Some("body_ratio_not_confirmed");
        }
    }
    if let Some(min_close_position_pct) = args.entry_min_close_position_pct {
        let close_position_pct = match direction {
            MarketVelocityTradeDirection::Long => {
                (latest.candle.close - latest.candle.low) / range * 100.0
            }
            MarketVelocityTradeDirection::Short => {
                (latest.candle.high - latest.candle.close) / range * 100.0
            }
            MarketVelocityTradeDirection::Both => return Some("invalid_trade_direction"),
        };
        if close_position_pct < min_close_position_pct {
            return Some("close_position_not_confirmed");
        }
    }
    if let Some(min_range_expansion_ratio) = args.entry_min_range_expansion_ratio {
        let Some(previous_range_avg) = latest.previous_range_avg.filter(|avg| valid_positive(*avg))
        else {
            return Some("range_expansion_not_ready");
        };
        if range / previous_range_avg < min_range_expansion_ratio {
            return Some("range_expansion_not_confirmed");
        }
    }
    None
}

fn valid_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
