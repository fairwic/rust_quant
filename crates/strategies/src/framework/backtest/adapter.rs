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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Default)]
    struct StatefulStrategy {
        calls: usize,
    }

    impl IndicatorStrategyBacktest for StatefulStrategy {
        type IndicatorCombine = ();
        type IndicatorValues = ();

        fn min_data_length(&self) -> usize {
            3
        }

        fn init_indicator_combine(&self) -> Self::IndicatorCombine {
            ()
        }

        fn build_indicator_values(
            _indicator_combine: &mut Self::IndicatorCombine,
            _candle: &CandleItem,
        ) -> Self::IndicatorValues {
            ()
        }

        fn generate_signal(
            &mut self,
            candles: &[CandleItem],
            _values: &mut Self::IndicatorValues,
            _risk_config: &BasicRiskStrategyConfig,
        ) -> SignalResult {
            self.calls += 1;

            let last = candles.last().expect("candles window is non-empty");
            let mut signal = SignalResult::default();
            signal.ts = last.ts;
            signal.open_price = last.c;

            // 依赖调用次数的状态机：用于验证 pipeline 在 i < 500 期间仍会调用策略但丢弃信号，
            // 从而与 legacy engine 的行为保持一致。
            if self.calls % 100 == 0 {
                signal.should_buy = true;
            }
            if self.calls % 100 == 50 {
                signal.should_sell = true;
            }

            signal
        }
    }

    fn build_candles(n: usize) -> Vec<CandleItem> {
        (0..n)
            .map(|i| {
                let base = 100.0 + i as f64 * 0.01;
                CandleItem {
                    o: base,
                    h: base * 1.001,
                    l: base * 0.999,
                    c: base,
                    v: 1.0,
                    ts: i as i64 * 60_000,
                    confirm: 1,
                }
            })
            .collect()
    }

    #[test]
    fn pipeline_matches_generic_engine() {
        let candles = build_candles(800);
        let mut risk = BasicRiskStrategyConfig::default();
        risk.max_loss_percent = 1.0;
        risk.is_used_signal_k_line_stop_loss = Some(false);
        risk.is_one_k_line_diff_stop_loss = Some(false);
        risk.is_move_stop_open_price_when_touch_price = Some(false);

        let mut legacy = StatefulStrategy::default();
        let legacy_result =
            run_indicator_strategy_backtest("TEST", &mut legacy, &candles, risk);

        let pipeline_result =
            run_indicator_strategy_backtest_pipeline("TEST", StatefulStrategy::default(), &candles, risk);

        assert_eq!(legacy_result.trade_records.len(), pipeline_result.trade_records.len());
        assert!((legacy_result.funds - pipeline_result.funds).abs() < 1e-9);
        assert!((legacy_result.win_rate - pipeline_result.win_rate).abs() < 1e-12);
    }
}
