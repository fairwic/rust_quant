//! 策略配置模块

pub mod strategy_config;
pub mod strategy_config_compat; // ⭐ 新增: 兼容层

// 重新导出
pub use strategy_config::*;
pub use strategy_config_compat::*; // ⭐ 导出兼容函数
