// 具体策略实现模块

// 通用执行器和辅助模块
// ✅ executor_common 已使用 trait 解耦，不再有循环依赖
pub mod executor_common;
// executor_common_lite 保留用于不需要完整功能的场景
pub mod executor_common_lite;
pub mod profit_stop_loss;
// redis_operations → 已移至 infrastructure::cache::strategy_cache
// support_resistance → 已移至 indicators::pattern::support_resistance

// 具体策略实现
// ✅ 已修复孤儿规则问题（使用适配器）
pub mod comprehensive_strategy;
pub mod engulfing_strategy;
pub mod macd_kdj_strategy;
// TODO: mult_combine_strategy依赖trading模块，暂时注释
// pub mod mult_combine_strategy;
pub mod squeeze_strategy;
// TODO: top_contract_strategy依赖big_data框架，暂时注释
// pub mod top_contract_strategy;
pub mod ut_boot_strategy;

// 执行器 - TODO: 这两个执行器依赖orchestration，暂时注释
// pub mod nwe_executor;
// pub mod vegas_executor;

// NWE 策略子模块
pub mod nwe_strategy;

// 重新导出
pub use executor_common::*;
pub use executor_common_lite::ExecutionContext as LiteExecutionContext;  // 避免冲突
pub use profit_stop_loss::*;
pub use comprehensive_strategy::*;  // ✅ 已恢复
pub use engulfing_strategy::*;
pub use macd_kdj_strategy::*;
// pub use mult_combine_strategy::*;
pub use squeeze_strategy::*;
// pub use top_contract_strategy::*;
pub use ut_boot_strategy::*;
// pub use nwe_executor::*;
// pub use vegas_executor::*;

