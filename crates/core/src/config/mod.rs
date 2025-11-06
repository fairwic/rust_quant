//! 配置管理模块

pub mod environment;
pub mod shutdown_manager;
pub mod email;

// 重新导出
pub use environment::*;
pub use shutdown_manager::{ShutdownConfig, ShutdownManager};
