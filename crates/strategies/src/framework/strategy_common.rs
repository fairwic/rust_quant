//! # Strategy Common
//!
//! 策略通用模块 - 已拆分为 backtest 子模块
//!
//! 此文件保留用于向后兼容，实际实现已迁移到 `backtest` 模块

// 重新导出 backtest 模块的所有内容，保持向后兼容
pub use crate::framework::backtest::*;
