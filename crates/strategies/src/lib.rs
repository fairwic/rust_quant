//! # Rust Quant Strategies
//! 
//! 策略引擎：策略框架、具体实现、回测引擎

pub mod framework;
pub mod implementations;
pub mod backtesting;

// 重新导出核心类型
pub use framework::*;
pub use implementations::*;
