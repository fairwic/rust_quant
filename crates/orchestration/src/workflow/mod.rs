// 工作流模块

// 基础任务（兼容层）
pub mod basic;
pub use crate::backtest::{executor as backtest_executor, runner as backtest_runner};
pub use crate::infra::{
    job_param_generator, progress_manager, strategy_config, signal_logger,
    strategy_execution_context, time_checker, data_sync, data_validator,
};
pub use crate::strategy::runner as strategy_runner;

// 数据任务（兼容层）
pub use crate::jobs::data::{
    asset_job, big_data_job, candles_job, tickets_job, tickets_volume_job,
    top_contract_job, trades_job, announcements_job, account_job,
};

// 风控任务（兼容层）
pub use crate::jobs::risk::{risk_balance_job, risk_position_job};

// 重新导出本地核心类型
pub use basic::*;
