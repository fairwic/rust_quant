//! 领域接口模块
//!
//! 定义领域层的抽象接口，由基础设施层实现

pub mod repository_trait;
pub mod strategy_trait;

pub use repository_trait::{
    CandleRepository, OrderRepository, PositionRepository, StrategyConfigRepository,
};
pub use strategy_trait::{BacktestResult, Backtestable, Strategy};
