use super::indicators::nearest_rank;
use super::model::{Candle, Direction, StrictVisualBreakoutResearchVariant};
use serde::Serialize;

const CANDIDATE_LENGTHS: [usize; 8] = [8, 12, 16, 24, 32, 48, 64, 96];
const MAXIMUM_WIDTH_RATIO: f64 = 0.03;
const MINIMUM_CONTAINMENT_RATIO: f64 = 0.80;
const MAXIMUM_DIRECTION_EFFICIENCY: f64 = 0.35;
const MINIMUM_EDGE_TRANSITIONS: usize = 3;
const MAXIMUM_UPPER_DRIFT: f64 = 0.25;
const MAXIMUM_LOWER_DRIFT: f64 = 0.35;
const MINIMUM_TOUCH_GAP: usize = 2;
const TOUCH_BAND_HEIGHT_RATIO: f64 = 0.10;
const TRANSITION_BAND_HEIGHT_RATIO: f64 = 0.20;
const ACCEPTANCE_WINDOW_BARS: usize = 3;
/// 双向新合同只在突破后的第 1～5 根完成棒内寻找首次合法回踩。
pub const STRICT_VISUAL_RETAINED_ACCEPTANCE_WINDOW_BARS: usize = 5;
const RETEST_BAND_PRICE_RATIO: f64 = 0.001;
const RETEST_BAND_ATR_RATIO: f64 = 0.25;
/// 强突破棒实体至少占整根高低振幅的 60%。
pub const STRICT_VISUAL_BREAKOUT_MIN_BODY_RATIO: f64 = 0.60;
/// 强突破棒方向实体位移至少达到开盘价的 25 bps，即 0.25%。
pub const STRICT_VISUAL_BREAKOUT_MIN_DIRECTIONAL_MOVE_RATIO: f64 = 0.0025;
/// 双向新合同把强突破实体占比放宽到 50%。
pub const STRICT_VISUAL_RETAINED_BREAKOUT_MIN_BODY_RATIO: f64 = 0.50;
/// 双向新合同要求方向实体涨跌幅至少 15 bps，即 0.15%。
pub const STRICT_VISUAL_RETAINED_BREAKOUT_MIN_DIRECTIONAL_MOVE_RATIO: f64 = 0.0015;
/// 合法回踩确认至少保留突破收盘越界幅度的 25%。
pub const STRICT_VISUAL_RETAINED_BREAKOUT_EXCESS_RATIO: f64 = 0.25;
/// V4 把已完成横盘长度不超过该阈值的 Fixed 退出冻结为 1R。
pub const STRICT_VISUAL_SHORT_RANGE_ONE_R_MAX_BARS: usize = 32;
/// V8 首次合法确认收盘必须至少高出冻结上沿 0.40 个突破来源 ATR。
pub const STRICT_VISUAL_BREAKOUT_MIN_ACCEPTANCE_MARGIN_ATR: f64 = 0.40;
/// 外部结构窗口最多回看 32 根 15 分钟完成棒，避免长横盘引用过远旧高。
pub const STRICT_VISUAL_EXTERNAL_STRUCTURE_MAX_LOOKBACK_BARS: usize = 32;

/// 完成突破棒在信号时可计算的方向实体强度；不读取后续 K 线。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrictVisualBreakoutBodyStrength {
    /// 实体绝对值除以完整高低振幅。
    pub body_ratio: f64,
    /// 按突破方向计算的实体位移除以开盘价；反向实体为负数。
    pub directional_move_ratio: f64,
    /// 两个冻结门槛是否同时满足。
    pub qualifies: bool,
}

/// 使用完成棒 OHLC 计算强突破门禁；多空方向完全镜像。
pub fn strict_visual_breakout_body_strength(
    candle: Candle,
    direction: Direction,
) -> StrictVisualBreakoutBodyStrength {
    strict_visual_breakout_body_strength_with_thresholds(
        candle,
        direction,
        STRICT_VISUAL_BREAKOUT_MIN_BODY_RATIO,
        STRICT_VISUAL_BREAKOUT_MIN_DIRECTIONAL_MOVE_RATIO,
    )
}

/// 按研究版本选择突破棒门槛；旧版本继续冻结 60%/25 bps，新合同使用 50%/15 bps。
pub fn strict_visual_breakout_body_strength_for_variant(
    candle: Candle,
    direction: Direction,
    variant: StrictVisualBreakoutResearchVariant,
) -> StrictVisualBreakoutBodyStrength {
    if variant.uses_symmetric_retained_breakout_contract() {
        strict_visual_breakout_body_strength_with_thresholds(
            candle,
            direction,
            STRICT_VISUAL_RETAINED_BREAKOUT_MIN_BODY_RATIO,
            STRICT_VISUAL_RETAINED_BREAKOUT_MIN_DIRECTIONAL_MOVE_RATIO,
        )
    } else {
        strict_visual_breakout_body_strength(candle, direction)
    }
}

fn strict_visual_breakout_body_strength_with_thresholds(
    candle: Candle,
    direction: Direction,
    minimum_body_ratio: f64,
    minimum_directional_move_ratio: f64,
) -> StrictVisualBreakoutBodyStrength {
    let full_range = candle.high - candle.low;
    let body = (candle.close - candle.open).abs();
    let directional_body = match direction {
        Direction::Long => candle.close - candle.open,
        Direction::Short => candle.open - candle.close,
    };
    let body_ratio = if full_range > 0.0 {
        body / full_range
    } else {
        0.0
    };
    let directional_move_ratio = if candle.open > 0.0 {
        directional_body / candle.open
    } else {
        0.0
    };
    StrictVisualBreakoutBodyStrength {
        body_ratio,
        directional_move_ratio,
        qualifies: body_ratio >= minimum_body_ratio
            && directional_move_ratio >= minimum_directional_move_ratio,
    }
}

/// 只按突破时已经冻结的区间长度选择 V4 退出分支，后续确认棒不能重算。
const fn frozen_short_range_one_r_target(
    variant: StrictVisualBreakoutResearchVariant,
    range_length_bars: usize,
) -> bool {
    variant.uses_short_range_one_r_target()
        && range_length_bars <= STRICT_VISUAL_SHORT_RANGE_ONE_R_MAX_BARS
}

/// 严格视觉横盘在某个完成时点可用于后续交易判断的冻结证据。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrictVisualRangeEvidence {
    /// 当前父横盘首根完成 K 线在输入序列中的索引。
    pub start_index: usize,
    /// 首次确认该活动横盘的完成棒索引；此前的左侧区域只是事后绘图。
    pub first_confirmation_index: usize,
    /// 当前边界最近一次由完成棒确认的索引；新边界只能影响其后的 K 线。
    pub boundary_confirmation_index: usize,
    /// 当前父横盘包含的完成 K 线数量，只取冻结离散长度集合。
    pub length_bars: usize,
    /// 高点 P90 上沿，价格单位与交易对一致。
    pub upper: f64,
    /// 低点 P10 下沿，价格单位与交易对一致。
    pub lower: f64,
    /// 位于边界容差带内的收盘比例，范围为 0～1。
    pub containment_ratio: f64,
    /// 收盘净位移除以完整收盘路径，范围为 0～1。
    pub direction_efficiency: f64,
    /// 上、下边界按完成棒时间顺序发生的独立切换次数。
    pub edge_transition_count: usize,
    /// 上沿独立触碰组数量，相邻贴边棒只记为同一组。
    pub upper_touch_groups: usize,
    /// 下沿独立触碰组数量，相邻贴边棒只记为同一组。
    pub lower_touch_groups: usize,
    /// 近期与早期 P80 上沿差异除以区间高度。
    pub upper_drift_ratio: f64,
    /// 近期与早期 P20 下沿差异除以区间高度。
    pub lower_drift_ratio: f64,
}

/// V9 在突破棒完成时冻结的外部结构上沿证据；确认棒不得重算。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrictVisualExternalStructureEvidence {
    /// 实际使用的横盘前完成棒数量，等于 `min(range_length_bars, 32)`。
    pub lookback_bars: usize,
    /// 外部窗口首根完成棒索引。
    pub window_start_index: usize,
    /// 外部窗口最高价所属完成棒索引；同价时取最近一根。
    pub external_high_index: usize,
    /// 横盘开始前自适应窗口内的最高价。
    pub external_high: f64,
    /// 横盘开始后、突破前是否已有完成收盘至少高出外部高点一个 tick。
    pub resolved_before_breakout: bool,
    /// 用于交易门禁的上沿；视觉 P90 上沿仍单独保留。
    pub trade_breakout_upper: f64,
    /// 突破棒完成收盘必须达到的最小价格。
    pub required_breakout_close: f64,
    /// 突破收盘相对交易上沿的 tick 数，可为负数。
    pub breakout_clearance_ticks: f64,
    /// 突破棒完成收盘是否至少越过交易上沿一个 tick。
    pub qualifies: bool,
}

/// 仅使用突破时已完成 K 线冻结 V9 外部结构证据；前置窗口不足时失败关闭。
pub fn strict_visual_external_structure_evidence(
    candles: &[Candle],
    range: StrictVisualRangeEvidence,
    breakout_index: usize,
    tick_size: f64,
) -> Option<StrictVisualExternalStructureEvidence> {
    if tick_size <= 0.0 || breakout_index <= range.start_index {
        return None;
    }
    let lookback_bars = range
        .length_bars
        .min(STRICT_VISUAL_EXTERNAL_STRUCTURE_MAX_LOOKBACK_BARS);
    let window_start_index = range.start_index.checked_sub(lookback_bars)?;
    let history = candles.get(window_start_index..range.start_index)?;
    let breakout = *candles.get(breakout_index)?;
    let (external_high_offset, external_high) = history
        .iter()
        .enumerate()
        .max_by(|left, right| {
            left.1
                .high
                .total_cmp(&right.1.high)
                .then_with(|| left.0.cmp(&right.0))
        })
        .map(|(offset, candle)| (offset, candle.high))?;
    let external_high_index = window_start_index + external_high_offset;
    let resolved_before_breakout = candles
        .get(range.start_index..breakout_index)?
        .iter()
        .any(|candle| candle.close >= external_high + tick_size);
    let trade_breakout_upper = if resolved_before_breakout {
        range.upper
    } else {
        range.upper.max(external_high)
    };
    let required_breakout_close = trade_breakout_upper + tick_size;
    let breakout_clearance_ticks = (breakout.close - trade_breakout_upper) / tick_size;
    Some(StrictVisualExternalStructureEvidence {
        lookback_bars,
        window_start_index,
        external_high_index,
        external_high,
        resolved_before_breakout,
        trade_breakout_upper,
        required_breakout_close,
        breakout_clearance_ticks,
        qualifies: breakout.close >= required_breakout_close,
    })
}

/// 弱离区相对于冻结横盘的方向；只描述结构，不代表开仓方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrictVisualDepartureSide {
    /// 完成收盘高于冻结上沿。
    Upper,
    /// 完成收盘低于冻结下沿。
    Lower,
}

/// V6 弱离区等待与紧邻完成棒决策共享的冻结证据。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrictVisualWeakDepartureEvidence {
    /// 弱离区前已经确认且保持不变的活动横盘。
    pub range: StrictVisualRangeEvidence,
    /// 首根弱离区完成棒索引。
    pub departure_index: usize,
    /// 紧邻决策棒索引；初次进入 pending 时为 `None`。
    pub confirmation_index: Option<usize>,
    /// 弱离区发生在冻结上沿或下沿。
    pub side: StrictVisualDepartureSide,
}

/// 单根完成 K 线对严格横盘状态产生的唯一结构事件。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StrictVisualRangeEvent {
    /// 首次确认活动横盘；该棒不能同时成为突破棒。
    Confirmed(StrictVisualRangeEvidence),
    /// 本棒确认了更长的重叠父区间，新边界从下一根开始生效。
    ParentUpgraded(StrictVisualRangeEvidence),
    /// 首次完成收盘高于此前已知上沿，活动区间已被消费。
    UpperBreak(StrictVisualRangeEvidence),
    /// 首次完成收盘低于此前已知下沿，活动区间已被消费。
    LowerBreak(StrictVisualRangeEvidence),
    /// V6 首根弱离区冻结原边界，等待紧邻下一根完成棒。
    WeakDeparturePending(StrictVisualWeakDepartureEvidence),
    /// V6 紧邻下一根完成收盘回到含边界的原区间，活动横盘恢复。
    WeakDepartureReturned(StrictVisualWeakDepartureEvidence),
    /// V6 紧邻下一根仍在任一边界外，旧横盘被消费且确认棒不补算锚点。
    WeakDepartureConsumed(StrictVisualWeakDepartureEvidence),
}

/// 严格横盘突破来源与后续确认共享的冻结交易证据。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StrictVisualBreakoutSignal {
    /// 突破前已经确认并冻结的严格横盘。
    pub range: StrictVisualRangeEvidence,
    /// 冻结突破方向；V1～V9 恒为多头，新合同允许完全镜像的空头。
    pub direction: Direction,
    /// 首次合格突破棒在输入序列中的索引。
    pub breakout_index: usize,
    /// 实际产生入场意图的完成棒索引；V1 等于突破棒，V2 为接受确认棒。
    pub signal_index: usize,
    /// 突破棒完成开盘价；V3 只用它冻结实体中点，不随确认棒更新。
    pub breakout_open: f64,
    /// 突破棒完成收盘价，只用于审计来源，不随确认棒更新。
    pub breakout_close: f64,
    /// V11/V12 在突破棒完成时冻结的整根极值外一 tick 结构止损；旧版本为 `None`。
    pub breakout_candle_extreme_stop_price: Option<f64>,
    /// V12 声明最终风险至少为确认信号 ATR 的该倍数；`None` 表示不扩展结构止损。
    pub breakout_candle_extreme_stop_min_atr_multiple: Option<f64>,
    /// 突破棒开收盘均价；V3 用它拒绝已经回吐过多实体位移的首次确认棒。
    pub breakout_body_midpoint: f64,
    /// 突破棒 ATR14，V2 延后确认时仍用于冻结止损与目标距离。
    pub source_atr: f64,
    /// 突破棒过滤量比；新合同只作诊断且允许用 0 表示当时不可用。
    pub source_volume_ratio: f64,
    /// 突破棒量能档位对应的 ATR 目标倍数；新合同不读取该值。
    pub source_take_profit_atr: f64,
    /// `true` 仅用于冻结 V1～V9 的旧量能合同，避免新合同的诊断值污染归因。
    pub volume_gate_applied: bool,
    /// V4 是否在既有 Fixed 退出分支使用 1R；在突破源棒完成时冻结。
    pub short_range_one_r_target: bool,
    /// 允许的边界回踩带，价格单位与交易对一致。
    pub retest_band: f64,
    /// 突破收盘相对交易边界的正向越界幅度；确认期间不可重算。
    pub breakout_excess: f64,
    /// 新合同确认棒必须守住的绝对收盘价；旧版本只保存对应边界。
    pub required_acceptance_close: f64,
    /// 新合同冻结的一个横盘高度量度目标；旧版本为 `None`。
    pub measured_move_target_price: Option<f64>,
    /// 突破棒完成时冻结的外部结构上沿；窗口不足时为 `None` 并由 V9 失败关闭。
    pub external_structure: Option<StrictVisualExternalStructureEvidence>,
}

/// 单根完成 K 线对严格横盘入场状态产生的唯一事件；类型名保留旧 API 兼容。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StrictVisualLongEntryEvent {
    /// 横盘确认、升级或未满足交易门禁的结构离开。
    Range(StrictVisualRangeEvent),
    /// V1 在合格突破棒收盘后直接形成入场信号。
    DirectSignal(StrictVisualBreakoutSignal),
    /// 冻结合格突破来源并开始有限接受窗口。
    AcceptanceArmed(StrictVisualBreakoutSignal),
    /// V2 首次完成上沿回踩并收盘守稳，当前棒形成入场信号。
    AcceptanceConfirmed(StrictVisualBreakoutSignal),
    /// V3 在 V2 首次可确认棒上跌破冻结实体中点，来源被消费且不得等待补开。
    AcceptanceBodyMidpointRejected(StrictVisualBreakoutSignal),
    /// V8 首次合法确认棒高出冻结上沿不足 0.40 个来源 ATR，来源被消费且不得补开。
    AcceptanceMarginRejected(StrictVisualBreakoutSignal),
    /// V9 在 V8 原确认时点拒绝未越过尚未解决外部结构高点的突破来源。
    ExternalStructureRejected(StrictVisualBreakoutSignal),
    /// V2 等待期间完成收盘回到冻结上沿或更低，来源永久失效。
    AcceptanceInvalidated(StrictVisualBreakoutSignal),
    /// 版本对应的最后一根完成棒仍未回踩确认，来源永久过期。
    AcceptanceExpired(StrictVisualBreakoutSignal),
}

impl StrictVisualLongEntryEvent {
    /// 只有 V1 直接信号或 V2 已完成接受确认时才交给主策略订单意图层。
    pub const fn entry_signal(self) -> Option<StrictVisualBreakoutSignal> {
        match self {
            Self::DirectSignal(signal) | Self::AcceptanceConfirmed(signal) => Some(signal),
            _ => None,
        }
    }
}

/// 严格横盘结构与有限接受窗口的组合状态；不读取未来 K 线或交易结果。
#[derive(Debug, Default)]
pub struct StrictVisualLongEntryState {
    range_state: StrictVisualConsolidationState,
    pending_acceptance: Option<StrictVisualBreakoutSignal>,
}

impl StrictVisualLongEntryState {
    /// 推进当前完成棒；V2 始终先处理旧来源，再允许当前棒创建新的横盘结构事件。
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        candles: &[Candle],
        index: usize,
        tick_size: f64,
        source_atr: Option<f64>,
        volume_event: bool,
        volume_ratio: Option<f64>,
        take_profit_atr: Option<f64>,
        variant: StrictVisualBreakoutResearchVariant,
    ) -> Option<StrictVisualLongEntryEvent> {
        let candle = *candles.get(index)?;

        if let Some(source) = self.pending_acceptance {
            let age = index.saturating_sub(source.breakout_index);
            let symmetric_contract = variant.uses_symmetric_retained_breakout_contract();
            let acceptance_window = if symmetric_contract {
                STRICT_VISUAL_RETAINED_ACCEPTANCE_WINDOW_BARS
            } else {
                ACCEPTANCE_WINDOW_BARS
            };
            let invalidated = match source.direction {
                Direction::Long => candle.close <= source.range.upper,
                Direction::Short => candle.close >= source.range.lower,
            };
            let touched_boundary = match source.direction {
                Direction::Long => candle.low <= source.range.upper + source.retest_band,
                Direction::Short => candle.high >= source.range.lower - source.retest_band,
            };
            let retained_breakout = match source.direction {
                Direction::Long => candle.close >= source.required_acceptance_close,
                Direction::Short => candle.close <= source.required_acceptance_close,
            };
            let accepted_by_v2 = (1..=acceptance_window).contains(&age)
                && !invalidated
                && touched_boundary
                && (!symmetric_contract || retained_breakout);
            if accepted_by_v2 {
                self.pending_acceptance = None;
                let decided = StrictVisualBreakoutSignal {
                    signal_index: index,
                    ..source
                };
                // V3 必须在 V2 原本确认的同一根棒上消费来源，避免过滤后等待产生新信号身份。
                if variant.requires_breakout_body_midpoint_hold()
                    && candle.close < source.breakout_body_midpoint
                {
                    return Some(StrictVisualLongEntryEvent::AcceptanceBodyMidpointRejected(
                        decided,
                    ));
                }
                // 分母必须固定为突破棒 ATR；确认棒传入的 ATR 只属于当前行情，不能重写来源身份。
                let acceptance_margin_atr = if source.source_atr > 0.0 {
                    (candle.close - source.range.upper) / source.source_atr
                } else {
                    f64::NEG_INFINITY
                };
                if variant.requires_acceptance_margin_40_atr()
                    && acceptance_margin_atr < STRICT_VISUAL_BREAKOUT_MIN_ACCEPTANCE_MARGIN_ATR
                {
                    return Some(StrictVisualLongEntryEvent::AcceptanceMarginRejected(
                        decided,
                    ));
                }
                if variant.requires_external_structure_clearance()
                    && !source
                        .external_structure
                        .is_some_and(|evidence| evidence.qualifies)
                {
                    return Some(StrictVisualLongEntryEvent::ExternalStructureRejected(
                        decided,
                    ));
                }
                return Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(decided));
            }
            if invalidated {
                self.pending_acceptance = None;
                return Some(StrictVisualLongEntryEvent::AcceptanceInvalidated(source));
            }
            if age >= acceptance_window {
                self.pending_acceptance = None;
                return Some(StrictVisualLongEntryEvent::AcceptanceExpired(source));
            }
            return None;
        }

        let range_event = self
            .range_state
            .update_for_variant(candles, index, tick_size, variant)?;
        let (direction, range) = match range_event {
            StrictVisualRangeEvent::UpperBreak(range) => (Direction::Long, range),
            StrictVisualRangeEvent::LowerBreak(range)
                if variant.uses_symmetric_retained_breakout_contract() =>
            {
                (Direction::Short, range)
            }
            _ => return Some(StrictVisualLongEntryEvent::Range(range_event)),
        };
        if variant.requires_breakout_body_strength()
            && !strict_visual_breakout_body_strength_for_variant(candle, direction, variant)
                .qualifies
        {
            // 弱离区仍消费活动横盘，但不能冻结为 V5 突破来源或进入接受窗口。
            return Some(StrictVisualLongEntryEvent::Range(range_event));
        }
        let Some(source_atr) = source_atr.filter(|value| *value > 0.0) else {
            return Some(StrictVisualLongEntryEvent::Range(range_event));
        };
        let symmetric_contract = variant.uses_symmetric_retained_breakout_contract();
        let (source_volume_ratio, source_take_profit_atr) = if symmetric_contract {
            (volume_ratio.unwrap_or_default(), 0.0)
        } else {
            let (Some(source_volume_ratio), Some(source_take_profit_atr)) =
                (volume_ratio, take_profit_atr)
            else {
                return Some(StrictVisualLongEntryEvent::Range(range_event));
            };
            if !volume_event || candle.close <= candle.open {
                return Some(StrictVisualLongEntryEvent::Range(range_event));
            }
            (source_volume_ratio, source_take_profit_atr)
        };
        let boundary = match direction {
            Direction::Long => range.upper,
            Direction::Short => range.lower,
        };
        let breakout_excess = match direction {
            Direction::Long => candle.close - boundary,
            Direction::Short => boundary - candle.close,
        };
        let retained_excess =
            tick_size.max(breakout_excess * STRICT_VISUAL_RETAINED_BREAKOUT_EXCESS_RATIO);
        let required_acceptance_close = if symmetric_contract {
            match direction {
                Direction::Long => boundary + retained_excess,
                Direction::Short => boundary - retained_excess,
            }
        } else {
            boundary
        };
        let range_height = range.upper - range.lower;
        let breakout_candle_extreme_stop_price =
            variant
                .uses_breakout_candle_extreme_stop()
                .then(|| match direction {
                    Direction::Long => round_down(candle.low - tick_size, tick_size),
                    Direction::Short => round_up(candle.high + tick_size, tick_size),
                });
        let breakout_candle_extreme_stop_min_atr_multiple = (variant
            == StrictVisualBreakoutResearchVariant::V12ExtremeStopMinOneAtr)
            .then_some(1.0);

        let source = StrictVisualBreakoutSignal {
            range,
            direction,
            breakout_index: index,
            signal_index: index,
            breakout_open: candle.open,
            breakout_close: candle.close,
            breakout_candle_extreme_stop_price,
            breakout_candle_extreme_stop_min_atr_multiple,
            breakout_body_midpoint: (candle.open + candle.close) * 0.5,
            source_atr,
            source_volume_ratio,
            source_take_profit_atr,
            volume_gate_applied: !symmetric_contract,
            short_range_one_r_target: frozen_short_range_one_r_target(variant, range.length_bars),
            retest_band: (boundary.abs() * RETEST_BAND_PRICE_RATIO)
                .max(source_atr * RETEST_BAND_ATR_RATIO),
            breakout_excess,
            required_acceptance_close,
            measured_move_target_price: symmetric_contract.then_some(match direction {
                Direction::Long => range.upper + range_height,
                Direction::Short => range.lower - range_height,
            }),
            external_structure: (!symmetric_contract)
                .then(|| {
                    strict_visual_external_structure_evidence(candles, range, index, tick_size)
                })
                .flatten(),
        };
        if variant.requires_retest_acceptance() {
            self.pending_acceptance = Some(source);
            Some(StrictVisualLongEntryEvent::AcceptanceArmed(source))
        } else {
            Some(StrictVisualLongEntryEvent::DirectSignal(source))
        }
    }
}

/// 多单保护位向下对齐到交易所价格精度，避免浮点误差把止损抬回突破棒内。
fn round_down(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).floor() * tick_size
}

/// 空单保护位向上对齐到交易所价格精度，避免浮点误差把止损压回突破棒内。
fn round_up(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).ceil() * tick_size
}

/// 只按完成 K 线向前推进的严格视觉横盘状态机。
#[derive(Debug, Default)]
pub struct StrictVisualConsolidationState {
    /// 上一完成时点已经确认、可供当前棒判断离区的冻结横盘。
    active: Option<StrictVisualRangeEvidence>,
    /// V6 首根弱离区冻结的原边界；只允许紧邻下一根完成棒解决。
    pending_weak_departure: Option<StrictVisualWeakDepartureEvidence>,
    /// 最近一次离开活动区间的完成棒；新横盘必须从其后重新积累。
    last_closed_index: Option<usize>,
}

impl StrictVisualConsolidationState {
    /// 推进一根完成 K 线；返回值只描述本棒新发生的确认、升级或离开事件。
    pub fn update(
        &mut self,
        candles: &[Candle],
        index: usize,
        tick_size: f64,
    ) -> Option<StrictVisualRangeEvent> {
        self.update_for_variant(
            candles,
            index,
            tick_size,
            StrictVisualBreakoutResearchVariant::Baseline,
        )
    }

    /// 按独立 Research 版本推进结构；只有 V6 改变弱离区生命周期。
    pub fn update_for_variant(
        &mut self,
        candles: &[Candle],
        index: usize,
        tick_size: f64,
        variant: StrictVisualBreakoutResearchVariant,
    ) -> Option<StrictVisualRangeEvent> {
        let candle = candles
            .get(index)
            .copied()
            .filter(|candle| candle.is_valid())?;
        if tick_size <= 0.0 {
            return None;
        }

        if let Some(pending) = self.pending_weak_departure.take() {
            if index <= pending.departure_index {
                self.pending_weak_departure = Some(pending);
                return None;
            }
            let resolved = StrictVisualWeakDepartureEvidence {
                confirmation_index: Some(index),
                ..pending
            };
            if candle.close >= pending.range.lower && candle.close <= pending.range.upper {
                // 回区棒只解决 pending，不能同棒升级边界或再次突破。
                return Some(StrictVisualRangeEvent::WeakDepartureReturned(resolved));
            }
            self.active = None;
            // 旧横盘在首根弱离区处结束，确认棒可以作为后续新结构的第一根。
            self.last_closed_index = Some(pending.departure_index);
            return Some(StrictVisualRangeEvent::WeakDepartureConsumed(resolved));
        }

        // 必须先用上一完成时点已知的边界判断离开；本棒发现的父区间不能反改本棒突破。
        if let Some(active) = self.active {
            if candle.close > active.upper {
                if variant.uses_weak_departure_probation()
                    && !strict_visual_breakout_body_strength_for_variant(
                        candle,
                        Direction::Long,
                        variant,
                    )
                    .qualifies
                {
                    let pending = StrictVisualWeakDepartureEvidence {
                        range: active,
                        departure_index: index,
                        confirmation_index: None,
                        side: StrictVisualDepartureSide::Upper,
                    };
                    self.pending_weak_departure = Some(pending);
                    return Some(StrictVisualRangeEvent::WeakDeparturePending(pending));
                }
                self.active = None;
                self.last_closed_index = Some(index);
                return Some(StrictVisualRangeEvent::UpperBreak(active));
            }
            if candle.close < active.lower {
                if variant.uses_weak_departure_probation()
                    && !strict_visual_breakout_body_strength_for_variant(
                        candle,
                        Direction::Short,
                        variant,
                    )
                    .qualifies
                {
                    let pending = StrictVisualWeakDepartureEvidence {
                        range: active,
                        departure_index: index,
                        confirmation_index: None,
                        side: StrictVisualDepartureSide::Lower,
                    };
                    self.pending_weak_departure = Some(pending);
                    return Some(StrictVisualRangeEvent::WeakDeparturePending(pending));
                }
                self.active = None;
                self.last_closed_index = Some(index);
                return Some(StrictVisualRangeEvent::LowerBreak(active));
            }

            if let Some(candidate) = longest_candidate(candles, index, tick_size, None) {
                let overlaps = candidate.upper >= active.lower && candidate.lower <= active.upper;
                if overlaps && candidate.start_index < active.start_index {
                    let upgraded = StrictVisualRangeEvidence {
                        first_confirmation_index: active.first_confirmation_index,
                        ..candidate
                    };
                    self.active = Some(upgraded);
                    return Some(StrictVisualRangeEvent::ParentUpgraded(upgraded));
                }
            }
            return None;
        }

        // 新合同先排除跨越旧突破的父窗口，避免无效长窗口遮住有效短窗口；旧版本保持冻结选择顺序。
        let minimum_start_index = variant
            .uses_symmetric_retained_breakout_contract()
            .then(|| self.last_closed_index.map(|closed| closed + 1))
            .flatten();
        let candidate = longest_candidate(candles, index, tick_size, minimum_start_index)?;
        if !variant.uses_symmetric_retained_breakout_contract()
            && self
                .last_closed_index
                .is_some_and(|closed| candidate.start_index <= closed)
        {
            return None;
        }
        self.active = Some(candidate);
        Some(StrictVisualRangeEvent::Confirmed(candidate))
    }

    /// 结构离开无论是否可交易都会被消费；只有同一上破棒满足既有量能合同才返回突破线。
    pub fn qualified_long_breakout_line(
        &mut self,
        candles: &[Candle],
        index: usize,
        tick_size: f64,
        volume_event: bool,
        has_take_profit_tier: bool,
    ) -> Option<f64> {
        let candle = candles.get(index)?;
        match self.update(candles, index, tick_size) {
            Some(StrictVisualRangeEvent::UpperBreak(range))
                if volume_event && has_take_profit_tier && candle.close > candle.open =>
            {
                Some(range.upper)
            }
            _ => None,
        }
    }
}

/// 当前完成棒只选择最长有效父区间，保持与冻结视觉指标相同的离散扫描顺序。
fn longest_candidate(
    candles: &[Candle],
    index: usize,
    tick_size: f64,
    minimum_start_index: Option<usize>,
) -> Option<StrictVisualRangeEvidence> {
    let mut selected = None;
    for length in CANDIDATE_LENGTHS {
        if length <= index + 1 {
            if let Some(candidate) =
                evaluate_candidate(candles, index, length, tick_size).filter(|candidate| {
                    minimum_start_index.is_none_or(|minimum| candidate.start_index >= minimum)
                })
            {
                selected = Some(candidate);
            }
        }
    }
    selected
}

/// 固定长度同时验证边界、往返、路径与漂移，任何条件缺失都不构造视觉横盘。
fn evaluate_candidate(
    candles: &[Candle],
    index: usize,
    length: usize,
    tick_size: f64,
) -> Option<StrictVisualRangeEvidence> {
    let start = index + 1 - length;
    let history = candles.get(start..=index)?;
    if history.iter().any(|candle| !candle.is_valid()) {
        return None;
    }
    let highs: Vec<f64> = history.iter().map(|candle| candle.high).collect();
    let lows: Vec<f64> = history.iter().map(|candle| candle.low).collect();
    let upper = nearest_rank(&highs, 90.0)?;
    let lower = nearest_rank(&lows, 10.0)?;
    let height = upper - lower;
    let middle = (upper + lower) * 0.5;
    if height <= 0.0 || middle <= 0.0 || height / middle > MAXIMUM_WIDTH_RATIO {
        return None;
    }

    let touch_band = (tick_size * 2.0).max(height * TOUCH_BAND_HEIGHT_RATIO);
    let upper_touch_groups = touch_groups(history, |candle| candle.high >= upper - touch_band);
    let lower_touch_groups = touch_groups(history, |candle| candle.low <= lower + touch_band);
    let contained = history
        .iter()
        .filter(|candle| candle.close <= upper + touch_band && candle.close >= lower - touch_band)
        .count();
    let containment_ratio = contained as f64 / length as f64;

    let half = length / 2;
    let earlier_highs = &highs[..half];
    let recent_highs = &highs[half..];
    let earlier_lows = &lows[..half];
    let recent_lows = &lows[half..];
    let upper_drift_ratio =
        (nearest_rank(recent_highs, 80.0)? - nearest_rank(earlier_highs, 80.0)?).abs() / height;
    let lower_drift_ratio =
        (nearest_rank(recent_lows, 20.0)? - nearest_rank(earlier_lows, 20.0)?).abs() / height;
    let direction_efficiency = close_direction_efficiency(history)?;
    let edge_transition_count = edge_transition_count(history, upper, lower, tick_size);

    (upper_touch_groups >= 2
        && lower_touch_groups >= 2
        && containment_ratio >= MINIMUM_CONTAINMENT_RATIO
        && upper_drift_ratio <= MAXIMUM_UPPER_DRIFT
        && lower_drift_ratio <= MAXIMUM_LOWER_DRIFT
        && direction_efficiency <= MAXIMUM_DIRECTION_EFFICIENCY
        && edge_transition_count >= MINIMUM_EDGE_TRANSITIONS)
        .then_some(StrictVisualRangeEvidence {
            start_index: start,
            first_confirmation_index: index,
            boundary_confirmation_index: index,
            length_bars: length,
            upper,
            lower,
            containment_ratio,
            direction_efficiency,
            edge_transition_count,
            upper_touch_groups,
            lower_touch_groups,
            upper_drift_ratio,
            lower_drift_ratio,
        })
}

fn touch_groups(history: &[Candle], touches: impl Fn(&Candle) -> bool) -> usize {
    let mut groups = 0;
    let mut last_touch = None;
    for (chronological_index, candle) in history.iter().enumerate() {
        if touches(candle) {
            if last_touch.is_none_or(|last| chronological_index - last >= MINIMUM_TOUCH_GAP) {
                groups += 1;
            }
            last_touch = Some(chronological_index);
        }
    }
    groups
}

fn close_direction_efficiency(history: &[Candle]) -> Option<f64> {
    let first = history.first()?.close;
    let last = history.last()?.close;
    let path = history
        .windows(2)
        .map(|pair| (pair[1].close - pair[0].close).abs())
        .sum::<f64>();
    Some(if path <= f64::EPSILON {
        0.0
    } else {
        (last - first).abs() / path
    })
}

fn edge_transition_count(history: &[Candle], upper: f64, lower: f64, tick_size: f64) -> usize {
    let band = (tick_size * 2.0).max((upper - lower) * TRANSITION_BAND_HEIGHT_RATIO);
    let mut previous_edge = 0_i8;
    let mut transitions = 0;
    for candle in history {
        let touches_upper = candle.high >= upper - band;
        let touches_lower = candle.low <= lower + band;
        let edge = match (touches_upper, touches_lower) {
            (true, false) => 1,
            (false, true) => -1,
            _ => 0,
        };
        if edge != 0 {
            if previous_edge != 0 && edge != previous_edge {
                transitions += 1;
            }
            previous_edge = edge;
        }
    }
    transitions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge_candle(index: usize, upper: bool) -> Candle {
        let (open, high, low, close) = if upper {
            (100.4, 101.0, 100.2, 100.7)
        } else {
            (99.6, 99.8, 99.0, 99.3)
        };
        Candle {
            timestamp_ms: index as i64 * 900_000,
            open,
            high,
            low,
            close,
            volume: 100.0,
        }
    }

    fn external_structure_fixture() -> (Vec<Candle>, StrictVisualRangeEvidence) {
        let mut candles = (0..8)
            .map(|index| Candle {
                timestamp_ms: index as i64 * 900_000,
                open: 90.0 + index as f64,
                high: if index == 5 {
                    102.0
                } else {
                    91.0 + index as f64
                },
                low: 89.0 + index as f64,
                close: 90.5 + index as f64,
                volume: 100.0,
            })
            .collect::<Vec<_>>();
        candles.extend((8..16).map(|index| edge_candle(index, index % 2 == 0)));
        let range = StrictVisualRangeEvidence {
            start_index: 8,
            first_confirmation_index: 15,
            boundary_confirmation_index: 15,
            length_bars: 8,
            upper: 101.0,
            lower: 99.0,
            containment_ratio: 1.0,
            direction_efficiency: 0.0,
            edge_transition_count: 3,
            upper_touch_groups: 2,
            lower_touch_groups: 2,
            upper_drift_ratio: 0.0,
            lower_drift_ratio: 0.0,
        };
        (candles, range)
    }

    #[test]
    fn external_structure_requires_one_tick_close_above_unresolved_pre_range_high() {
        let (mut candles, range) = external_structure_fixture();
        candles.push(Candle {
            timestamp_ms: 16 * 900_000,
            open: 100.8,
            high: 102.0,
            low: 100.7,
            close: 101.5,
            volume: 300.0,
        });
        let evidence = strict_visual_external_structure_evidence(&candles, range, 16, 0.01)
            .expect("full pre-range window should freeze external evidence");
        assert_eq!(evidence.lookback_bars, 8);
        assert_eq!(evidence.external_high_index, 5);
        assert_eq!(evidence.external_high, 102.0);
        assert!(!evidence.resolved_before_breakout);
        assert_eq!(evidence.trade_breakout_upper, 102.0);
        assert!(!evidence.qualifies);
    }

    #[test]
    fn v9_consumes_external_failure_at_the_v8_confirmation_point() {
        let (mut candles, _) = external_structure_fixture();
        let variant = StrictVisualBreakoutResearchVariant::V9ExternalStructureClearance;
        let mut state = StrictVisualLongEntryState::default();
        for index in 0..16 {
            state.update(&candles, index, 0.01, Some(0.5), false, None, None, variant);
        }
        candles.push(Candle {
            timestamp_ms: 16 * 900_000,
            open: 100.8,
            high: 101.6,
            low: 100.7,
            close: 101.5,
            volume: 300.0,
        });
        assert!(matches!(
            state.update(
                &candles,
                16,
                0.01,
                Some(0.5),
                true,
                Some(3.0),
                Some(2.7),
                variant,
            ),
            Some(StrictVisualLongEntryEvent::AcceptanceArmed(_))
        ));
        candles.push(Candle {
            timestamp_ms: 17 * 900_000,
            open: 101.3,
            high: 101.4,
            low: 101.1,
            close: 101.3,
            volume: 100.0,
        });
        assert!(matches!(
            state.update(&candles, 17, 0.01, Some(0.5), false, None, None, variant,),
            Some(StrictVisualLongEntryEvent::ExternalStructureRejected(_))
        ));
    }

    fn accepted_signal(variant: StrictVisualBreakoutResearchVariant) -> StrictVisualBreakoutSignal {
        let mut candles: Vec<_> = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect();
        let mut state = StrictVisualLongEntryState::default();
        for index in 0..8 {
            state.update(&candles, index, 0.01, None, false, None, None, variant);
        }
        candles.push(Candle {
            timestamp_ms: 8 * 900_000,
            open: 100.8,
            high: 101.4,
            low: 100.7,
            close: 101.2,
            volume: 300.0,
        });
        assert!(matches!(
            state.update(
                &candles,
                8,
                0.01,
                Some(2.0),
                true,
                Some(3.0),
                Some(4.5),
                variant,
            ),
            Some(StrictVisualLongEntryEvent::AcceptanceArmed(_))
        ));
        candles.push(Candle {
            timestamp_ms: 9 * 900_000,
            open: 101.1,
            high: 101.3,
            low: 100.99,
            close: 101.15,
            volume: 100.0,
        });
        let Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(signal)) =
            state.update(&candles, 9, 0.01, Some(2.0), false, None, None, variant)
        else {
            panic!("the first completed retest should confirm the frozen source");
        };
        signal
    }

    /// 冻结双向新合同来源；成交量参数故意为空以证明它不再是入场门禁。
    fn armed_symmetric_source(
        direction: Direction,
        variant: StrictVisualBreakoutResearchVariant,
    ) -> (Vec<Candle>, StrictVisualLongEntryState) {
        let mut candles: Vec<_> = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect();
        let mut state = StrictVisualLongEntryState::default();
        for index in 0..8 {
            state.update(&candles, index, 0.01, None, false, None, None, variant);
        }
        let (open, high, low, close) = match direction {
            Direction::Long => (100.9, 101.25, 100.85, 101.11),
            Direction::Short => (99.1, 99.15, 98.75, 98.89),
        };
        candles.push(Candle {
            timestamp_ms: 8 * 900_000,
            open,
            high,
            low,
            close,
            volume: 1.0,
        });
        let Some(StrictVisualLongEntryEvent::AcceptanceArmed(source)) =
            state.update(&candles, 8, 0.01, Some(1.0), false, None, None, variant)
        else {
            panic!("the symmetric source should arm without volume evidence");
        };
        assert_eq!(source.direction, direction);
        assert!(!source.volume_gate_applied);
        (candles, state)
    }

    /// 构造已冻结 V8 强突破来源，供接受余量边界测试只改变确认棒。
    fn armed_v8_acceptance(
        source_atr: f64,
        breakout_open: f64,
        breakout_close: f64,
    ) -> (Vec<Candle>, StrictVisualLongEntryState) {
        let variant = StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr;
        let mut candles: Vec<_> = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect();
        let mut state = StrictVisualLongEntryState::default();
        for index in 0..8 {
            state.update(
                &candles,
                index,
                0.01,
                Some(source_atr),
                true,
                Some(3.0),
                Some(2.7),
                variant,
            );
        }
        candles.push(Candle {
            timestamp_ms: 8 * 900_000,
            open: breakout_open,
            high: breakout_close + 0.05,
            low: breakout_open - 0.05,
            close: breakout_close,
            volume: 300.0,
        });
        assert!(matches!(
            state.update(
                &candles,
                8,
                0.01,
                Some(source_atr),
                true,
                Some(3.0),
                Some(2.7),
                variant,
            ),
            Some(StrictVisualLongEntryEvent::AcceptanceArmed(_))
        ));
        (candles, state)
    }

    #[test]
    fn confirmation_cannot_backfill_a_breakout_on_the_same_bar() {
        let mut candles: Vec<_> = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect();
        let mut state = StrictVisualConsolidationState::default();
        for index in 0..7 {
            assert_eq!(state.update(&candles, index, 0.01), None);
        }
        assert!(matches!(
            state.update(&candles, 7, 0.01),
            Some(StrictVisualRangeEvent::Confirmed(_))
        ));

        candles.push(Candle {
            timestamp_ms: 8 * 900_000,
            open: 100.8,
            high: 101.4,
            low: 100.7,
            close: 101.2,
            volume: 300.0,
        });
        let Some(StrictVisualRangeEvent::UpperBreak(range)) = state.update(&candles, 8, 0.01)
        else {
            panic!("next completed candle should consume the confirmed upper boundary");
        };
        assert_eq!(range.first_confirmation_index, 7);
        assert_eq!(range.boundary_confirmation_index, 7);
        assert_eq!(range.length_bars, 8);
        assert_eq!(range.upper, 101.0);
    }

    #[test]
    fn new_contract_cannot_let_an_overlapping_parent_hide_a_fresh_range() {
        let variant = StrictVisualBreakoutResearchVariant::V10SymmetricRetainedBreakout;
        let mut candles: Vec<_> = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect();
        let mut state = StrictVisualConsolidationState::default();
        for index in 0..8 {
            state.update_for_variant(&candles, index, 0.01, variant);
        }
        candles.push(Candle {
            timestamp_ms: 8 * 900_000,
            open: 100.8,
            high: 101.4,
            low: 100.7,
            close: 101.2,
            volume: 300.0,
        });
        assert!(matches!(
            state.update_for_variant(&candles, 8, 0.01, variant),
            Some(StrictVisualRangeEvent::UpperBreak(_))
        ));

        for index in 9..16 {
            candles.push(edge_candle(index, index % 2 == 0));
            assert_eq!(
                state.update_for_variant(&candles, index, 0.01, variant),
                None
            );
        }
        candles.push(edge_candle(16, true));
        let rebuilt = state.update_for_variant(&candles, 16, 0.01, variant);
        assert!(
            matches!(rebuilt, Some(StrictVisualRangeEvent::Confirmed(_))),
            "unexpected rebuilt event: {rebuilt:?}"
        );
    }

    #[test]
    fn body_strength_gate_rejects_both_reported_btc_weak_departures() {
        let low_body_ratio = Candle {
            timestamp_ms: 0,
            open: 63_073.7,
            high: 63_125.4,
            low: 63_061.7,
            close: 63_096.5,
            volume: 1.0,
        };
        let low_directional_move = Candle {
            timestamp_ms: 900_000,
            open: 63_895.9,
            high: 64_030.6,
            low: 63_873.1,
            close: 64_012.6,
            volume: 1.0,
        };

        let first = strict_visual_breakout_body_strength(low_body_ratio, Direction::Long);
        let second = strict_visual_breakout_body_strength(low_directional_move, Direction::Long);
        assert!(!first.qualifies);
        assert!(first.body_ratio < STRICT_VISUAL_BREAKOUT_MIN_BODY_RATIO);
        assert!(first.directional_move_ratio < STRICT_VISUAL_BREAKOUT_MIN_DIRECTIONAL_MOVE_RATIO);
        assert!(!second.qualifies);
        assert!(second.body_ratio >= STRICT_VISUAL_BREAKOUT_MIN_BODY_RATIO);
        assert!(second.directional_move_ratio < STRICT_VISUAL_BREAKOUT_MIN_DIRECTIONAL_MOVE_RATIO);
    }

    #[test]
    fn body_strength_gate_is_mirrored_for_downward_departures() {
        let strong_short = Candle {
            timestamp_ms: 0,
            open: 100.5,
            high: 100.6,
            low: 99.7,
            close: 99.8,
            volume: 1.0,
        };
        let strength = strict_visual_breakout_body_strength(strong_short, Direction::Short);

        assert!(strength.qualifies);
        assert_eq!(
            strict_visual_breakout_body_strength(strong_short, Direction::Long).qualifies,
            false
        );
    }

    #[test]
    fn v5_consumes_a_weak_departure_without_arming_an_anchor() {
        let mut candles: Vec<_> = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect();
        let mut state = StrictVisualLongEntryState::default();
        for index in 0..8 {
            state.update(
                &candles,
                index,
                0.01,
                Some(1.0),
                true,
                Some(3.0),
                Some(2.7),
                StrictVisualBreakoutResearchVariant::V5BreakoutBodyStrength,
            );
        }
        candles.push(Candle {
            timestamp_ms: 8 * 900_000,
            open: 100.99,
            high: 101.25,
            low: 100.98,
            close: 101.2,
            volume: 300.0,
        });

        assert!(matches!(
            state.update(
                &candles,
                8,
                0.01,
                Some(1.0),
                true,
                Some(3.0),
                Some(2.7),
                StrictVisualBreakoutResearchVariant::V5BreakoutBodyStrength,
            ),
            Some(StrictVisualLongEntryEvent::Range(
                StrictVisualRangeEvent::UpperBreak(_)
            ))
        ));
    }

    #[test]
    fn v5_strong_departure_arms_the_existing_v3_acceptance_contract() {
        let mut candles: Vec<_> = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect();
        let mut state = StrictVisualLongEntryState::default();
        for index in 0..8 {
            state.update(
                &candles,
                index,
                0.01,
                Some(1.0),
                true,
                Some(3.0),
                Some(2.7),
                StrictVisualBreakoutResearchVariant::V5BreakoutBodyStrength,
            );
        }
        candles.push(Candle {
            timestamp_ms: 8 * 900_000,
            open: 100.7,
            high: 101.35,
            low: 100.65,
            close: 101.3,
            volume: 300.0,
        });

        assert!(matches!(
            state.update(
                &candles,
                8,
                0.01,
                Some(1.0),
                true,
                Some(3.0),
                Some(2.7),
                StrictVisualBreakoutResearchVariant::V5BreakoutBodyStrength,
            ),
            Some(StrictVisualLongEntryEvent::AcceptanceArmed(_))
        ));
    }

    #[test]
    fn v6_restores_the_frozen_range_after_the_next_bar_returns_inside() {
        let mut candles: Vec<_> = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect();
        let mut state = StrictVisualLongEntryState::default();
        for index in 0..8 {
            state.update(
                &candles,
                index,
                0.01,
                Some(1.0),
                true,
                Some(3.0),
                Some(2.7),
                StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation,
            );
        }
        candles.push(Candle {
            timestamp_ms: 8 * 900_000,
            open: 100.99,
            high: 101.25,
            low: 100.98,
            close: 101.2,
            volume: 300.0,
        });
        assert!(matches!(
            state.update(
                &candles,
                8,
                0.01,
                Some(1.0),
                true,
                Some(3.0),
                Some(2.7),
                StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation,
            ),
            Some(StrictVisualLongEntryEvent::Range(
                StrictVisualRangeEvent::WeakDeparturePending(_)
            ))
        ));

        candles.push(Candle {
            timestamp_ms: 9 * 900_000,
            open: 101.1,
            high: 101.15,
            low: 100.8,
            close: 100.9,
            volume: 100.0,
        });
        let Some(StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDepartureReturned(
            returned,
        ))) = state.update(
            &candles,
            9,
            0.01,
            Some(1.0),
            false,
            None,
            None,
            StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation,
        )
        else {
            panic!("the adjacent completed close inside must restore the frozen range");
        };
        assert_eq!(returned.departure_index, 8);
        assert_eq!(returned.confirmation_index, Some(9));

        candles.push(Candle {
            timestamp_ms: 10 * 900_000,
            open: 100.7,
            high: 101.4,
            low: 100.65,
            close: 101.3,
            volume: 300.0,
        });
        assert!(matches!(
            state.update(
                &candles,
                10,
                0.01,
                Some(1.0),
                true,
                Some(3.0),
                Some(2.7),
                StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation,
            ),
            Some(StrictVisualLongEntryEvent::AcceptanceArmed(_))
        ));
    }

    #[test]
    fn v6_consumes_an_outside_confirmation_without_backfilling_its_strong_body() {
        let mut candles: Vec<_> = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect();
        let mut state = StrictVisualLongEntryState::default();
        for index in 0..8 {
            state.update(
                &candles,
                index,
                0.01,
                Some(1.0),
                true,
                Some(3.0),
                Some(2.7),
                StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation,
            );
        }
        candles.push(Candle {
            timestamp_ms: 8 * 900_000,
            open: 100.99,
            high: 101.25,
            low: 100.98,
            close: 101.2,
            volume: 300.0,
        });
        state.update(
            &candles,
            8,
            0.01,
            Some(1.0),
            true,
            Some(3.0),
            Some(2.7),
            StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation,
        );
        candles.push(Candle {
            timestamp_ms: 9 * 900_000,
            open: 101.1,
            high: 102.0,
            low: 101.05,
            close: 101.8,
            volume: 400.0,
        });

        let Some(StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDepartureConsumed(
            consumed,
        ))) = state.update(
            &candles,
            9,
            0.01,
            Some(1.0),
            true,
            Some(3.0),
            Some(2.7),
            StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation,
        )
        else {
            panic!("outside confirmation must consume without arming an anchor");
        };
        assert_eq!(consumed.departure_index, 8);
        assert_eq!(consumed.confirmation_index, Some(9));
        assert_eq!(consumed.side, StrictVisualDepartureSide::Upper);
    }

    #[test]
    fn v6_applies_the_same_one_bar_probation_to_a_weak_lower_departure() {
        let mut candles: Vec<_> = (0..8)
            .map(|index| edge_candle(index, index % 2 == 0))
            .collect();
        let mut state = StrictVisualConsolidationState::default();
        for index in 0..8 {
            state.update_for_variant(
                &candles,
                index,
                0.01,
                StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation,
            );
        }
        candles.push(Candle {
            timestamp_ms: 8 * 900_000,
            open: 99.01,
            high: 99.02,
            low: 98.75,
            close: 98.8,
            volume: 300.0,
        });
        let Some(StrictVisualRangeEvent::WeakDeparturePending(pending)) = state.update_for_variant(
            &candles,
            8,
            0.01,
            StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation,
        ) else {
            panic!("weak lower departure must enter the mirrored pending state");
        };
        assert_eq!(pending.side, StrictVisualDepartureSide::Lower);

        candles.push(Candle {
            timestamp_ms: 9 * 900_000,
            open: 98.9,
            high: 99.2,
            low: 98.85,
            close: 99.1,
            volume: 100.0,
        });
        assert!(matches!(
            state.update_for_variant(
                &candles,
                9,
                0.01,
                StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation,
            ),
            Some(StrictVisualRangeEvent::WeakDepartureReturned(_))
        ));
    }

    #[test]
    fn v8_rejects_a_subthreshold_margin_and_consumes_the_source() {
        let variant = StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr;
        let (mut candles, mut state) = armed_v8_acceptance(1.0, 100.7, 101.3);
        candles.push(Candle {
            timestamp_ms: 9 * 900_000,
            open: 101.3,
            high: 101.45,
            low: 101.0,
            close: 101.39,
            volume: 100.0,
        });
        assert!(matches!(
            state.update(&candles, 9, 0.01, Some(50.0), false, None, None, variant,),
            Some(StrictVisualLongEntryEvent::AcceptanceMarginRejected(_))
        ));

        candles.push(Candle {
            timestamp_ms: 10 * 900_000,
            open: 101.4,
            high: 101.7,
            low: 101.0,
            close: 101.6,
            volume: 100.0,
        });
        assert!(state
            .update(&candles, 10, 0.01, Some(1.0), false, None, None, variant,)
            .is_none_or(|event| event.entry_signal().is_none()));
    }

    #[test]
    fn v8_accepts_exactly_point_four_frozen_source_atr() {
        let variant = StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr;
        let (mut candles, mut state) = armed_v8_acceptance(2.5, 100.7, 101.3);
        candles.push(Candle {
            timestamp_ms: 9 * 900_000,
            open: 101.8,
            high: 102.1,
            low: 101.0,
            close: 102.0,
            volume: 100.0,
        });
        let Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(signal)) =
            state.update(&candles, 9, 0.01, Some(0.01), false, None, None, variant)
        else {
            panic!("0.40 source ATR must pass even when the confirmation ATR differs");
        };
        assert_eq!(signal.source_atr, 2.5);
        assert_eq!(
            (candles[9].close - signal.range.upper) / signal.source_atr,
            STRICT_VISUAL_BREAKOUT_MIN_ACCEPTANCE_MARGIN_ATR
        );
    }

    #[test]
    fn v8_preserves_the_body_midpoint_rejection_order() {
        let variant = StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr;
        let (mut candles, mut state) = armed_v8_acceptance(1.0, 100.8, 101.4);
        candles.push(Candle {
            timestamp_ms: 9 * 900_000,
            open: 101.2,
            high: 101.3,
            low: 101.0,
            close: 101.05,
            volume: 100.0,
        });
        assert!(matches!(
            state.update(&candles, 9, 0.01, Some(20.0), false, None, None, variant,),
            Some(StrictVisualLongEntryEvent::AcceptanceBodyMidpointRejected(
                _
            ))
        ));
    }

    #[test]
    fn v4_keeps_v3_signal_identity_and_freezes_the_short_range_branch() {
        let v3 = accepted_signal(StrictVisualBreakoutResearchVariant::V3BodyMidpointHold);
        let v4 = accepted_signal(StrictVisualBreakoutResearchVariant::V4ShortRangeOneR);

        assert_eq!(v3.range.length_bars, 8);
        assert!(!v3.short_range_one_r_target);
        assert!(v4.short_range_one_r_target);
        let mut normalized_v4 = v4;
        normalized_v4.short_range_one_r_target = false;
        assert_eq!(v3, normalized_v4);
        assert!(frozen_short_range_one_r_target(
            StrictVisualBreakoutResearchVariant::V4ShortRangeOneR,
            32,
        ));
        assert!(!frozen_short_range_one_r_target(
            StrictVisualBreakoutResearchVariant::V4ShortRangeOneR,
            48,
        ));
    }

    #[test]
    fn symmetric_contract_uses_50pct_body_and_15bps_directional_move() {
        let candle = Candle {
            timestamp_ms: 0,
            open: 100.9,
            high: 101.25,
            low: 100.85,
            close: 101.11,
            volume: 1.0,
        };
        assert!(!strict_visual_breakout_body_strength(candle, Direction::Long).qualifies);
        assert!(
            strict_visual_breakout_body_strength_for_variant(
                candle,
                Direction::Long,
                StrictVisualBreakoutResearchVariant::V10SymmetricRetainedBreakout,
            )
            .qualifies
        );
    }

    #[test]
    fn symmetric_long_waits_after_weak_retention_and_can_confirm_on_bar_five() {
        let variant = StrictVisualBreakoutResearchVariant::V10SymmetricRetainedBreakout;
        let (mut candles, mut state) = armed_symmetric_source(Direction::Long, variant);
        for index in 9..=13 {
            let close = if index == 9 {
                101.02
            } else if index == 13 {
                101.04
            } else {
                101.4
            };
            candles.push(Candle {
                timestamp_ms: index as i64 * 900_000,
                open: 101.3,
                high: 101.5,
                low: if matches!(index, 9 | 13) {
                    101.0
                } else {
                    101.3
                },
                close,
                volume: 1.0,
            });
            let event = state.update(&candles, index, 0.01, Some(1.0), false, None, None, variant);
            if index < 13 {
                assert_eq!(event, None);
            } else {
                let Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(signal)) = event else {
                    panic!("bar five should be included in the acceptance window");
                };
                assert_eq!(signal.signal_index, 13);
                assert_eq!(signal.measured_move_target_price, Some(103.0));
                assert!(signal.required_acceptance_close > signal.range.upper);
            }
        }
    }

    #[test]
    fn symmetric_short_is_a_true_mirror_and_reentry_invalidates() {
        let variant = StrictVisualBreakoutResearchVariant::V10SymmetricRetainedBreakout;
        let (mut candles, mut state) = armed_symmetric_source(Direction::Short, variant);
        candles.push(Candle {
            timestamp_ms: 9 * 900_000,
            open: 98.9,
            high: 99.0,
            low: 98.8,
            close: 98.96,
            volume: 1.0,
        });
        let Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(signal)) =
            state.update(&candles, 9, 0.01, Some(1.0), false, None, None, variant)
        else {
            panic!("the mirrored short retest should confirm");
        };
        assert_eq!(signal.direction, Direction::Short);
        assert_eq!(signal.measured_move_target_price, Some(97.0));

        let (mut invalidated, mut invalidated_state) =
            armed_symmetric_source(Direction::Short, variant);
        invalidated.push(Candle {
            timestamp_ms: 9 * 900_000,
            open: 98.9,
            high: 99.2,
            low: 98.8,
            close: 99.0,
            volume: 1.0,
        });
        assert!(matches!(
            invalidated_state.update(&invalidated, 9, 0.01, Some(1.0), false, None, None, variant,),
            Some(StrictVisualLongEntryEvent::AcceptanceInvalidated(_))
        ));
    }

    #[test]
    fn v11_freezes_one_tick_outside_the_breakout_candle_for_both_directions() {
        let variant = StrictVisualBreakoutResearchVariant::V11BreakoutCandleExtremeStop;
        let long = accepted_signal(variant);
        assert!((long.breakout_candle_extreme_stop_price.unwrap() - 100.69).abs() < 1e-9);

        let (mut candles, mut state) = armed_symmetric_source(Direction::Short, variant);
        candles.push(Candle {
            timestamp_ms: 9 * 900_000,
            open: 98.9,
            high: 99.0,
            low: 98.8,
            close: 98.96,
            volume: 1.0,
        });
        let Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(short)) =
            state.update(&candles, 9, 0.01, Some(1.0), false, None, None, variant)
        else {
            panic!("the mirrored V11 short retest should confirm");
        };
        assert!((short.breakout_candle_extreme_stop_price.unwrap() - 99.16).abs() < 1e-9);
    }

    #[test]
    fn v12_only_adds_the_one_atr_floor_to_the_v11_signal_contract() {
        let v11 =
            accepted_signal(StrictVisualBreakoutResearchVariant::V11BreakoutCandleExtremeStop);
        let mut v12 = accepted_signal(StrictVisualBreakoutResearchVariant::V12ExtremeStopMinOneAtr);

        assert_eq!(
            v11.breakout_candle_extreme_stop_price,
            v12.breakout_candle_extreme_stop_price
        );
        assert_eq!(v11.breakout_candle_extreme_stop_min_atr_multiple, None);
        assert_eq!(v12.breakout_candle_extreme_stop_min_atr_multiple, Some(1.0));
        v12.breakout_candle_extreme_stop_min_atr_multiple = None;
        assert_eq!(v11, v12);
    }
}
