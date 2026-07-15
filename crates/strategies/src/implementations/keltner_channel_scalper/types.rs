use crate::strategy_common::SignalResult;
use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Keltner Channel 1m scalp 的执行动作；v1 research 只产生研究信号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeltnerChannelScalperAction {
    /// 外层下轨跌破后重新站回内层下轨，且 ADX 低于水平线。
    Long,
    /// 外层上轨突破后重新跌回内层上轨，且 ADX 高于水平线。
    Short,
    /// 通道、ADX 或风控证据不足时保持观望。
    Flat,
}

/// Keltner re-entry setup 的交易解释；默认 Reversal 保持用户原始反转策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeltnerChannelScalperEntryMode {
    /// 外层突破后回到内层，按均值回归方向交易。
    Reversal,
    /// 外层突破后回到内层，但按 re-entry 失败后的原趋势延续方向交易。
    Continuation,
    /// 只把外层突破当作极值背景，当前 K 线突破上一根高/低点后按反转方向交易。
    ExtremeMomentumReversal,
}

impl Default for KeltnerChannelScalperEntryMode {
    fn default() -> Self {
        Self::Reversal
    }
}

/// Keltner Channel 1m scalp 的指标参数与 ATR/R 风控参数。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeltnerChannelScalperThresholds {
    /// Keltner 中线 EMA 与 ATR 宽度共用周期，默认 50。
    pub keltner_length: usize,
    /// 外层 Keltner ATR 倍数，默认 3.75。
    pub outer_multiplier: f64,
    /// 内层 Keltner ATR 倍数，默认 2.75。
    pub inner_multiplier: f64,
    /// ADX +DI/-DI 趋势长度，默认 12。
    pub adx_trend_length: usize,
    /// ADX 平滑长度，默认 12。
    pub adx_smoothing: usize,
    /// ADX 水平线；空单要求严格高于该值，多单要求严格低于该值。
    pub adx_level: f64,
    /// 多单 ADX 下界；用于研究期过滤极低趋势强度的假反转，0 表示不启用。
    pub min_long_adx: f64,
    /// 收盘重新进入内层通道后，距离内轨的最小 ATR 倍数；0 表示不启用。
    pub min_inner_reclaim_atr: f64,
    /// 收盘重新进入内层通道后，距离内轨的最大 ATR 倍数；0 表示不启用过度追价过滤。
    pub max_inner_reclaim_atr: f64,
    /// 当前 ATR 占价格的最小百分比；0 表示不启用低波动过滤。
    pub min_atr_pct: f64,
    /// Keltner EMA basis 斜率的最小 ATR 归一化幅度；0 表示不启用趋势方向过滤。
    pub min_basis_slope_atr: f64,
    /// 允许的最大逆势 EMA basis 斜率；0 表示不启用极端逆势过滤。
    pub max_adverse_basis_slope_atr: f64,
    /// 是否要求 re-entry 后价格进一步穿越 Keltner basis，作为更强反转确认。
    pub require_basis_cross: bool,
    /// 止损距离，单位 ATR 倍数。
    pub stop_atr_mult: f64,
    /// 第一档止盈目标，单位 R。
    pub target_r_1: f64,
    /// 第二档止盈目标，单位 R。
    pub target_r_2: f64,
    /// 第三档止盈目标，单位 R。
    pub target_r_3: f64,
    /// re-entry K 线实体占整根 K 线区间的最小比例；0 表示不启用该过滤。
    pub min_reentry_body_ratio: f64,
    /// re-entry K 线实体占整根 K 线区间的最大比例；0 表示不启用失败突破形态过滤。
    pub max_reentry_body_ratio: f64,
    /// re-entry K 线反转影线占整根 K 线区间的最小比例；0 表示不启用该过滤。
    pub min_rejection_wick_ratio: f64,
    /// re-entry K 线按交易方向收在区间有利端的最小比例；0 表示不启用收盘强度过滤。
    pub min_reentry_close_progress_ratio: f64,
    /// 外层突破到重新回到内层最多允许经历的 K 线数；0 表示不启用，1 表示只接受同根突破同根回收。
    pub max_breakout_reentry_candles: usize,
}

impl Default for KeltnerChannelScalperThresholds {
    fn default() -> Self {
        Self {
            keltner_length: 50,
            outer_multiplier: 3.75,
            inner_multiplier: 2.75,
            adx_trend_length: 12,
            adx_smoothing: 12,
            adx_level: 30.0,
            min_long_adx: 0.0,
            min_inner_reclaim_atr: 0.0,
            max_inner_reclaim_atr: 0.0,
            min_atr_pct: 0.0,
            min_basis_slope_atr: 0.0,
            max_adverse_basis_slope_atr: 0.0,
            require_basis_cross: false,
            stop_atr_mult: 1.0,
            target_r_1: 1.0,
            target_r_2: 2.0,
            target_r_3: 3.0,
            min_reentry_body_ratio: 0.0,
            max_reentry_body_ratio: 0.0,
            min_rejection_wick_ratio: 0.0,
            min_reentry_close_progress_ratio: 0.0,
            max_breakout_reentry_candles: 0,
        }
    }
}

/// 回测调参面；默认保持用户给定指标参数，仅提供研究期方向开关和冷却窗口。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeltnerChannelScalperBacktestTuning {
    /// 入场后冷却的 1m K 线数量，避免同一回归结构重复开仓。
    pub cooldown_candles: usize,
    /// 查找外层通道突破的最近 K 线窗口；最后一根可同时完成突破与收回。
    pub reentry_lookback_candles: usize,
    /// 是否允许多单研究信号。
    pub allow_long: bool,
    /// 是否允许空单研究信号。
    pub allow_short: bool,
    /// 是否等待下一根已完成 K 线确认回归方向；默认关闭以保持 v1 同根回归行为。
    pub confirm_next_candle: bool,
    /// re-entry setup 的交易解释；默认反转，Continuation 仅用于研究扫描。
    pub entry_mode: KeltnerChannelScalperEntryMode,
    /// 指标与风控参数。
    pub thresholds: KeltnerChannelScalperThresholds,
}

impl Default for KeltnerChannelScalperBacktestTuning {
    fn default() -> Self {
        Self {
            cooldown_candles: 3,
            reentry_lookback_candles: 6,
            allow_long: true,
            allow_short: true,
            confirm_next_candle: false,
            entry_mode: KeltnerChannelScalperEntryMode::Reversal,
            thresholds: KeltnerChannelScalperThresholds::default(),
        }
    }
}

/// live/paper 可注入的 Keltner 快照；回测 adapter 也会从已完成 1m K 线构造同一形状。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeltnerChannelScalperSignalSnapshot {
    /// 交易对标识，用于回测审计和后续产品映射。
    pub symbol: String,
    /// 策略周期；v1 只接受 `1m`。
    pub timeframe: String,
    /// 当前评估价格，通常为最新已确认 1m K 线收盘价。
    pub price: f64,
    /// Keltner EMA(50) 中线。
    pub basis: f64,
    /// 内层通道上轨：EMA(50) + ATR(50) * 2.75。
    pub inner_upper: f64,
    /// 内层通道下轨：EMA(50) - ATR(50) * 2.75。
    pub inner_lower: f64,
    /// 外层通道上轨：EMA(50) + ATR(50) * 3.75。
    pub outer_upper: f64,
    /// 外层通道下轨：EMA(50) - ATR(50) * 3.75。
    pub outer_lower: f64,
    /// 当前 ATR(50)，用于通道宽度和 ATR/R 风控。
    pub atr: f64,
    /// 当前 ADX(12,12)。
    pub adx: f64,
    /// EMA(50) basis 相对 12 根前 basis 的 ATR 归一化斜率。
    pub basis_slope_atr: f64,
    /// 最近窗口内是否突破过外层上轨。
    pub outer_upper_breached: bool,
    /// 最近窗口内是否跌破过外层下轨。
    pub outer_lower_breached: bool,
    /// 最新收盘是否已经回到内层上轨下方。
    pub returned_inside_inner_upper: bool,
    /// 最新收盘是否已经回到内层下轨上方。
    pub returned_inside_inner_lower: bool,
    /// re-entry K 线实体占 high-low 区间的比例，用于过滤噪声级收回。
    pub reentry_body_ratio: f64,
    /// re-entry 方向对应的拒绝影线比例；多单看下影线，空单看上影线。
    pub rejection_wick_ratio: f64,
    /// re-entry 方向对应的收盘位置强度；多单靠近高点、空单靠近低点时更强。
    pub reentry_close_progress_ratio: f64,
    /// 外层突破到当前 re-entry K 线之间相隔的 K 线数；0 表示同根突破同根回收。
    pub breakout_reentry_candles: usize,
    /// 外层下轨极值后，当前 K 线是否突破上一根高点，用于独立的短周期动量反转研究。
    pub bullish_momentum_break: bool,
    /// 外层上轨极值后，当前 K 线是否跌破上一根低点，用于独立的短周期动量反转研究。
    pub bearish_momentum_break: bool,
}

/// 执行器配置；只识别带版本 key，并允许上游直接注入指标快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeltnerChannelScalperConfig {
    /// 策略审计 key；执行器只接受 `keltner_channel_scalper_1m_v1_research`。
    pub strategy_key: Option<String>,
    /// 当前版本的指标和风控参数。
    pub thresholds: KeltnerChannelScalperThresholds,
    /// 上游已计算的快照；缺失时返回 flat，避免用单根 K 线伪造信号。
    pub snapshot: Option<KeltnerChannelScalperSignalSnapshot>,
}

/// 策略评估后的领域决策，保留原因后再转换为通用信号契约。
#[derive(Debug, Clone, PartialEq)]
pub struct KeltnerChannelScalperDecision {
    /// 多、空或观望动作。
    pub action: KeltnerChannelScalperAction,
    /// 阻断原因或成交确认原因；用于审计、回测诊断和后续 paper 验证。
    pub reasons: Vec<String>,
}

impl KeltnerChannelScalperDecision {
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
            KeltnerChannelScalperAction::Long => self.apply_long_signal(&mut signal),
            KeltnerChannelScalperAction::Short => self.apply_short_signal(&mut signal),
            KeltnerChannelScalperAction::Flat => {}
        }
        signal
    }

    fn result_payload(&self) -> Value {
        json!({
            "strategy": "keltner_channel_scalper_1m_v1_research",
            "action": self.action_name(),
            "reasons": self.reasons,
        })
    }

    fn action_name(&self) -> &'static str {
        match self.action {
            KeltnerChannelScalperAction::Long => "long",
            KeltnerChannelScalperAction::Short => "short",
            KeltnerChannelScalperAction::Flat => "flat",
        }
    }

    fn apply_long_signal(&self, signal: &mut SignalResult) {
        signal.should_buy = true;
        signal.direction = SignalDirection::Long;
        signal.signal_kline_stop_loss_price = reason_value(&self.reasons, "STOP_PRICE");
        signal.stop_loss_source = Some("KeltnerChannelScalper".to_string());
        signal.atr_take_profit_level_1 = reason_value(&self.reasons, "TARGET_1");
        signal.atr_take_profit_level_2 = reason_value(&self.reasons, "TARGET_2");
        signal.atr_take_profit_level_3 = reason_value(&self.reasons, "TARGET_3");
    }

    fn apply_short_signal(&self, signal: &mut SignalResult) {
        signal.should_sell = true;
        signal.direction = SignalDirection::Short;
        signal.signal_kline_stop_loss_price = reason_value(&self.reasons, "STOP_PRICE");
        signal.stop_loss_source = Some("KeltnerChannelScalper".to_string());
        signal.atr_take_profit_level_1 = reason_value(&self.reasons, "TARGET_1");
        signal.atr_take_profit_level_2 = reason_value(&self.reasons, "TARGET_2");
        signal.atr_take_profit_level_3 = reason_value(&self.reasons, "TARGET_3");
    }
}

/// 把策略生成价格四舍五入到确定精度，便于测试与 payload 对齐。
pub fn round_price(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// 从 reasons 里反解形如 `PREFIX:1234.5` 的数值。
pub fn reason_value(reasons: &[String], prefix: &str) -> Option<f64> {
    reasons
        .iter()
        .find_map(|reason| reason.strip_prefix(prefix)?.strip_prefix(':')?.parse().ok())
}
