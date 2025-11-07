//! 回测引擎模块

pub mod engine;
pub mod metrics;

pub use engine::{BacktestEngine, BacktestConfig, BacktestReport};
pub use metrics::*;

