use crate::strategy_common::SignalResult;
use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};

/// Bollinger Bands + RSI组合策略：TradingView经典高胜率策略
///
/// 原理：
/// - 布林带识别超买超卖区域
/// - RSI确认动能衰竭
/// - 双重确认提高胜率
///
/// 入场条件：
/// - 做多: 价格 < 下轨 AND RSI < 超卖阈值
/// - 做空: 价格 > 上轨 AND RSI > 超买阈值
///
/// 优势：
/// - 在震荡市和趋势反转中表现优异
/// - TradingView上有数千个成功案例
/// - 数学基础扎实，不易过拟合

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum BbRsiAction {
    Long,  // 布林带下轨+RSI超卖 → 做多
    Short, // 布林带上轨+RSI超买 → 做空
    Flat,  // 无信号
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BbRsiThresholds {
    /// 布林带周期（标准20）
    pub bb_period: usize,
    /// 布林带标准差倍数（标准2.0）
    pub bb_std_dev: f64,
    /// RSI周期（标准14）
    pub rsi_period: usize,
    /// RSI超买阈值（标准70）
    pub rsi_overbought: f64,
    /// RSI超卖阈值（标准30）
    pub rsi_oversold: f64,
    /// 止盈倍数（相对ATR）
    pub take_profit_atr_mult: f64,
    /// 止损倍数（相对ATR）
    pub stop_loss_atr_mult: f64,
    /// 是否允许做空
    pub allow_short: bool,
    /// 是否允许做多
    pub allow_long: bool,
    /// 价格必须突破布林带多少百分比才算触发
    pub bb_breakout_pct: f64,
}

impl Default for BbRsiThresholds {
    fn default() -> Self {
        Self {
            bb_period: 20,
            bb_std_dev: 2.0,
            rsi_period: 14,
            rsi_overbought: 70.0,
            rsi_oversold: 30.0,
            take_profit_atr_mult: 2.0,
            stop_loss_atr_mult: 1.5,
            allow_short: true,
            allow_long: true,
            bb_breakout_pct: 0.0, // 触及即可，不要求突破
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BbRsiSignalSnapshot {
    pub price: f64,
    pub rsi: f64,
    pub atr: f64,
    pub bb_upper: f64,          // 布林带上轨
    pub bb_middle: f64,         // 布林带中轨(SMA)
    pub bb_lower: f64,          // 布林带下轨
    pub bb_width: f64,          // 布林带宽度（上轨-下轨）
    pub price_bb_position: f64, // 价格在布林带中的位置 (0=下轨, 0.5=中轨, 1=上轨)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BbRsiDecision {
    pub action: BbRsiAction,
    pub reasons: Vec<String>,
}

impl BbRsiDecision {
    pub fn to_signal(
        &self,
        snapshot: &BbRsiSignalSnapshot,
        thresholds: &BbRsiThresholds,
        ts: i64,
    ) -> SignalResult {
        let mut signal = SignalResult::default();
        signal.ts = ts;
        signal.open_price = snapshot.price;

        match self.action {
            BbRsiAction::Long => {
                signal.should_buy = true;
                signal.direction = SignalDirection::Long;
                // 止损：布林带下轨下方
                let stop = snapshot.bb_lower - snapshot.atr * thresholds.stop_loss_atr_mult;
                signal.signal_kline_stop_loss_price = Some(stop);
                signal.stop_loss_source = Some("BB_Lower".to_string());
                // 止盈：当前价 + N倍ATR 或 布林带中轨
                let target = (snapshot.price + snapshot.atr * thresholds.take_profit_atr_mult)
                    .max(snapshot.bb_middle);
                signal.atr_take_profit_level_1 = Some(target);
                signal.atr_take_profit_level_2 = Some(target);
                signal.atr_take_profit_level_3 = Some(target);
            }
            BbRsiAction::Short => {
                signal.should_sell = true;
                signal.direction = SignalDirection::Short;
                let stop = snapshot.bb_upper + snapshot.atr * thresholds.stop_loss_atr_mult;
                signal.signal_kline_stop_loss_price = Some(stop);
                signal.stop_loss_source = Some("BB_Upper".to_string());
                let target = (snapshot.price - snapshot.atr * thresholds.take_profit_atr_mult)
                    .min(snapshot.bb_middle);
                signal.atr_take_profit_level_1 = Some(target);
                signal.atr_take_profit_level_2 = Some(target);
                signal.atr_take_profit_level_3 = Some(target);
            }
            BbRsiAction::Flat => {}
        }

        signal
    }
}

/// 回测调参配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BbRsiBacktestTuning {
    pub bb_period: usize,
    pub bb_std_dev: f64,
    pub rsi_period: usize,
    pub rsi_overbought: f64,
    pub rsi_oversold: f64,
    pub take_profit_atr_mult: f64,
    pub stop_loss_atr_mult: f64,
    pub atr_period: usize,
    pub allow_short: bool,
    pub allow_long: bool,
    pub bb_breakout_pct: f64,
    pub cooldown_candles: usize, // 冷却期
}

impl Default for BbRsiBacktestTuning {
    fn default() -> Self {
        let t = BbRsiThresholds::default();
        Self {
            bb_period: t.bb_period,
            bb_std_dev: t.bb_std_dev,
            rsi_period: t.rsi_period,
            rsi_overbought: t.rsi_overbought,
            rsi_oversold: t.rsi_oversold,
            take_profit_atr_mult: t.take_profit_atr_mult,
            stop_loss_atr_mult: t.stop_loss_atr_mult,
            atr_period: 14,
            allow_short: t.allow_short,
            allow_long: t.allow_long,
            bb_breakout_pct: t.bb_breakout_pct,
            cooldown_candles: 5, // 默认5根K线冷却
        }
    }
}

impl BbRsiBacktestTuning {
    pub fn thresholds(&self) -> BbRsiThresholds {
        BbRsiThresholds {
            bb_period: self.bb_period,
            bb_std_dev: self.bb_std_dev,
            rsi_period: self.rsi_period,
            rsi_overbought: self.rsi_overbought,
            rsi_oversold: self.rsi_oversold,
            take_profit_atr_mult: self.take_profit_atr_mult,
            stop_loss_atr_mult: self.stop_loss_atr_mult,
            allow_short: self.allow_short,
            allow_long: self.allow_long,
            bb_breakout_pct: self.bb_breakout_pct,
        }
    }
}
