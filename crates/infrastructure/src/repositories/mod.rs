//! 数据访问层模块
//!
//! 实现 domain 层定义的 Repository 接口

pub mod backtest_repository;
pub mod candle_repository;
pub mod exchange_api_config_repository;
pub mod position_repository;
pub mod signal_log_repository;
pub mod strategy_config_repository;
pub mod swap_order_repository;

pub use backtest_repository::SqlxBacktestRepository;
pub use candle_repository::SqlxCandleRepository;
pub use exchange_api_config_repository::{
    ExchangeAppkeyConfigEntity, SqlxExchangeApiConfigRepository, SqlxStrategyApiConfigRepository,
};
// pub use position_repository::{PositionEntity, SqlxPositionRepository};
pub use signal_log_repository::{SignalLogEntity, SignalLogRepository};
pub use strategy_config_repository::{
    SqlxStrategyConfigRepository, StrategyConfigEntity, StrategyConfigEntityModel,
};
pub use swap_order_repository::{SqlxSwapOrderRepository, SwapOrderEntity};

// TODO: 添加其他 Repository 实现
// - AccountRepository (待添加)
