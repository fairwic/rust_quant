//! FilterStage - 信号过滤与Shadow Trading阶段

use crate::framework::backtest::pipeline::{BacktestContext, BacktestStage, StageResult};
use crate::framework::backtest::shadow_trading::ShadowTradeManager;

/// 信号过滤阶段
///
/// 处理被过滤的信号，创建Shadow Trade
pub struct FilterStage {
    shadow_manager: ShadowTradeManager,
}

impl FilterStage {
    pub fn new() -> Self {
        Self {
            shadow_manager: ShadowTradeManager::new(),
        }
    }

    /// 获取Shadow Trading管理器（用于最终收集filtered_signals）
    pub fn into_shadow_manager(self) -> ShadowTradeManager {
        self.shadow_manager
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
        self.shadow_manager.update_trades(&ctx.candle);

        // 如果有信号但被过滤，创建Shadow Trade
        if ctx.is_signal_filtered {
            if let Some(ref signal) = ctx.signal {
                self.shadow_manager
                    .process_filtered_signal(signal, &ctx.candle, &ctx.inst_id);
            }
            // 信号被过滤，跳过后续开仓逻辑（但仍需执行风控）
        }

        StageResult::Continue
    }
}
