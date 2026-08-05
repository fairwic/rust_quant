use super::sell_climax_base_reclaim::evaluate as evaluate_sell_climax_base_reclaim;
use super::{blocked, SignalEvaluation, SignalState};
use crate::app::tradingview_velocity_parity::model::{
    Candle, Direction, IndicatorSeries, ParityRuleVersion,
};

/// 组合 V18+ 的 V11 主策略与 V17 箱体补充，并固定同棒由 V11 优先。
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate(
    state: &mut SignalState,
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    tick_size: f64,
    current_position: Option<Direction>,
    entries_enabled: bool,
    rule_version: ParityRuleVersion,
) -> SignalEvaluation {
    let range = state.range_squeeze_v15.evaluate(
        candles,
        indicators,
        index,
        tick_size,
        current_position,
        entries_enabled,
        rule_version,
    );
    state.reject_false_breakout_lower_wick =
        rule_version.rejects_false_breakout_short_on_long_lower_wick();
    state.enable_upthrust_failed_acceptance = rule_version.enables_upthrust_failed_acceptance();
    let mut main = state.evaluate(
        candles,
        indicators,
        index,
        tick_size,
        current_position,
        entries_enabled,
        ParityRuleVersion::CandidateV11,
    );
    state.reject_false_breakout_lower_wick = false;
    state.enable_upthrust_failed_acceptance = false;
    if let Some(stop_entry) = range.stop_entry {
        if main.intent.is_none() {
            main.stop_entry = Some(stop_entry);
        } else {
            main.blocked.push(blocked(
                stop_entry.intent.signal_time_ms,
                stop_entry.intent.direction,
                if rule_version.enables_upthrust_failed_acceptance() {
                    "V20_RANGE_SQUEEZE_SHADOWED_BY_V11_SIGNAL"
                } else if rule_version.is_v19_composite() {
                    "V19_RANGE_SQUEEZE_SHADOWED_BY_V11_SIGNAL"
                } else {
                    "V18_RANGE_SQUEEZE_SHADOWED_BY_V11_SIGNAL"
                },
            ));
        }
    }
    main.blocked.extend(range.blocked);
    if state.sell_climax_base_reclaim_variant.is_enabled()
        && entries_enabled
        && current_position != Some(Direction::Long)
        && main.intent.is_none()
        && main.stop_entry.is_none()
    {
        main.intent = evaluate_sell_climax_base_reclaim(candles, indicators, index, tick_size);
    }
    main
}
