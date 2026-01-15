//! # Pipeline回测框架
//!
//! 提供组件化的回测执行Pipeline，降低代码阅读复杂性。
//!
//! ## 核心组件
//! - [`BacktestStage`] - 阶段trait
//! - [`BacktestContext`] - 统一状态容器
//! - [`PipelineRunner`] - Pipeline执行器

mod context;
mod runner;
mod stage;
pub mod stages;

pub use context::BacktestContext;
pub use runner::PipelineRunner;
pub use stage::{BacktestStage, StageResult};
