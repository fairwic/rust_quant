//! # 策略任务基础模块
//!
//! 这个模块提供了策略测试的基础功能，现在已经重构为模块化结构。
//! 主要功能通过以下子模块提供：
//!
//! - `data_sync`: 数据同步功能
//! - `data_validator`: 数据验证功能
//! - `progress_manager`: 进度管理和断点续传
//! - `strategy_config`: 策略配置管理
//! - `backtest_executor`: 回测执行引擎
//! - `strategy_runner`: 策略运行器
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use rust_quant_orchestration::workflow::strategy_runner::test_random_strategy_with_config;
//! use rust_quant_orchestration::workflow::progress_manager::RandomStrategyConfig;
//!
//! // 执行带断点续传的策略测试
//! let config = RandomStrategyConfig::default();
//! test_random_strategy_with_config("BTC-USDT", "1H", semaphore, config).await?;
//! ```

// 重新导出主要的公共接口，保持向后兼容
// TODO: 这些模块暂时被禁用
// pub use crate::workflow::data_sync::*;
// pub use crate::workflow::data_validator::*;
// pub use crate::workflow::progress_manager::*;
// pub use crate::workflow::strategy_config::*;
// pub use crate::workflow::backtest_executor::*;
// pub use crate::workflow::strategy_runner::*;
