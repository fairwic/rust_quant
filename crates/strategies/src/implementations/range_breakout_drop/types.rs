use crate::strategy_common::SignalResult;
use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// 震荡突破下跌策略的执行动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RangeBreakoutDropAction {
    /// 满足做空条件：震荡结束并向下突破
    Short,
    /// 观望：条件不满足
    Flat,
}

/// 策略决策结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeBreakoutDropDecision {
    pub action: RangeBreakoutDropAction,
    pub reasons: Vec<String>,
    /// 止损价格（做空时使用）
    pub stop_price: Option<f64>,
    /// 止盈目标价格
    pub target_prices: Vec<f64>,
}

impl RangeBreakoutDropDecision {
    /// 将决策转换为信号结果，用于回测和实盘
    pub fn to_signal(self, price: f64, ts: i64) -> SignalResult {
        let is_short_signal = matches!(self.action, RangeBreakoutDropAction::Short);
        let is_filtered =
            matches!(self.action, RangeBreakoutDropAction::Flat) && !self.reasons.is_empty();

        let mut signal = SignalResult {
            open_price: price,
            ts,
            should_buy: false,
            should_sell: is_short_signal,
            direction: if is_short_signal || is_filtered {
                SignalDirection::Short
            } else {
                SignalDirection::Close
            },
            filter_reasons: if is_filtered {
                self.reasons.clone()
            } else {
                vec![]
            },
            // 设置止损价格
            signal_kline_stop_loss_price: self.stop_price,
            // 设置止盈价格（使用第一个目标）
            short_signal_take_profit_price: self.target_prices.first().copied(),
            ..Default::default()
        };
        signal.single_value =
            Some(json!({"action": self.action, "reasons": self.reasons}).to_string());
        signal
    }
}

/// 策略阈值配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RangeBreakoutDropThresholds {
    /// 震荡区间识别窗口（K线数量）
    pub range_lookback_candles: usize,
    /// 震荡区间最大波动幅度（百分比），超过此值不认为是震荡
    pub max_range_volatility_pct: f64,
    /// 震荡区间最小波动幅度（百分比），低于此值说明流动性不足
    pub min_range_volatility_pct: f64,
    /// 突破确认的最小K线实体比例（过滤上下影线较长的十字星）
    pub min_breakout_body_ratio: f64,
    /// 突破确认的最小下跌幅度（相对ATR倍数）
    pub min_breakout_move_atr: f64,
    /// 突破确认的最小成交量倍数（相对震荡期均量）
    pub min_breakout_volume_mult: f64,
    /// 趋势过滤：要求价格低于慢速EMA
    pub require_bearish_ema: bool,
    /// 慢速EMA周期（用于趋势过滤）
    pub slow_ema_period: usize,
    /// 长期趋势EMA周期（用于市场环境过滤，如200）
    pub long_term_ema_period: usize,
    /// 要求价格低于长期EMA（强趋势过滤）
    pub require_below_long_term_ema: bool,
    /// 止损ATR倍数
    pub stop_atr_mult: f64,
    /// 第一止盈目标（R倍数）
    pub target_r_1: f64,
    /// 第二止盈目标（R倍数）
    pub target_r_2: f64,
    /// 第三止盈目标（R倍数）
    pub target_r_3: f64,
    /// ATR计算周期
    pub atr_period: usize,
    /// RSI周期（用于超买过滤）
    pub rsi_period: usize,
    /// RSI超买阈值（突破前RSI不应过低，说明有反弹空间）
    pub rsi_min_before_drop: f64,
}

impl Default for RangeBreakoutDropThresholds {
    fn default() -> Self {
        Self {
            range_lookback_candles: 20,
            max_range_volatility_pct: 3.0,
            min_range_volatility_pct: 0.5,
            min_breakout_body_ratio: 0.55,
            min_breakout_move_atr: 0.8,
            min_breakout_volume_mult: 1.5,
            require_bearish_ema: true,
            slow_ema_period: 50,
            long_term_ema_period: 200,
            require_below_long_term_ema: false,
            stop_atr_mult: 1.5,
            target_r_1: 1.0,
            target_r_2: 2.0,
            target_r_3: 3.5,
            atr_period: 14,
            rsi_period: 14,
            rsi_min_before_drop: 40.0,
        }
    }
}

/// 回测调参配置
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RangeBreakoutDropBacktestTuning {
    /// 震荡区间识别窗口
    pub range_lookback_candles: usize,
    /// 震荡区间最大波动幅度
    pub max_range_volatility_pct: f64,
    /// 震荡区间最小波动幅度
    pub min_range_volatility_pct: f64,
    /// 突破K线最小实体比例
    pub min_breakout_body_ratio: f64,
    /// 突破最小移动ATR倍数
    pub min_breakout_move_atr: f64,
    /// 突破最小成交量倍数
    pub min_breakout_volume_mult: f64,
    /// 是否要求价格低于慢速EMA
    pub require_bearish_ema: bool,
    /// 慢速EMA周期
    pub slow_ema_period: usize,
    /// 长期趋势EMA周期
    pub long_term_ema_period: usize,
    /// 是否要求价格低于长期EMA
    pub require_below_long_term_ema: bool,
    /// 止损ATR倍数
    pub stop_atr_mult: f64,
    /// 第一止盈目标R倍数
    pub target_r_1: f64,
    /// 第二止盈目标R倍数
    pub target_r_2: f64,
    /// 第三止盈目标R倍数
    pub target_r_3: f64,
    /// ATR周期
    pub atr_period: usize,
    /// RSI周期
    pub rsi_period: usize,
    /// RSI最小值
    pub rsi_min_before_drop: f64,
    /// 冷却K线数（避免连续开仓）
    pub cooldown_candles: usize,
    /// 是否允许做空
    pub allow_short: bool,
}

impl Default for RangeBreakoutDropBacktestTuning {
    fn default() -> Self {
        Self {
            range_lookback_candles: 20,
            max_range_volatility_pct: 3.0,
            min_range_volatility_pct: 0.5,
            min_breakout_body_ratio: 0.55,
            min_breakout_move_atr: 0.8,
            min_breakout_volume_mult: 1.5,
            require_bearish_ema: true,
            slow_ema_period: 50,
            long_term_ema_period: 200,
            require_below_long_term_ema: false,
            stop_atr_mult: 1.5,
            target_r_1: 1.0,
            target_r_2: 2.0,
            target_r_3: 3.5,
            atr_period: 14,
            rsi_period: 14,
            rsi_min_before_drop: 40.0,
            cooldown_candles: 6,
            allow_short: true,
        }
    }
}

impl RangeBreakoutDropBacktestTuning {
    pub fn thresholds(&self) -> RangeBreakoutDropThresholds {
        RangeBreakoutDropThresholds {
            range_lookback_candles: self.range_lookback_candles,
            max_range_volatility_pct: self.max_range_volatility_pct,
            min_range_volatility_pct: self.min_range_volatility_pct,
            min_breakout_body_ratio: self.min_breakout_body_ratio,
            min_breakout_move_atr: self.min_breakout_move_atr,
            min_breakout_volume_mult: self.min_breakout_volume_mult,
            require_bearish_ema: self.require_bearish_ema,
            slow_ema_period: self.slow_ema_period,
            long_term_ema_period: self.long_term_ema_period,
            require_below_long_term_ema: self.require_below_long_term_ema,
            stop_atr_mult: self.stop_atr_mult,
            target_r_1: self.target_r_1,
            target_r_2: self.target_r_2,
            target_r_3: self.target_r_3,
            atr_period: self.atr_period,
            rsi_period: self.rsi_period,
            rsi_min_before_drop: self.rsi_min_before_drop,
        }
    }
}

/// 市场快照：包含策略评估所需的所有技术指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeBreakoutDropSignalSnapshot {
    pub exchange: String,
    pub symbol: String,
    pub price: f64,
    /// 震荡区间上边界
    pub range_high: f64,
    /// 震荡区间下边界
    pub range_low: f64,
    /// 震荡区间波动幅度（百分比）
    pub range_volatility_pct: f64,
    /// 是否处于震荡状态
    pub in_ranging_mode: bool,
    /// 当前K线是否突破震荡下边界
    pub breakout_confirmed: bool,
    /// 是否是收盘价突破（true）还是最低价触及（false）
    pub is_close_breakout: bool,
    /// 突破K线实体比例
    pub breakout_body_ratio: f64,
    /// 突破移动距离（ATR倍数）
    pub breakout_move_atr: f64,
    /// 突破成交量倍数（相对震荡期均量）
    pub breakout_volume_mult: f64,
    /// 慢速EMA值
    pub slow_ema: f64,
    /// 价格是否低于慢速EMA（空头趋势）
    pub price_below_ema: bool,
    /// 长期EMA值（如200EMA）
    pub long_term_ema: f64,
    /// 价格是否低于长期EMA（市场整体趋势）
    pub price_below_long_term_ema: bool,
    /// ATR值
    pub atr: f64,
    /// RSI值
    pub rsi: f64,
    /// 当前K线方向（1=阳线，-1=阴线，0=十字星）
    pub candle_direction: i8,
}

/// 辅助函数：四舍五入价格到合理精度
pub fn round_price(price: f64) -> f64 {
    if price >= 1000.0 {
        (price * 100.0).round() / 100.0
    } else if price >= 10.0 {
        (price * 1000.0).round() / 1000.0
    } else {
        (price * 10000.0).round() / 10000.0
    }
}
