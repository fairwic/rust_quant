use super::adapter::IndicatorStrategyBacktest;
use super::pipeline::stages::{FilterStage, PositionStage, SignalStage};
use super::pipeline::PipelineRunner;
use super::types::{BackTestResult, BasicRiskStrategyConfig};
use crate::CandleItem;
/// 回测引擎：仅保留 Pipeline 架构
/// 提供组件化的回测执行 Pipeline，降低代码阅读复杂性。
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
    // 策略自己定义指标预热长度，入口只负责把该约束传入 pipeline，
    // 避免不同策略在统一回测引擎里混用固定 warm-up 规则。
    let min_data_length = strategy.min_data_length();
    // 回测阶段按信号生成、信号过滤、持仓推进串联；每个阶段只修改 BacktestContext
    // 中属于自己的状态，便于后续对比 legacy engine 或定位某根 K 线的决策来源。
    let mut pipeline = PipelineRunner::new()
        .add_stage(SignalStage::new(strategy))
        .add_stage(FilterStage::new())
        .add_stage(PositionStage::new());
    pipeline.run(candles_list, inst_id, basic_risk_config, min_data_length)
}
