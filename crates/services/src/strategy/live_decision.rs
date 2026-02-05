use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::{
    deal_signal, BasicRiskStrategyConfig, SignalResult, TradingState,
};
use rust_quant_strategies::framework::types::TradeSide;

#[derive(Debug, Clone)]
pub struct LiveDecisionOutcome {
    pub opened_side: Option<TradeSide>,
    pub closed: bool,
    pub closed_side: Option<TradeSide>,
}

pub fn apply_live_decision(
    state: &mut TradingState,
    signal: &mut SignalResult,
    candle: &CandleItem,
    risk: BasicRiskStrategyConfig,
) -> LiveDecisionOutcome {
    let before = state.trade_position.clone();
    let before_side = before.as_ref().map(|p| p.trade_side);

    let updated = deal_signal(state.clone(), signal, candle, risk, &[], 0);
    let after = updated.trade_position.clone();
    let after_side = after.as_ref().map(|p| p.trade_side);

    *state = updated;

    let opened_side = match (&before, &after) {
        (None, Some(pos)) => Some(pos.trade_side),
        (Some(prev), Some(curr)) if prev.trade_side != curr.trade_side => Some(curr.trade_side),
        _ => None,
    };

    let closed_side = match (&before, &after) {
        (Some(prev), None) => Some(prev.trade_side),
        (Some(prev), Some(curr)) if prev.trade_side != curr.trade_side => Some(prev.trade_side),
        _ => None,
    };

    LiveDecisionOutcome {
        opened_side,
        closed: before.is_some() && (after.is_none() || opened_side.is_some()),
        closed_side,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_open_when_filter_reason_present() {
        let mut state = TradingState::default();
        let mut signal = SignalResult {
            should_buy: true,
            should_sell: false,
            open_price: 100.0,
            filter_reasons: vec!["FIB_STRICT_MAJOR_BEAR_BLOCK_LONG".to_string()],
            ..SignalResult::default()
        };
        let candle = CandleItem {
            o: 100.0,
            h: 101.0,
            l: 99.0,
            c: 100.0,
            v: 1.0,
            ts: 1,
            confirm: 1,
        };
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            ..Default::default()
        };

        let outcome = apply_live_decision(&mut state, &mut signal, &candle, risk);

        assert!(outcome.opened_side.is_none());
        assert!(state.trade_position.is_none());
    }

    #[test]
    fn stop_loss_triggers_close() {
        let mut state = TradingState::default();
        state.trade_position = Some(rust_quant_strategies::framework::backtest::TradePosition {
            trade_side: TradeSide::Long,
            open_price: 100.0,
            position_nums: 1.0,
            signal_high_low_diff: 1.0,
            ..Default::default()
        });

        let mut signal = SignalResult {
            should_buy: false,
            should_sell: false,
            open_price: 100.0,
            ..SignalResult::default()
        };
        let candle = CandleItem {
            o: 100.0,
            h: 101.0,
            l: 97.0,
            c: 98.0,
            v: 1.0,
            ts: 2,
            confirm: 1,
        };
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            ..Default::default()
        };

        let outcome = apply_live_decision(&mut state, &mut signal, &candle, risk);

        assert!(outcome.closed);
        assert!(state.trade_position.is_none());
    }
}
