use crate::trading::indicator::rsi_rma::Rsi;
use crate::trading::indicator::signal_weight::{
    SignalCondition, SignalDeriect, SignalScoreWithDeriact, SignalType, SignalWeightsConfig,
};
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::strategy::strategy_common;
use crate::trading::strategy::strategy_common::{
    BackTestResult, SignalResult, TradingStrategyConfig,
};
use fast_log::print;
use futures::io::sink;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt::Display;
use std::sync::Arc;
use ta::indicators::{BollingerBands, BollingerBandsOutput, ExponentialMovingAverage};
use ta::indicators::{MovingAverageConvergenceDivergence, RelativeStrengthIndex};
use ta::{Close, DataItem, High, Low, Next, Open, Volume};
use tracing::error;

use super::bollings::BollingerBandsSignalConfig;
use super::volume_indicator::VolumeRatioIndicator;

// 成交量趋势
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct VolumeTrendSignalValue {
    pub is_increasing: bool,
    pub is_decreasing: bool,
    pub is_stable: bool,   // 是否稳定
    pub volume_ratio: f64, // 添加 volume_ratio 字段
}

// 成交量信号配置
#[derive(Debug, Serialize, Deserialize)]
pub struct VolumeSignalConfig {
    pub volume_bar_num: usize,      // 看前10根K线
    pub volume_increase_ratio: f64, // 放量倍数
    pub volume_decrease_ratio: f64, // 缩量倍数
    pub is_open: bool,              // 是否开启
}

// ema信号配置
#[derive(Debug, Serialize, Deserialize)]
pub struct EmaSignalConfig {
    pub ema1_length: usize,
    pub ema2_length: usize,
    pub ema3_length: usize,
    pub ema4_length: usize,
    pub ema5_length: usize,
    pub ema_breakthrough_threshold: f64, // 新增：ema突破价格的阈值
    pub is_open: bool,
}

#[derive(Debug)]
pub struct EmaIndicator {
    pub ema1_indicator: ExponentialMovingAverage,
    pub ema2_indicator: ExponentialMovingAverage,
    pub ema3_indicator: ExponentialMovingAverage,
    pub ema4_indicator: ExponentialMovingAverage,
    pub ema5_indicator: ExponentialMovingAverage,
}
impl EmaIndicator {
    fn new(ema1: usize, ema2: usize, ema3: usize, ema4: usize, ema5: usize) -> Self {
        Self {
            ema1_indicator: ExponentialMovingAverage::new(ema1).unwrap(),
            ema2_indicator: ExponentialMovingAverage::new(ema2).unwrap(),
            ema3_indicator: ExponentialMovingAverage::new(ema3).unwrap(),
            ema4_indicator: ExponentialMovingAverage::new(ema4).unwrap(),
            ema5_indicator: ExponentialMovingAverage::new(ema5).unwrap(),
        }
    }
}

impl Default for EmaSignalConfig {
    fn default() -> Self {
        Self {
            ema1_length: 12,
            ema2_length: 144,
            ema3_length: 169,
            ema4_length: 576,
            ema5_length: 676,
            ema_breakthrough_threshold: 0.003,
            is_open: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EmaSignalValue {
    pub ema1_value: f64,
    pub ema2_value: f64,
    pub ema3_value: f64,
    pub ema4_value: f64,
    pub ema5_value: f64,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct BollingerSignalValue {
    pub lower: f64,
    pub upper: f64,
    pub middle: f64,
    pub is_long_signal: bool,
    pub is_short_signal: bool,
    pub is_close_signal: bool,
}

// rsi信号配置
#[derive(Debug, Serialize, Deserialize)]
pub struct RsiSignalConfig {
    pub rsi_length: usize,   // rsi周期
    pub rsi_oversold: f64,   // rsi超卖阈值
    pub rsi_overbought: f64, // rsi超买阈值
    pub is_open: bool,       // 是否开启
}
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RsiSignalValue {
    pub rsi_value: f64,      // rsi值
    pub is_oversold: bool,   // 是否超卖
    pub is_overbought: bool, // 是否超买
}

impl VolumeTrendSignalValue {
    pub fn new(
        is_increasing: bool,
        is_decreasing: bool,
        is_stable: bool,
        volume_ratio: f64,
    ) -> Self {
        Self {
            is_increasing,
            is_decreasing,
            is_stable,
            volume_ratio,
        }
    }
}

// ema趋势
#[derive(Debug, Serialize, Deserialize)]
pub struct EmaTouchTrendSignalConfig {
    pub ema1_with_ema2_ratio: f64,      //eam2与eam3的相差幅度
    pub ema2_with_ema3_ratio: f64,      //eam2与eam3的相差幅度
    pub ema3_with_ema4_ratio: f64,      //eam2与eam3的相差幅度
    pub ema4_with_ema5_ratio: f64,      //eam2与eam3的相差幅度
    pub price_with_ema_high_ratio: f64, //价格与ema4的相差幅度
    pub price_with_ema_low_ratio: f64,  //价格与ema4的相差幅度
    pub is_open: bool,                  //是否开启
}
impl Default for EmaTouchTrendSignalConfig {
    fn default() -> Self {
        Self {
            ema1_with_ema2_ratio: 1.010,      //ema1与ema2的相差幅度
            ema4_with_ema5_ratio: 1.012,      //ema4与ema5的相差幅度
            ema3_with_ema4_ratio: 1.012,      //ema3与ema4的相差幅度
            ema2_with_ema3_ratio: 1.012,      //ema2与ema3的相差幅度
            price_with_ema_high_ratio: 1.005, //价格与ema4的相差幅度
            price_with_ema_low_ratio: 0.995,  //价格与ema4的相差幅度
            is_open: true,                    //是否开启
        }
    }
}
// ema趋势
#[derive(Debug, Serialize, Deserialize)]
pub struct EmaTouchTrendSignalValue {
    pub is_uptrend: bool,                          //是否多头趋势
    pub is_downtrend: bool,                        //是否空头趋势
    pub is_in_uptrend_touch_ema2: bool,            //是否在多头趋势触碰ema2
    pub is_in_uptrend_touch_ema3: bool,            //是否在多头趋势触碰ema3
    pub is_in_uptrend_touch_ema2_ema3_nums: usize, //当前多头趋势中触碰ema2和ema3的次数

    pub is_in_uptrend_touch_ema4: bool, //是否在多头趋势触碰ema4
    pub is_in_uptrend_touch_ema5: bool, //是否在多头趋势触碰ema4
    pub is_in_uptrend_touch_ema4_ema5_nums: usize, //当前多头趋势中触碰ema4和ema5的次数

    pub is_in_downtrend_touch_ema2: bool, //是否在空头趋势触碰ema2
    pub is_in_downtrend_touch_ema3: bool, //是否在空头趋势触碰ema3
    pub is_in_downtrend_touch_ema2_ema3_nums: usize, //当前空头趋势触碰ema2和ema3的次数

    pub is_in_downtrend_touch_ema4: bool, //是否在空头趋势触碰ema4
    pub is_in_downtrend_touch_ema5: bool, //是否在空头趋势触碰ema5
    pub is_in_downtrend_touch_ema4_ema5_nums: usize, //当前空头趋势中触碰ema4和ema5的次数

    pub is_long_signal: bool,  //是否多头开仓
    pub is_short_signal: bool, //是否空头开仓
}
impl Default for EmaTouchTrendSignalValue {
    fn default() -> Self {
        Self {
            is_uptrend: false,
            is_downtrend: false,
            is_in_uptrend_touch_ema2: false,
            is_in_uptrend_touch_ema3: false,
            is_in_uptrend_touch_ema2_ema3_nums: 0,
            is_in_uptrend_touch_ema4: false,
            is_in_uptrend_touch_ema5: false,
            is_in_uptrend_touch_ema4_ema5_nums: 0,
            is_in_downtrend_touch_ema2: false,
            is_in_downtrend_touch_ema3: false,
            is_in_downtrend_touch_ema2_ema3_nums: 0,
            is_in_downtrend_touch_ema4: false,
            is_in_downtrend_touch_ema5: false,
            is_in_downtrend_touch_ema4_ema5_nums: 0,
            is_long_signal: false,
            is_short_signal: false,
        }
    }
}

// 新增：检查均线交叉
pub struct EmaCross {
    pub is_golden_cross: bool,
    pub is_death_cross: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct VegasIndicatorSignalValue {
    pub ema_values: EmaSignalValue,                // ema信号配置
    pub volume_value: VolumeTrendSignalValue,      // 新增：成交量信号配置
    pub ema_touch_value: EmaTouchTrendSignalValue, // ema趋势
    pub rsi_value: RsiSignalValue,                 //rsi信号配置
    pub bollinger_value: BollingerSignalValue,     //bollinger信号配置
    pub signal_weights_value: SignalWeightsConfig, // 新增权重配置
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VegasIndicator {
    pub ema_signal: EmaSignalConfig,                       // ema信号配置
    pub volume_signal: VolumeSignalConfig,                 // 新增：成交量信号配置
    pub ema_touch_trend_signal: EmaTouchTrendSignalConfig, // ema趋势
    pub rsi_signal: RsiSignalConfig,                       //rsi信号配置
    pub bollinger_signal: BollingerBandsSignalConfig,      //bollinger信号配置
    pub signal_weights: SignalWeightsConfig,               // 新增权重配置
}

impl Display for VegasIndicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "vegas indicator :ema0:{} ema2:{} ema3:{}",
            self.ema_signal.ema1_length, self.ema_signal.ema2_length, self.ema_signal.ema3_length
        )
    }
}
impl VegasIndicator {
    pub fn new(ema1: usize, ema2: usize, ema3: usize, ema4: usize, ema5: usize) -> Self {
        Self {
            ema_touch_trend_signal: EmaTouchTrendSignalConfig::default(),
            bollinger_signal: BollingerBandsSignalConfig::default(),
            ema_signal: EmaSignalConfig {
                ema1_length: ema1,
                ema2_length: ema2,
                ema3_length: ema3,
                ema4_length: ema4,
                ema5_length: ema5,
                ema_breakthrough_threshold: 0.003,
                is_open: true,
            },
            //11
            volume_signal: VolumeSignalConfig {
                volume_bar_num: 4,
                volume_increase_ratio: 2.5,
                volume_decrease_ratio: 0.5,
                is_open: true,
            },
            rsi_signal: RsiSignalConfig {
                rsi_length: 12,       // 默认RSI周期
                rsi_oversold: 25.0,   // 默认25
                rsi_overbought: 75.0, // 默认75
                is_open: true,
            },
            signal_weights: SignalWeightsConfig {
                weights: vec![
                    (SignalType::SimpleBreakEma2through, 2.0),
                    (SignalType::VolumeTrend, 1.5),
                    (SignalType::Rsi, 1.0),
                    (SignalType::TrendStrength, 1.5),
                    (SignalType::EmaDivergence, 1.8),
                    (SignalType::PriceLevel, 1.2),
                ],
                min_total_weight: 3.0, // 需要至少3分才触发信号
            },
        }
    }

    pub fn get_min_data_length(&mut self) -> usize {
        3400
    }
    pub fn calculate_ema(
        &mut self,
        data: &DataItem,
        ema_indicator: &mut EmaIndicator,
    ) -> EmaSignalValue {
        let mut ema_signal_value = EmaSignalValue::default();
        ema_signal_value.ema1_value = ema_indicator.ema1_indicator.next(data.close());
        ema_signal_value.ema2_value = ema_indicator.ema2_indicator.next(data.close());
        ema_signal_value.ema3_value = ema_indicator.ema3_indicator.next(data.close());
        ema_signal_value.ema4_value = ema_indicator.ema4_indicator.next(data.close());
        ema_signal_value.ema5_value = ema_indicator.ema5_indicator.next(data.close());
        ema_signal_value
    }

    pub fn get_data_items_and_ema_values(
        &mut self,
        prices: &Vec<CandlesEntity>,
    ) -> (Vec<DataItem>, VegasIndicatorSignalValue) {
        if prices.len() < self.ema_signal.ema5_length + 10 {
            error!(
                "数据长度不足: {} < {}",
                prices.len(),
                self.ema_signal.ema5_length + 10
            );
            return (vec![], VegasIndicatorSignalValue::default());
        }

        let mut vegas_indicator_signal_value = VegasIndicatorSignalValue::default();

        let mut data_items = vec![];
        //ema
        let mut ema_signal_value = EmaSignalValue::default();
        let mut ema_indicator = EmaIndicator::new(
            self.ema_signal.ema1_length,
            self.ema_signal.ema2_length,
            self.ema_signal.ema3_length,
            self.ema_signal.ema4_length,
            self.ema_signal.ema5_length,
        );
        //volume
        let mut volume_ratio_indicator =
            VolumeRatioIndicator::new(self.volume_signal.volume_bar_num);

        //rsi
        let mut rsi_indicator = Rsi::new(self.rsi_signal.rsi_length);
        //bollinger
        let mut bollinger_indicator = BollingerBands::new(
            self.bollinger_signal.period,
            self.bollinger_signal.multiplier,
        )
        .unwrap();

        for price in prices {
            let data_item = DataItem::builder()
                .open(price.o.parse().unwrap())
                .high(price.h.parse().unwrap())
                .low(price.l.parse().unwrap())
                .close(price.c.parse().unwrap())
                .volume(price.vol.parse().unwrap())
                .build()
                .unwrap();

            //计算ema
            vegas_indicator_signal_value.ema_values =
                self.calculate_ema(&data_item, &mut ema_indicator);

            //计算volume
            vegas_indicator_signal_value.volume_value.volume_ratio =
                volume_ratio_indicator.next(data_item.volume());

            //计算rsi
            vegas_indicator_signal_value.rsi_value.rsi_value =
                rsi_indicator.next(data_item.close());

            //计算bollinger
            let bollinger_value = bollinger_indicator.next(data_item.close());
            vegas_indicator_signal_value.bollinger_value.upper = bollinger_value.upper;
            vegas_indicator_signal_value.bollinger_value.lower = bollinger_value.lower;
            vegas_indicator_signal_value.bollinger_value.middle = bollinger_value.average;

            //
            data_items.push(data_item);
        }

        (data_items, vegas_indicator_signal_value)
    }
    //22
    pub fn get_trade_signal(
        &mut self,
        data: &[CandlesEntity],
        weights: &SignalWeightsConfig,
    ) -> SignalResult {
        let mut signal_result = SignalResult {
            should_buy: false,
            should_sell: false,
            price: 0.0,
            ts: 0,
            single_detail: None,
        };

        // 转换数据
        let (data_items, vegas_indicator_signal_value) =
            self.get_data_items_and_ema_values(&data.to_vec());

        let current_price = data.last().unwrap().c.parse::<f64>().unwrap();
        let lower_price = data.last().unwrap().l.parse::<f64>().unwrap();
        signal_result.price = current_price;
        signal_result.ts = data.last().unwrap().ts;
        let last_data_item = data.last().unwrap();

        let mut conditions = vec![];
        // 检查ema2被突破
        let (price_above, price_below) = self
            .check_breakthrough_conditions(&data_items, vegas_indicator_signal_value.ema_values);
        if price_above || price_below {
            conditions.push((
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above,
                    price_below,
                },
            ));
        }

        //新增ema排列，回调触碰关键均线位置
        if self.ema_touch_trend_signal.is_open {
            let ema_trend = self
                .calculate_ema_touch_trend(&data_items, vegas_indicator_signal_value.ema_values);
            if ema_trend.is_long_signal || ema_trend.is_short_signal {
                conditions.push((
                    SignalType::EmaTrend,
                    SignalCondition::EmaTouchTrend {
                        is_long_signal: ema_trend.is_long_signal,
                        is_short_signal: ema_trend.is_short_signal,
                    },
                ));
            }
        }

        //成交量
        if self.volume_signal.is_open {
            let res = self.check_volume_trend(&vegas_indicator_signal_value.volume_value);
            conditions.push((
                SignalType::VolumeTrend,
                SignalCondition::Volume {
                    is_increasing: res.is_increasing,
                    ratio: res.volume_ratio,
                },
            ))
        }

        // 计算RSI
        if self.rsi_signal.is_open {
            let current_rsi = self.get_valid_rsi(
                &vegas_indicator_signal_value.rsi_value,
                vegas_indicator_signal_value.ema_values,
            );
            conditions.push((
                SignalType::Rsi,
                SignalCondition::RsiLevel {
                    current: current_rsi,
                    oversold: self.rsi_signal.rsi_oversold,
                    overbought: self.rsi_signal.rsi_overbought,
                    is_valid: true,
                },
            ));
        }

        //判断布林带
        let bollinger_value =
            self.check_bollinger_signal(&data_items, vegas_indicator_signal_value);
        if self.bollinger_signal.is_open {
            conditions.push((
                SignalType::Bollinger,
                SignalCondition::Bollinger {
                    is_long_signal: bollinger_value.is_long_signal,
                    is_short_signal: bollinger_value.is_short_signal,
                    is_close_signal: bollinger_value.is_close_signal,
                },
            ));
        }

        // 检查突破的持续性
        let breakthrough_confirmed = self.check_breakthrough_confirmation(&data_items, price_above);

        //计算振幅
        let k_line_amplitude = self.calculate_k_line_amplitude(&data_items);

        println!("conditions: {:?}", conditions);
        // 计算得分
        let score = weights.calculate_score(conditions.clone());
        // println!("score: {:#?}", score);
        if let Some(signal_direction) = weights.is_signal_valid(&score) {
            match signal_direction {
                SignalDeriect::IsLong => {
                    signal_result.should_buy = true;
                    signal_result.single_detail = Some(json!(score.details).to_string());
                }
                SignalDeriect::IsShort => {
                    signal_result.should_sell = true;
                    signal_result.single_detail = Some(json!(score.details).to_string());
                }
            }
        };

        if signal_result.should_buy || signal_result.should_sell {
            println!(
                "产生信号: {}",
                signal_result.single_detail.as_ref().unwrap()
            );
        }

        signal_result
    }

    //新增函数计算当前k线价格的振幅
    fn calculate_k_line_amplitude(&self, data_items: &[DataItem]) -> f64 {
        let mut amplitude = 0.0;
        if let Some(last_item) = data_items.last() {
            // 计算最高价和最低价之间的差异
            let high = High::high(last_item);
            let low = Low::low(last_item);
            // 使用开盘价作为基准计算振幅百分比
            let open = Open::open(last_item);
            if open != 0.0 {
                // 振幅计算: (最高价 - 最低价) / 开盘价 * 100
                amplitude = (high - low) / open * 100.0;
            }
        }
        amplitude
    }

    //获取有效的rsi
    fn get_valid_rsi(&self, rsi_value: &RsiSignalValue, ema_value: EmaSignalValue) -> f64 {
        let current_rsi = rsi_value.rsi_value;
        return current_rsi;
        // todo 12/19 04:00 rsi超卖，但是不在ema4附近，所以无效
        // todo 12/19 23:00
        // todo 2025-02-25 06:00:00 rsi超卖，但是价格开盘和收盘都低于ema1，所以无法买入
        // todo 2025 03-10 06:00:00 确实有效的rsi id=103
        // todo 2025-01-13 18:00:00
        //如果当前价格是下跌,判断是不是ema4附近，否则为无效rsi
        // if data_items.last().unwrap().close() < ema_value.ema4_value {
        //     current_rsi
        // } else {
        //     return 0.0;
        // }
    }

    // 辅助方法：检查成交量是否显著增加
    // fn check_volume_increase(&self, data: &[DataItem]) -> bool {
    //     if data.len() < 5 { return false; }
    //
    //     let current_volume = data.last().unwrap().volume();  // 使用真实成交量
    //     let avg_volume: f64 = data[data.len() - 6..data.len() - 1].iter().map(|x| x.volume())  // 使用真实成交量.sum::<f64>() / 5.0;
    //
    //     // println!("成交量检查 - 当前: {}, 平均: {}, 倍数: {}", current_volume, avg_volume, current_volume / avg_volume);
    //     current_volume > avg_volume * self.volume_signal.volume_increase_ratio // 倍数大于1.5
    // }
    // 辅助方法：计算EMA趋势
    fn calculate_ema_touch_trend(
        &mut self,
        data: &[DataItem],
        ema_value: EmaSignalValue,
    ) -> EmaTouchTrendSignalValue {
        //判断ema 是否空头排列，或者多头排列或者多头排列
        let mut ema_touch_trend_value = EmaTouchTrendSignalValue::default();
        //todo  优化时间点 2024-12-09 08:00:00
        if ema_value.ema2_value > ema_value.ema3_value
            && ema_value.ema3_value > ema_value.ema4_value
        {
            ema_touch_trend_value.is_uptrend = true;
            //当前ema_vaue_1 >emalue_2 的时候， 价格最低下跌到em2附近的时候，且ema1 与 ema2 相差幅度大于0.012
            if ema_value.ema1_value > ema_value.ema2_value
                && data.last().unwrap().low()
                    <= ema_value.ema2_value * self.ema_touch_trend_signal.price_with_ema_high_ratio
                && ema_value.ema1_value
                    > ema_value.ema2_value * self.ema_touch_trend_signal.ema1_with_ema2_ratio
            {
                ema_touch_trend_value.is_long_signal = true;
            } else {
                // 当开盘价格大于ema4的时候， 当价格下跌接近ema4或者ema5位置时候=>价格接近ema4,ema5均线附近 ,且ema4 乘以一定比例依旧<于ema3=> 说明价格下跌幅度较大
                if ((data.last().unwrap().open() > ema_value.ema4_value)
                    && data.last().unwrap().low()
                        <= ema_value.ema4_value * self.ema_touch_trend_signal.ema3_with_ema4_ratio
                    || data.last().unwrap().low()
                        <= ema_value.ema5_value * self.ema_touch_trend_signal.ema4_with_ema5_ratio)
                    && (ema_value.ema4_value * self.ema_touch_trend_signal.ema3_with_ema4_ratio
                        <= ema_value.ema3_value
                        || ema_value.ema4_value * self.ema_touch_trend_signal.ema4_with_ema5_ratio
                            <= ema_value.ema3_value)
                {
                    ema_touch_trend_value.is_in_uptrend_touch_ema4_ema5_nums += 1;
                    if data.last().unwrap().low() <= ema_value.ema4_value {
                        ema_touch_trend_value.is_in_uptrend_touch_ema4 = true;
                    } else {
                        ema_touch_trend_value.is_in_uptrend_touch_ema5 = true;
                    }
                    ema_touch_trend_value.is_long_signal = true;
                }
            }
        } else if ema_value.ema1_value < ema_value.ema2_value
            && ema_value.ema2_value < ema_value.ema3_value
            && ema_value.ema3_value < ema_value.ema4_value
        {
            ema_touch_trend_value.is_downtrend = true;

            //当前ema_vaue_1 <emalue_2 的时候，价格最高上涨到em2附近的时候且ema1 与 ema2 相差幅度大于0.012
            if data.last().unwrap().high()
                >= ema_value.ema2_value * self.ema_touch_trend_signal.price_with_ema_low_ratio
                && ema_value.ema2_value
                    > ema_value.ema1_value * self.ema_touch_trend_signal.ema1_with_ema2_ratio
            {
                ema_touch_trend_value.is_short_signal = true;
            } else {
                //当价格到达接近ema4或者ema5位置时候,且ema3 与 ema4 或 ema5 相差幅度大于0.09

                //当价格到达接近ema4或者ema5位置时候,且ema3 与 ema4 或 ema5 相差幅度大于0.09
                if ((data.last().unwrap().high()
                    * self.ema_touch_trend_signal.price_with_ema_high_ratio
                    >= ema_value.ema4_value)
                    || (data.last().unwrap().high()
                        * self.ema_touch_trend_signal.price_with_ema_high_ratio
                        >= ema_value.ema5_value))
                    && ((ema_value.ema3_value * self.ema_touch_trend_signal.ema3_with_ema4_ratio
                        < ema_value.ema4_value)
                        || (ema_value.ema3_value
                            * self.ema_touch_trend_signal.ema3_with_ema4_ratio
                            < ema_value.ema5_value))
                {
                    ema_touch_trend_value.is_in_downtrend_touch_ema4_ema5_nums += 1;
                    if data.last().unwrap().high()
                        * self.ema_touch_trend_signal.price_with_ema_high_ratio
                        >= ema_value.ema4_value
                    {
                        ema_touch_trend_value.is_in_downtrend_touch_ema4 = true;
                    } else {
                        ema_touch_trend_value.is_in_downtrend_touch_ema5 = true;
                    }
                    ema_touch_trend_value.is_short_signal = true;
                }
            }
        }
        println!("ema_touch_trend_value: {:#?}", ema_touch_trend_value);
        ema_touch_trend_value
    }

    // 检查突破信号
    fn check_breakout_signals(
        &self,
        price: f64,
        ema2: f64,
        ema3: f64,
        trend: &EmaTouchTrendSignalValue,
        volume_increase: bool,
    ) -> bool {
        let price_above_ema2 = price > ema2;
        let price_below_ema3 = price < ema3;
        // 简化判断条件
        price_above_ema2 || price_below_ema3
    }

    /// Runs the backtest asynchronously.
    pub fn run_test(
        &mut self,
        candles: &Vec<CandlesEntity>,
        fib_levels: &Vec<f64>,
        strategy_config: TradingStrategyConfig,
        is_fibonacci_profit: bool,
        is_open_long: bool,
        is_open_short: bool,
        is_judge_trade_time: bool,
    ) -> BackTestResult {
        let min_length = self.get_min_data_length();
        strategy_common::run_test(
            |candles| self.get_trade_signal(candles, &SignalWeightsConfig::default()),
            candles,
            fib_levels,
            strategy_config,
            min_length,
            is_fibonacci_profit,
            is_open_long,
            is_open_short,
        )
    }
    // 修改：计算趋势强度，使用EMA12的短期趋势
    fn calculate_trend_strength(&mut self, data: &[DataItem]) -> f64 {
        const TREND_LOOKBACK: usize = 5; // 看最近5根K线的趋势

        if data.len() < TREND_LOOKBACK + self.ema_signal.ema1_length {
            return 0.5;
        }

        // 计算包含足够历史数据的EMA序列
        let calc_range = &data[data.len() - TREND_LOOKBACK - self.ema_signal.ema1_length..];
        let mut ema1 = ExponentialMovingAverage::new(self.ema_signal.ema1_length).unwrap();
        let mut ema1_values = Vec::new();

        // 先计算EMA初始值
        let sma1: f64 = calc_range[0..self.ema_signal.ema1_length]
            .iter()
            .map(|x| x.close())
            .sum::<f64>()
            / self.ema_signal.ema1_length as f64;

        ema1_values.push(sma1);

        // 连续计算EMA值
        for i in self.ema_signal.ema1_length..calc_range.len() {
            let ema_value = ema1.next(calc_range[i].close());
            ema1_values.push(ema_value);
        }

        // 只取最后TREND_LOOKBACK个值计算趋势
        let trend_values = &ema1_values[ema1_values.len() - TREND_LOOKBACK..];

        // 计算EMA12的角度（斜率）
        let ema1_angle = (trend_values.last().unwrap() - trend_values.first().unwrap())
            / trend_values.first().unwrap();

        // 计算当前价格与EMA12的距离
        let current_price = data.last().unwrap().close();
        let price_distance =
            (current_price - trend_values.last().unwrap()).abs() / trend_values.last().unwrap();

        println!(
            "趋势角度分析 - EMA12角度: {:.4}, 价格距离: {:.4}",
            ema1_angle, price_distance
        );
        println!("EMA12序列: {:?}", trend_values);

        // 综合评分 (0.0-1.0)
        let strength = (ema1_angle.abs() * 0.8 + (1.0 - price_distance) * 0.2)
            .max(0.0)
            .min(1.0);

        strength
    }

    // 新增：检查突破确认
    fn check_breakthrough_confirmation(&self, data: &[DataItem], is_upward: bool) -> bool {
        // 实现突破确认逻辑
        // 可以检查:
        // 1. 突破后的持续性
        // 2. 回测支撑/阻力的表现
        // 3. 成交量配合
        true // 临时返回值
    }

    // 新增：计算动态回调幅度
    fn calculate_dynamic_pullback_threshold(&self, _data: &[DataItem]) -> f64 {
        // 实现动态回调幅度计算逻辑
        // 可以考虑:
        // 1. 价格波动性
        // 2. 均线角度
        // 3. 成交量变化
        // 返回回调幅度
        0.005 // 临时返回值
    }

    // 修改成交量趋势判断
    fn check_volume_trend(&self, volume_trend: &VolumeTrendSignalValue) -> VolumeTrendSignalValue {
        VolumeTrendSignalValue {
            is_increasing: volume_trend.volume_ratio > self.volume_signal.volume_increase_ratio, // 放量
            is_decreasing: volume_trend.volume_ratio < self.volume_signal.volume_decrease_ratio, // 缩量
            is_stable: volume_trend.volume_ratio >= self.volume_signal.volume_decrease_ratio
                && volume_trend.volume_ratio <= self.volume_signal.volume_increase_ratio, // 稳定
            volume_ratio: volume_trend.volume_ratio,
        }
    }

    // todo 优化：检查关键价位买入和卖出信号
    fn check_key_price_level_sell(
        &self,
        current_price: f64,
        volume_trend: &VolumeTrendSignalValue,
    ) -> Option<String> {
        // 定义价位级别和对应的提前预警距离
        const PRICE_LEVELS: [(f64, f64, f64, &str); 8] = [
            // (价位区间, 提前预警百分比, 建议回撤百分比, 级别描述)
            (10000.0, 0.02, 0.015, "万元"), // 万元级别
            (1000.0, 0.015, 0.01, "千元"),  // 千元级别
            (100.0, 0.01, 0.008, "百元"),   // 百元级别
            (10.0, 0.008, 0.005, "十元"),   // 十元级别
            (1.0, 0.005, 0.003, "元"),      // 1元级别
            (0.1, 0.003, 0.002, "角"),      // 0.1元级别
            (0.01, 0.002, 0.001, "分"),     // 0.01元级别
            (0.001, 0.001, 0.0005, "厘"),   // 0.001元级别
        ];

        // 修改：从大到小遍历找到第一个小于等于当前价格的级别
        let (interval, alert_percent, pullback_percent, level_name) = PRICE_LEVELS
            .iter()
            .find(|&&(level, _, _, _)| current_price >= level)
            .unwrap_or(&(0.001, 0.001, 0.0005, "微"));

        // 计算下一个关键价位（根据价格级别调整精度）
        let price_unit = if *interval >= 1.0 {
            *interval / 10.0 // 对于大于1元的价格，使用十分之一作为单位
        } else {
            *interval // 对于小于1元的价格，使用当前区间作为单位
        };

        let next_key_level = if *interval >= 1.0 {
            let magnitude = 10f64.powi((*interval as f64).log10().floor() as i32);
            (*interval / magnitude).floor() * magnitude
        } else {
            let magnitude = 10f64.powi((1.0 / *interval as f64).log10().ceil() as i32);
            (*interval * magnitude).floor() / magnitude
        };
        let distance_to_key = next_key_level - current_price;
        let alert_distance = next_key_level * alert_percent;

        println!(
            "价位分析 - 当前价格: {:.4}, 下一关键位: {:.4}, 距离: {:.4}, 预警距离: {:.4} [{}级别]",
            current_price, next_key_level, distance_to_key, alert_distance, level_name
        );

        // 如果接近关键价位且成交量增加，生成卖出信号
        if distance_to_key > 0.0 && distance_to_key < alert_distance && volume_trend.is_increasing {
            // 动态计算建议卖出价格
            let suggested_sell_price = if *interval >= 1.0 {
                // 大额价格使用百分比回撤
                next_key_level * (1.0 - pullback_percent)
            } else {
                // 小额价格使用固定点位回撤
                next_key_level - (price_unit * pullback_percent)
            };

            // 根据价格级别确定信号类型
            let signal_type = if *interval >= 100.0 {
                "重要"
            } else {
                "普通"
            };

            println!("价位分析详情:");
            println!("  价格级别: {} (区间: {:.4})", level_name, interval);
            println!("  预警比例: {:.2}%", alert_percent * 100.0);
            println!("  建议回撤: {:.2}%", pullback_percent * 100.0);
            println!("  建议卖价: {:.4}", suggested_sell_price);

            let format_str = if *interval >= 1.0 {
                format!(
                        "{}价位卖出信号: 当前价格({:.2})接近{}级别关键位({:.2})，建议在{:.2}卖出 [回撤{:.1}%]",
                        signal_type, current_price, level_name, next_key_level, suggested_sell_price,
                        pullback_percent * 100.0
                    )
            } else {
                format!(
                        "{}价位卖出信号: 当前价格({:.4})接近{}级别关键位({:.4})，建议在{:.4}卖出 [回撤{:.2}%]",
                        signal_type, current_price, level_name, next_key_level, suggested_sell_price,
                        pullback_percent * 100.0
                    )
            };

            return Some(format_str);
        }

        None
    }

    // 新增方法：检查突破条件
    fn check_breakthrough_conditions(
        &self,
        data: &[DataItem],
        ema_value: EmaSignalValue,
    ) -> (bool, bool) {
        if data.len() < 2 {
            return (false, false);
        }
        let current_price = data.last().unwrap().close();
        let prev_price = data[data.len() - 2].close();

        // 向上突破条件：当前价格突破ema2上轨，且前一根K线价格低于EMA2
        let price_above = current_price
            > ema_value.ema2_value * (1.0 + self.ema_signal.ema_breakthrough_threshold)
            && prev_price < ema_value.ema2_value;

        // 向下突破条件：当前价格突破ema2下轨，且前一根K线价格高于EMA2
        //todo  优化时间点k线：2025-02-19 22:00:00
        //todo  优化时间点k线路 2025-03-07 08:00:00
        let mut price_below = false;
        if (current_price < ema_value.ema1_value
            && current_price
                < ema_value.ema2_value * (1.0 - self.ema_signal.ema_breakthrough_threshold)
            && prev_price > ema_value.ema2_value)
            || (current_price
                < ema_value.ema5_value * (1.0 - self.ema_signal.ema_breakthrough_threshold)
                && prev_price > ema_value.ema5_value)
        {
            price_below = true;
        }

        (price_above, price_below)
    }

    //检查布林带信号
    fn check_bollinger_signal(
        &mut self,
        data_items: &[DataItem],
        vegas_indicator_signal_value: VegasIndicatorSignalValue,
    ) -> BollingerSignalValue {
        //todo 成功示例  2024-11-30 14:00:00, 2024-11-30 14:00:00, 2024-12-07 22:00:00
        //todo 错误示例  2024-12-06 21:00:00
        //todo 考虑在上升时期的时候，因为布林带做空，止盈到价格触碰到布林带中轨平仓
        //todo 考虑在k线收盘之后，价格依然低于布林带下轨位置，如果有多单则平仓
        let mut bollinger_value = BollingerSignalValue {
            lower: 0.0,
            upper: 0.0,
            middle: 0.0,
            is_long_signal: false,
            is_short_signal: false,
            is_close_signal: false,
        };
        if self.bollinger_signal.is_open {
            let ema_signal_values = vegas_indicator_signal_value.ema_values;
            let mut bolling_bands = vegas_indicator_signal_value.bollinger_value;
            println!("bolling_bands: {:?}", bolling_bands);

            //如果ema是多头排列 则当触达ema下轨的时候可以开多，当触达ema上轨的时候可以平仓，但是不能开空单
            if ema_signal_values.ema1_value > ema_signal_values.ema2_value
                && ema_signal_values.ema2_value > ema_signal_values.ema3_value
                && ema_signal_values.ema3_value > ema_signal_values.ema4_value
            {
                if bollinger_value.lower > data_items.last().unwrap().low() {
                    bollinger_value.is_long_signal = true;
                }
                if bollinger_value.upper < data_items.last().unwrap().high() {
                    bollinger_value.is_short_signal = true;
                }
            }
            if ema_signal_values.ema2_value < ema_signal_values.ema3_value
                && ema_signal_values.ema3_value < ema_signal_values.ema4_value
            {
                //示例2025/02/15 01:00:00
                if bollinger_value.lower > data_items.last().unwrap().low() {
                    bollinger_value.is_long_signal = true;
                }
                if bollinger_value.upper < data_items.last().unwrap().high() {
                    bollinger_value.is_short_signal = true;
                }
            }
        }
        bollinger_value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_price_level_sell() {
        // 测试不同价格区间的情况
        let test_cases = vec![
            // (当前价格, 期望的关键价位, 期望包含的文本)
            (9980.0, 9000.0, "万元级别"),
            (1990.0, 1000.0, "千元级别"),
            (198.0, 100.0, "百元级别"),
            (19.95, 10.0, "十元级别"),
            (1.98, 1.0, "元级别"),
            (0.098, 0.09, "角级别"),
            (0.0098, 0.009, "分级别"),
            (0.00098, 0.0009, "厘级别"),
        ];

        for (price, expected_level, expected_text) in test_cases {
            let key_level = if price >= 1.0 {
                let magnitude = 10f64.powi((price as f64).log10().floor() as i32);
                (price / magnitude).floor() * magnitude
            } else {
                let magnitude = 10f64.powi((1.0 / price as f64).log10().ceil() as i32);
                (price * magnitude).floor() / magnitude
            };

            assert_eq!(
                key_level, expected_level,
                "价格 {} 的关键价位应该是 {}",
                price, expected_level
            );
            println!(
                "价格 {} 的关键价位是 {} [{}]",
                price, key_level, expected_text
            );
        }
    }
}
