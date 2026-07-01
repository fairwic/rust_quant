use crate::strategy_common::SignalResult;
use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// 做空策略栈的执行动作；当前 v1 只产生空单或观望。
pub enum BearShortAction {
    /// 满足做空结构并生成带止损的空单信号。
    Short,
    /// 过滤条件未满足，返回 flat 信号并保留原因。
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// 做空策略栈的子预设，每个值都对应一个对外可审计的 strategy key。
pub enum BearShortPreset {
    /// 主跌顺势做空，要求 4h 空头结构和 15m failed reclaim。
    #[serde(rename = "bear_breakdown_short_v1")]
    BearBreakdown,
    /// 冲高衰竭反转做空，风险更高，信号层会降半仓。
    #[serde(rename = "exhaustion_fade_short_v1")]
    ExhaustionFade,
}

impl Default for BearShortPreset {
    fn default() -> Self {
        Self::BearBreakdown
    }
}

impl BearShortPreset {
    pub fn strategy_key(self) -> &'static str {
        match self {
            BearShortPreset::BearBreakdown => "bear_breakdown_short_v1",
            BearShortPreset::ExhaustionFade => "exhaustion_fade_short_v1",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BearShortStackThresholds {
    /// 主跌顺势所需最小 OI 增量，百分比值。
    pub min_oi_growth_pct: f64,
    /// funding 深度转负阈值；低于该值说明空头可能已经拥挤，不再追空。
    pub deeply_negative_funding_rate: f64,
    /// 跌幅扩张上限，单位 ATR 倍数，防止空在尾端。
    pub max_downside_extension_atr: f64,
    /// 最小多空比，用于确认多头尚未完全出清。
    pub min_long_short_ratio: f64,
    /// 主跌顺势止损缓冲，单位 ATR 倍数。
    pub breakdown_stop_atr_buffer: f64,
    /// 主跌顺势第一止盈目标，单位 R。
    pub breakdown_target_r_1: f64,
    /// 主跌顺势第二止盈目标，单位 R。
    pub breakdown_target_r_2: f64,
    /// 衰竭反转所需 OI 异常放大门槛，百分比值。
    pub exhaustion_min_oi_growth_pct: f64,
    /// 衰竭反转所需 funding 热度门槛。
    pub exhaustion_hot_funding_rate: f64,
    /// 衰竭反转止损缓冲，单位 ATR 倍数；默认比主跌顺势更紧。
    pub exhaustion_stop_atr_buffer: f64,
    /// 衰竭反转第一止盈目标，单位 R。
    pub exhaustion_target_r_1: f64,
    /// 衰竭反转第二止盈目标，单位 R。
    pub exhaustion_target_r_2: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
/// 回测调参面，只用于 research/backtest，不作为 live 策略 key 兼容层。
pub struct BearShortStackBacktestTuning {
    /// 是否允许使用合成 OI/funding/多空比上下文；只能用于 pipeline 接线测试，不能作为商品绩效证据。
    pub allow_synthetic_market_context: bool,
    /// 同一交易对连续开仓后的冷却 K 线数量。
    pub cooldown_candles: usize,
    /// 主跌初始破位所需的最小波动倍数。
    pub breakdown_initial_move_range_mult: f64,
    /// 主跌初始破位所需的最小量能倍数。
    pub breakdown_initial_volume_mult: f64,
    /// failed reclaim 与破位锚点的最小 ATR 距离。
    pub breakdown_min_reclaim_distance_atr: f64,
    /// failed reclaim 与破位锚点的最大 ATR 距离，防止追空尾端。
    pub breakdown_max_reclaim_distance_atr: f64,
    /// 前支撑跌破所需的最小 K 线区间比例。
    pub breakdown_min_support_break_range: f64,
    /// failed reclaim 执行 K 线所需的最小实体比例。
    pub breakdown_min_body_ratio: f64,
    /// failed reclaim 执行 K 线所需的最小量能倍数。
    pub breakdown_min_volume_mult: f64,
    /// 主跌顺势止损缓冲 ATR 倍数，用于研究回测扫描风险边界。
    pub breakdown_stop_atr_buffer: f64,
    /// 主跌顺势第一止盈目标 R 倍数，用于研究回测扫描 exit 空间。
    pub breakdown_target_r_1: f64,
    /// 主跌顺势第二止盈目标 R 倍数，用于研究回测扫描 exit 空间。
    pub breakdown_target_r_2: f64,
    /// 衰竭吹顶所需的创新高扩张倍数。
    pub exhaustion_new_high_range_mult: f64,
    /// 衰竭失败回落 K 线所需的最小实体比例。
    pub exhaustion_min_body_ratio: f64,
    /// 衰竭失败回落 K 线所需的最小量能倍数。
    pub exhaustion_min_volume_mult: f64,
    /// 衰竭失败回落距离相对 ATR 的最小倍数，用于过滤创新高后回落不足的噪音。
    pub exhaustion_min_rejection_atr: f64,
    /// 衰竭反转止损缓冲 ATR 倍数，用于研究回测扫描风险边界。
    pub exhaustion_stop_atr_buffer: f64,
    /// 衰竭反转第一止盈目标 R 倍数，用于研究回测扫描 exit 空间。
    pub exhaustion_target_r_1: f64,
    /// 衰竭反转第二止盈目标 R 倍数，用于研究回测扫描 exit 空间。
    pub exhaustion_target_r_2: f64,
    /// 衰竭反转最大持仓 K 线数，避免反转失败后长时间占用仓位。
    pub exhaustion_max_holding_candles: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
/// 回测用市场上下文，承接 funding、OI、多空比和 taker flow 过滤器。
pub struct BearShortStackBacktestMarketContext {
    /// 对齐执行 K 线的毫秒时间戳。
    pub ts: i64,
    /// 当前资金费率。
    pub funding_rate: f64,
    /// OI 相对前一快照的增长百分比。
    pub oi_growth_pct: f64,
    /// 多空比，用于判断多头是否仍有拥挤出清空间。
    pub long_short_ratio: f64,
    /// taker buy 成交量。
    pub taker_buy_volume: f64,
    /// taker sell 成交量。
    pub taker_sell_volume: f64,
}

impl Default for BearShortStackBacktestTuning {
    fn default() -> Self {
        Self {
            allow_synthetic_market_context: false,
            cooldown_candles: 12,
            breakdown_initial_move_range_mult: 1.35,
            breakdown_initial_volume_mult: 1.25,
            breakdown_min_reclaim_distance_atr: 0.35,
            breakdown_max_reclaim_distance_atr: 0.8,
            breakdown_min_support_break_range: 0.15,
            breakdown_min_body_ratio: 0.45,
            breakdown_min_volume_mult: 1.8,
            breakdown_stop_atr_buffer: 0.35,
            breakdown_target_r_1: 0.8,
            breakdown_target_r_2: 1.6,
            exhaustion_new_high_range_mult: 1.35,
            exhaustion_min_body_ratio: 0.35,
            exhaustion_min_volume_mult: 1.3,
            exhaustion_min_rejection_atr: 0.0,
            exhaustion_stop_atr_buffer: 0.5,
            exhaustion_target_r_1: 0.8,
            exhaustion_target_r_2: 1.6,
            exhaustion_max_holding_candles: 32,
        }
    }
}

impl BearShortStackBacktestTuning {
    /// Returns the validated real-market-context tuning used by live and OKX context backtests.
    pub fn real_context_default(preset: BearShortPreset) -> Self {
        match preset {
            BearShortPreset::BearBreakdown => Self::default(),
            BearShortPreset::ExhaustionFade => Self {
                cooldown_candles: 12,
                exhaustion_new_high_range_mult: 1.25,
                exhaustion_min_body_ratio: 0.30,
                exhaustion_min_volume_mult: 1.30,
                exhaustion_min_rejection_atr: 1.40,
                exhaustion_stop_atr_buffer: 0.35,
                exhaustion_target_r_1: 1.0,
                exhaustion_target_r_2: 2.0,
                ..Default::default()
            },
        }
    }
}

impl Default for BearShortStackThresholds {
    fn default() -> Self {
        Self {
            min_oi_growth_pct: 0.5,
            deeply_negative_funding_rate: -0.0005,
            max_downside_extension_atr: 1.5,
            min_long_short_ratio: 1.0,
            breakdown_stop_atr_buffer: 0.35,
            breakdown_target_r_1: 0.8,
            breakdown_target_r_2: 1.6,
            exhaustion_min_oi_growth_pct: 0.5,
            exhaustion_hot_funding_rate: 0.00003,
            exhaustion_stop_atr_buffer: 0.35,
            exhaustion_target_r_1: 1.0,
            exhaustion_target_r_2: 2.0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BearShortStackConfig {
    /// 策略审计 key；允许父策略 key 或两个带版本的子预设 key。
    pub strategy_key: Option<String>,
    /// 默认子预设；snapshot 显式指定 ExhaustionFade 时会优先使用 snapshot。
    pub preset: BearShortPreset,
    /// 当前版本的做空过滤、止损和止盈门槛。
    pub thresholds: BearShortStackThresholds,
    /// 上游聚合后的做空市场快照；缺失时返回 flat。
    pub snapshot: Option<BearShortSignalSnapshot>,
}

impl BearShortStackConfig {
    /// Normalizes product-level child strategy keys into the preset used by evaluation.
    pub fn apply_strategy_key_preset(&mut self) {
        if self.strategy_key.as_deref() == Some("exhaustion_fade_short_v1") {
            self.preset = BearShortPreset::ExhaustionFade;
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BearShortSignalSnapshot {
    /// 交易所名称；v1 live 首发只允许 Binance 和 OKX。
    pub exchange: String,
    /// 永续合约交易对，只接受 BTC/ETH。
    pub symbol: String,
    /// 当前评估价格。
    pub price: f64,
    /// failed reclaim 高点，止损锚定在该高点上方。
    pub failed_reclaim_high: f64,
    /// 15m ATR，用于止损缓冲和尾端扩张判断。
    pub atr_15m: f64,
    /// 本次快照指定的子预设。
    pub preset: BearShortPreset,
    /// 4h 趋势方向；主跌顺势必须为空头结构。
    pub trend_4h: String,
    /// 1h 结构，主跌顺势要求 lower high 或等价弱反弹。
    pub trend_1h: String,
    /// 15m 是否已破位。
    pub breakdown_confirmed: bool,
    /// 反抽是否未能重新站回 VWAP、EMA 带或前支撑。
    pub failed_reclaim_confirmed: bool,
    /// 是否出现 price down + OI up。
    pub price_down_with_oi_up: bool,
    /// OI 变化百分比，用于确认下跌是否伴随持仓增量。
    pub oi_growth_pct: f64,
    /// 当前 funding rate；深度负值会阻断主跌追空。
    pub funding_rate: f64,
    /// 多空比，用于判断多头拥挤是否仍有出清空间。
    pub long_short_ratio: f64,
    /// 下跌相对 ATR 的扩张倍数，用于避免空在尾端。
    pub downside_extension_atr: f64,
    /// 是否出现创新高后失败。
    pub new_high_failed: bool,
    /// taker flow 是否不再确认新高。
    pub taker_flow_diverged: bool,
    /// 盘口失衡是否不再确认新高。
    pub orderbook_imbalance_diverged: bool,
    /// 回落后的反抽是否仍然无法站回 VWAP。
    pub pullback_failed_below_vwap: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// 策略评估后的领域决策，先保留原因，再转换为通用 SignalResult。
pub struct BearShortDecision {
    /// 做空或观望动作。
    pub action: BearShortAction,
    /// 实际命中的子预设。
    pub preset: BearShortPreset,
    /// 阻断原因或成交确认原因；同时用于审计和回测诊断。
    pub reasons: Vec<String>,
}

impl BearShortDecision {
    /// Checks whether a specific audit reason is present.
    pub fn has_reason(&self, reason: &str) -> bool {
        self.reasons.iter().any(|item| item == reason)
    }

    /// Converts the domain decision into the shared backtest/live signal contract.
    pub fn to_signal(&self, price: f64, ts: i64) -> SignalResult {
        let mut signal = SignalResult {
            open_price: price,
            ts,
            filter_reasons: self.reasons.clone(),
            single_result: Some(self.result_payload().to_string()),
            ..Default::default()
        };
        if self.action == BearShortAction::Short {
            self.apply_short_signal(&mut signal, price);
        }
        signal
    }

    fn result_payload(&self) -> Value {
        json!({
            "strategy": "bear_short_stack_v1",
            "preset": self.preset.strategy_key(),
            "action": self.action_name(),
            "reasons": self.reasons,
        })
    }

    fn action_name(&self) -> &'static str {
        match self.action {
            BearShortAction::Short => "short",
            BearShortAction::Flat => "flat",
        }
    }

    fn apply_short_signal(&self, signal: &mut SignalResult, price: f64) {
        let stop = reason_value(&self.reasons, "STOP_PRICE").unwrap_or(price);
        let defaults = BearShortStackThresholds::default();
        let (default_target_r_1, default_target_r_2) = match self.preset {
            BearShortPreset::BearBreakdown => {
                (defaults.breakdown_target_r_1, defaults.breakdown_target_r_2)
            }
            BearShortPreset::ExhaustionFade => (
                defaults.exhaustion_target_r_1,
                defaults.exhaustion_target_r_2,
            ),
        };
        let target_r_1 = reason_value(&self.reasons, "TARGET_R_1").unwrap_or(default_target_r_1);
        let target_r_2 = reason_value(&self.reasons, "TARGET_R_2").unwrap_or(default_target_r_2);
        let risk = (stop - price).max(0.0);
        signal.should_sell = true;
        signal.direction = SignalDirection::Short;
        signal.signal_kline_stop_loss_price = Some(round_price(stop));
        signal.atr_take_profit_level_1 = Some(round_price(price - risk * target_r_1));
        signal.atr_take_profit_level_2 = Some(round_price(price - risk * target_r_2));
        signal.atr_take_profit_level_3 = signal.atr_take_profit_level_1;
        if self.preset == BearShortPreset::ExhaustionFade {
            signal.dynamic_adjustments = vec!["HALF_RISK".to_string()];
        }
    }
}

pub fn round_price(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn reason_value(reasons: &[String], prefix: &str) -> Option<f64> {
    reasons
        .iter()
        .find_map(|reason| reason.strip_prefix(prefix)?.strip_prefix(':')?.parse().ok())
}
