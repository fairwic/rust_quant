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
//! - `redis_operations` → `infrastructure::cache::strategy_cache`
//! - `support_resistance` → `indicators::pattern::support_resistance`
//!
//! ## 新增模块
//!
//! - `cache` - 策略相关的业务特定缓存（从infrastructure迁移）

pub mod adapters; // 适配器模块
pub mod cache;
pub mod framework;
pub mod implementations; // 策略缓存模块

// 重新导出核心类型（包含 strategy_common/backtest）
pub use framework::*;

// 重新导出 domain 类型供内部使用
pub use rust_quant_domain::{SignalResult as DomainSignalResult, StrategyStatus, StrategyType, Timeframe, TradingSignal};

// 重新导出 common 类型
pub use rust_quant_common::CandleItem;

// ⭐ TradeSide 在本地定义（framework::types）
// pub use rust_quant_domain::OrderSide as TradeSide;

// 工具函数重导出
pub mod time_util {
    pub use rust_quant_common::utils::time::*;
}
