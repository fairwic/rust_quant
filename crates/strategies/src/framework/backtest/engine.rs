use super::adapter::IndicatorStrategyBacktest;
use super::pipeline::stages::{FilterStage, PositionStage, SignalStage};
use super::pipeline::PipelineRunner;
use super::types::{BackTestResult, BasicRiskStrategyConfig};
use crate::CandleItem;

/// 回测引擎：仅保留 Pipeline 架构
///
/// 提供组件化的回测执行 Pipeline，降低代码阅读复杂性。
///
/// # 类型参数
/// - `S`: 实现 `IndicatorStrategyBacktest` trait 的策略
pub fn run_back_test<S>(
    inst_id: &str,
    strategy: S,
    candles_list: &[CandleItem],
    basic_risk_config: BasicRiskStrategyConfig,
) -> BackTestResult
where
    S: IndicatorStrategyBacktest + Send + Sync + 'static,
    S::IndicatorCombine: Send + Sync + 'static,
    S::IndicatorValues: Send + Sync + 'static,
{
    let min_data_length = strategy.min_data_length();

    let mut pipeline = PipelineRunner::new()
        .add_stage(SignalStage::new(strategy))
        .add_stage(FilterStage::new())
        .add_stage(PositionStage::new());

    pipeline.run(candles_list, inst_id, basic_risk_config, min_data_length)
}
