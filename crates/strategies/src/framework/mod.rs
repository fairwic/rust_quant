// 策略框架核心模块
pub mod strategy_trait;
pub mod strategy_registry;
pub mod strategy_manager;
pub mod strategy_common;
pub mod config;
pub mod types;  // ⭐ 新增: 框架类型定义

// 重新导出核心类型
pub use strategy_trait::*;
pub use strategy_registry::*;
pub use strategy_manager::*;
pub use strategy_common::*;
pub use config::*;
pub use types::*;  // ⭐ 导出TradeSide等类型

