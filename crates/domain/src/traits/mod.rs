//! 领域接口模块
//!
//! 定义领域层的抽象接口，由基础设施层实现
pub mod fund_monitoring_repository;
pub mod funding_rate_repository;

pub use economic_event_repository::*;

pub mod economic_event_repository;

pub mod exchange_trait;
pub mod repository_trait;
pub mod strategy_trait;

pub use exchange_trait::{
    ExchangeAccount, ExchangeContracts, ExchangeMarketData, ExchangePublicData,
};
pub use repository_trait::{
    AuditLogRepository, BacktestLogRepository, CandleRepository, ExchangeApiConfigRepository,
    OrderRepository, PositionRepository, StrategyApiConfigRepository, StrategyConfigRepository,
    SwapOrderRepository,
};
pub use strategy_trait::{BacktestResult, Backtestable, Strategy};
