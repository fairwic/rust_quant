//! 数据访问层模块
//!
//! 实现 domain 层定义的 Repository 接口

pub mod candle_repository;
pub mod position_repository;
pub mod strategy_config_repository;

pub use candle_repository::SqlxCandleRepository;
pub use position_repository::{PositionEntity, SqlxPositionRepository};
pub use strategy_config_repository::{
    SqlxStrategyConfigRepository, StrategyConfigEntity, StrategyConfigEntityModel,
};

// TODO: 添加其他 Repository 实现
// - OrderRepository (待添加)
// - AccountRepository (待添加)
