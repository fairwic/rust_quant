use rust_quant_common::types::CandleItem;
use serde::{Deserialize, Serialize};
use ta::indicators::BollingerBands;
use ta::Next;
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BollingBandsSignalConfig {
    /// 计算周期。
    pub period: usize,
    /// multiplier，用于配置运行参数。
    pub multiplier: f64,
    /// 是否处于打开状态。
    pub is_open: bool,
    /// consecutivetouchtimes，用于配置运行参数。
    pub consecutive_touch_times: usize,
}
impl Default for BollingBandsSignalConfig {
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
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
    /// upper，用于交易策略计算。
    pub upper: f64,
    /// lower，用于交易策略计算。
    pub lower: f64,
    /// 平均。
    pub average: f64,
    /// consecutivetouchtimes，用于交易策略计算。
    pub consecutive_touch_times: usize,
}
/// 布林带加强版
impl BollingBandsPlusIndicator {
    /// 构建 回测与策略研究 所需实例，并集中初始化依赖和默认状态。
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
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
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
        BollingBandsPlusIndicatorOutput {
            upper: bollinger_bands_output.upper,
            lower: bollinger_bands_output.lower,
            average: bollinger_bands_output.average,
            consecutive_touch_times: self.consecutive_touch_up_times,
        }
    }
}
