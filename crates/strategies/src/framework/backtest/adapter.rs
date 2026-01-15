use crate::CandleItem;

use super::engine::run_back_test_generic;
use super::types::{BackTestResult, BasicRiskStrategyConfig, SignalResult};

/// 通用的“指标驱动”策略回测适配器接口
///
/// 新增策略只需实现该 trait，即可自动复用 run_back_test_generic
/// 的全部交易撮合/风险处理逻辑
pub trait IndicatorStrategyBacktest {
    type IndicatorCombine;
    type IndicatorValues;

    /// 最小数据长度（例如指标 warm-up 所需的蜡烛数量）
    fn min_data_length(&self) -> usize;

    /// 初始化指标组合（每次回测都会获得全新的副本）
    fn init_indicator_combine(&self) -> Self::IndicatorCombine;

    /// 根据传入的 K 线更新指标值
    fn build_indicator_values(
        indicator_combine: &mut Self::IndicatorCombine,
        candle: &CandleItem,
    ) -> Self::IndicatorValues;

    /// 基于当前窗口/指标值生成信号
    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        values: &mut Self::IndicatorValues,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult;
}

/// 针对实现了 [`IndicatorStrategyBacktest`] 的策略，统一执行回测
pub fn run_indicator_strategy_backtest<S: IndicatorStrategyBacktest>(
    inst_id: &str,
    strategy: &mut S,
    candles_list: &Vec<CandleItem>,
    risk_config: BasicRiskStrategyConfig,
) -> BackTestResult {
    let min_len = strategy.min_data_length();
    let mut indicator_combine = strategy.init_indicator_combine();
    run_back_test_generic(
        inst_id,
        |candles, values| strategy.generate_signal(candles, values, &risk_config),
        candles_list,
        risk_config,
        min_len,
        &mut indicator_combine,
        |ic, candle| S::build_indicator_values(ic, candle),
    )
}

// ============================================================================
// Pipeline版本适配器（用于对比测试）
// ============================================================================

use super::engine::run_back_test_pipeline;

/// 使用Pipeline架构执行回测（适配IndicatorStrategyBacktest trait）
///
/// 与`run_indicator_strategy_backtest`功能相同，但使用Pipeline架构
pub fn run_indicator_strategy_backtest_pipeline<S>(
    inst_id: &str,
    strategy: S,
    candles_list: &[CandleItem],
    risk_config: BasicRiskStrategyConfig,
) -> BackTestResult
where
    S: IndicatorStrategyBacktest + Send + Sync + 'static,
    S::IndicatorCombine: Send + Sync + 'static,
    S::IndicatorValues: Send + Sync + 'static,
{
    run_back_test_pipeline(inst_id, strategy, candles_list, risk_config)
}
