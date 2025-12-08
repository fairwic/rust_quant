//! 回测绩效指标计算模块
//!
//! 提供夏普比率、年化收益率、最大回撤、波动率等核心指标的计算

mod metrics;

pub use metrics::{calculate_performance_metrics, PerformanceCalculator};
