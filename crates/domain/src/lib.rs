//! # Rust Quant Domain
//!
//! 领域模型层 - 纯粹的业务逻辑，不依赖任何基础设施
//!
//! ## 架构原则
//!
//! 1. **领域纯粹性**: 不依赖任何外部框架 (sqlx, redis 等)
//! 2. **业务规则集中**: 所有业务验证和规则都在这里
//! 3. **类型安全**: 使用值对象保证业务约束
//! 4. **可测试性**: 可以独立测试，不需要数据库或外部服务
//!
//! ## 模块组织
//!
//! - `entities`: 业务实体 (聚合根)，如 Order, Candle, StrategyConfig
//! - `value_objects`: 值对象，如 Price, Volume, Signal
//! - `enums`: 业务枚举，如 OrderSide, OrderStatus, StrategyType
//! - `traits`: 领域接口，定义抽象行为
//!
//! ## 使用示例
//!
//! ```rust
//! use rust_quant_domain::entities::Order;
//! use rust_quant_domain::value_objects::{Price, Volume};
//! use rust_quant_domain::enums::{OrderSide, OrderType};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // 创建订单 - 带业务验证
//! let order = Order::new(
//!     "ORDER-001".to_string(),
//!     "BTC-USDT".to_string(),
//!     OrderSide::Buy,
//!     OrderType::Limit,
//!     Price::new(50000.0)?,  // 自动验证价格 > 0
//!     Volume::new(1.0)?,     // 自动验证数量 >= 0
//! )?;
//! # Ok(())
//! # }
//! ```

pub mod entities;
pub mod enums;
pub mod traits;
pub mod value_objects;

// 重新导出核心类型

// 实体
pub use entities::{
    BasicRiskConfig, Candle, MarginMode, Order, OrderError, Position, PositionError,
    PositionStatus, StrategyConfig,
};

// 值对象
pub use value_objects::{
    Leverage, LeverageError, Percentage, PercentageError, Price, PriceError, SignalDirection,
    SignalResult, SignalStrength, Symbol, SymbolError, TradingSignal, Volume, VolumeError,
};

// 枚举
pub use enums::{
    OrderSide, OrderStatus, OrderType, PositionSide, StrategyStatus, StrategyType, Timeframe,
};

// 接口
pub use traits::{
    BacktestResult, Backtestable, CandleRepository, OrderRepository, Strategy,
    StrategyConfigRepository,
};

// 兼容旧代码的别名
pub use BasicRiskConfig as BasicRiskStrategyConfig;
