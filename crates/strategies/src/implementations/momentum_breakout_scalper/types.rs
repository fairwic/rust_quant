use crate::strategy_common::SignalResult;
use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Momentum Breakout Scalper 的执行动作；v1 面向 BTC/ETH 永续短周期顺势突破回踩。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MomentumBreakoutAction {
    /// 上升趋势中回踩快速 EMA 后恢复的多单。
    Long,
    /// 下降趋势中反抽快速 EMA 后续跌的空单。
    Short,
    /// 任一过滤器不满足时返回观望，并保留过滤原因。
    Flat,
}

/// live/paper 信号层使用的趋势确认与止盈止损门槛。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MomentumBreakoutThresholds {
    /// 趋势强度门槛：(fast_ema-slow_ema)/price 的最小绝对百分比，过滤无趋势震荡。
    pub min_trend_strength_pct: f64,
    /// 回踩深度上限，单位 ATR：价格距快速 EMA 不超过该倍数才视为有效回踩。
    pub max_pullback_atr: f64,
    /// 动量确认：恢复 K 线实体相对自身振幅的最小比例。
    pub min_resume_body_ratio: f64,
    /// 止损宽度，单位 ATR 倍数。
    pub stop_atr_mult: f64,
    /// 第一档止盈，单位 ATR：触达后把止损移到保本，把多数浮盈单变为不亏。
    pub target_atr_mult_1: f64,
    /// 第二档止盈，单位 ATR：触达后把止损移到第一档，锁定部分利润。
    pub target_atr_mult_2: f64,
    /// 第三档止盈，单位 ATR：触达后完全平仓，让趋势单跑出大盈亏比。
    pub target_atr_mult_3: f64,
    /// 最大允许的单根恢复 K 线振幅（按价格百分比），过滤插针追单。
    pub max_entry_amp_pct: f64,
}

impl Default for MomentumBreakoutThresholds {
    fn default() -> Self {
        Self {
            min_trend_strength_pct: 0.08,
            max_pullback_atr: 0.8,
            min_resume_body_ratio: 0.45,
            stop_atr_mult: 1.0,
            target_atr_mult_1: 1.0,
            target_atr_mult_2: 2.5,
            target_atr_mult_3: 5.0,
            max_entry_amp_pct: 1.2,
        }
    }
}

/// 回测调参面；用于研究短周期顺势入场频率与胜率/回撤关系。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MomentumBreakoutBacktestTuning {
    /// 快速 EMA 窗口（回踩锚点）。
    pub fast_ema_period: usize,
    /// 慢速 EMA 窗口（趋势方向）。
    pub slow_ema_period: usize,
    /// ATR 窗口，用于止损止盈、回踩深度与振幅过滤。
    pub atr_period: usize,
    /// 同方向连续开仓后的冷却 K 线数。
    pub cooldown_candles: usize,
    /// 是否允许做空。
    pub allow_short: bool,
    /// 趋势强度门槛百分比。
    pub min_trend_strength_pct: f64,
    /// 回踩深度上限（ATR）。
    pub max_pullback_atr: f64,
    /// 恢复 K 线最小实体比例。
    pub min_resume_body_ratio: f64,
    /// 止损 ATR 倍数。
    pub stop_atr_mult: f64,
    /// 第一档止盈 ATR 倍数（移到保本）。
    pub target_atr_mult_1: f64,
    /// 第二档止盈 ATR 倍数（移到第一档）。
    pub target_atr_mult_2: f64,
    /// 第三档止盈 ATR 倍数（完全平仓）。
    pub target_atr_mult_3: f64,
    /// 最大入场 K 线振幅百分比。
    pub max_entry_amp_pct: f64,
}

impl Default for MomentumBreakoutBacktestTuning {
    fn default() -> Self {
        let thresholds = MomentumBreakoutThresholds::default();
        Self {
            fast_ema_period: 12,
            slow_ema_period: 48,
            atr_period: 14,
            cooldown_candles: 3,
            allow_short: true,
            min_trend_strength_pct: thresholds.min_trend_strength_pct,
            max_pullback_atr: thresholds.max_pullback_atr,
            min_resume_body_ratio: thresholds.min_resume_body_ratio,
            stop_atr_mult: thresholds.stop_atr_mult,
            target_atr_mult_1: thresholds.target_atr_mult_1,
            target_atr_mult_2: thresholds.target_atr_mult_2,
            target_atr_mult_3: thresholds.target_atr_mult_3,
            max_entry_amp_pct: thresholds.max_entry_amp_pct,
        }
    }
}

impl MomentumBreakoutBacktestTuning {
    /// 把调参面投影为 live 共用 thresholds，消除双口径。
    pub fn thresholds(&self) -> MomentumBreakoutThresholds {
        MomentumBreakoutThresholds {
            min_trend_strength_pct: self.min_trend_strength_pct,
            max_pullback_atr: self.max_pullback_atr,
            min_resume_body_ratio: self.min_resume_body_ratio,
            stop_atr_mult: self.stop_atr_mult,
            target_atr_mult_1: self.target_atr_mult_1,
            target_atr_mult_2: self.target_atr_mult_2,
            target_atr_mult_3: self.target_atr_mult_3,
            max_entry_amp_pct: self.max_entry_amp_pct,
        }
    }
}

/// 上游聚合后的顺势信号快照；纯 OHLCV 派生。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MomentumBreakoutSignalSnapshot {
    /// 交易所名称；v1 live 首发只允许 Binance 和 OKX。
    pub exchange: String,
    /// 永续合约交易对，只接受 BTC/ETH。
    pub symbol: String,
    /// 当前评估价格（最新收盘）。
    pub price: f64,
    /// 快速 EMA。
    pub fast_ema: f64,
    /// 慢速 EMA。
    pub slow_ema: f64,
    /// 当前 ATR。
    pub atr: f64,
    /// 价格距快速 EMA 的距离（ATR 倍数，绝对值）。
    pub pullback_atr: f64,
    /// 恢复 K 线实体相对振幅比例。
    pub resume_body_ratio: f64,
    /// 恢复 K 线方向：1=阳线，-1=阴线，0=十字。
    pub resume_direction: i8,
    /// 入场 K 线振幅百分比。
    pub entry_amp_pct: f64,
}

/// 策略评估后的领域决策，先保留原因，再转换为通用 SignalResult。
#[derive(Debug, Clone, PartialEq)]
pub struct MomentumBreakoutDecision {
    /// 多、空或观望动作。
    pub action: MomentumBreakoutAction,
    /// 阻断原因或成交确认原因；同时用于审计和回测诊断。
    pub reasons: Vec<String>,
}

impl MomentumBreakoutDecision {
    /// 检查某个审计原因是否存在。
    pub fn has_reason(&self, reason: &str) -> bool {
        self.reasons.iter().any(|item| item == reason)
    }

    /// 把领域决策转换为回测/live 共用的信号契约。
    pub fn to_signal(&self, price: f64, ts: i64) -> SignalResult {
        let mut signal = SignalResult {
            open_price: price,
            ts,
            filter_reasons: self.reasons.clone(),
            single_result: Some(self.result_payload().to_string()),
            ..Default::default()
        };
        match self.action {
            MomentumBreakoutAction::Long => self.apply_long_signal(&mut signal, price),
            MomentumBreakoutAction::Short => self.apply_short_signal(&mut signal, price),
            MomentumBreakoutAction::Flat => {}
        }
        signal
    }

    fn result_payload(&self) -> Value {
        json!({
            "strategy": "momentum_breakout_scalper_v1",
            "action": self.action_name(),
            "reasons": self.reasons,
        })
    }

    fn action_name(&self) -> &'static str {
        match self.action {
            MomentumBreakoutAction::Long => "long",
            MomentumBreakoutAction::Short => "short",
            MomentumBreakoutAction::Flat => "flat",
        }
    }

    fn apply_long_signal(&self, signal: &mut SignalResult, price: f64) {
        let stop = reason_value(&self.reasons, "STOP_PRICE").unwrap_or(price);
        signal.should_buy = true;
        signal.direction = SignalDirection::Long;
        signal.signal_kline_stop_loss_price = Some(round_price(stop));
        signal.stop_loss_source = Some("MomentumBreakout".to_string());
        // 三档独立止盈：level_1<level_2<level_3，触发 level_1 移到保本、level_2 移到 level_1、
        // level_3 完全平仓，让趋势单跑出大盈亏比而把多数单的下行风险压到保本附近。
        signal.atr_take_profit_level_1 = reason_value(&self.reasons, "TARGET_1").map(round_price);
        signal.atr_take_profit_level_2 = reason_value(&self.reasons, "TARGET_2").map(round_price);
        signal.atr_take_profit_level_3 = reason_value(&self.reasons, "TARGET_3").map(round_price);
    }

    fn apply_short_signal(&self, signal: &mut SignalResult, price: f64) {
        let stop = reason_value(&self.reasons, "STOP_PRICE").unwrap_or(price);
        signal.should_sell = true;
        signal.direction = SignalDirection::Short;
        signal.signal_kline_stop_loss_price = Some(round_price(stop));
        signal.stop_loss_source = Some("MomentumBreakout".to_string());
        signal.atr_take_profit_level_1 = reason_value(&self.reasons, "TARGET_1").map(round_price);
        signal.atr_take_profit_level_2 = reason_value(&self.reasons, "TARGET_2").map(round_price);
        signal.atr_take_profit_level_3 = reason_value(&self.reasons, "TARGET_3").map(round_price);
    }
}

/// 把策略生成价格四舍五入到确定精度。
pub fn round_price(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// 从 reasons 里反解形如 `PREFIX:1234.5` 的数值。
pub fn reason_value(reasons: &[String], prefix: &str) -> Option<f64> {
    reasons
        .iter()
        .find_map(|reason| reason.strip_prefix(prefix)?.strip_prefix(':')?.parse().ok())
}
