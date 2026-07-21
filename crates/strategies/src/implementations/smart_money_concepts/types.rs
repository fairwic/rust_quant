use crate::strategy_common::SignalResult;
use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Smart Money Concepts 策略动作；v1 research 只表达方向，不触发任何 live mutation。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmartMoneyConceptsAction {
    /// 已确认结构向上突破，生成多单研究信号。
    Long,
    /// 已确认结构向下突破，生成空单研究信号。
    Short,
    /// 结构、位置或风控证据不足时保持观望。
    Flat,
}

/// 已确认的 SMC 结构事件；名称保留 CHoCH/BOS，方便和 TradingView 标注对照。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartMoneyConceptsEvent {
    /// 当前 K 线没有形成可交易结构突破。
    None,
    /// 向上突破前高，初版按趋势切换处理。
    BullishChoch,
    /// 向上突破前高，延续多头结构。
    BullishBos,
    /// 向下跌破前低，初版按趋势切换处理。
    BearishChoch,
    /// 向下跌破前低，延续空头结构。
    BearishBos,
    /// 向下扫前低但收回，表示低点流动性被清理后的多头反转尝试。
    BullishLiquiditySweep,
    /// 向上扫前高但收回，表示高点流动性被清理后的空头反转尝试。
    BearishLiquiditySweep,
    /// 当前 K 线和两根前 K 线之间形成多头 fair value gap。
    BullishFairValueGap,
    /// 当前 K 线和两根前 K 线之间形成空头 fair value gap。
    BearishFairValueGap,
}

/// 仅由当前及此前已完成 K 线计算的结构特征；用于给其他研究策略做因果分层。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CausalMarketStructureFeatures {
    /// 当前收盘是否首次突破最近一个已确认摆动高点。
    pub bullish_structure_break: bool,
    /// 突破前结构已是抬高高低点时，标记为多头 BOS 延续。
    pub bullish_bos: bool,
    /// 突破前结构是降低高低点时，标记为多头 CHoCH 反转。
    pub bullish_choch: bool,
    /// 当前三根已完成 K 线是否形成多头 fair value gap。
    pub bullish_fvg: bool,
    /// 突破前最近两轮交替摆动是否构成高低点同步降低的空头结构。
    pub prior_bearish_structure: bool,
    /// 突破前最近两轮交替摆动是否构成高低点同步抬高的多头结构。
    pub prior_bullish_structure: bool,
    /// 最近一次多头 CHoCH 是否尚未被其保护低点的收盘跌破所否定。
    pub bullish_choch_active: bool,
    /// 最近一次仍有效多头 CHoCH 距当前已完成 K 线的数量。
    pub bullish_choch_age_bars: Option<usize>,
    /// 最近一次仍有效多头 CHoCH 突破的结构价格。
    pub bullish_choch_break_level: Option<f64>,
    /// 被当前收盘突破的最近已确认摆动高点；未突破时仍保留最近结构位。
    pub latest_confirmed_swing_high: Option<f64>,
    /// 最近一个已确认摆动低点；可用于审计 CHoCH 的保护低点。
    pub latest_confirmed_swing_low: Option<f64>,
    /// 当前结构突破收盘超过结构位的幅度，单位 ATR。
    pub bullish_structure_break_margin_atr: Option<f64>,
    /// 多头 FVG 下边界，即两根前 K 线最高价。
    pub bullish_fvg_lower: Option<f64>,
    /// 多头 FVG 上边界，即当前 K 线最低价。
    pub bullish_fvg_upper: Option<f64>,
    /// 当前新生多头 FVG 的宽度，单位 ATR。
    pub bullish_fvg_gap_atr: Option<f64>,
    /// 当前新生多头 FVG 中间位移 K 线的实体，单位 ATR。
    pub bullish_fvg_displacement_body_atr: Option<f64>,
    /// 最近一个尚未完全填补的有效多头 FVG 下边界。
    pub active_bullish_fvg_lower: Option<f64>,
    /// 最近一个尚未完全填补的有效多头 FVG 上边界。
    pub active_bullish_fvg_upper: Option<f64>,
    /// 最近一个有效多头 FVG 自形成后经过的已完成 K 线数量。
    pub active_bullish_fvg_age_bars: Option<usize>,
    /// 最近一个有效多头 FVG 已被价格向下填补的比例，范围 0～100。
    pub active_bullish_fvg_mitigated_pct: Option<f64>,
}

impl Default for SmartMoneyConceptsEvent {
    fn default() -> Self {
        Self::None
    }
}

impl SmartMoneyConceptsEvent {
    /// 事件是否对应多头入场方向。
    pub fn is_bullish(self) -> bool {
        matches!(
            self,
            Self::BullishChoch
                | Self::BullishBos
                | Self::BullishLiquiditySweep
                | Self::BullishFairValueGap
        )
    }

    /// 事件是否对应空头入场方向。
    pub fn is_bearish(self) -> bool {
        matches!(
            self,
            Self::BearishChoch
                | Self::BearishBos
                | Self::BearishLiquiditySweep
                | Self::BearishFairValueGap
        )
    }

    /// 审计 reason 使用稳定字符串，避免图表术语变化影响回测检索。
    pub fn reason(self) -> &'static str {
        match self {
            Self::None => "NO_STRUCTURE_BREAK",
            Self::BullishChoch => "SMART_MONEY_BULLISH_CHOCH",
            Self::BullishBos => "SMART_MONEY_BULLISH_BOS",
            Self::BearishChoch => "SMART_MONEY_BEARISH_CHOCH",
            Self::BearishBos => "SMART_MONEY_BEARISH_BOS",
            Self::BullishLiquiditySweep => "SMART_MONEY_BULLISH_LIQUIDITY_SWEEP",
            Self::BearishLiquiditySweep => "SMART_MONEY_BEARISH_LIQUIDITY_SWEEP",
            Self::BullishFairValueGap => "SMART_MONEY_BULLISH_FVG",
            Self::BearishFairValueGap => "SMART_MONEY_BEARISH_FVG",
        }
    }
}

/// SMC v1 的结构与风险门槛；默认偏研究保守，后续通过回测扫描再调整。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmartMoneyConceptsThresholds {
    /// 入场价格相对突破结构位的最大距离，单位 ATR；过大视为追单。
    pub max_entry_extension_atr: f64,
    /// 若要求回踩，价格距离 OB/FVG 边界的最大距离，单位 ATR。
    pub max_retest_distance_atr: f64,
    /// 是否必须回踩 order block 或 FVG 后才允许开仓。
    pub require_retest: bool,
    /// 是否要求结构突破方向和同周期快慢均线趋势一致。
    pub require_trend_alignment: bool,
    /// 是否要求多头位于 discount 半区、空头位于 premium 半区。
    pub require_premium_discount_zone: bool,
    /// 快慢均线趋势强度下限，单位百分比。
    pub min_trend_strength_pct: f64,
    /// 信号触发实体相对 ATR 的下限；用于过滤没有 displacement 的弱结构。
    pub min_displacement_body_atr: f64,
    /// ATR 占价格百分比下限，过低视为无波动结构。
    pub min_atr_pct: f64,
    /// ATR 占价格百分比上限，过高视为容易假突破。
    pub max_atr_pct: f64,
    /// 结构低/高之外的止损缓冲，单位 ATR。
    pub stop_atr_buffer: f64,
    /// 第一档止盈目标，单位 R。
    pub target_r_1: f64,
    /// 第二档止盈目标，单位 R。
    pub target_r_2: f64,
    /// 第三档止盈目标，单位 R。
    pub target_r_3: f64,
}

impl Default for SmartMoneyConceptsThresholds {
    fn default() -> Self {
        Self {
            max_entry_extension_atr: 1.2,
            max_retest_distance_atr: 0.75,
            require_retest: false,
            require_trend_alignment: false,
            require_premium_discount_zone: false,
            min_trend_strength_pct: 0.05,
            min_displacement_body_atr: 0.0,
            min_atr_pct: 0.0,
            max_atr_pct: 100.0,
            stop_atr_buffer: 0.25,
            target_r_1: 1.0,
            target_r_2: 2.0,
            target_r_3: 3.0,
        }
    }
}

/// 回测调参面；所有字段只作用于 research/backtest，不作为生产兼容配置。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmartMoneyConceptsBacktestTuning {
    /// pivot 需要等待的确认 K 线数量；只允许使用确认后的结构位。
    pub pivot_confirmation_bars: usize,
    /// 同一交易对开仓后的冷却 K 线数量。
    pub cooldown_candles: usize,
    /// 突破后等待 OB 回踩的最大 K 线数量；只在 require_retest=true 时生效。
    pub retest_max_wait_candles: usize,
    /// 是否允许研究空头结构信号。
    pub allow_short: bool,
    /// 是否启用扫流动性后收回的 SMC 反转信号。
    pub enable_liquidity_sweep: bool,
    /// 是否启用同周期 fair value gap 延续信号。
    pub enable_fair_value_gap: bool,
    /// 是否把已确认的 SMC 子信号作为 trap/fade 反向交易候选。
    pub fade_signal: bool,
    /// 快速趋势窗口；用于过滤逆势结构突破。
    pub trend_fast_window: usize,
    /// 慢速趋势窗口；必须不小于 fast，作为同周期方向过滤。
    pub trend_slow_window: usize,
    /// 回测使用的结构与风险门槛。
    pub thresholds: SmartMoneyConceptsThresholds,
}

impl Default for SmartMoneyConceptsBacktestTuning {
    fn default() -> Self {
        Self {
            pivot_confirmation_bars: 5,
            cooldown_candles: 8,
            retest_max_wait_candles: 4,
            allow_short: true,
            enable_liquidity_sweep: false,
            enable_fair_value_gap: false,
            fade_signal: false,
            trend_fast_window: 20,
            trend_slow_window: 96,
            thresholds: SmartMoneyConceptsThresholds::default(),
        }
    }
}

/// live/paper 可传入的结构快照；回测 adapter 也会从 OHLCV 增量构造同一形状。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SmartMoneyConceptsSignalSnapshot {
    /// 交易对标识；v1 research 不限制币种，但必须保留用于回测审计。
    pub symbol: String,
    /// 当前评估价格，通常为最新已确认 K 线收盘价。
    pub price: f64,
    /// 当前 ATR，用于限制追单距离、止损缓冲和回踩距离。
    pub atr: f64,
    /// 当前结构事件；None 表示没有可交易突破。
    pub event: SmartMoneyConceptsEvent,
    /// 被突破的确认结构位，例如前高或前低。
    pub break_level: f64,
    /// 多头止损保护位；None 表示结构低点不足，禁止多单。
    pub protected_low: Option<f64>,
    /// 空头止损保护位；None 表示结构高点不足，禁止空单。
    pub protected_high: Option<f64>,
    /// 最近 order block 下沿；用于收紧多头止损候选。
    pub order_block_low: Option<f64>,
    /// 最近 order block 上沿；用于收紧空头止损候选。
    pub order_block_high: Option<f64>,
    /// 入场价距离突破位的距离，单位 ATR。
    pub entry_extension_atr: f64,
    /// 入场价距离 OB/FVG 最近边界的距离，单位 ATR。
    pub retest_distance_atr: f64,
    /// 同周期趋势方向：long、short 或 flat。
    pub trend_bias: String,
    /// 快慢均线距离占价格百分比，用于过滤震荡区假突破。
    pub trend_strength_pct: f64,
    /// 触发信号的实体大小相对 ATR 的比例，用于判断是否具备 displacement。
    pub displacement_body_atr: f64,
    /// 当前价格在最近确认摆动低高区间中的位置，0 为低点、100 为高点。
    pub range_position_pct: Option<f64>,
}

/// 执行器配置：只接受带版本 strategy key，并允许上游直接注入结构快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SmartMoneyConceptsConfig {
    /// 策略审计 key；执行器只接受 `smart_money_concepts_v1_research`。
    pub strategy_key: Option<String>,
    /// 当前版本的结构与风险门槛。
    pub thresholds: SmartMoneyConceptsThresholds,
    /// 上游聚合后的结构快照；缺失时返回 flat。
    pub snapshot: Option<SmartMoneyConceptsSignalSnapshot>,
}

/// 策略评估后的领域决策，先保留审计原因，再转换为通用 SignalResult。
#[derive(Debug, Clone, PartialEq)]
pub struct SmartMoneyConceptsDecision {
    /// 多、空或观望动作。
    pub action: SmartMoneyConceptsAction,
    /// 阻断原因或成交确认原因；同时用于回测审计和参数迭代。
    pub reasons: Vec<String>,
}

impl SmartMoneyConceptsDecision {
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
            SmartMoneyConceptsAction::Long => self.apply_long_signal(&mut signal, price),
            SmartMoneyConceptsAction::Short => self.apply_short_signal(&mut signal, price),
            SmartMoneyConceptsAction::Flat => {}
        }
        signal
    }

    fn result_payload(&self) -> Value {
        json!({
            "strategy": "smart_money_concepts_v1_research",
            "action": self.action_name(),
            "reasons": self.reasons,
        })
    }

    fn action_name(&self) -> &'static str {
        match self.action {
            SmartMoneyConceptsAction::Long => "long",
            SmartMoneyConceptsAction::Short => "short",
            SmartMoneyConceptsAction::Flat => "flat",
        }
    }

    fn apply_long_signal(&self, signal: &mut SignalResult, price: f64) {
        let stop = reason_value(&self.reasons, "STOP_PRICE").unwrap_or(price);
        signal.should_buy = true;
        signal.direction = SignalDirection::Long;
        signal.signal_kline_stop_loss_price = Some(round_price(stop));
        signal.stop_loss_source = Some("SmartMoneyStructure".to_string());
        signal.atr_take_profit_level_1 = reason_value(&self.reasons, "TARGET_1").map(round_price);
        signal.atr_take_profit_level_2 = reason_value(&self.reasons, "TARGET_2").map(round_price);
        signal.atr_take_profit_level_3 = reason_value(&self.reasons, "TARGET_3").map(round_price);
    }

    fn apply_short_signal(&self, signal: &mut SignalResult, price: f64) {
        let stop = reason_value(&self.reasons, "STOP_PRICE").unwrap_or(price);
        signal.should_sell = true;
        signal.direction = SignalDirection::Short;
        signal.signal_kline_stop_loss_price = Some(round_price(stop));
        signal.stop_loss_source = Some("SmartMoneyStructure".to_string());
        signal.atr_take_profit_level_1 = reason_value(&self.reasons, "TARGET_1").map(round_price);
        signal.atr_take_profit_level_2 = reason_value(&self.reasons, "TARGET_2").map(round_price);
        signal.atr_take_profit_level_3 = reason_value(&self.reasons, "TARGET_3").map(round_price);
    }
}

/// 把策略生成价格四舍五入到确定精度。
pub fn round_price(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn reason_value(reasons: &[String], prefix: &str) -> Option<f64> {
    reasons
        .iter()
        .find_map(|reason| reason.strip_prefix(prefix)?.strip_prefix(':')?.parse().ok())
}
