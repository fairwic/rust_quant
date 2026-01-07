// 具体策略实现模块

// 通用执行器和辅助模块
// ✅ executor_common 已使用 trait 解耦，不再有循环依赖
pub mod executor_common;
// executor_common_lite 保留用于不需要完整功能的场景
pub mod executor_common_lite;
pub mod profit_stop_loss;

// 具体策略实现
pub mod engulfing_strategy;

// 执行器
pub mod nwe_executor;
pub mod vegas_backtest;
pub mod vegas_executor;

// NWE 策略子模块
pub mod nwe_strategy;

// 重新导出
pub use engulfing_strategy::*;
pub use executor_common::*;
pub use executor_common_lite::ExecutionContext as LiteExecutionContext; // 避免冲突
pub use nwe_executor::*;
pub use profit_stop_loss::*;
pub use vegas_backtest::*;
pub use vegas_executor::*;
