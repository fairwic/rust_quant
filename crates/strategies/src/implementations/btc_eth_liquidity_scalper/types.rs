use crate::strategy_common::SignalResult;
use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// BTC/ETH 流动性剥头皮的执行动作；v1 只允许 BTC/ETH 永续。
pub enum BtcEthLiquidityScalperAction {
    /// 主趋势向上且回踩确认后的多单信号。
    Long,
    /// 主趋势向下且回踩确认后的空单信号。
    Short,
    /// 任一过滤器不满足时返回观望，并保留过滤原因。
    Flat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
/// live/paper 信号层使用的流动性、拥挤度和反追高门槛。
pub struct BtcEthLiquidityScalperThresholds {
    /// 回踩距离上限，单位为 5m ATR 倍数；超过后视为追高或追低。
    pub max_anchor_distance_atr: f64,
    /// 锚点到止损的 ATR 缓冲倍数；与 `max_anchor_distance_atr` 解耦，便于独立扫描止损宽度。
    pub stop_atr_buffer: f64,
    /// 同向 taker 主动成交强度门槛，0-1 区间。
    pub min_taker_aggression: f64,
    /// 同向盘口失衡门槛，0-1 区间。
    pub min_orderbook_imbalance: f64,
    /// OI 同向扩张门槛，百分比值；低于该值时降仓而不是直接阻断。
    pub min_oi_expansion_pct: f64,
    /// funding 拥挤上限；多头看正值，空头看负值。
    pub max_abs_funding_rate: f64,
    /// 最大买卖价差，单位 bps。
    pub max_spread_bps: f64,
    /// 最小盘口深度，单位 USD 名义金额。
    pub min_depth_usd: f64,
    /// 突破 K 线实体上限，单位为 ATR 倍数，用于防止追过度扩张。
    pub max_breakout_candle_atr: f64,
    /// 第一止盈目标，单位 R；live 与 backtest 共用同一份配置避免双口径。
    pub target_r_1: f64,
    /// 第二止盈目标，单位 R。
    pub target_r_2: f64,
    /// 第三止盈目标，单位 R；用于实现完整三档止盈，而不是 level_2 的拷贝。
    pub target_r_3: f64,
}

impl Default for BtcEthLiquidityScalperThresholds {
    fn default() -> Self {
        Self {
            max_anchor_distance_atr: 0.7,
            stop_atr_buffer: 0.7,
            min_taker_aggression: 0.55,
            min_orderbook_imbalance: 0.55,
            min_oi_expansion_pct: 0.5,
            max_abs_funding_rate: 0.00035,
            max_spread_bps: 3.0,
            min_depth_usd: 10_000_000.0,
            max_breakout_candle_atr: 1.5,
            target_r_1: 0.8,
            target_r_2: 1.6,
            target_r_3: 2.4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
/// 回测调参面；用于研究短周期入场频率，不作为 live 配置兼容层。
pub struct BtcEthLiquidityScalperBacktestTuning {
    /// 是否允许使用合成盘口/OI/funding 上下文；只能用于 pipeline 接线测试，不能作为商品绩效证据。
    pub allow_synthetic_market_context: bool,
    /// 同一交易对连续开仓后的冷却 K 线数量。
    pub cooldown_candles: usize,
    /// 是否允许剥头皮策略在回测中做空。
    pub allow_short: bool,
    /// Fast trend window in execution candles; short-cycle research may reduce it from the 5m default.
    pub trend_fast_window: usize,
    /// Slow trend window in execution candles; also controls the rolling setup window length.
    pub trend_slow_window: usize,
    /// 长窗口方向一致性的最小比例。
    pub min_directional_ratio_48: f64,
    /// 短窗口方向一致性的最小比例。
    pub min_directional_ratio_24: f64,
    /// 冲击 K 线相对平均波动的最小倍数。
    pub impulse_move_range_mult: f64,
    /// 冲击 K 线最小实体比例。
    pub impulse_min_body_ratio: f64,
    /// 冲击 K 线最小量能倍数。
    pub impulse_min_volume_mult: f64,
    /// 回踩深度下限，防止没有回踩就追入。
    pub pullback_min_depth: f64,
    /// 回踩深度上限，超过后视为结构失效。
    pub pullback_max_depth: f64,
    /// 恢复 K 线实体相对冲击 K 线的最小倍数。
    pub resume_extension_body_mult: f64,
    /// 是否要求恢复 K 线突破前一根 K 线极值。
    pub require_previous_extreme_break: bool,
    /// 是否要求 OI 与价格同向扩张；短周期默认可作为降仓因素而非硬门槛。
    pub require_oi_confirmation: bool,
    /// 第一止盈目标，单位 R。
    pub target_r_1: f64,
    /// 第二止盈目标，单位 R。
    pub target_r_2: f64,
    /// 第三止盈目标，单位 R；默认应严格大于 target_r_2。
    pub target_r_3: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
/// 回测用市场上下文，承接 funding、OI、taker flow 和流动性过滤器。
pub struct BtcEthLiquidityScalperBacktestMarketContext {
    /// 对齐执行 K 线的毫秒时间戳。
    pub ts: i64,
    /// 当前资金费率。
    pub funding_rate: f64,
    /// OI 相对前一快照的增长百分比。
    pub oi_expansion_pct: f64,
    /// taker buy 成交量。
    pub taker_buy_volume: f64,
    /// taker sell 成交量。
    pub taker_sell_volume: f64,
    /// 盘口同向失衡强度，0-1 区间。
    pub orderbook_imbalance: f64,
    /// 当前买卖价差，单位 bps。
    pub spread_bps: f64,
    /// 当前可成交深度，单位 USD 名义金额。
    pub depth_usd: f64,
}

impl Default for BtcEthLiquidityScalperBacktestTuning {
    fn default() -> Self {
        Self {
            allow_synthetic_market_context: false,
            cooldown_candles: 24,
            allow_short: false,
            trend_fast_window: 20,
            trend_slow_window: 48,
            min_directional_ratio_48: 0.5,
            min_directional_ratio_24: 0.65,
            impulse_move_range_mult: 1.4,
            impulse_min_body_ratio: 0.55,
            impulse_min_volume_mult: 1.5,
            pullback_min_depth: 0.15,
            pullback_max_depth: 0.85,
            resume_extension_body_mult: 0.35,
            require_previous_extreme_break: true,
            require_oi_confirmation: false,
            target_r_1: 0.8,
            target_r_2: 1.6,
            target_r_3: 2.4,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BtcEthLiquidityScalperConfig {
    /// 策略审计 key；执行器只接受 `btc_eth_liquidity_scalper_v1`。
    pub strategy_key: Option<String>,
    /// 当前版本的入场、拥挤度和流动性门槛。
    pub thresholds: BtcEthLiquidityScalperThresholds,
    /// 上游聚合后的市场快照；缺失时返回 flat，避免用单根 candle 伪造 live 信号。
    pub snapshot: Option<BtcEthLiquidityScalperSignalSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BtcEthLiquidityScalperSignalSnapshot {
    /// 交易所名称；v1 live 首发只允许 Binance 和 OKX。
    pub exchange: String,
    /// 永续合约交易对，只接受 BTC/ETH。
    pub symbol: String,
    /// 当前评估价格。
    pub price: f64,
    /// 回踩锚点，通常来自 VWAP、EMA20 或微结构突破位。
    pub anchor_price: f64,
    /// 5m ATR，用于控制回踩距离、止损和突破扩张。
    pub atr_5m: f64,
    /// 4h 趋势方向，用于排除高周期冲突。
    pub trend_4h: String,
    /// 1h 趋势方向，必须与执行方向一致。
    pub trend_1h: String,
    /// 5m 执行方向，只允许 `long` 或 `short` 触发交易信号。
    pub execution_bias: String,
    /// 是否已有放量冲击；false 表示不进入回踩执行阶段。
    pub volume_impulse_confirmed: bool,
    /// 是否已回踩到 VWAP、EMA20 或突破锚点附近。
    pub pullback_to_anchor: bool,
    /// 同向 taker 主动成交强度，0-1 区间。
    pub taker_aggression: f64,
    /// 同向盘口失衡强度，0-1 区间。
    pub orderbook_imbalance: f64,
    /// OI 变化百分比，用于判断价格推动是否有持仓扩张确认。
    pub oi_expansion_pct: f64,
    /// 当前 funding rate；极端拥挤时用于反追高过滤。
    pub funding_rate: f64,
    /// 当前买卖价差，单位 bps。
    pub spread_bps: f64,
    /// 可成交深度，单位 USD 名义金额。
    pub depth_usd: f64,
    /// 突破 K 线实体相对 ATR 的倍数，用于过滤过度扩张。
    pub breakout_candle_atr: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// 策略评估后的领域决策，先保留原因，再转换为通用 SignalResult。
pub struct BtcEthLiquidityScalperDecision {
    /// 多、空或观望动作。
    pub action: BtcEthLiquidityScalperAction,
    /// 阻断原因或成交确认原因；同时用于审计和回测诊断。
    pub reasons: Vec<String>,
}

impl BtcEthLiquidityScalperDecision {
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
        match self.action {
            BtcEthLiquidityScalperAction::Long => self.apply_long_signal(&mut signal, price),
            BtcEthLiquidityScalperAction::Short => self.apply_short_signal(&mut signal, price),
            BtcEthLiquidityScalperAction::Flat => {}
        }
        signal
    }

    fn result_payload(&self) -> Value {
        json!({
            "strategy": "btc_eth_liquidity_scalper_v1",
            "action": self.action_name(),
            "reasons": self.reasons,
        })
    }

    fn action_name(&self) -> &'static str {
        match self.action {
            BtcEthLiquidityScalperAction::Long => "long",
            BtcEthLiquidityScalperAction::Short => "short",
            BtcEthLiquidityScalperAction::Flat => "flat",
        }
    }

    fn apply_long_signal(&self, signal: &mut SignalResult, price: f64) {
        let stop = reason_value(&self.reasons, "STOP_PRICE").unwrap_or(price);
        let defaults = BtcEthLiquidityScalperThresholds::default();
        // 与 bear_short_stack 保持一致：止盈 R 倍数优先从 reasons 解析（live/backtest 同源），
        // 缺失时回退到 thresholds 默认值，避免回测扫到的目标在 live 被静默丢弃。
        let target_r_1 = reason_value(&self.reasons, "TARGET_R_1").unwrap_or(defaults.target_r_1);
        let target_r_2 = reason_value(&self.reasons, "TARGET_R_2").unwrap_or(defaults.target_r_2);
        let target_r_3 = reason_value(&self.reasons, "TARGET_R_3").unwrap_or(defaults.target_r_3);
        let risk = (price - stop).max(0.0);
        signal.should_buy = true;
        signal.direction = SignalDirection::Long;
        signal.signal_kline_stop_loss_price = Some(round_price(stop));
        signal.atr_take_profit_level_1 = Some(round_price(price + risk * target_r_1));
        signal.atr_take_profit_level_2 = Some(round_price(price + risk * target_r_2));
        signal.atr_take_profit_level_3 = Some(round_price(price + risk * target_r_3));
        signal.dynamic_adjustments = self.dynamic_adjustments();
    }

    fn apply_short_signal(&self, signal: &mut SignalResult, price: f64) {
        let stop = reason_value(&self.reasons, "STOP_PRICE").unwrap_or(price);
        let defaults = BtcEthLiquidityScalperThresholds::default();
        // 同 apply_long_signal：把 R 倍数集中到 reasons，让 backtest tuning 和 live 走同一份配置。
        let target_r_1 = reason_value(&self.reasons, "TARGET_R_1").unwrap_or(defaults.target_r_1);
        let target_r_2 = reason_value(&self.reasons, "TARGET_R_2").unwrap_or(defaults.target_r_2);
        let target_r_3 = reason_value(&self.reasons, "TARGET_R_3").unwrap_or(defaults.target_r_3);
        let risk = (stop - price).max(0.0);
        signal.should_sell = true;
        signal.direction = SignalDirection::Short;
        signal.signal_kline_stop_loss_price = Some(round_price(stop));
        signal.atr_take_profit_level_1 = Some(round_price(price - risk * target_r_1));
        signal.atr_take_profit_level_2 = Some(round_price(price - risk * target_r_2));
        signal.atr_take_profit_level_3 = Some(round_price(price - risk * target_r_3));
        signal.dynamic_adjustments = self.dynamic_adjustments();
    }

    fn dynamic_adjustments(&self) -> Vec<String> {
        if self.has_reason("OI_NOT_CONFIRMED_REDUCE_SIZE") {
            vec!["REDUCE_SIZE_NO_OI_CONFIRMATION".to_string()]
        } else {
            vec![]
        }
    }
}

/// Rounds strategy-generated prices to a deterministic precision for tests and payloads.
pub fn round_price(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn reason_value(reasons: &[String], prefix: &str) -> Option<f64> {
    reasons
        .iter()
        .find_map(|reason| reason.strip_prefix(prefix)?.strip_prefix(':')?.parse().ok())
}
