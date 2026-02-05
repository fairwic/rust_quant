use crate::leg_detection_indicator::LegDetectionValue;
use crate::market_structure_indicator::MarketStructureValue;
use serde::{Deserialize, Serialize};

/// 锤子形态信号值
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub struct KlineHammerSignalValue {
    /// 上影线比例
    pub up_shadow_ratio: f64,
    /// 下影线比例
    pub down_shadow_ratio: f64,
    /// 实体比例
    pub body_ratio: f64,
    /// 是否开多信号
    pub is_long_signal: bool,
    /// 是否开空信号
    pub is_short_signal: bool,
    /// 是否是锤子形态
    pub is_hammer: bool,
    /// 是否是上吊线形态
    pub is_hanging_man: bool,
}

/// 吞没形态指标值
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct EngulfingSignalValue {
    /// 是否吞没形态
    pub is_engulfing: bool,
    /// 是否有效吞没形态
    pub is_valid_engulfing: bool,
    /// 实体比例
    pub body_ratio: f64,
}

/// 成交量趋势信号值
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct VolumeTrendSignalValue {
    /// 是否增长,对比上一跟k线路
    pub is_increasing_than_pre: bool,
    /// 是否下降,对比上一跟k线路
    pub is_decreasing_than_pre: bool,
    /// 是否大于指标设置的成交量放大的比例
    pub is_increase_than_ratio: bool,
    /// 成交量比例(当前成交量/前N根K线成交量平均值)
    pub volume_ratio: f64,
    /// 成交量值
    pub volume_value: f64,
}

/// EMA信号值
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EmaSignalValue {
    pub ema1_value: f64,
    pub ema2_value: f64,
    pub ema3_value: f64,
    pub ema4_value: f64,
    pub ema5_value: f64,
    pub ema6_value: f64,
    pub ema7_value: f64,
    /// 是否多头排列
    pub is_long_trend: bool,
    /// 是否空头排列
    pub is_short_trend: bool,
    /// 最近是否发生金叉
    #[serde(default)]
    pub is_golden_cross: bool,
    /// 最近是否发生死叉
    #[serde(default)]
    pub is_death_cross: bool,
}

/// 布林带信号值
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct BollingerSignalValue {
    pub lower: f64,
    pub upper: f64,
    pub middle: f64,
    /// 连续触达上轨/下轨次数
    pub consecutive_touch_times: usize,
    pub is_long_signal: bool,
    pub is_short_signal: bool,
    pub is_close_signal: bool,
    /// 虽然触发了布林带开多，或者开空，但是被过滤了
    pub is_force_filter_signal: bool,
}

/// RSI信号值
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct RsiSignalValue {
    /// RSI值
    pub rsi_value: f64,
    /// 是否超卖
    pub is_oversold: bool,
    /// 是否超买
    pub is_overbought: bool,
}

/// MACD 信号值
/// 用于判断动量方向和过滤逆势交易
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct MacdSignalValue {
    /// MACD 线值 (DIF)
    pub macd_line: f64,
    /// 信号线值 (DEA)
    pub signal_line: f64,
    /// 柱状图值 (MACD - Signal)
    pub histogram: f64,
    /// 是否金叉 (MACD 上穿 Signal)
    pub is_golden_cross: bool,
    /// 是否死叉 (MACD 下穿 Signal)
    pub is_death_cross: bool,
    /// 柱状图是否递增
    pub histogram_increasing: bool,
    /// 柱状图是否递减
    pub histogram_decreasing: bool,
    /// MACD 线是否在零轴上方
    pub above_zero: bool,
    /// 前一根柱状图值（用于判断趋势）
    pub prev_histogram: f64,
    /// 柱状图是否正在改善（企稳）
    /// 对于做多：histogram > prev_histogram（负值变小）
    /// 用于识别触底反弹信号
    pub histogram_improving: bool,
}

/// Fib 回撤入场信号值（趋势回调/反弹入场）
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
pub struct FibRetracementSignalValue {
    pub is_long_signal: bool,
    pub is_short_signal: bool,
    pub swing_high: f64,
    pub swing_low: f64,
    /// 当前价格相对于 swing 区间的位置（0=Low, 1=High）
    pub retracement_ratio: f64,
    /// 是否处于触发区间
    pub in_zone: bool,
    /// 触发区间对应的价格下/上界
    pub fib_price_low: f64,
    pub fib_price_high: f64,
    /// 成交量比值（当前/均量）
    pub volume_ratio: f64,
    pub volume_confirmed: bool,
    /// 大趋势方向
    pub major_bullish: bool,
    pub major_bearish: bool,
    /// 小趋势（腿部）方向
    pub leg_bullish: bool,
    pub leg_bearish: bool,
    /// swing 是否为上涨波段（true=上涨波段后回调, false=下跌波段后反弹）
    pub swing_is_upswing: bool,
    /// 建议止损位（基于 swing high/low + buffer）
    pub suggested_stop_loss: f64,
}

/// EMA趋势信号值
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub struct EmaTouchTrendSignalValue {
    /// 是否多头趋势
    pub is_uptrend: bool,
    /// 是否空头趋势
    pub is_downtrend: bool,
    /// 是否在多头趋势触碰ema2
    pub is_in_uptrend_touch_ema2: bool,
    /// 是否在多头趋势触碰ema3
    pub is_in_uptrend_touch_ema3: bool,
    /// 当前多头趋势中触碰ema2和ema3的次数
    pub is_in_uptrend_touch_ema2_ema3_nums: usize,
    /// 是否在多头趋势触碰ema4
    pub is_in_uptrend_touch_ema4: bool,
    /// 是否在多头趋势触碰ema5
    pub is_in_uptrend_touch_ema5: bool,
    /// 当前多头趋势中触碰ema4和ema5的次数
    pub is_in_uptrend_touch_ema4_ema5_nums: usize,
    /// 是否在空头趋势触碰ema2
    pub is_touch_ema2: bool,
    /// 是否在空头趋势触碰ema3
    pub is_touch_ema3: bool,
    /// 当前空头趋势触碰ema2和ema3的次数
    pub is_ema2_ema3_nums: usize,
    /// 是否在空头趋势触碰ema4
    pub is_touch_ema4: bool,
    /// 是否在空头趋势触碰ema5
    pub is_touch_ema5: bool,
    /// 当前空头趋势中触碰ema4和ema5的次数
    pub is_touch_ema4_ema5_nums: usize,
    /// 是否在空头趋势触碰ema7
    pub is_touch_ema7: bool,
    /// 当前空头趋势中触碰ema7的次数
    pub is_touch_ema7_nums: usize,
    /// 是否多头开仓
    pub is_long_signal: bool,
    /// 是否空头开仓
    pub is_short_signal: bool,
}

/// Vegas指标综合信号值
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct VegasIndicatorSignalValue {
    /// EMA信号配置
    pub ema_values: EmaSignalValue,
    /// 成交量信号配置
    pub volume_value: VolumeTrendSignalValue,
    /// EMA趋势
    pub ema_touch_value: EmaTouchTrendSignalValue,
    /// RSI信号配置
    pub rsi_value: RsiSignalValue,
    /// 布林带信号配置
    pub bollinger_value: BollingerSignalValue,
    /// 吞没形态指标
    pub engulfing_value: EngulfingSignalValue,
    /// 锤子形态指标
    pub kline_hammer_value: KlineHammerSignalValue,
    /// 腿部识别
    #[serde(default)]
    pub leg_detection_value: LegDetectionValue,
    /// 市场结构
    #[serde(default)]
    pub market_structure_value: MarketStructureValue,
    /// EMA距离过滤（新增）
    #[serde(default)]
    pub ema_distance_filter: super::ema_filter::EmaDistanceFilter,
    /// MACD 信号值（新增）
    #[serde(default)]
    pub macd_value: MacdSignalValue,
    /// Fib 回撤入场信号值（新增）
    #[serde(default)]
    pub fib_retracement_value: FibRetracementSignalValue,
}

/// 检查均线交叉
pub struct EmaCross {
    pub is_golden_cross: bool,
    pub is_death_cross: bool,
}

#[cfg(test)]
mod tests {
    use super::VegasIndicatorSignalValue;

    #[test]
    fn vegas_signal_value_includes_market_structure() {
        let value = VegasIndicatorSignalValue::default();
        let json = serde_json::to_value(&value).expect("serialize vegas signal value");
        assert!(json.get("market_structure_value").is_some());
    }
}
