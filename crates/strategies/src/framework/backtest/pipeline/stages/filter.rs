//! FilterStage - 信号过滤与Shadow Trading阶段

use crate::framework::backtest::pipeline::{BacktestContext, BacktestStage, StageResult};

/// 信号过滤阶段
///
/// 处理被过滤的信号，创建Shadow Trade
pub struct FilterStage;

impl FilterStage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FilterStage {
    fn default() -> Self {
        Self::new()
    }
}

impl BacktestStage for FilterStage {
    fn name(&self) -> &'static str {
        "FilterStage"
    }

    fn process(&mut self, ctx: &mut BacktestContext) -> StageResult {
        // 更新现有Shadow Trade
        ctx.shadow_manager.update_trades(&ctx.candle);

        // 如果有信号但被过滤，创建Shadow Trade
        if ctx.is_signal_filtered {
            if let Some(ref signal) = ctx.signal {
                ctx.shadow_manager
                    .process_filtered_signal(signal, &ctx.candle, &ctx.inst_id);
            }
            // 信号被过滤，跳过后续开仓逻辑（但仍需执行风控）
        }

        StageResult::Continue
    }
}
