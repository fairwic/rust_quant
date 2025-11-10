//! 回测引擎模块

pub mod engine;
pub mod metrics;

pub use engine::{BacktestConfig, BacktestEngine, BacktestReport};
pub use metrics::*;
