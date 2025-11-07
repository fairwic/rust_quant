//! # Rust Quant Strategies
//! 
//! 策略引擎：策略框架、具体实现、回测引擎
//! 
//! ## 职责
//! 
//! 这个包只包含纯粹的策略逻辑：
//! - 策略框架定义
//! - 具体策略实现
//! - 回测引擎
//! 
//! ## 已移出的模块
//! 
//! - `cache` → `infrastructure::cache` (缓存由基础设施层管理)
//! - `redis_operations` → `infrastructure::cache::strategy_cache`
//! - `support_resistance` → `indicators::pattern::support_resistance`

pub mod framework;
pub mod implementations;
pub mod backtesting;

// 重新导出核心类型
pub use framework::*;
pub use implementations::*;

// 重新导出 domain 类型供内部使用
pub use rust_quant_domain::{
    StrategyType, StrategyStatus, Timeframe,
    SignalResult, TradingSignal,
};

// 重新导出 common 类型
pub use rust_quant_common::{CandleItem, TradeSide};

// 工具函数重导出
pub mod time_util {
    pub use rust_quant_common::utils::time::*;
}
