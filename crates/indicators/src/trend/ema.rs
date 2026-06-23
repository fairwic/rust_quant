use crate::trend::vegas::signal::EmaSignalValue;
use ta::indicators::ExponentialMovingAverage;
#[derive(Debug, Clone)]
pub struct EmaIndicator {
    /// 第 1 条 EMA 指标值。
    pub ema1_indicator: ExponentialMovingAverage,
    /// 第 2 条 EMA 指标值。
    pub ema2_indicator: ExponentialMovingAverage,
    /// 第 3 条 EMA 指标值。
    pub ema3_indicator: ExponentialMovingAverage,
    /// 第 4 条 EMA 指标值。
    pub ema4_indicator: ExponentialMovingAverage,
    /// 第 5 条 EMA 指标值。
    pub ema5_indicator: ExponentialMovingAverage,
    /// 第 6 条 EMA 指标值。
    pub ema6_indicator: ExponentialMovingAverage,
    /// 第 7 条 EMA 指标值。
    pub ema7_indicator: ExponentialMovingAverage,
    // 保存周期以供回看窗口动态计算
    pub ema1_length: usize,
    /// 第 2 条 EMA 的计算周期。
    pub ema2_length: usize,
    /// 第 3 条 EMA 的计算周期。
    pub ema3_length: usize,
    /// 第 4 条 EMA 的计算周期。
    pub ema4_length: usize,
    /// 第 5 条 EMA 的计算周期。
    pub ema5_length: usize,
    /// 第 6 条 EMA 的计算周期。
    pub ema6_length: usize,
    /// 第 7 条 EMA 的计算周期。
    pub ema7_length: usize,
    /// 上一根K线的EMA数值，供交叉检测使用
    pub last_signal_value: Option<EmaSignalValue>,
}
impl EmaIndicator {
    /// 构建 回测与策略研究 所需实例，并集中初始化依赖和默认状态。
    pub fn new(
        ema1: usize,
        ema2: usize,
        ema3: usize,
        ema4: usize,
        ema5: usize,
        ema6: usize,
        ema7: usize,
    ) -> Self {
        Self {
            ema1_indicator: ExponentialMovingAverage::new(ema1).unwrap(),
            ema2_indicator: ExponentialMovingAverage::new(ema2).unwrap(),
            ema3_indicator: ExponentialMovingAverage::new(ema3).unwrap(),
            ema4_indicator: ExponentialMovingAverage::new(ema4).unwrap(),
            ema5_indicator: ExponentialMovingAverage::new(ema5).unwrap(),
            ema6_indicator: ExponentialMovingAverage::new(ema6).unwrap(),
            ema7_indicator: ExponentialMovingAverage::new(ema7).unwrap(),
            ema1_length: ema1,
            ema2_length: ema2,
            ema3_length: ema3,
            ema4_length: ema4,
            ema5_length: ema5,
            ema6_length: ema6,
            ema7_length: ema7,
            last_signal_value: None,
        }
    }
    /// 获取 EMA 指标所需的最大周期
    pub fn max_period(&self) -> usize {
        [
            self.ema1_length,
            self.ema2_length,
            self.ema3_length,
            self.ema4_length,
            self.ema5_length,
            self.ema6_length,
            self.ema7_length,
        ]
        .iter()
        .max()
        .unwrap_or(&0)
        .to_owned()
    }
}
