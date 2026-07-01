use crate::strategy_common::SignalResult;
use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};

/// SuperTrend策略：TradingView最热门的趋势跟随指标
///
/// 原理：基于ATR构建动态支撑/阻力带，价格突破带线时产生信号。
/// - 上涨趋势：价格在绿线上方，持有多单
/// - 下跌趋势：价格在红线下方，持有空单
/// - 信号产生：线颜色翻转时（绿→红 或 红→绿）
///
/// 来源：TradingView社区最高点赞策略之一
/// 参考：https://mudrex.com/learn/supertrend-indicator/
///       https://pineify.app/resources/blog/supertrend-indicator-complete-guide-to-signals-settings-and-proven-strategies

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SuperTrendDirection {
    Up,   // 绿线，做多
    Down, // 红线，做空
    Flat, // 未初始化
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SuperTrendAction {
    Long,  // 翻绿，买入
    Short, // 翻红，卖出
    Hold,  // 维持当前趋势
    Flat,  // 无信号
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperTrendThresholds {
    /// ATR周期（标准10）
    pub atr_period: usize,
    /// ATR倍数（标准3.0）
    pub atr_multiplier: f64,
    /// 止盈倍数（相对ATR）
    pub take_profit_atr_mult: f64,
    /// 是否允许做空
    pub allow_short: bool,
    /// 是否允许做多
    pub allow_long: bool,
}

impl Default for SuperTrendThresholds {
    fn default() -> Self {
        Self {
            atr_period: 10,
            atr_multiplier: 3.0,
            take_profit_atr_mult: 2.0,
            allow_short: true,
            allow_long: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperTrendSignalSnapshot {
    pub price: f64,
    pub atr: f64,
    pub supertrend_line: f64, // SuperTrend线当前值
    pub current_direction: SuperTrendDirection,
    pub prev_direction: SuperTrendDirection,
    pub basic_band: f64, // (H+L)/2
    pub upper_band: f64, // 基础带 + ATR×倍数
    pub lower_band: f64, // 基础带 - ATR×倍数
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperTrendDecision {
    pub action: SuperTrendAction,
    pub reasons: Vec<String>,
}

impl SuperTrendDecision {
    pub fn to_signal(
        &self,
        snapshot: &SuperTrendSignalSnapshot,
        thresholds: &SuperTrendThresholds,
    ) -> SignalResult {
        let mut signal = SignalResult::default();
        signal.ts = chrono::Utc::now().timestamp_millis();
        signal.open_price = snapshot.price;

        match self.action {
            SuperTrendAction::Long => {
                signal.should_buy = true;
                signal.direction = SignalDirection::Long;
                // 止损：SuperTrend线
                signal.signal_kline_stop_loss_price = Some(snapshot.supertrend_line);
                signal.stop_loss_source = Some("SuperTrend".to_string());
                // 止盈：当前价 + N倍ATR
                let target = snapshot.price + snapshot.atr * thresholds.take_profit_atr_mult;
                signal.atr_take_profit_level_1 = Some(target);
                signal.atr_take_profit_level_2 = Some(target);
                signal.atr_take_profit_level_3 = Some(target);
            }
            SuperTrendAction::Short => {
                signal.should_sell = true;
                signal.direction = SignalDirection::Short;
                signal.signal_kline_stop_loss_price = Some(snapshot.supertrend_line);
                signal.stop_loss_source = Some("SuperTrend".to_string());
                let target = snapshot.price - snapshot.atr * thresholds.take_profit_atr_mult;
                signal.atr_take_profit_level_1 = Some(target);
                signal.atr_take_profit_level_2 = Some(target);
                signal.atr_take_profit_level_3 = Some(target);
            }
            _ => {}
        }

        signal
    }
}

/// 回测调参配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SuperTrendBacktestTuning {
    pub atr_period: usize,
    pub atr_multiplier: f64,
    pub take_profit_atr_mult: f64,
    pub allow_short: bool,
    pub allow_long: bool,
}

impl Default for SuperTrendBacktestTuning {
    fn default() -> Self {
        let t = SuperTrendThresholds::default();
        Self {
            atr_period: t.atr_period,
            atr_multiplier: t.atr_multiplier,
            take_profit_atr_mult: t.take_profit_atr_mult,
            allow_short: t.allow_short,
            allow_long: t.allow_long,
        }
    }
}

impl SuperTrendBacktestTuning {
    pub fn thresholds(&self) -> SuperTrendThresholds {
        SuperTrendThresholds {
            atr_period: self.atr_period,
            atr_multiplier: self.atr_multiplier,
            take_profit_atr_mult: self.take_profit_atr_mult,
            allow_short: self.allow_short,
            allow_long: self.allow_long,
        }
    }
}
