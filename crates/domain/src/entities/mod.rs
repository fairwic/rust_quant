//! 业务实体模块
//!
//! 实体是具有唯一标识的领域对象，通常作为聚合根

pub mod backtest;
pub mod candle;
pub mod dynamic_config_log;
pub mod economic_event;
pub mod exchange_api_config;
pub mod filtered_signal_log;
pub mod funding_rate;
pub mod order;
pub mod position;
pub mod strategy_config;
pub mod swap_order;

pub use backtest::{BacktestDetail, BacktestLog, BacktestPerformanceMetrics, BacktestWinRateStats};
pub use candle::Candle;
pub use dynamic_config_log::DynamicConfigLog;
pub use economic_event::{EconomicEvent, EventImportance};
pub use exchange_api_config::{ExchangeApiConfig, StrategyApiConfig};
pub use filtered_signal_log::FilteredSignalLog;
pub use funding_rate::FundingRate;
pub use order::{Order, OrderError};
pub use position::{MarginMode, Position, PositionError, PositionStatus};
pub use strategy_config::{BasicRiskConfig, StrategyConfig};
pub use swap_order::SwapOrder;

pub mod fund_flow;
pub use fund_flow::{FundFlow, FundFlowAlert, FundFlowSide, MarketAnomaly, TickerSnapshot};
