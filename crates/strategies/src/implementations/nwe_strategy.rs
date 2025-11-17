// ⭐ 指标组合已移至 indicators 包
// pub mod indicator_combine;  // 已废弃

use rust_quant_indicators::KlineHammerIndicator;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

use rust_quant_indicators::momentum::RsiIndicator;
use rust_quant_indicators::trend::ema_indicator::EmaIndicator;
use rust_quant_indicators::trend::nwe_indicator::NweIndicator;
use rust_quant_indicators::volatility::ATRStopLoos;
use rust_quant_indicators::volume::VolumeRatioIndicator;
use ta::Next;
// ⭐ 使用新的 indicators::nwe 模块
use crate::strategy_common::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::{time_util, CandleItem};
use rust_quant_indicators::trend::nwe::{
    NweIndicatorCombine, NweIndicatorConfig, NweIndicatorValues,
};

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
    pub k_line_hammer_shadow_ratio: f64,
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
            k_line_hammer_shadow_ratio: 0.45,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NweStrategy {
    pub config: NweStrategyConfig,
    pub combine_indicator: NweIndicatorCombine,
    /// Vegas 通道 EMA 过滤器（12, 144, 169）
    pub vegas_ema_indicator: Option<EmaIndicator>,
}
impl NweStrategy {}

impl NweStrategy {
    /// 创建 Nwe 策略实例（零 clone 优化）✨
    pub fn new(config: NweStrategyConfig) -> Self {
        // ⭐ 转换为 NweIndicatorConfig
        let indicator_config = NweIndicatorConfig {
            rsi_period: config.rsi_period,
            volume_bar_num: config.volume_bar_num,
            nwe_period: config.nwe_period,
            nwe_multi: config.nwe_multi,
            atr_period: config.atr_period,
            atr_multiplier: config.atr_multiplier,
            k_line_hammer_shadow_ratio: config.k_line_hammer_shadow_ratio,
            min_k_line_num: config.min_k_line_num,
        };

        Self {
            combine_indicator: NweIndicatorCombine::new(&indicator_config),
            // Vegas EMA 默认使用 12,144,169，其余周期按 Vegas 典型配置占位
            vegas_ema_indicator: Some(EmaIndicator::new(12, 144, 169, 576, 676, 2304, 2704)),
            config,
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
    pub fn check_nwe(&self, candles: &[CandleItem], values: &NweSignalValues) -> (bool, bool) {
        let mut is_buy = false;
        let mut is_sell = false;

        let middle = (values.nwe_upper + values.nwe_lower) / 2.0;
        let previous_candle = &candles[candles.len() - 2];
        let current_candle = candles.last().unwrap();
        let kline_hammer_indicator_output = KlineHammerIndicator::new(
            self.config.k_line_hammer_shadow_ratio,
            self.config.k_line_hammer_shadow_ratio,
        )
        .next(current_candle);

        let is_hanging_man = kline_hammer_indicator_output.is_hanging_man;
        let is_hammer = kline_hammer_indicator_output.is_hammer;
        //检查rsi

        //如果上一根k线路的的收盘价格小于nwe的lower,且最新k线的收盘价大于nwe,且不超过中轨，且没有长的上影线，则进行买入
        if previous_candle.c < values.nwe_lower &&
        //前一根k线是下跌的
            previous_candle.c < previous_candle.o
            && current_candle.c > values.nwe_lower
            && current_candle.c < middle
        && !is_hanging_man
        //rsi超卖区间
        && values.rsi_value < self.config.rsi_oversold
        {
            is_buy = true;
        } else if previous_candle.c > values.nwe_upper
        //前一根k线是上涨的
            && previous_candle.c > previous_candle.o
            && current_candle.c < values.nwe_upper
            && current_candle.c > middle
        && !is_hammer
        && values.rsi_value > self.config.rsi_overbought
        {
            //如果上一根k线路的的收盘价格大于nwe的upper,且最新k线的收盘价小于nwe，且不超过中轨，则进行卖出
            is_sell = true;
        }
        (is_buy, is_sell)
    }

    /// 使用 Vegas 通道的 EMA 排列（12,144,169）过滤方向：
    /// - ema12 > ema144 > ema169 → 只做多
    /// - ema12 < ema144 < ema169 → 只做空
    /// - 其他情况 → 不启用 Vegas 方向过滤
    fn apply_vegas_trend_filter(
        &mut self,
        candles: &[CandleItem],
        signal_result: &mut SignalResult,
    ) {
        let ema_indicator = match self.vegas_ema_indicator.as_mut() {
            Some(indicator) => indicator,
            None => {
                return;
            }
        };

        let last_candle = match candles.last() {
            Some(candle) => candle,
            None => {
                return;
            }
        };

        let close_price = last_candle.c;

        let ema12 = ema_indicator.ema1_indicator.next(close_price);
        let ema144 = ema_indicator.ema2_indicator.next(close_price);
        let ema169 = ema_indicator.ema3_indicator.next(close_price);

        let is_bull_trend = ema12 > ema144 && ema144 > ema169;
        let is_bear_trend = ema12 < ema144 && ema144 < ema169;

        if !is_bull_trend && !is_bear_trend {
            // 其他情况：不启用 Vegas 过滤
            return;
        }

        // 只在已有 NWE 信号的基础上做方向过滤
        if is_bull_trend && !is_bear_trend {
            if !signal_result.should_buy {
                signal_result.should_sell = false;
            }
        } else if is_bear_trend && !is_bull_trend {
            if !signal_result.should_sell {
                signal_result.should_buy = false;
            }
        }
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
            move_stop_open_price_when_touch_price: None,
        };
        let rsi = values.rsi_value;
        let volume_ratio = values.volume_ratio;
        let atr = values.atr_value;
        let upper = values.nwe_upper;
        let lower = values.nwe_lower;

        //检查nwe是否超卖或超买
        let (is_nwe_buy, is_nwe_sell) = self.check_nwe(candles, values);
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
                signal_result.signal_kline_stop_loss_price = Some(values.atr_long_stop);
                //设置移动止损当达到一个特定的价格位置的时候，移动止损线到开仓价格附近
                signal_result.move_stop_open_price_when_touch_price =
                    Some(lower + (upper - lower) * 0.5);
            }
            if is_nwe_sell {
                signal_result.should_sell = true;
                signal_result.signal_kline_stop_loss_price = Some(values.atr_short_stop);
                //设置移动止损当达到一个特定的价格位置的时候，移动止损线到开仓价格附近
                signal_result.move_stop_open_price_when_touch_price =
                    Some(upper - (upper - lower) * 0.5);
                //设置止损价格,信号k止损
                // signal_result.signal_kline_stop_loss_price = Some(candles.last().unwrap().h);
            }
            //设置止损价格,atr止损
        }

        // 使用 Vegas EMA 排列进行方向过滤
        self.apply_vegas_trend_filter(candles, &mut signal_result);

        signal_result.ts = candles.last().unwrap().ts;
        signal_result.open_price = candles.last().unwrap().c;

        // info!("NWE signal values: {:#?}", values);
        // info!(
        // "ts : {:#?}",
        //     rust_quant_common::utils::time::mill_time_to_datetime_shanghai(signal_result.ts)
        // );
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

        // ⭐ 使用新的 indicators::nwe::NweIndicatorCombine
        let indicator_config = NweIndicatorConfig {
            rsi_period: self.config.rsi_period,
            volume_bar_num: self.config.volume_bar_num,
            nwe_period: self.config.nwe_period,
            nwe_multi: self.config.nwe_multi,
            atr_period: self.config.atr_period,
            atr_multiplier: self.config.atr_multiplier,
            k_line_hammer_shadow_ratio: self.config.k_line_hammer_shadow_ratio,
            min_k_line_num: self.config.min_k_line_num,
        };
        let mut ic = NweIndicatorCombine::new(&indicator_config);

        let min_len = self.get_min_data_length();

        run_back_test_generic(
            |candles, values: &mut NweSignalValues| self.get_trade_signal(candles, values),
            candles,
            risk,
            min_len,
            &mut ic,
            |ic, data_item| {
                // ⭐ 使用新的 next() 方法，返回 NweIndicatorValues
                let indicator_values = ic.next(data_item);

                // 转换为策略层的 NweSignalValues
                NweSignalValues {
                    rsi_value: indicator_values.rsi_value,
                    volume_ratio: indicator_values.volume_ratio,
                    atr_value: indicator_values.atr_value,
                    atr_short_stop: indicator_values.atr_short_stop,
                    atr_long_stop: indicator_values.atr_long_stop,
                    nwe_upper: indicator_values.nwe_upper,
                    nwe_lower: indicator_values.nwe_lower,
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
