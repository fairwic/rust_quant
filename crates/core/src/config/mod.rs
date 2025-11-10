//! 配置管理模块

pub mod email;
pub mod environment;
pub mod shutdown_manager;

// 重新导出
pub use environment::*;
pub use shutdown_manager::{ShutdownConfig, ShutdownManager};
