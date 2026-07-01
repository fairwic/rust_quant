use crate::strategy_common::SignalResult;
use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Range Reversion Scalper 的执行动作；v1 面向 BTC/ETH 永续短周期均值回归。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RangeReversionAction {
    /// 价格下穿布林下轨 + RSI 超卖 + 区间震荡确认后的多单。
    Long,
    /// 价格上穿布林上轨 + RSI 超买 + 区间震荡确认后的空单。
    Short,
    /// 任一过滤器不满足时返回观望，并保留过滤原因。
    Flat,
}

/// live/paper 信号层使用的均值回归入场门槛与止盈止损（R 倍数）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RangeReversionThresholds {
    /// 布林带回归触发的标准差倍数；价格偏离中轨超过该倍数才视为可回归极值。
    pub band_k: f64,
    /// 多单 RSI 超卖上限；RSI 低于该值才允许做多回归。
    pub rsi_long_max: f64,
    /// 空单 RSI 超买下限；RSI 高于该值才允许做空回归。
    pub rsi_short_min: f64,
    /// 止损宽度，单位为 ATR 倍数；从入场价向不利方向偏移。
    pub stop_atr_mult: f64,
    /// 止盈宽度，单位为 ATR 倍数；落袋目标，三档止盈合并为单一目标。
    pub target_atr_mult: f64,
    /// 趋势过滤：慢速 EMA 相对自身 lookback 的最大斜率（按价格百分比）。
    /// 超过该值视为单边趋势，禁止逆势抄底/摸顶。
    pub max_trend_slope_pct: f64,
    /// 最大允许的单根入场 K 线振幅（按价格百分比），过滤插针后追单。
    pub max_entry_amp_pct: f64,
}

impl Default for RangeReversionThresholds {
    fn default() -> Self {
        Self {
            band_k: 2.0,
            rsi_long_max: 30.0,
            rsi_short_min: 70.0,
            stop_atr_mult: 1.6,
            target_atr_mult: 1.3,
            max_trend_slope_pct: 0.45,
            max_entry_amp_pct: 1.2,
        }
    }
}

/// 回测调参面；用于研究短周期入场频率与胜率/回撤的关系。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RangeReversionBacktestTuning {
    /// 布林带 / 中轨 SMA 的窗口长度。
    pub band_period: usize,
    /// RSI 计算窗口（Wilder 平滑）。
    pub rsi_period: usize,
    /// ATR 计算窗口（Wilder 平滑），用于止损止盈与振幅过滤。
    pub atr_period: usize,
    /// 慢速 EMA 窗口，用于趋势过滤。
    pub trend_ema_period: usize,
    /// 慢速 EMA 斜率回看的 K 线数。
    pub trend_slope_lookback: usize,
    /// 同一方向连续开仓后的冷却 K 线数，控制频次与过拟合。
    pub cooldown_candles: usize,
    /// 是否允许做空（短周期双向通常都开）。
    pub allow_short: bool,
    /// 是否允许做多。
    pub allow_long: bool,
    /// 布林带回归触发的标准差倍数。
    pub band_k: f64,
    /// 多单 RSI 超卖上限。
    pub rsi_long_max: f64,
    /// 空单 RSI 超买下限。
    pub rsi_short_min: f64,
    /// 止损 ATR 倍数。
    pub stop_atr_mult: f64,
    /// 止盈 ATR 倍数。
    pub target_atr_mult: f64,
    /// 慢速 EMA 最大斜率百分比（趋势过滤）。
    pub max_trend_slope_pct: f64,
    /// 最大入场 K 线振幅百分比（插针过滤）。
    pub max_entry_amp_pct: f64,
}

impl Default for RangeReversionBacktestTuning {
    fn default() -> Self {
        let thresholds = RangeReversionThresholds::default();
        Self {
            band_period: 20,
            rsi_period: 14,
            atr_period: 14,
            trend_ema_period: 100,
            trend_slope_lookback: 24,
            cooldown_candles: 3,
            allow_short: true,
            allow_long: true,
            band_k: thresholds.band_k,
            rsi_long_max: thresholds.rsi_long_max,
            rsi_short_min: thresholds.rsi_short_min,
            stop_atr_mult: thresholds.stop_atr_mult,
            target_atr_mult: thresholds.target_atr_mult,
            max_trend_slope_pct: thresholds.max_trend_slope_pct,
            max_entry_amp_pct: thresholds.max_entry_amp_pct,
        }
    }
}

impl RangeReversionBacktestTuning {
    /// 把调参面的入场/出场门槛投影为 live 共用的 thresholds，消除双口径。
    pub fn thresholds(&self) -> RangeReversionThresholds {
        RangeReversionThresholds {
            band_k: self.band_k,
            rsi_long_max: self.rsi_long_max,
            rsi_short_min: self.rsi_short_min,
            stop_atr_mult: self.stop_atr_mult,
            target_atr_mult: self.target_atr_mult,
            max_trend_slope_pct: self.max_trend_slope_pct,
            max_entry_amp_pct: self.max_entry_amp_pct,
        }
    }
}

/// 上游聚合后的均值回归信号快照；纯 OHLCV 派生，无需外部订单流。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RangeReversionSignalSnapshot {
    /// 交易所名称；v1 live 首发只允许 Binance 和 OKX。
    pub exchange: String,
    /// 永续合约交易对，只接受 BTC/ETH。
    pub symbol: String,
    /// 当前评估价格（最新收盘）。
    pub price: f64,
    /// 布林中轨（SMA）。
    pub band_mid: f64,
    /// 布林带宽（k * 标准差）。
    pub band_width: f64,
    /// 当前 RSI。
    pub rsi: f64,
    /// 当前 ATR，用于止损止盈与振幅过滤。
    pub atr: f64,
    /// 慢速 EMA 斜率绝对值，按价格百分比。
    pub trend_slope_pct: f64,
    /// 入场 K 线振幅百分比 (high-low)/price。
    pub entry_amp_pct: f64,
}

/// 策略评估后的领域决策，先保留原因，再转换为通用 SignalResult。
#[derive(Debug, Clone, PartialEq)]
pub struct RangeReversionDecision {
    /// 多、空或观望动作。
    pub action: RangeReversionAction,
    /// 阻断原因或成交确认原因；同时用于审计和回测诊断。
    pub reasons: Vec<String>,
}

impl RangeReversionDecision {
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
            RangeReversionAction::Long => self.apply_long_signal(&mut signal, price),
            RangeReversionAction::Short => self.apply_short_signal(&mut signal, price),
            RangeReversionAction::Flat => {}
        }
        signal
    }

    fn result_payload(&self) -> Value {
        json!({
            "strategy": "range_reversion_scalper_v1",
            "action": self.action_name(),
            "reasons": self.reasons,
        })
    }

    fn action_name(&self) -> &'static str {
        match self.action {
            RangeReversionAction::Long => "long",
            RangeReversionAction::Short => "short",
            RangeReversionAction::Flat => "flat",
        }
    }

    fn apply_long_signal(&self, signal: &mut SignalResult, price: f64) {
        let stop = reason_value(&self.reasons, "STOP_PRICE").unwrap_or(price);
        let target = reason_value(&self.reasons, "TARGET_PRICE").unwrap_or(price);
        signal.should_buy = true;
        signal.direction = SignalDirection::Long;
        signal.signal_kline_stop_loss_price = Some(round_price(stop));
        signal.stop_loss_source = Some("RangeReversion".to_string());
        // 单一止盈：三档合并为同一目标，回测 update_atr_tiered_levels 先判 level_3 即全平落袋。
        let tp = round_price(target);
        signal.atr_take_profit_level_1 = Some(tp);
        signal.atr_take_profit_level_2 = Some(tp);
        signal.atr_take_profit_level_3 = Some(tp);
    }

    fn apply_short_signal(&self, signal: &mut SignalResult, price: f64) {
        let stop = reason_value(&self.reasons, "STOP_PRICE").unwrap_or(price);
        let target = reason_value(&self.reasons, "TARGET_PRICE").unwrap_or(price);
        signal.should_sell = true;
        signal.direction = SignalDirection::Short;
        signal.signal_kline_stop_loss_price = Some(round_price(stop));
        signal.stop_loss_source = Some("RangeReversion".to_string());
        let tp = round_price(target);
        signal.atr_take_profit_level_1 = Some(tp);
        signal.atr_take_profit_level_2 = Some(tp);
        signal.atr_take_profit_level_3 = Some(tp);
    }
}

/// 把策略生成价格四舍五入到确定精度，便于测试与 payload 对齐。
pub fn round_price(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// 从 reasons 里反解形如 `PREFIX:1234.5` 的数值，live 与 backtest 共用同一份目标。
pub fn reason_value(reasons: &[String], prefix: &str) -> Option<f64> {
    reasons
        .iter()
        .find_map(|reason| reason.strip_prefix(prefix)?.strip_prefix(':')?.parse().ok())
}
