// 工作流模块

// 基础任务
pub mod basic;
// TODO: strategy_config 有依赖问题，暂时禁用
// pub mod strategy_config;
// TODO: 以下模块有依赖问题，暂时禁用
pub mod strategy_runner;  // ✅ strategy_runner 已解耦，可以使用
pub mod strategy_execution_context;  // ✅ 新增: trait 实现
// pub mod progress_manager;
// pub mod data_validator;
// pub mod data_sync;
// pub mod job_param_generator;

// 数据任务
// TODO: 数据任务依赖rbatis等已废弃模块，暂时禁用
// pub mod candles_job;
// pub mod tickets_job;
// pub mod tickets_volume_job;
// pub mod trades_job;
// pub mod asset_job;
// pub mod big_data_job;
// pub mod top_contract_job;

// 风控任务
// TODO: 风控任务有依赖问题，暂时禁用
// pub mod risk_banlance_job;
// pub mod risk_order_job;
// pub mod risk_positon_job;

// 其他任务
// TODO: 其他任务有依赖问题，暂时禁用
// pub mod announcements_job;
// pub mod account_job;
// pub mod task_classification;
// pub mod backtest_executor;

// 重新导出核心类型
pub use basic::*;
// pub use strategy_config::*;
pub use strategy_runner::*;  // ✅ 导出 strategy_runner
pub use strategy_execution_context::*;  // ✅ 导出执行上下文
// pub use progress_manager::*;

// 导出风控任务
// pub use risk_banlance_job::*;
// pub use risk_order_job::*;
// pub use risk_positon_job::*;

// 导出数据同步任务
// pub use candles_job::*;
// pub use tickets_job::*;
