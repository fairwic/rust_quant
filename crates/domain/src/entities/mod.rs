//! 业务实体模块
//!
//! 实体是具有唯一标识的领域对象，通常作为聚合根

pub mod backtest;
pub mod candle;
pub mod exchange_api_config;
pub mod order;
pub mod position;
pub mod strategy_config;
pub mod swap_order;
pub mod funding_rate;
pub mod economic_event;
pub mod filtered_signal_log;

pub use backtest::{BacktestDetail, BacktestLog, BacktestPerformanceMetrics, BacktestWinRateStats};
pub use candle::Candle;
pub use exchange_api_config::{ExchangeApiConfig, StrategyApiConfig};
pub use filtered_signal_log::FilteredSignalLog;
pub use order::{Order, OrderError};
pub use position::{MarginMode, Position, PositionError, PositionStatus};
pub use strategy_config::{BasicRiskConfig, StrategyConfig};
pub use swap_order::SwapOrder;
pub use funding_rate::FundingRate;
pub use economic_event::{EconomicEvent, EventImportance};
