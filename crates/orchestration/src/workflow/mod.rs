// 工作流模块

// 基础任务
pub mod basic;
// TODO: strategy_config 有依赖问题，暂时禁用
// pub mod strategy_config;
// TODO: 以下模块有依赖问题，暂时禁用
pub mod strategy_runner;  // ✅ strategy_runner 已解耦，可以使用
pub mod strategy_execution_context;  // ✅ 新增: trait 实现
pub mod time_checker;  // ✅ 新增: 时间检查器
pub mod signal_logger;  // ✅ 新增: 信号日志记录器
// pub mod progress_manager;
pub mod data_validator;  // ✅ 已从src/迁移
pub mod data_sync;  // ✅ 已从src/迁移
// pub mod job_param_generator;

// 数据任务
pub mod candles_job;  // ✅ 已从src/迁移并重构为Repository模式
pub mod tickets_job;  // ✅ 已从src/迁移
pub mod tickets_volume_job;  // ✅ 已从src/迁移
pub mod trades_job;  // ✅ 已从src/迁移
pub mod asset_job;  // ✅ 已从src/迁移
pub mod big_data_job;  // ✅ 已从src/迁移
pub mod top_contract_job;  // ✅ 已从src/迁移

// 风控任务
// TODO: risk_banlance_job和risk_order_job待迁移
// pub mod risk_banlance_job;
// pub mod risk_order_job;
pub mod risk_positon_job;  // ✅ 已从src/迁移

// 其他任务
pub mod announcements_job;  // ✅ 已从src/迁移
pub mod account_job;  // ✅ 已从src/迁移
// pub mod task_classification;
// pub mod backtest_executor;

// 重新导出核心类型
pub use basic::*;
// pub use strategy_config::*;
pub use strategy_runner::*;  // ✅ 导出 strategy_runner
pub use strategy_execution_context::*;  // ✅ 导出执行上下文
pub use time_checker::*;  // ✅ 导出时间检查器
pub use signal_logger::*;  // ✅ 导出信号日志记录器
// pub use progress_manager::*;

// 导出风控任务
// pub use risk_banlance_job::*;
// pub use risk_order_job::*;
// pub use risk_positon_job::*;

// 导出数据同步任务
// pub use candles_job::*;
// pub use tickets_job::*;
