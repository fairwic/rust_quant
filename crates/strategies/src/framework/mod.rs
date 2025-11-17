// 策略框架核心模块
pub mod config;
pub mod execution_traits;
pub mod strategy_common;
pub mod strategy_manager;
pub mod strategy_registry;
pub mod strategy_trait;
pub mod types; // ⭐ 新增: 框架类型定义 // ⭐ 新增: 执行接口定义（解耦循环依赖）
pub mod backtest; // ⭐ 新增: 回测模块（从strategy_common拆分）

// 重新导出核心类型
pub use config::*;
pub use strategy_common::*; // strategy_common 重新导出 backtest，保持向后兼容
pub use strategy_manager::*;
pub use strategy_registry::*;
pub use strategy_trait::*;
// types 的内容已在 strategy_common 中定义，避免重复导出
pub use execution_traits::*; // ⭐ 导出执行接口
// 注意：不直接导出 backtest，因为 strategy_common 已经重新导出了，避免重复
