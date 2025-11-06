use serde::{Deserialize, Serialize};
use ta::indicators::BollingerBands;

use crate::trading::indicator::sma::Sma;
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::CandleItem;
use ta::{DataItem, High, Low, Next};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BollingBandsSignalConfig {
    pub period: usize,
    pub multiplier: f64,
    pub is_open: bool,
    pub consecutive_touch_times: usize,
}

impl Default for BollingBandsSignalConfig {
    fn default() -> Self {
        Self {
            period: 13,
            multiplier: 2.5,
            is_open: true,
            consecutive_touch_times: 4,
        }
    }
}

///布林带加强版
#[derive(Debug, Clone, Default)]
pub struct BollingBandsPlusIndicator {
    //布林带
    pub bollinger_bands: BollingerBands,
    //连续触达次数
    pub consecutive_touch_up_times: usize,
    //连续触达下轨次数
    pub consecutive_touch_down_times: usize,
    // 保存周期以供回看窗口动态计算
    pub period: usize,
}

/// 布林带加强版输出
#[derive(Debug, Clone, Default)]
pub struct BollingBandsPlusIndicatorOutput {
    pub upper: f64,
    pub lower: f64,
    pub average: f64,
    pub consecutive_touch_times: usize,
}

/// 布林带加强版
impl BollingBandsPlusIndicator {
    pub fn new(period: usize, multiplier: f64, consecutive_touch_times: usize) -> Self {
        Self {
            bollinger_bands: BollingerBands::new(period, multiplier).unwrap(),
            consecutive_touch_up_times: consecutive_touch_times,
            consecutive_touch_down_times: consecutive_touch_times,
            period,
        }
    }
}

/// 布林带加强版
impl Next<&CandleItem> for BollingBandsPlusIndicator {
    type Output = BollingBandsPlusIndicatorOutput;
    fn next(&mut self, input: &CandleItem) -> Self::Output {
        let bollinger_bands_output = self.bollinger_bands.next(input.c);
        if input.h > bollinger_bands_output.upper {
            self.consecutive_touch_up_times += 1;
        } else {
            self.consecutive_touch_up_times = 0;
        }

        if input.l < bollinger_bands_output.lower {
            self.consecutive_touch_down_times += 1;
        } else {
            self.consecutive_touch_down_times = 0;
        }

        let mut output = BollingBandsPlusIndicatorOutput {
            upper: bollinger_bands_output.upper,
            lower: bollinger_bands_output.lower,
            average: bollinger_bands_output.average,
            consecutive_touch_times: self.consecutive_touch_up_times,
        };
        output
    }
}
