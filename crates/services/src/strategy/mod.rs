//! 策略相关服务模块

pub mod strategy_config_service;
pub mod strategy_execution_service;

pub use strategy_config_service::StrategyConfigService;
pub use strategy_execution_service::StrategyExecutionService;

// TODO: 添加其他策略服务
// - BacktestService: 回测服务
// - StrategyOptimizationService: 参数优化服务
