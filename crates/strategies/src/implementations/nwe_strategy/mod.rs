pub mod indicator_combine;
use std::thread::current;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

use rust_quant_indicators::volatility::atr::ATR;
use rust_quant_indicators::volatility::atr::ATRStopLoos;
use rust_quant_indicators::trend::nwe_indicator::NweIndicator;
use rust_quant_indicators::momentum::rsi::RsiIndicator;
use rust_quant_indicators::volume_indicator::VolumeRatioIndicator;
use crate::nwe_strategy::indicator_combine::NweIndicatorCombine;
use crate::strategy_common::{
    BackTestResult, BasicRiskStrategyConfig, SignalResult,
};
use crate::{CandleItem, time_util};

/// NWE 策略配置与执行器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NweStrategyConfig {
    pub period: String,

    pub rsi_period: usize,
    pub rsi_overbought: f64,
    pub rsi_oversold: f64,

    pub atr_period: usize,
    pub atr_multiplier: f64,

    pub nwe_period: usize,
    pub nwe_multi: f64,
    pub volume_bar_num: usize,
    pub volume_ratio: f64,
    pub min_k_line_num: usize,
}

impl Default for NweStrategyConfig {
    fn default() -> Self {
        Self {
            period: "5m".to_string(),
            rsi_period: 14,
            rsi_overbought: 75.0,
            rsi_oversold: 25.0,

            atr_period: 14,
            atr_multiplier: 0.5,

            nwe_period: 8,
            nwe_multi: 3.0,

            volume_bar_num: 4,
            volume_ratio: 0.9,

            min_k_line_num: 500,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NweStrategy {
    pub config: NweStrategyConfig,
    pub combine_indicator: NweIndicatorCombine,
}

impl NweStrategy {
    /// 创建 Nwe 策略实例（零 clone 优化）✨
    pub fn new(config: NweStrategyConfig) -> Self {
        Self {
            combine_indicator: NweIndicatorCombine::new(&config),  // 传引用
            config,  // 直接 move，无需 clone
        }
    }
    pub fn get_strategy_name() -> String {
        "nwe".to_string()
    }

    pub fn get_min_data_length(&self) -> usize {
        self.config.min_k_line_num.max(self.config.nwe_period)
    }

    /**
     *
     */
    pub fn check_nwe(candles: &[CandleItem], values: &NweSignalValues) -> (bool, bool) {
        let mut is_buy = false;
        let mut is_sell = false;

        let middle = (values.nwe_upper + values.nwe_lower) / 2.0;
        let previous_candle = &candles[candles.len() - 2];
        let current_candle = candles.last().unwrap();

        //如果上一根k线路的的收盘价格小于nwe的lower,且最新k线的收盘价大于nwe,且不超过中轨，则进行买入
        if previous_candle.c < values.nwe_lower &&
        //前一根k线是下跌的
            previous_candle.c < previous_candle.o
            && current_candle.c > values.nwe_lower
            && current_candle.c < middle
        {
            is_buy = true;
        } else if previous_candle.c > values.nwe_upper
        //前一根k线是上涨的
            && previous_candle.c > previous_candle.o
            && current_candle.c < values.nwe_upper
            && current_candle.c > middle
        {
            //如果上一根k线路的的收盘价格大于nwe的upper,且最新k线的收盘价小于nwe，且不超过中轨，则进行卖出
            is_sell = true;
        }
        (is_buy, is_sell)
    }

    /**
     * 检查rsi是否超卖或超买
     */
    fn check_rsi(rsi: f64, rsi_oversold: f64, rsi_overbought: f64) -> (bool, bool) {
        let mut is_buy = false;
        let mut is_sell = false;
        if rsi < rsi_oversold {
            is_buy = true;
        } else if rsi > rsi_overbought {
            is_sell = true;
        }
        (is_buy, is_sell)
    }
    /**
     * 检查成交量比率是否超卖或超买
     */
    fn check_volume_ratio(volume_ratio: f64, volume_ratio_threshold: f64) -> (bool, bool) {
        let mut is_buy = false;
        let mut is_sell = false;
        if volume_ratio < volume_ratio_threshold {
            is_buy = true;
        } else if volume_ratio > volume_ratio_threshold {
            is_sell = true;
        }
        (is_buy, is_sell)
    }

    pub fn get_indicator_combine(&self) -> NweIndicatorCombine {
        self.combine_indicator.clone()
    }

    /// 生成信号：
    /// - close 下穿 lower → 做多（结合 RSI/Volume/ATR 过滤）
    /// - close 上穿 upper → 做空（结合 RSI/Volume/ATR 过滤）
    pub fn get_trade_signal(
        &mut self,
        candles: &[CandleItem],
        values: &NweSignalValues,
    ) -> SignalResult {
        let mut signal_result = SignalResult {
            should_buy: false,
            should_sell: false,
            open_price: 0.0,
            best_open_price: None,
            best_take_profit_price: None,
            ts: 0,
            single_value: None,
            single_result: None,
            signal_kline_stop_loss_price: None,
        };
        let rsi = values.rsi_value;
        let volume_ratio = values.volume_ratio;
        let atr = values.atr_value;
        let upper = values.nwe_upper;
        let lower = values.nwe_lower;

        //检查nwe是否超卖或超买
        let (is_nwe_buy, is_nwe_sell) = Self::check_nwe(candles, values);
        if is_nwe_buy || is_nwe_sell {
            // //检查rsi是否超卖或超买
            // let (is_rsi_buy, is_rsi_sell) =
            //     Self::check_rsi(rsi, self.config.rsi_oversold, self.config.rsi_overbought);

            // //检查成交量比率是否超卖或超买
            // let (is_volume_ratio_buy, is_volume_ratio_sell) =
            //     Self::check_volume_ratio(volume_ratio, self.config.volume_ratio);

            //如果上一根k线路的的收盘价格小于nwe的lower,且最新k线的收盘价大于nwe，且rsi超卖区间，则进行买入
            if is_nwe_buy {
                signal_result.should_buy = true;
                //设置止损价格,信号k止损
                // signal_result.signal_kline_stop_loss_price = Some(candles.last().unwrap().l);
            }
            if is_nwe_sell {
                signal_result.should_sell = true;
                //设置止损价格,信号k止损
                // signal_result.signal_kline_stop_loss_price = Some(candles.last().unwrap().h);
            }
            //设置止损价格,atr止损
            signal_result.signal_kline_stop_loss_price = Some(values.atr_short_stop);
        }
        signal_result.ts = candles.last().unwrap().ts;
        signal_result.open_price = candles.last().unwrap().c;

        info!("NWE signal values: {:#?}", values);
        info!("ts : {:#?}", rust_quant_common::utils::time::mill_time_to_datetime_shanghai(signal_result.ts));
        signal_result.single_value = Some(json!(values.clone()).to_string());
        signal_result.single_result = Some(json!(signal_result.clone()).to_string());

        signal_result
    }

    /// 运行回测：仅使用 RSI、Volume、NWE、ATR 指标（复用可插拔的 indicator_combine）
    pub fn run_test(
        &mut self,
        candles: &Vec<CandleItem>,
        risk: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        use crate::strategy_common::{self, run_back_test_generic};

        // 复用自定义的 indicator_combine 容器
        let mut ic = NweIndicatorCombine::default();
        // 懒初始化指标
        ic.rsi_indicator = Some(RsiIndicator::new(self.config.rsi_period));
        ic.volume_indicator = Some(VolumeRatioIndicator::new(self.config.volume_bar_num, true));
        ic.nwe_indicator = Some(NweIndicator::new(
            self.config.nwe_period as f64,
            self.config.nwe_multi,
            500,
        ));
        ic.atr_indicator = Some(
            ATRStopLoos::new(self.config.atr_period, self.config.atr_multiplier)
                .expect("ATR period must be > 0"),
        );

        let min_len = self.get_min_data_length();

        run_back_test_generic(
            |candles, values: &mut NweSignalValues| self.get_trade_signal(candles, values),
            candles,
            risk,
            min_len,
            &mut ic,
            |ic, data_item| {
                // 推进指标并返回当前值集合
                let rsi = if let Some(r) = &mut ic.rsi_indicator {
                    r.next(data_item.c)
                } else {
                    00.0
                };
                let volume_ratio = if let Some(v) = &mut ic.volume_indicator {
                    v.next(data_item.v)
                } else {
                    0.0
                };
                let (short_stop, long_stop, atr_value) = if let Some(a) = &mut ic.atr_indicator {
                    let (short_stop, long_stop, atr_value) =
                        a.next(data_item.h, data_item.l, data_item.c);
                    (short_stop, long_stop, atr_value)
                } else {
                    (0.0, 0.0, 0.0)
                };
                let (upper, lower) = if let Some(n) = &mut ic.nwe_indicator {
                    n.next(data_item.c)
                } else {
                    (0.0, 0.0)
                };
                NweSignalValues {
                    rsi_value: rsi,
                    volume_ratio: volume_ratio,
                    atr_value: atr_value,
                    atr_short_stop: short_stop,
                    atr_long_stop: long_stop,
                    nwe_upper: upper,
                    nwe_lower: lower,
                }
            },
        )
    }
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NweSignalValues {
    pub rsi_value: f64,

    pub volume_ratio: f64,

    pub atr_value: f64,
    pub atr_short_stop: f64,
    pub atr_long_stop: f64,

    pub nwe_upper: f64,
    pub nwe_lower: f64,
}
