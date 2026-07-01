use crate::strategy_common::SignalResult;
use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};

/// RSI Divergence策略：TradingView高胜率反转策略
///
/// 原理：价格与RSI指标的背离预示趋势反转
/// - 看涨背离(Bullish Divergence)：价格新低 + RSI未新低 → 做多
/// - 看跌背离(Bearish Divergence)：价格新高 + RSI未新高 → 做空
///
/// 优势：在震荡市和熊市中捕捉反转，胜率可达68-87%
///
/// 来源：https://www.tradingview.com/script/qSLcZSyw-RSI-Divergence-Indicator-strategy/
///       https://www.tradealgo.com/trading-guides/technical-analysis/divergence-trading-how-rsi-and-macd-divergences-signal-major-reversals

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum DivergenceType {
    BullishRegular, // 常规看涨背离：价格新低，RSI不创新低
    BearishRegular, // 常规看跌背离：价格新高，RSI不创新高
    BullishHidden,  // 隐藏看涨背离：价格高点，RSI新低（趋势延续）
    BearishHidden,  // 隐藏看跌背离：价格低点，RSI新高（趋势延续）
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DivergenceAction {
    Long,  // 看涨背离，买入
    Short, // 看跌背离，卖出
    Flat,  // 无背离
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiDivergenceThresholds {
    /// RSI周期（标准14）
    pub rsi_period: usize,
    /// 回看周期：检测N根K线内的背离
    pub lookback_period: usize,
    /// RSI超买阈值（标准70）
    pub rsi_overbought: f64,
    /// RSI超卖阈值（标准30）
    pub rsi_oversold: f64,
    /// 止盈倍数（相对ATR）
    pub take_profit_atr_mult: f64,
    /// 止损倍数（相对ATR）
    pub stop_loss_atr_mult: f64,
    /// 是否启用隐藏背离
    pub enable_hidden_divergence: bool,
    /// 是否允许做空
    pub allow_short: bool,
    /// 是否允许做多
    pub allow_long: bool,
}

impl Default for RsiDivergenceThresholds {
    fn default() -> Self {
        Self {
            rsi_period: 14,
            lookback_period: 14,
            rsi_overbought: 70.0,
            rsi_oversold: 30.0,
            take_profit_atr_mult: 2.0,
            stop_loss_atr_mult: 1.5,
            enable_hidden_divergence: false,
            allow_short: true,
            allow_long: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiDivergenceSignalSnapshot {
    pub price: f64,
    pub rsi: f64,
    pub atr: f64,
    pub divergence_type: DivergenceType,
    pub price_low_idx: usize,    // 价格低点位置
    pub price_high_idx: usize,   // 价格高点位置
    pub rsi_low_idx: usize,      // RSI低点位置
    pub rsi_high_idx: usize,     // RSI高点位置
    pub current_price_low: f64,  // 当前价格低点
    pub current_price_high: f64, // 当前价格高点
    pub prev_price_low: f64,     // 前一价格低点
    pub prev_price_high: f64,    // 前一价格高点
    pub current_rsi_low: f64,    // 当前RSI低点
    pub current_rsi_high: f64,   // 当前RSI高点
    pub prev_rsi_low: f64,       // 前一RSI低点
    pub prev_rsi_high: f64,      // 前一RSI高点
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiDivergenceDecision {
    pub action: DivergenceAction,
    pub reasons: Vec<String>,
}

impl RsiDivergenceDecision {
    pub fn to_signal(
        &self,
        snapshot: &RsiDivergenceSignalSnapshot,
        thresholds: &RsiDivergenceThresholds,
        ts: i64,
    ) -> SignalResult {
        let mut signal = SignalResult::default();
        signal.ts = ts;
        signal.open_price = snapshot.price;

        match self.action {
            DivergenceAction::Long => {
                signal.should_buy = true;
                signal.direction = SignalDirection::Long;
                // 止损：当前低点下方
                let stop =
                    snapshot.current_price_low - snapshot.atr * thresholds.stop_loss_atr_mult;
                signal.signal_kline_stop_loss_price = Some(stop);
                signal.stop_loss_source = Some("RSI_Divergence".to_string());
                // 止盈：当前价 + N倍ATR
                let target = snapshot.price + snapshot.atr * thresholds.take_profit_atr_mult;
                signal.atr_take_profit_level_1 = Some(target);
                signal.atr_take_profit_level_2 = Some(target);
                signal.atr_take_profit_level_3 = Some(target);
            }
            DivergenceAction::Short => {
                signal.should_sell = true;
                signal.direction = SignalDirection::Short;
                let stop =
                    snapshot.current_price_high + snapshot.atr * thresholds.stop_loss_atr_mult;
                signal.signal_kline_stop_loss_price = Some(stop);
                signal.stop_loss_source = Some("RSI_Divergence".to_string());
                let target = snapshot.price - snapshot.atr * thresholds.take_profit_atr_mult;
                signal.atr_take_profit_level_1 = Some(target);
                signal.atr_take_profit_level_2 = Some(target);
                signal.atr_take_profit_level_3 = Some(target);
            }
            DivergenceAction::Flat => {}
        }

        signal
    }
}

/// 回测调参配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RsiDivergenceBacktestTuning {
    pub rsi_period: usize,
    pub lookback_period: usize,
    pub rsi_overbought: f64,
    pub rsi_oversold: f64,
    pub take_profit_atr_mult: f64,
    pub stop_loss_atr_mult: f64,
    pub atr_period: usize,
    pub enable_hidden_divergence: bool,
    pub allow_short: bool,
    pub allow_long: bool,
}

impl Default for RsiDivergenceBacktestTuning {
    fn default() -> Self {
        let t = RsiDivergenceThresholds::default();
        Self {
            rsi_period: t.rsi_period,
            lookback_period: t.lookback_period,
            rsi_overbought: t.rsi_overbought,
            rsi_oversold: t.rsi_oversold,
            take_profit_atr_mult: t.take_profit_atr_mult,
            stop_loss_atr_mult: t.stop_loss_atr_mult,
            atr_period: 14,
            enable_hidden_divergence: t.enable_hidden_divergence,
            allow_short: t.allow_short,
            allow_long: t.allow_long,
        }
    }
}

impl RsiDivergenceBacktestTuning {
    pub fn thresholds(&self) -> RsiDivergenceThresholds {
        RsiDivergenceThresholds {
            rsi_period: self.rsi_period,
            lookback_period: self.lookback_period,
            rsi_overbought: self.rsi_overbought,
            rsi_oversold: self.rsi_oversold,
            take_profit_atr_mult: self.take_profit_atr_mult,
            stop_loss_atr_mult: self.stop_loss_atr_mult,
            enable_hidden_divergence: self.enable_hidden_divergence,
            allow_short: self.allow_short,
            allow_long: self.allow_long,
        }
    }
}
