//! RiskStage - 风控检查阶段

use crate::framework::backtest::pipeline::{BacktestContext, BacktestStage, StageResult};
use crate::framework::backtest::risk::check_risk_config;
use crate::framework::backtest::types::SignalResult;
use rust_quant_trading::audit::RiskDecision;

/// 风控检查阶段
///
/// 执行止盈止损检查
pub struct RiskStage;

impl RiskStage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RiskStage {
    fn default() -> Self {
        Self::new()
    }
}

impl BacktestStage for RiskStage {
    fn name(&self) -> &'static str {
        "RiskStage"
    }

    fn process(&mut self, ctx: &mut BacktestContext) -> StageResult {
        // 没有持仓则跳过
        if !ctx.has_position() {
            return StageResult::Continue;
        }

        // 获取信号（用于风控参考）
        let signal = ctx.signal.clone().unwrap_or_default();

        // 执行风控检查
        let prev_position = ctx.trading_state.trade_position.is_some();
        ctx.trading_state = check_risk_config(
            &ctx.risk_config,
            ctx.trading_state.clone(),
            &signal,
            &ctx.candle,
        );

        // 检查是否平仓
        let curr_position = ctx.trading_state.trade_position.is_some();
        if prev_position && !curr_position {
            ctx.closed_position = true;
            ctx.current_position = None;
        } else {
            ctx.current_position = ctx.trading_state.trade_position.clone();
        }

        let decision = if !prev_position {
            "SKIP"
        } else if curr_position {
            "HOLD"
        } else {
            "CLOSE"
        };
        ctx.audit_trail.record_risk_decision(RiskDecision {
            ts: ctx.candle.ts,
            decision: decision.to_string(),
            reason: ctx.close_reason.clone(),
            risk_json: None,
        });

        StageResult::Continue
    }
}
