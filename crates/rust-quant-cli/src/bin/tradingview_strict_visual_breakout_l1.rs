use anyhow::{bail, Context, Result};
use chrono::{Datelike, FixedOffset, TimeZone};
use rust_quant_cli::app::tradingview_velocity_parity::{
    compute_indicators, load_frozen_top60_from_quant_core, strict_visual_breakout_body_strength,
    volume_take_profit_atr, Candle, Direction, FrozenSymbolCandles, ParityRuleVersion,
    StrictVisualBreakoutResearchVariant, StrictVisualBreakoutSignal, StrictVisualDepartureSide,
    StrictVisualLongEntryEvent, StrictVisualLongEntryState, StrictVisualRangeEvent,
    FROZEN_UNIVERSE_MANIFEST_SHA256, FROZEN_UNIVERSE_VERSION,
    STRICT_VISUAL_BREAKOUT_MIN_BODY_RATIO, STRICT_VISUAL_BREAKOUT_MIN_DIRECTIONAL_MOVE_RATIO,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const CANDLE_INTERVAL_MS: i64 = 15 * 60 * 1_000;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const MIN_CANDIDATES: usize = 15;
const MIN_SYMBOLS: usize = 8;
const MIN_MONTHS: usize = 3;
const MIN_EVENT_CLUSTERS: usize = 10;
const MIN_BODY_MIDPOINT_REJECTIONS: usize = 3;
const MIN_BODY_STRENGTH_REJECTIONS: usize = 3;
const MIN_ACCEPTANCE_MARGIN_REJECTIONS: usize = 30;
const MIN_EXTERNAL_STRUCTURE_REJECTIONS: usize = 3;

/// L1 只读扫描参数；部分成员模式必须显式开启，不能冒充正式 Top60 结论。
#[derive(Debug)]
struct Args {
    /// 机器结果写入位置；为空时输出到标准输出。
    output: Option<PathBuf>,
    /// `true` 允许使用本地完整子集，`false` 要求 manifest 全成员完整。
    allow_partial_diagnostic: bool,
    /// 本轮只读扫描的严格横盘入场时序版本。
    variant: StrictVisualBreakoutResearchVariant,
}

/// 未纳入 L1 扫描的冻结成员及其因果数据缺口。
#[derive(Debug, Serialize)]
struct ExcludedSymbol {
    /// 冻结 manifest 中的交易对名称。
    symbol: String,
    /// 统一评价窗口内实际加载的完成 K 线数量。
    evaluation_loaded: usize,
    /// 统一评价窗口应有的 15 分钟 K 线数量。
    evaluation_expected: usize,
    /// 排除原因，不包含任何后验交易结果。
    reason: &'static str,
}

/// 一次严格横盘上破在信号完成时已经冻结的全部 L1 特征。
#[derive(Debug, Serialize)]
struct L1Candidate {
    /// 产生上破的交易对。
    symbol: String,
    /// 首次合格放量上破来源完成时间，Unix 毫秒时间戳。
    breakout_time_ms: i64,
    /// 实际形成入场意图的完成时间；V1 等于突破时间，V2/V3 为接受确认时间。
    signal_time_ms: i64,
    /// 接受确认距突破来源的 15 分钟 K 线根数；V1 为 0。
    acceptance_age_bars: usize,
    /// 上海时区自然月，仅用于时间覆盖审计。
    shanghai_month: String,
    /// 按相邻信号不超过 60 分钟串联的全市场事件簇。
    event_cluster_id: String,
    /// 活动区间首根 K 线时间，Unix 毫秒时间戳。
    range_start_time_ms: i64,
    /// 活动区间首次可见时间，Unix 毫秒时间戳。
    first_confirmation_time_ms: i64,
    /// 当前冻结边界最近一次确认时间，Unix 毫秒时间戳。
    boundary_confirmation_time_ms: i64,
    /// 当前父横盘长度，单位为 15 分钟 K 线根数。
    range_length_bars: usize,
    /// 突破前冻结的 P90 上沿。
    upper: f64,
    /// 突破前冻结的 P10 下沿。
    lower: f64,
    /// 首次放量上破棒完成开盘价。
    breakout_open: f64,
    /// 首次放量上破棒完成收盘价。
    breakout_close: f64,
    /// 首次放量上破棒完成最高价，仅用于与冻结外部高点核对。
    breakout_high: f64,
    /// 突破源实体绝对值除以完整高低振幅。
    breakout_body_ratio: f64,
    /// 突破源阳线实体涨幅除以开盘价。
    breakout_directional_move_ratio: f64,
    /// 突破棒完成时冻结的实体中点；V3 确认时不得重算。
    breakout_body_midpoint: f64,
    /// 实际信号棒完成收盘价。
    signal_close: f64,
    /// 突破来源 ATR14；V2 确认时不允许重算风险来源。
    source_atr: f64,
    /// 主策略过滤量比，只使用突破来源当时的成交量证据。
    volume_ratio: f64,
    /// 主策略量能档位对应的 ATR 止盈倍数。
    take_profit_atr: f64,
    /// 区间收盘容纳率，范围为 0～1。
    containment_ratio: f64,
    /// 区间收盘方向效率，范围为 0～1。
    direction_efficiency: f64,
    /// 区间上下沿按时间发生的独立切换次数。
    edge_transition_count: usize,
    /// V9 自适应外部窗口完成棒数量；前置证据不足时为空。
    external_lookback_bars: Option<usize>,
    /// 外部窗口首根完成棒时间。
    external_window_start_time_ms: Option<i64>,
    /// 外部窗口最高价所属完成棒时间。
    external_high_time_ms: Option<i64>,
    /// 横盘开始前冻结的外部最高价。
    external_high: Option<f64>,
    /// 横盘开始后、突破前是否已有完成收盘解决该外部高点。
    external_high_resolved_before_breakout: Option<bool>,
    /// V9 实际要求突破的交易上沿。
    trade_breakout_upper: Option<f64>,
    /// 突破收盘至少需要达到的一个 tick 外侧价格。
    required_breakout_close: Option<f64>,
    /// 突破收盘相对交易上沿的 tick 数。
    breakout_clearance_ticks: Option<f64>,
    /// 突破收盘相对交易上沿的来源 ATR 倍数。
    breakout_clearance_atr: Option<f64>,
}

/// V8 原本确认、但 V9 因外部结构上沿未清除而拒绝的无 outcome 记录。
#[derive(Debug, Serialize)]
struct L1ExternalStructureRejection {
    symbol: String,
    breakout_time_ms: i64,
    decision_time_ms: i64,
    range_start_time_ms: i64,
    range_length_bars: usize,
    visual_upper: f64,
    external_lookback_bars: usize,
    external_window_start_time_ms: i64,
    external_high_time_ms: i64,
    external_high: f64,
    resolved_before_breakout: bool,
    trade_breakout_upper: f64,
    required_breakout_close: f64,
    breakout_high: f64,
    breakout_close: f64,
    breakout_clearance_ticks: f64,
    breakout_clearance_atr: f64,
}

/// 一个方向上弱离区棒的无 outcome 聚合分布。
#[derive(Debug, Default, Serialize)]
struct L1WeakDepartureCounts {
    /// 未同时通过两项强度门槛的离区总数。
    total: usize,
    /// 实体占比低于 60% 的离区数；可与方向位移失败重叠。
    body_ratio_failed: usize,
    /// 方向实体位移低于 25 bps 的离区数；可与实体占比失败重叠。
    directional_move_failed: usize,
    /// 两项门槛同时失败的离区数。
    both_failed: usize,
}

impl L1WeakDepartureCounts {
    /// 只累计未通过的完成棒，保留两项失败的交集关系。
    fn record(&mut self, body_ratio: f64, directional_move_ratio: f64) {
        let body_failed = body_ratio < STRICT_VISUAL_BREAKOUT_MIN_BODY_RATIO;
        let move_failed =
            directional_move_ratio < STRICT_VISUAL_BREAKOUT_MIN_DIRECTIONAL_MOVE_RATIO;
        if !body_failed && !move_failed {
            return;
        }
        self.total += 1;
        self.body_ratio_failed += usize::from(body_failed);
        self.directional_move_failed += usize::from(move_failed);
        self.both_failed += usize::from(body_failed && move_failed);
    }
}

/// V3 原本可冻结、但被 V5 强度门禁拒绝的突破来源。
#[derive(Debug, Serialize)]
struct L1BodyStrengthRejection {
    /// 被拒绝来源所属交易对。
    symbol: String,
    /// 弱突破完成时间，Unix 毫秒时间戳。
    breakout_time_ms: i64,
    /// 突破前冻结上沿。
    upper: f64,
    /// 弱突破完成棒 OHLC。
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    /// 完成棒实体占整根振幅比例。
    body_ratio: f64,
    /// 完成棒阳线实体涨幅比例。
    directional_move_ratio: f64,
}

/// V3 在 V2 首次可确认时点拒绝的来源；只记录当时可见字段，不包含交易结果。
#[derive(Debug, Serialize)]
struct L1BodyMidpointRejection {
    /// 被拒绝来源所属交易对。
    symbol: String,
    /// 合格突破棒完成时间，Unix 毫秒时间戳。
    breakout_time_ms: i64,
    /// V2 原本会确认、V3 作出拒绝决定的完成时间，Unix 毫秒时间戳。
    decision_time_ms: i64,
    /// 突破前冻结的 P90 上沿。
    upper: f64,
    /// 突破棒完成开盘价。
    breakout_open: f64,
    /// 突破棒完成收盘价。
    breakout_close: f64,
    /// 突破棒开收盘均价，V3 的唯一新增门禁。
    breakout_body_midpoint: f64,
    /// 决策棒完成收盘价；严格低于实体中点才进入本列表。
    confirmation_close: f64,
}

/// L1 机器结果只表达覆盖与身份，不包含成交后标签或收益字段。
#[derive(Debug, Serialize)]
struct L1Report {
    schema_version: &'static str,
    research_level: &'static str,
    strategy_version: &'static str,
    baseline_strategy_version: &'static str,
    universe_version: &'static str,
    universe_manifest_sha256: &'static str,
    dataset_fingerprint_sha256: String,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
    manifest_evaluation_end_ms: i64,
    expected_symbols: usize,
    included_symbols: usize,
    full_universe_complete: bool,
    excluded_symbols: Vec<ExcludedSymbol>,
    confirmed_ranges: usize,
    parent_upgrades: usize,
    /// 所有完成收盘上离冻结上沿的次数，不区分实体强弱。
    upper_breaks: usize,
    /// 同时通过 60% 实体占比与 25 bps 方向位移的上离次数。
    strong_upper_breaks: usize,
    /// 未通过强度门禁的上离分布；只含信号时可见 OHLC。
    weak_upper_departures: L1WeakDepartureCounts,
    /// 所有完成收盘下离冻结下沿的次数，不区分实体强弱。
    lower_breaks: usize,
    /// 镜像通过两项强度门禁的下离次数。
    strong_lower_breaks: usize,
    /// 未通过强度门禁的下离分布；只含信号时可见 OHLC。
    weak_lower_departures: L1WeakDepartureCounts,
    /// 同一数据上 V3 原合同能够冻结的合格上破来源数。
    baseline_qualified_breakout_sources: usize,
    /// 当前版本最终冻结的合格上破来源数。
    qualified_breakout_sources: usize,
    /// V3 合格但被 V5 两项强度门禁拒绝的来源数。
    body_strength_rejected_sources: usize,
    /// V6 首根弱离区进入单根观察期的总次数。
    weak_departure_pending_total: usize,
    /// 发生在冻结上沿之外的 pending 次数。
    weak_departure_pending_upper: usize,
    /// 发生在冻结下沿之外的 pending 次数。
    weak_departure_pending_lower: usize,
    /// 紧邻下一根完成收盘回到原冻结区间的总次数。
    weak_departure_returned_total: usize,
    /// 弱上离区后紧邻回区的次数。
    weak_departure_returned_upper: usize,
    /// 弱下离区后紧邻回区的次数。
    weak_departure_returned_lower: usize,
    /// 紧邻下一根仍停留在首次离区同侧并消费旧区间的次数。
    weak_departure_consumed_same_side: usize,
    /// 紧邻下一根直接越过原区间另一侧并消费旧区间的次数。
    weak_departure_consumed_opposite_side: usize,
    /// 扫描结束仍未等到紧邻决策棒的 pending 数；通过门禁必须为 0。
    weak_departure_unresolved: usize,
    /// pending 到决策棒的最大 15 分钟 K 线根数；V6 通过门禁不得超过 1。
    max_weak_departure_age_bars: usize,
    /// 区外确认棒被错误补算为旧横盘锚点的次数；因果实现必须为 0。
    outside_confirmation_anchor_count: usize,
    acceptance_confirmed: usize,
    body_midpoint_rejected: usize,
    acceptance_margin_rejected: usize,
    /// V8 原本确认、但 V9 未越过冻结外部交易上沿的候选数。
    external_structure_rejected: usize,
    /// V9 因前置完成棒不足而无法构造外部证据的决策数；通过门禁必须为 0。
    external_structure_missing_evidence: usize,
    acceptance_invalidated: usize,
    acceptance_expired: usize,
    qualified_candidates: usize,
    covered_symbols: usize,
    covered_shanghai_months: usize,
    event_clusters_60m: usize,
    l1_gate_passed: bool,
    l1_gate: &'static str,
    label_boundary: &'static str,
    candidates: Vec<L1Candidate>,
    body_strength_rejections: Vec<L1BodyStrengthRejection>,
    body_midpoint_rejections: Vec<L1BodyMidpointRejection>,
    external_structure_rejections: Vec<L1ExternalStructureRejection>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    let mut dataset = load_frozen_top60_from_quant_core().await?;
    if dataset.universe_version != FROZEN_UNIVERSE_VERSION
        || dataset.manifest_sha256 != FROZEN_UNIVERSE_MANIFEST_SHA256
    {
        bail!("冻结币池 identity 与编译时常量不一致");
    }
    let dataset_fingerprint_sha256 = dataset_fingerprint(&dataset.symbols);
    let evaluation_start_ms = dataset.window_start_ms;
    let manifest_evaluation_end_ms = dataset
        .window_end_ms
        .checked_sub(1)
        .context("冻结评价上界下溢")?;
    let evaluation_end_ms = if args.allow_partial_diagnostic {
        modal_snapshot_end(&dataset.symbols, evaluation_start_ms)
            .context("冻结 Top60 没有正式窗口 K 线")?
    } else {
        manifest_evaluation_end_ms
    };
    let expected_symbols = dataset.coverage.expected_symbol_count;
    let mut eligible_symbols = Vec::new();
    let mut excluded_symbols = Vec::new();
    for symbol in std::mem::take(&mut dataset.symbols) {
        let coverage =
            replay_window_coverage(&symbol.candles, evaluation_start_ms, evaluation_end_ms)?;
        if !coverage.is_complete || !symbol.warmup_is_complete {
            excluded_symbols.push(ExcludedSymbol {
                symbol: symbol.symbol,
                evaluation_loaded: coverage.loaded,
                evaluation_expected: coverage.expected,
                reason: match (coverage.is_complete, symbol.warmup_is_complete) {
                    (false, false) => "评价窗口与 60 天预热均不完整",
                    (false, true) => "评价窗口不完整",
                    (true, false) => "60 天指标预热不完整",
                    (true, true) => unreachable!("完整成员不会被排除"),
                },
            });
        } else {
            eligible_symbols.push(symbol);
        }
    }
    let full_universe_complete = eligible_symbols.len() == expected_symbols
        && evaluation_end_ms == manifest_evaluation_end_ms;
    if !full_universe_complete && !args.allow_partial_diagnostic {
        bail!(
            "严格 L1 需要完整 Top60；本地只有 {}/{} 个完整成员",
            eligible_symbols.len(),
            expected_symbols
        );
    }
    if eligible_symbols.is_empty() {
        bail!("没有同时具备完整评价窗口和 60 天预热的冻结成员");
    }

    let mut confirmed_ranges = 0;
    let mut parent_upgrades = 0;
    let mut upper_breaks = 0;
    let mut strong_upper_breaks = 0;
    let mut weak_upper_departures = L1WeakDepartureCounts::default();
    let mut lower_breaks = 0;
    let mut strong_lower_breaks = 0;
    let mut weak_lower_departures = L1WeakDepartureCounts::default();
    let mut baseline_qualified_breakout_sources = 0;
    let mut qualified_breakout_sources = 0;
    let mut body_strength_rejected_sources = 0;
    let mut weak_departure_pending_total: usize = 0;
    let mut weak_departure_pending_upper: usize = 0;
    let mut weak_departure_pending_lower: usize = 0;
    let mut weak_departure_returned_total: usize = 0;
    let mut weak_departure_returned_upper: usize = 0;
    let mut weak_departure_returned_lower: usize = 0;
    let mut weak_departure_consumed_same_side: usize = 0;
    let mut weak_departure_consumed_opposite_side: usize = 0;
    let mut max_weak_departure_age_bars: usize = 0;
    let outside_confirmation_anchor_count: usize = 0;
    let mut acceptance_confirmed = 0;
    let mut body_midpoint_rejected = 0;
    let mut acceptance_margin_rejected = 0;
    let mut external_structure_rejected = 0;
    let mut external_structure_missing_evidence = 0;
    let mut acceptance_invalidated = 0;
    let mut acceptance_expired = 0;
    let mut candidates = Vec::new();
    let mut body_strength_rejections = Vec::new();
    let mut body_midpoint_rejections = Vec::new();
    let mut external_structure_rejections = Vec::new();
    for symbol in &eligible_symbols {
        let indicators = compute_indicators(&symbol.candles, ParityRuleVersion::CandidateV20);
        let mut state = StrictVisualLongEntryState::default();
        for (index, candle) in symbol.candles.iter().copied().enumerate() {
            if candle.timestamp_ms > evaluation_end_ms {
                break;
            }
            let point = &indicators.points[index];
            let take_profit_atr = point.filtered_volume_ratio.and_then(volume_take_profit_atr);
            let Some(event) = state.update(
                &symbol.candles,
                index,
                symbol.tick_size,
                point.atr14,
                point.volume_event,
                point.filtered_volume_ratio,
                take_profit_atr,
                args.variant,
            ) else {
                continue;
            };
            if candle.timestamp_ms < evaluation_start_ms {
                continue;
            }
            match event {
                StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::Confirmed(_)) => {
                    confirmed_ranges += 1
                }
                StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::ParentUpgraded(_)) => {
                    parent_upgrades += 1
                }
                StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::LowerBreak(_)) => {
                    lower_breaks += 1;
                    let strength = strict_visual_breakout_body_strength(candle, Direction::Short);
                    if strength.qualifies {
                        strong_lower_breaks += 1;
                    } else {
                        weak_lower_departures
                            .record(strength.body_ratio, strength.directional_move_ratio);
                    }
                }
                StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::UpperBreak(range)) => {
                    upper_breaks += 1;
                    let strength = strict_visual_breakout_body_strength(candle, Direction::Long);
                    if strength.qualifies {
                        strong_upper_breaks += 1;
                    } else {
                        weak_upper_departures
                            .record(strength.body_ratio, strength.directional_move_ratio);
                    }
                    let baseline_source = point.atr14.is_some_and(|value| value > 0.0)
                        && point.volume_event
                        && point.filtered_volume_ratio.is_some()
                        && take_profit_atr.is_some()
                        && candle.close > candle.open;
                    if baseline_source {
                        baseline_qualified_breakout_sources += 1;
                        if args.variant.requires_breakout_body_strength() && !strength.qualifies {
                            body_strength_rejected_sources += 1;
                            body_strength_rejections.push(L1BodyStrengthRejection {
                                symbol: symbol.symbol.clone(),
                                breakout_time_ms: candle.timestamp_ms,
                                upper: range.upper,
                                open: candle.open,
                                high: candle.high,
                                low: candle.low,
                                close: candle.close,
                                body_ratio: strength.body_ratio,
                                directional_move_ratio: strength.directional_move_ratio,
                            });
                        }
                    }
                }
                StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::WeakDeparturePending(pending),
                ) => {
                    weak_departure_pending_total += 1;
                    let strength = match pending.side {
                        StrictVisualDepartureSide::Upper => {
                            upper_breaks += 1;
                            weak_departure_pending_upper += 1;
                            strict_visual_breakout_body_strength(candle, Direction::Long)
                        }
                        StrictVisualDepartureSide::Lower => {
                            lower_breaks += 1;
                            weak_departure_pending_lower += 1;
                            strict_visual_breakout_body_strength(candle, Direction::Short)
                        }
                    };
                    match pending.side {
                        StrictVisualDepartureSide::Upper => weak_upper_departures
                            .record(strength.body_ratio, strength.directional_move_ratio),
                        StrictVisualDepartureSide::Lower => weak_lower_departures
                            .record(strength.body_ratio, strength.directional_move_ratio),
                    }
                    let baseline_source = matches!(pending.side, StrictVisualDepartureSide::Upper)
                        && point.atr14.is_some_and(|value| value > 0.0)
                        && point.volume_event
                        && point.filtered_volume_ratio.is_some()
                        && take_profit_atr.is_some()
                        && candle.close > candle.open;
                    if baseline_source {
                        baseline_qualified_breakout_sources += 1;
                        body_strength_rejected_sources += 1;
                        body_strength_rejections.push(L1BodyStrengthRejection {
                            symbol: symbol.symbol.clone(),
                            breakout_time_ms: candle.timestamp_ms,
                            upper: pending.range.upper,
                            open: candle.open,
                            high: candle.high,
                            low: candle.low,
                            close: candle.close,
                            body_ratio: strength.body_ratio,
                            directional_move_ratio: strength.directional_move_ratio,
                        });
                    }
                }
                StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::WeakDepartureReturned(resolved),
                ) => {
                    if symbol.candles[resolved.departure_index].timestamp_ms < evaluation_start_ms {
                        continue;
                    }
                    let age = resolved
                        .confirmation_index
                        .unwrap_or(index)
                        .saturating_sub(resolved.departure_index);
                    max_weak_departure_age_bars = max_weak_departure_age_bars.max(age);
                    weak_departure_returned_total += 1;
                    match resolved.side {
                        StrictVisualDepartureSide::Upper => weak_departure_returned_upper += 1,
                        StrictVisualDepartureSide::Lower => weak_departure_returned_lower += 1,
                    }
                }
                StrictVisualLongEntryEvent::Range(
                    StrictVisualRangeEvent::WeakDepartureConsumed(resolved),
                ) => {
                    if symbol.candles[resolved.departure_index].timestamp_ms < evaluation_start_ms {
                        continue;
                    }
                    let age = resolved
                        .confirmation_index
                        .unwrap_or(index)
                        .saturating_sub(resolved.departure_index);
                    max_weak_departure_age_bars = max_weak_departure_age_bars.max(age);
                    let same_side = match resolved.side {
                        StrictVisualDepartureSide::Upper => candle.close > resolved.range.upper,
                        StrictVisualDepartureSide::Lower => candle.close < resolved.range.lower,
                    };
                    if same_side {
                        weak_departure_consumed_same_side += 1;
                    } else {
                        weak_departure_consumed_opposite_side += 1;
                    }
                }
                StrictVisualLongEntryEvent::DirectSignal(signal) => {
                    upper_breaks += 1;
                    baseline_qualified_breakout_sources += 1;
                    qualified_breakout_sources += 1;
                    let strength = strict_visual_breakout_body_strength(candle, Direction::Long);
                    if strength.qualifies {
                        strong_upper_breaks += 1;
                    } else {
                        weak_upper_departures
                            .record(strength.body_ratio, strength.directional_move_ratio);
                    }
                    candidates.push(candidate_from_signal(symbol, signal)?);
                }
                StrictVisualLongEntryEvent::AcceptanceArmed(_) => {
                    upper_breaks += 1;
                    baseline_qualified_breakout_sources += 1;
                    qualified_breakout_sources += 1;
                    let strength = strict_visual_breakout_body_strength(candle, Direction::Long);
                    if strength.qualifies {
                        strong_upper_breaks += 1;
                    } else {
                        weak_upper_departures
                            .record(strength.body_ratio, strength.directional_move_ratio);
                    }
                }
                StrictVisualLongEntryEvent::AcceptanceConfirmed(signal) => {
                    acceptance_confirmed += 1;
                    candidates.push(candidate_from_signal(symbol, signal)?);
                }
                StrictVisualLongEntryEvent::AcceptanceBodyMidpointRejected(signal) => {
                    body_midpoint_rejected += 1;
                    body_midpoint_rejections.push(body_midpoint_rejection(symbol, signal));
                }
                StrictVisualLongEntryEvent::AcceptanceMarginRejected(_) => {
                    acceptance_margin_rejected += 1
                }
                StrictVisualLongEntryEvent::ExternalStructureRejected(signal) => {
                    external_structure_rejected += 1;
                    if signal.external_structure.is_some() {
                        external_structure_rejections
                            .push(external_structure_rejection(symbol, signal)?);
                    } else {
                        external_structure_missing_evidence += 1;
                    }
                }
                StrictVisualLongEntryEvent::AcceptanceInvalidated(_) => acceptance_invalidated += 1,
                StrictVisualLongEntryEvent::AcceptanceExpired(_) => acceptance_expired += 1,
            }
        }
    }

    assign_event_clusters(&mut candidates);
    let covered_symbols = candidates
        .iter()
        .map(|candidate| candidate.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let covered_shanghai_months = candidates
        .iter()
        .map(|candidate| candidate.shanghai_month.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let event_clusters_60m = candidates
        .iter()
        .map(|candidate| candidate.event_cluster_id.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let weak_departure_unresolved = weak_departure_pending_total.saturating_sub(
        weak_departure_returned_total
            + weak_departure_consumed_same_side
            + weak_departure_consumed_opposite_side,
    );
    let l1_gate_passed = candidates.len() >= MIN_CANDIDATES
        && covered_symbols >= MIN_SYMBOLS
        && covered_shanghai_months >= MIN_MONTHS
        && event_clusters_60m >= MIN_EVENT_CLUSTERS
        && (!args.variant.requires_breakout_body_midpoint_hold()
            || body_midpoint_rejected >= MIN_BODY_MIDPOINT_REJECTIONS)
        && (!args.variant.requires_breakout_body_strength()
            || body_strength_rejected_sources >= MIN_BODY_STRENGTH_REJECTIONS)
        && (!args.variant.uses_weak_departure_probation()
            || (weak_departure_returned_total >= 3
                && max_weak_departure_age_bars <= 1
                && outside_confirmation_anchor_count == 0))
        && (!args.variant.requires_acceptance_margin_40_atr()
            || acceptance_margin_rejected >= MIN_ACCEPTANCE_MARGIN_REJECTIONS)
        && (!args.variant.requires_external_structure_clearance()
            || (external_structure_rejected >= MIN_EXTERNAL_STRUCTURE_REJECTIONS
                && external_structure_missing_evidence == 0));
    let report = L1Report {
        schema_version: "strict_visual_breakout_l1_v9",
        research_level: "L1_OUTCOME_BLIND_COVERAGE",
        strategy_version: args
            .variant
            .strategy_version(ParityRuleVersion::CandidateV20),
        baseline_strategy_version: match args.variant {
            StrictVisualBreakoutResearchVariant::V5BreakoutBodyStrength => {
                StrictVisualBreakoutResearchVariant::V3BodyMidpointHold
                    .strategy_version(ParityRuleVersion::CandidateV20)
            }
            StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation => {
                StrictVisualBreakoutResearchVariant::V5BreakoutBodyStrength
                    .strategy_version(ParityRuleVersion::CandidateV20)
            }
            StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr => {
                StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation
                    .strategy_version(ParityRuleVersion::CandidateV20)
            }
            StrictVisualBreakoutResearchVariant::V9ExternalStructureClearance => {
                StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr
                    .strategy_version(ParityRuleVersion::CandidateV20)
            }
            StrictVisualBreakoutResearchVariant::V4ShortRangeOneR => {
                StrictVisualBreakoutResearchVariant::V3BodyMidpointHold
                    .strategy_version(ParityRuleVersion::CandidateV20)
            }
            StrictVisualBreakoutResearchVariant::V3BodyMidpointHold => {
                StrictVisualBreakoutResearchVariant::V2RetestAcceptance
                    .strategy_version(ParityRuleVersion::CandidateV20)
            }
            StrictVisualBreakoutResearchVariant::V2RetestAcceptance => {
                StrictVisualBreakoutResearchVariant::V1
                    .strategy_version(ParityRuleVersion::CandidateV20)
            }
            StrictVisualBreakoutResearchVariant::V10SymmetricRetainedBreakout
            | StrictVisualBreakoutResearchVariant::V11BreakoutCandleExtremeStop
            | StrictVisualBreakoutResearchVariant::V12ExtremeStopMinOneAtr => {
                StrictVisualBreakoutResearchVariant::V9ExternalStructureClearance
                    .strategy_version(ParityRuleVersion::CandidateV20)
            }
            StrictVisualBreakoutResearchVariant::V1
            | StrictVisualBreakoutResearchVariant::Baseline => {
                "volume_anchor_upthrust_failed_acceptance_short_15m_research_v20"
            }
        },
        universe_version: FROZEN_UNIVERSE_VERSION,
        universe_manifest_sha256: FROZEN_UNIVERSE_MANIFEST_SHA256,
        dataset_fingerprint_sha256,
        evaluation_start_ms,
        evaluation_end_ms,
        manifest_evaluation_end_ms,
        expected_symbols,
        included_symbols: eligible_symbols.len(),
        full_universe_complete,
        excluded_symbols,
        confirmed_ranges,
        parent_upgrades,
        upper_breaks,
        strong_upper_breaks,
        weak_upper_departures,
        lower_breaks,
        strong_lower_breaks,
        weak_lower_departures,
        baseline_qualified_breakout_sources,
        qualified_breakout_sources,
        body_strength_rejected_sources,
        weak_departure_pending_total,
        weak_departure_pending_upper,
        weak_departure_pending_lower,
        weak_departure_returned_total,
        weak_departure_returned_upper,
        weak_departure_returned_lower,
        weak_departure_consumed_same_side,
        weak_departure_consumed_opposite_side,
        weak_departure_unresolved,
        max_weak_departure_age_bars,
        outside_confirmation_anchor_count,
        acceptance_confirmed,
        body_midpoint_rejected,
        acceptance_margin_rejected,
        external_structure_rejected,
        external_structure_missing_evidence,
        acceptance_invalidated,
        acceptance_expired,
        qualified_candidates: candidates.len(),
        covered_symbols,
        covered_shanghai_months,
        event_clusters_60m,
        l1_gate_passed,
        l1_gate: if args.variant.requires_external_structure_clearance() {
            "candidates>=15 && symbols>=8 && shanghai_months>=3 && chained_60m_event_clusters>=10 && body_midpoint_rejections>=3 && body_strength_rejected_sources>=3 && weak_departure_returns>=3 && max_pending_age_bars<=1 && outside_confirmation_anchors==0 && acceptance_margin_rejections>=30 && external_structure_rejections>=3 && external_structure_missing_evidence==0"
        } else if args.variant.requires_acceptance_margin_40_atr() {
            "candidates>=15 && symbols>=8 && shanghai_months>=3 && chained_60m_event_clusters>=10 && body_midpoint_rejections>=3 && body_strength_rejected_sources>=3 && weak_departure_returns>=3 && max_pending_age_bars<=1 && outside_confirmation_anchors==0 && acceptance_margin_rejections>=30"
        } else if args.variant.uses_weak_departure_probation() {
            "candidates>=15 && symbols>=8 && shanghai_months>=3 && chained_60m_event_clusters>=10 && body_midpoint_rejections>=3 && body_strength_rejected_sources>=3 && weak_departure_returns>=3 && max_pending_age_bars<=1 && outside_confirmation_anchors==0"
        } else if args.variant.requires_breakout_body_strength() {
            "candidates>=15 && symbols>=8 && shanghai_months>=3 && chained_60m_event_clusters>=10 && body_midpoint_rejections>=3 && body_strength_rejected_sources>=3"
        } else if args.variant.requires_breakout_body_midpoint_hold() {
            "candidates>=15 && symbols>=8 && shanghai_months>=3 && chained_60m_event_clusters>=10 && body_midpoint_rejections>=3"
        } else {
            "candidates>=15 && symbols>=8 && shanghai_months>=3 && chained_60m_event_clusters>=10"
        },
        label_boundary: "NO_MFE_NO_MAE_NO_EXIT_NO_WIN_LOSS_NO_R_NO_PNL_NO_PROFIT_FACTOR",
        candidates,
        body_strength_rejections,
        body_midpoint_rejections,
        external_structure_rejections,
    };
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(output) = args.output {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建 L1 输出目录失败：{}", parent.display()))?;
        }
        std::fs::write(&output, format!("{json}\n"))
            .with_context(|| format!("写入 L1 报告失败：{}", output.display()))?;
        println!("{}", output.display());
    } else {
        println!("{json}");
    }
    Ok(())
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut output = None;
    let mut allow_partial_diagnostic = false;
    let mut variant = StrictVisualBreakoutResearchVariant::V1;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--output" => {
                output = Some(PathBuf::from(
                    args.next().context("--output requires a path")?,
                ));
            }
            "--allow-partial-diagnostic" => allow_partial_diagnostic = true,
            "--variant" => {
                variant = match args
                    .next()
                    .context(
                        "--variant requires v1, v2-retest-acceptance, v3-body-midpoint-hold, v4-short-range-32-one-r, v5-breakout-body-strength-60pct-25bps, v6-weak-departure-one-bar-probation, v8-acceptance-margin-0-40-atr, or v9-external-structure-clearance",
                    )?
                    .as_str()
                {
                    "v1" => StrictVisualBreakoutResearchVariant::V1,
                    "v2-retest-acceptance" => {
                        StrictVisualBreakoutResearchVariant::V2RetestAcceptance
                    }
                    "v3-body-midpoint-hold" => {
                        StrictVisualBreakoutResearchVariant::V3BodyMidpointHold
                    }
                    "v4-short-range-32-one-r" => {
                        StrictVisualBreakoutResearchVariant::V4ShortRangeOneR
                    }
                    "v5-breakout-body-strength-60pct-25bps" => {
                        StrictVisualBreakoutResearchVariant::V5BreakoutBodyStrength
                    }
                    "v6-weak-departure-one-bar-probation" => {
                        StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation
                    }
                    "v8-acceptance-margin-0-40-atr" => {
                        StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr
                    }
                    "v9-external-structure-clearance" => {
                        StrictVisualBreakoutResearchVariant::V9ExternalStructureClearance
                    }
                    other => bail!("unsupported --variant: {other}"),
                };
            }
            "--help" | "-h" => {
                println!(
                    "Usage: tradingview_strict_visual_breakout_l1 [--variant v1|v2-retest-acceptance|v3-body-midpoint-hold|v4-short-range-32-one-r|v5-breakout-body-strength-60pct-25bps|v6-weak-departure-one-bar-probation|v8-acceptance-margin-0-40-atr|v9-external-structure-clearance] [--output PATH] [--allow-partial-diagnostic]"
                );
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    Ok(Args {
        output,
        allow_partial_diagnostic,
        variant,
    })
}

fn candidate_from_signal(
    symbol: &FrozenSymbolCandles,
    signal: StrictVisualBreakoutSignal,
) -> Result<L1Candidate> {
    let candle = symbol.candles[signal.signal_index];
    let breakout_candle = symbol.candles[signal.breakout_index];
    let breakout_strength = strict_visual_breakout_body_strength(breakout_candle, Direction::Long);
    let range = signal.range;
    let shanghai = FixedOffset::east_opt(8 * 60 * 60).context("上海时区偏移无效")?;
    let timestamp = shanghai
        .timestamp_millis_opt(candle.timestamp_ms)
        .single()
        .context("信号时间戳超出 chrono 范围")?;
    let external = signal.external_structure;
    Ok(L1Candidate {
        symbol: symbol.symbol.clone(),
        breakout_time_ms: symbol.candles[signal.breakout_index].timestamp_ms,
        signal_time_ms: candle.timestamp_ms,
        acceptance_age_bars: signal.signal_index - signal.breakout_index,
        shanghai_month: format!("{:04}-{:02}", timestamp.year(), timestamp.month()),
        event_cluster_id: String::new(),
        range_start_time_ms: symbol.candles[range.start_index].timestamp_ms,
        first_confirmation_time_ms: symbol.candles[range.first_confirmation_index].timestamp_ms,
        boundary_confirmation_time_ms: symbol.candles[range.boundary_confirmation_index]
            .timestamp_ms,
        range_length_bars: range.length_bars,
        upper: range.upper,
        lower: range.lower,
        breakout_open: signal.breakout_open,
        breakout_close: signal.breakout_close,
        breakout_high: breakout_candle.high,
        breakout_body_ratio: breakout_strength.body_ratio,
        breakout_directional_move_ratio: breakout_strength.directional_move_ratio,
        breakout_body_midpoint: signal.breakout_body_midpoint,
        signal_close: candle.close,
        source_atr: signal.source_atr,
        volume_ratio: signal.source_volume_ratio,
        take_profit_atr: signal.source_take_profit_atr,
        containment_ratio: range.containment_ratio,
        direction_efficiency: range.direction_efficiency,
        edge_transition_count: range.edge_transition_count,
        external_lookback_bars: external.map(|evidence| evidence.lookback_bars),
        external_window_start_time_ms: external
            .map(|evidence| symbol.candles[evidence.window_start_index].timestamp_ms),
        external_high_time_ms: external
            .map(|evidence| symbol.candles[evidence.external_high_index].timestamp_ms),
        external_high: external.map(|evidence| evidence.external_high),
        external_high_resolved_before_breakout: external
            .map(|evidence| evidence.resolved_before_breakout),
        trade_breakout_upper: external.map(|evidence| evidence.trade_breakout_upper),
        required_breakout_close: external.map(|evidence| evidence.required_breakout_close),
        breakout_clearance_ticks: external.map(|evidence| evidence.breakout_clearance_ticks),
        breakout_clearance_atr: external.map(|evidence| {
            (signal.breakout_close - evidence.trade_breakout_upper) / signal.source_atr
        }),
    })
}

/// 把 V9 在 V8 原确认时点消费的外部结构失败转换为无 outcome 审计记录。
fn external_structure_rejection(
    symbol: &FrozenSymbolCandles,
    signal: StrictVisualBreakoutSignal,
) -> Result<L1ExternalStructureRejection> {
    let evidence = signal
        .external_structure
        .context("V9 外部结构拒绝缺少冻结证据")?;
    let breakout = symbol.candles[signal.breakout_index];
    Ok(L1ExternalStructureRejection {
        symbol: symbol.symbol.clone(),
        breakout_time_ms: breakout.timestamp_ms,
        decision_time_ms: symbol.candles[signal.signal_index].timestamp_ms,
        range_start_time_ms: symbol.candles[signal.range.start_index].timestamp_ms,
        range_length_bars: signal.range.length_bars,
        visual_upper: signal.range.upper,
        external_lookback_bars: evidence.lookback_bars,
        external_window_start_time_ms: symbol.candles[evidence.window_start_index].timestamp_ms,
        external_high_time_ms: symbol.candles[evidence.external_high_index].timestamp_ms,
        external_high: evidence.external_high,
        resolved_before_breakout: evidence.resolved_before_breakout,
        trade_breakout_upper: evidence.trade_breakout_upper,
        required_breakout_close: evidence.required_breakout_close,
        breakout_high: breakout.high,
        breakout_close: breakout.close,
        breakout_clearance_ticks: evidence.breakout_clearance_ticks,
        breakout_clearance_atr: (breakout.close - evidence.trade_breakout_upper)
            / signal.source_atr,
    })
}

/// 把被 V3 消费的首次弱确认转换为无 outcome 审计记录。
fn body_midpoint_rejection(
    symbol: &FrozenSymbolCandles,
    signal: StrictVisualBreakoutSignal,
) -> L1BodyMidpointRejection {
    L1BodyMidpointRejection {
        symbol: symbol.symbol.clone(),
        breakout_time_ms: symbol.candles[signal.breakout_index].timestamp_ms,
        decision_time_ms: symbol.candles[signal.signal_index].timestamp_ms,
        upper: signal.range.upper,
        breakout_open: signal.breakout_open,
        breakout_close: signal.breakout_close,
        breakout_body_midpoint: signal.breakout_body_midpoint,
        confirmation_close: symbol.candles[signal.signal_index].close,
    }
}

/// 同方向信号按相邻时间串联，避免把一次全市场脉冲误当成多个独立样本。
fn assign_event_clusters(candidates: &mut [L1Candidate]) {
    candidates.sort_by(|left, right| {
        left.signal_time_ms
            .cmp(&right.signal_time_ms)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    let mut last_time = None;
    let mut cluster_start = 0;
    for candidate in candidates {
        if last_time.is_none_or(|last| candidate.signal_time_ms - last > EVENT_CLUSTER_MS) {
            cluster_start = candidate.signal_time_ms;
        }
        candidate.event_cluster_id = format!("long-{cluster_start}");
        last_time = Some(candidate.signal_time_ms);
    }
}

#[derive(Debug)]
struct ReplayWindowCoverage {
    expected: usize,
    loaded: usize,
    is_complete: bool,
}

fn modal_snapshot_end(symbols: &[FrozenSymbolCandles], evaluation_start_ms: i64) -> Option<i64> {
    let mut counts = BTreeMap::<i64, usize>::new();
    for timestamp_ms in symbols.iter().filter_map(|symbol| {
        symbol
            .candles
            .iter()
            .rev()
            .find(|candle| candle.timestamp_ms >= evaluation_start_ms)
            .map(|candle| candle.timestamp_ms)
    }) {
        *counts.entry(timestamp_ms).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
        .map(|(timestamp_ms, _)| timestamp_ms)
}

fn replay_window_coverage(
    candles: &[Candle],
    start_ms: i64,
    end_ms: i64,
) -> Result<ReplayWindowCoverage> {
    if end_ms < start_ms
        || start_ms.rem_euclid(CANDLE_INTERVAL_MS) != 0
        || end_ms.rem_euclid(CANDLE_INTERVAL_MS) != 0
    {
        bail!("L1 评价窗口没有对齐 15 分钟");
    }
    let expected =
        usize::try_from((end_ms - start_ms) / CANDLE_INTERVAL_MS + 1).context("L1 评价根数溢出")?;
    let selected = candles
        .iter()
        .filter(|candle| (start_ms..=end_ms).contains(&candle.timestamp_ms))
        .collect::<Vec<_>>();
    let loaded = selected.len();
    let is_complete = loaded == expected
        && selected
            .first()
            .is_some_and(|candle| candle.timestamp_ms == start_ms)
        && selected
            .last()
            .is_some_and(|candle| candle.timestamp_ms == end_ms)
        && selected
            .windows(2)
            .all(|pair| pair[1].timestamp_ms - pair[0].timestamp_ms == CANDLE_INTERVAL_MS);
    Ok(ReplayWindowCoverage {
        expected,
        loaded,
        is_complete,
    })
}

/// 数据指纹覆盖全部已加载成员与 OHLCV，避免相同 manifest 名称掩盖本地快照变化。
fn dataset_fingerprint(symbols: &[FrozenSymbolCandles]) -> String {
    let mut hasher = Sha256::new();
    for symbol in symbols {
        hasher.update(symbol.symbol.as_bytes());
        hasher.update(symbol.tick_size.to_bits().to_le_bytes());
        for candle in &symbol.candles {
            hasher.update(candle.timestamp_ms.to_le_bytes());
            hasher.update(candle.open.to_bits().to_le_bytes());
            hasher.update(candle.high.to_bits().to_le_bytes());
            hasher.update(candle.low.to_bits().to_le_bytes());
            hasher.update(candle.close.to_bits().to_le_bytes());
            hasher.update(candle.volume.to_bits().to_le_bytes());
        }
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range_edge_candle(index: usize, upper: bool) -> Candle {
        let (open, high, low, close) = if upper {
            (100.4, 101.0, 100.2, 100.7)
        } else {
            (99.6, 99.8, 99.0, 99.3)
        };
        Candle {
            timestamp_ms: index as i64 * CANDLE_INTERVAL_MS,
            open,
            high,
            low,
            close,
            volume: 100.0,
        }
    }

    fn armed_retest_fixture(
        variant: StrictVisualBreakoutResearchVariant,
        breakout_close: f64,
    ) -> (Vec<Candle>, StrictVisualLongEntryState) {
        let mut candles = (0..8)
            .map(|index| range_edge_candle(index, index % 2 == 0))
            .collect::<Vec<_>>();
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
                variant,
            );
        }
        candles.push(Candle {
            timestamp_ms: 8 * CANDLE_INTERVAL_MS,
            open: 100.8,
            high: breakout_close + 0.2,
            low: 100.7,
            close: breakout_close,
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
                variant,
            ),
            Some(StrictVisualLongEntryEvent::AcceptanceArmed(_))
        ));
        (candles, state)
    }

    #[test]
    fn event_clusters_chain_signals_within_sixty_minutes() {
        let mut candidates = [0, 3_000_000, 6_000_000, 10_000_000]
            .into_iter()
            .map(|signal_time_ms| L1Candidate {
                symbol: format!("S{signal_time_ms}"),
                breakout_time_ms: signal_time_ms,
                signal_time_ms,
                acceptance_age_bars: 0,
                shanghai_month: "2026-01".to_owned(),
                event_cluster_id: String::new(),
                range_start_time_ms: 0,
                first_confirmation_time_ms: 0,
                boundary_confirmation_time_ms: 0,
                range_length_bars: 8,
                upper: 1.0,
                lower: 0.9,
                breakout_open: 1.0,
                breakout_close: 1.1,
                breakout_high: 1.1,
                breakout_body_ratio: 0.8,
                breakout_directional_move_ratio: 0.01,
                breakout_body_midpoint: 1.05,
                signal_close: 1.1,
                source_atr: 0.1,
                volume_ratio: 3.0,
                take_profit_atr: 2.7,
                containment_ratio: 1.0,
                direction_efficiency: 0.0,
                edge_transition_count: 3,
                external_lookback_bars: None,
                external_window_start_time_ms: None,
                external_high_time_ms: None,
                external_high: None,
                external_high_resolved_before_breakout: None,
                trade_breakout_upper: None,
                required_breakout_close: None,
                breakout_clearance_ticks: None,
                breakout_clearance_atr: None,
            })
            .collect::<Vec<_>>();
        assign_event_clusters(&mut candidates);
        assert_eq!(
            candidates[0].event_cluster_id,
            candidates[2].event_cluster_id
        );
        assert_ne!(
            candidates[2].event_cluster_id,
            candidates[3].event_cluster_id
        );
    }

    #[test]
    fn qualified_breakout_requires_a_later_bullish_volume_bar_with_target_tier() {
        let mut candles = (0..8)
            .map(|index| range_edge_candle(index, index % 2 == 0))
            .collect::<Vec<_>>();
        let mut state = StrictVisualLongEntryState::default();
        for index in 0..8 {
            assert!(state
                .update(
                    &candles,
                    index,
                    0.01,
                    Some(1.0),
                    true,
                    Some(3.0),
                    Some(2.7),
                    StrictVisualBreakoutResearchVariant::V1,
                )
                .is_none_or(|event| event.entry_signal().is_none()));
        }
        candles.push(Candle {
            timestamp_ms: 8 * CANDLE_INTERVAL_MS,
            open: 100.8,
            high: 101.4,
            low: 100.7,
            close: 101.2,
            volume: 300.0,
        });
        let event = state
            .update(
                &candles,
                8,
                0.01,
                Some(1.0),
                true,
                Some(3.0),
                Some(2.7),
                StrictVisualBreakoutResearchVariant::V1,
            )
            .expect("V1 direct signal");
        assert_eq!(
            event.entry_signal().map(|signal| signal.range.upper),
            Some(101.0)
        );
    }

    #[test]
    fn v2_accepts_only_a_later_retest_that_closes_above_the_frozen_upper() {
        let (mut candles, mut state) = armed_retest_fixture(
            StrictVisualBreakoutResearchVariant::V2RetestAcceptance,
            101.2,
        );
        candles.push(Candle {
            timestamp_ms: 9 * CANDLE_INTERVAL_MS,
            open: 101.4,
            high: 101.6,
            low: 101.2,
            close: 101.3,
            volume: 80.0,
        });
        let Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(signal)) = state.update(
            &candles,
            9,
            0.01,
            Some(1.1),
            false,
            Some(1.0),
            None,
            StrictVisualBreakoutResearchVariant::V2RetestAcceptance,
        ) else {
            panic!("later completed retest should confirm V2");
        };
        assert_eq!(signal.breakout_index, 8);
        assert_eq!(signal.signal_index, 9);
        assert_eq!(signal.breakout_open, 100.8);
        assert_eq!(signal.breakout_body_midpoint, 101.0);
        assert_eq!(signal.source_atr, 1.0);
        assert_eq!(signal.source_take_profit_atr, 2.7);
    }

    #[test]
    fn v3_rejects_the_first_v2_confirmation_below_the_frozen_body_midpoint() {
        let (mut candles, mut state) = armed_retest_fixture(
            StrictVisualBreakoutResearchVariant::V3BodyMidpointHold,
            101.4,
        );
        candles.push(Candle {
            timestamp_ms: 9 * CANDLE_INTERVAL_MS,
            open: 101.3,
            high: 101.4,
            low: 101.0,
            close: 101.05,
            volume: 80.0,
        });
        let Some(StrictVisualLongEntryEvent::AcceptanceBodyMidpointRejected(signal)) = state
            .update(
                &candles,
                9,
                0.01,
                Some(1.1),
                false,
                Some(1.0),
                None,
                StrictVisualBreakoutResearchVariant::V3BodyMidpointHold,
            )
        else {
            panic!("V3 must consume the first V2 confirmation below the body midpoint");
        };
        assert_eq!(signal.breakout_body_midpoint, 101.1);
        assert_eq!(signal.signal_index, 9);

        candles.push(Candle {
            timestamp_ms: 10 * CANDLE_INTERVAL_MS,
            open: 101.1,
            high: 101.5,
            low: 101.0,
            close: 101.3,
            volume: 80.0,
        });
        assert!(state
            .update(
                &candles,
                10,
                0.01,
                Some(1.1),
                false,
                Some(1.0),
                None,
                StrictVisualBreakoutResearchVariant::V3BodyMidpointHold,
            )
            .is_none_or(|event| event.entry_signal().is_none()));
    }

    #[test]
    fn v3_retains_a_v2_confirmation_at_the_frozen_body_midpoint() {
        let (mut candles, mut state) = armed_retest_fixture(
            StrictVisualBreakoutResearchVariant::V3BodyMidpointHold,
            101.4,
        );
        candles.push(Candle {
            timestamp_ms: 9 * CANDLE_INTERVAL_MS,
            open: 101.3,
            high: 101.5,
            low: 101.0,
            close: 101.1,
            volume: 80.0,
        });
        let Some(StrictVisualLongEntryEvent::AcceptanceConfirmed(signal)) = state.update(
            &candles,
            9,
            0.01,
            Some(1.1),
            false,
            Some(1.0),
            None,
            StrictVisualBreakoutResearchVariant::V3BodyMidpointHold,
        ) else {
            panic!("V3 uses an inclusive body-midpoint hold");
        };
        assert_eq!(signal.breakout_body_midpoint, 101.1);
        assert_eq!(signal.signal_index, 9);
    }

    #[test]
    fn v2_closes_the_source_on_reentry_or_after_the_third_waiting_bar() {
        let (mut invalidated_candles, mut invalidated_state) = armed_retest_fixture(
            StrictVisualBreakoutResearchVariant::V2RetestAcceptance,
            101.2,
        );
        invalidated_candles.push(Candle {
            timestamp_ms: 9 * CANDLE_INTERVAL_MS,
            open: 101.2,
            high: 101.3,
            low: 100.9,
            close: 101.0,
            volume: 80.0,
        });
        assert!(matches!(
            invalidated_state.update(
                &invalidated_candles,
                9,
                0.01,
                Some(1.1),
                false,
                Some(1.0),
                None,
                StrictVisualBreakoutResearchVariant::V2RetestAcceptance,
            ),
            Some(StrictVisualLongEntryEvent::AcceptanceInvalidated(_))
        ));

        let (mut expired_candles, mut expired_state) = armed_retest_fixture(
            StrictVisualBreakoutResearchVariant::V2RetestAcceptance,
            101.2,
        );
        for index in 9..=11 {
            expired_candles.push(Candle {
                timestamp_ms: index as i64 * CANDLE_INTERVAL_MS,
                open: 101.5,
                high: 101.8,
                low: 101.4,
                close: 101.6,
                volume: 80.0,
            });
            let event = expired_state.update(
                &expired_candles,
                index,
                0.01,
                Some(1.1),
                false,
                Some(1.0),
                None,
                StrictVisualBreakoutResearchVariant::V2RetestAcceptance,
            );
            if index < 11 {
                assert_eq!(event, None);
            } else {
                assert!(matches!(
                    event,
                    Some(StrictVisualLongEntryEvent::AcceptanceExpired(_))
                ));
            }
        }
    }
}
