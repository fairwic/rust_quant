//! PositionStage - 仓位管理阶段

use crate::framework::backtest::pipeline::{BacktestContext, BacktestStage, StageResult};
use crate::framework::backtest::position::{open_long_position, open_short_position};
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
        // 构建 SignalResult
        // 如果 PipeLine 上游没有生成信号（ctx.signal 为 None），我们需要构建一个空的 SignalResult
        // 以便 deal_signal 可以处理挂单（Pending Orders）和持仓风控（Risk Checks）
        let mut signal = ctx.signal.clone().unwrap_or_else(|| {
            use crate::strategy_common::SignalResult;
            let mut s = SignalResult::default();
            // 必须设置时间戳，否则 deal_signal 可能无法正确判断时间
            s.ts = ctx.candle.ts;
            s
        });

        // 统一委托给 deal_signal 处理
        // deal_signal 处理了：
        // 1. 开仓 (Open Position)
        // 2. 平仓/反手 (Close/Reversal)
        // 3. 挂单触发 (Limit Orders / last_signal_result)
        // 4. 风控检查 (Risk Management via check_risk_config)
        // 5. 止盈止损更新 (Stop Loss / Take Profit updates)
        ctx.trading_state = deal_signal(
            ctx.trading_state.clone(),
            &mut signal,
            &ctx.candle,
            ctx.risk_config.clone(),
            &[], // candle_item_list 未被使用
            ctx.candle_index,
        );

        // 更新 Context 中的状态以供后续 Stage 使用（虽然 RiskStage 主要依赖 context check，但保持状态同步是个好习惯）
        ctx.current_position = ctx.trading_state.trade_position.clone();

        StageResult::Continue
    }
}
