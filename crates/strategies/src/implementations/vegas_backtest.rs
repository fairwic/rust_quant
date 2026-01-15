use rust_quant_indicators::trend::signal_weight::SignalWeightsConfig;
use rust_quant_indicators::trend::vegas::{
    IndicatorCombine, VegasIndicatorSignalValue, VegasStrategy,
};

use crate::framework::backtest::adapter::IndicatorStrategyBacktest;
use crate::framework::backtest::conversions::{convert_domain_signal, to_domain_basic_risk_config};
use crate::framework::backtest::trait_impl::BackTestAbleStrategyTrait;
use crate::framework::backtest::types::{BasicRiskStrategyConfig, SignalResult};
use crate::strategy_common::get_multi_indicator_values;
use crate::CandleItem;
use crate::StrategyType;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;

/// Vegas 策略回测适配器
///
/// 将 indicators 包中的 `VegasStrategy` 接入通用回测框架，
/// 让 orchestration 在新增策略时无需编写重复逻辑。
#[derive(Debug, Clone)]
pub struct VegasBacktestAdapter {
    strategy: VegasStrategy,
    signal_weights: SignalWeightsConfig,
    btc_macro: Option<BtcMacroFilter>,
}

#[derive(Debug, Clone)]
struct BtcMacroFilter {
    candles: Arc<Vec<CandleItem>>,
    ts_to_index: HashMap<i64, usize>,
}

impl BtcMacroFilter {
    fn new(candles: Arc<Vec<CandleItem>>) -> Self {
        let mut ts_to_index = HashMap::with_capacity(candles.len().saturating_mul(2));
        for (idx, c) in candles.iter().enumerate() {
            ts_to_index.insert(c.ts, idx);
        }
        Self { candles, ts_to_index }
    }

    fn momentum(&self, ts: i64, lookback: usize) -> Option<f64> {
        let idx = *self.ts_to_index.get(&ts)?;
        if idx < lookback {
            return None;
        }
        let now = self.candles.get(idx)?.c;
        let prev = self.candles.get(idx - lookback)?.c;
        if prev.abs() < 1e-12 {
            return None;
        }
        Some(now / prev - 1.0)
    }
}

impl VegasBacktestAdapter {
    pub fn new(strategy: VegasStrategy) -> Self {
        let signal_weights = strategy.signal_weights.clone().unwrap_or_default();
        Self {
            strategy,
            signal_weights,
            btc_macro: None,
        }
    }

    pub fn with_btc_macro_candles(mut self, btc_candles: Arc<Vec<CandleItem>>) -> Self {
        self.btc_macro = Some(BtcMacroFilter::new(btc_candles));
        self
    }

    pub fn strategy(&self) -> &VegasStrategy {
        &self.strategy
    }

    pub fn strategy_mut(&mut self) -> &mut VegasStrategy {
        &mut self.strategy
    }

    fn apply_btc_macro_veto(&self, eth_window: &[CandleItem], signal: &mut SignalResult) {
        let Some(btc_macro) = self.btc_macro.as_ref() else {
            return;
        };

        let enabled = env::var("BTC_MACRO_FILTER")
            .map(|v| v != "0")
            .unwrap_or(true);
        if !enabled {
            return;
        }

        let last = match eth_window.last() {
            Some(v) => v,
            None => return,
        };

        let lookback = env::var("BTC_MACRO_LOOKBACK")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(2)
            .max(1);

        if eth_window.len() <= lookback {
            return;
        }

        let btc_mom = match btc_macro.momentum(last.ts, lookback) {
            Some(v) => v,
            None => return,
        };
        let eth_prev = eth_window[eth_window.len() - 1 - lookback].c;
        if eth_prev.abs() < 1e-12 {
            return;
        }
        let eth_mom = eth_window[eth_window.len() - 1].c / eth_prev - 1.0;
        let rel_mom = eth_mom - btc_mom;

        let btc_mom_threshold = env::var("BTC_MACRO_BTC_MOM_THRESHOLD")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.008)
            .abs();
        let eth_rel_override = env::var("BTC_MACRO_ETH_REL_OVERRIDE")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.015)
            .abs();
        let eth_abs_override = env::var("BTC_MACRO_ETH_ABS_OVERRIDE")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.02)
            .abs();

        let btc_trend_up = btc_mom >= btc_mom_threshold;
        let btc_trend_down = btc_mom <= -btc_mom_threshold;
        if !btc_trend_up && !btc_trend_down {
            return;
        }

        let eth_own_bull = rel_mom >= eth_rel_override || eth_mom >= eth_abs_override;
        let eth_own_bear = rel_mom <= -eth_rel_override || eth_mom <= -eth_abs_override;

        // BTC 大盘上行：默认禁做空 ETH；除非 ETH 自身显著走弱（相对 BTC 明显弱势/大阴线）
        if btc_trend_up && signal.should_sell && !eth_own_bear {
            signal.should_sell = false;
            signal.best_open_price = None;
            signal.filter_reasons.push("BTC_MACRO_VETO_SHORT".to_string());
        }

        // BTC 大盘下行：默认禁做多 ETH；除非 ETH 自身显著走强（相对 BTC 明显强势/大阳线）
        if btc_trend_down && signal.should_buy && !eth_own_bull {
            signal.should_buy = false;
            signal.best_open_price = None;
            signal.filter_reasons.push("BTC_MACRO_VETO_LONG".to_string());
        }
    }
}

impl IndicatorStrategyBacktest for VegasBacktestAdapter {
    type IndicatorCombine = IndicatorCombine;
    type IndicatorValues = VegasIndicatorSignalValue;

    fn min_data_length(&self) -> usize {
        self.strategy.min_k_line_num.max(1)
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {
        self.strategy.get_indicator_combine()
    }

    fn build_indicator_values(
        indicator_combine: &mut Self::IndicatorCombine,
        candle: &CandleItem,
    ) -> Self::IndicatorValues {
        get_multi_indicator_values(indicator_combine, candle)
    }

    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        values: &mut Self::IndicatorValues,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        let domain_risk = to_domain_basic_risk_config(risk_config);
        let domain_signal =
            self.strategy
                .get_trade_signal(candles, values, &self.signal_weights, &domain_risk);
        let mut signal = convert_domain_signal(domain_signal);
        self.apply_btc_macro_veto(candles, &mut signal);
        signal
    }
}
