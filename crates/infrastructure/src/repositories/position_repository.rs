//! 持仓仓储预留模块。
//!
//! 当前执行闭环的订单和仓位状态由 `swap_order_repository`、`execution_tasks`
//! 以及 Web 侧交易记录承接；正式恢复本地持仓仓储时需要直接按 Postgres
//! repository 重新实现。
