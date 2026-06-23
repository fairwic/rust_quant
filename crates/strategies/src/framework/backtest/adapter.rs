use super::engine::run_back_test;
use super::types::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;
/// 通用的“指标驱动”策略回测适配器接口
///
/// 新增策略只需实现该 trait，即可复用 pipeline 回测流程。
pub trait IndicatorStrategyBacktest {
    type IndicatorCombine;
    type IndicatorValues;
    fn min_data_length(&self) -> usize;
    fn init_indicator_combine(&self) -> Self::IndicatorCombine;
    fn build_indicator_values(
        indicator_combine: &mut Self::IndicatorCombine,
        candle: &CandleItem,
    ) -> Self::IndicatorValues;
    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        values: &mut Self::IndicatorValues,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult;
}
pub fn run_indicator_strategy_backtest<S>(
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
    run_back_test(inst_id, strategy, candles_list, risk_config)
}
#[cfg(test)]
mod tests {
    #[test]
    fn pipeline_backtest_runs_and_records_trades() {
        use crate::framework::backtest::adapter::run_indicator_strategy_backtest;
        use crate::framework::backtest::types::BasicRiskStrategyConfig;
        #[derive(Debug, Clone, Default)]
        struct Strategy;
        impl crate::framework::backtest::adapter::IndicatorStrategyBacktest for Strategy {
            type IndicatorCombine = ();
            type IndicatorValues = ();
            fn min_data_length(&self) -> usize {
                3
            }
            fn init_indicator_combine(&self) -> Self::IndicatorCombine {}
            fn build_indicator_values(
                _: &mut Self::IndicatorCombine,
                _: &crate::CandleItem,
            ) -> Self::IndicatorValues {
            }
            /// 生成 回测与策略研究 需要的派生数据，供后续执行、展示或审计使用。
            fn generate_signal(
                &mut self,
                candles: &[crate::CandleItem],
                _: &mut Self::IndicatorValues,
                _: &BasicRiskStrategyConfig,
            ) -> crate::framework::backtest::types::SignalResult {
                let last = candles.last().unwrap();
                let mut s = crate::framework::backtest::types::SignalResult {
                    ts: last.ts,
                    open_price: last.c,
                    ..crate::framework::backtest::types::SignalResult::default()
                };
                if s.ts % 2 == 0 {
                    s.should_buy = true;
                }
                s
            }
        }
        let candles: Vec<crate::CandleItem> = (0..800)
            .map(|i| crate::CandleItem {
                o: 100.0,
                h: 101.0,
                l: 99.0,
                c: 100.0,
                v: 1.0,
                ts: i,
                confirm: 1,
            })
            .collect();
        let risk = BasicRiskStrategyConfig {
            max_loss_percent: 1.0,
            ..BasicRiskStrategyConfig::default()
        };
        let result = run_indicator_strategy_backtest("TEST", Strategy, &candles, risk);
        assert!(result.open_trades > 0);
        assert!(!result.audit_trail.signal_snapshots.is_empty());
    }
}
