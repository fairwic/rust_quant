//! Pipeline阶段trait定义

use super::context::BacktestContext;

/// 阶段执行结果
#[derive(Debug, Clone)]
pub enum StageResult {
    /// 继续执行下一阶段
    Continue,
    /// 跳过后续阶段（本K线处理完成）
    Skip,
    /// 触发平仓出场
    Exit { price: f64, reason: String },
}

/// 回测Pipeline阶段trait
///
/// 所有阶段实现此trait，Pipeline按顺序执行各阶段
pub trait BacktestStage: Send + Sync {
    /// 阶段名称（用于调试和日志）
    fn name(&self) -> &'static str;

    /// 执行阶段逻辑
    ///
    /// # 参数
    /// - `ctx`: 回测上下文（可变）
    ///
    /// # 返回
    /// - `StageResult`: 阶段执行结果
    fn process(&mut self, ctx: &mut BacktestContext) -> StageResult;
}
