//! PositionStage - 仓位管理阶段

use crate::framework::backtest::pipeline::{BacktestContext, BacktestStage, StageResult};
use crate::framework::backtest::signal::deal_signal;

/// 仓位管理阶段
///
/// 处理开仓/更新仓位
pub struct PositionStage;

impl PositionStage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PositionStage {
    fn default() -> Self {
        Self::new()
    }
}

impl BacktestStage for PositionStage {
    fn name(&self) -> &'static str {
        "PositionStage"
    }

    fn process(&mut self, ctx: &mut BacktestContext) -> StageResult {
        // 对齐 legacy engine.rs 的 should_process_signal 判断：
        // 仅当存在交易信号/持仓/挂单时才进入 deal_signal
        let has_position = ctx.trading_state.trade_position.is_some();
        let has_pending = ctx.trading_state.last_signal_result.is_some();
        let has_signal = ctx
            .signal
            .as_ref()
            .map(|s| s.should_buy || s.should_sell)
            .unwrap_or(false);
        if !(has_signal || has_position || has_pending) {
            return StageResult::Continue;
        }

        // 如果没有信号（None），构建一个空信号供 deal_signal 做风控/挂单处理
        let mut signal = ctx.signal.clone().unwrap_or_else(|| {
            use crate::strategy_common::SignalResult;
            let mut s = SignalResult::default();
            s.ts = ctx.candle.ts;
            s.open_price = ctx.candle.c;
            s
        });

        // 统一委托给 deal_signal 处理
        // deal_signal 处理了：
        // 1. 开仓 (Open Position)
        // 2. 平仓/反手 (Close/Reversal)
        // 3. 挂单触发 (Limit Orders / last_signal_result)
        // 4. 风控检查 (Risk Management via check_risk_config)
        // 5. 止盈止损更新 (Stop Loss / Take Profit updates)
        let trading_state = std::mem::take(&mut ctx.trading_state);
        ctx.trading_state = deal_signal(
            trading_state,
            &mut signal,
            &ctx.candle,
            ctx.risk_config,
            &[], // candle_item_list 未被使用
            ctx.candle_index,
        );

        // 更新 Context 中的状态以供后续 Stage 使用（虽然 RiskStage 主要依赖 context check，但保持状态同步是个好习惯）
        ctx.current_position = ctx.trading_state.trade_position.clone();

        StageResult::Continue
    }
}
