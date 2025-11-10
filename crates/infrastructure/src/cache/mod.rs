//! 缓存模块
//!
//! 提供通用缓存能力，不包含业务特定逻辑

pub mod generic_cache;
pub mod indicator_cache;
pub mod strategy_cache;

pub use generic_cache::*;
pub use indicator_cache::*;
pub use strategy_cache::*;

// 业务特定缓存已移动到对应的包：
// - arc_vegas_indicator_values -> strategies包
// - ema_indicator_values -> indicators包
// - arc_nwe_indicator_values -> strategies包
// - latest_candle_cache -> market包
