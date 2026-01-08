// 工作流模块

// 基础任务（兼容层）
pub mod basic;
pub mod websocket_handler;
pub mod funding_rate_job;

pub use crate::backtest::{executor as backtest_executor, runner as backtest_runner};
pub use crate::infra::{
    data_sync, data_validator, job_param_generator, progress_manager, signal_logger,
    strategy_config, strategy_execution_context, time_checker,
};
pub use crate::strategy::runner as strategy_runner;

// 数据任务（兼容层）
pub use crate::jobs::data::{
    account_job, announcements_job, asset_job, big_data_job, candles_job, tickets_job,
    tickets_volume_job, top_contract_job, trades_job,
};

// 风控任务（兼容层）
pub use crate::jobs::risk::{risk_balance_job, risk_position_job};

// 重新导出本地核心类型
pub use basic::*;
