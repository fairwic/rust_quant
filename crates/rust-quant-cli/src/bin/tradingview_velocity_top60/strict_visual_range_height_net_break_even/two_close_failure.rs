use super::*;
use chrono::{Datelike, TimeZone, Utc};
use std::collections::BTreeSet;

const FAILURE_CLOSES_REQUIRED: usize = 2;
const MIN_ARMED_TRADES: usize = 30;
const MIN_ARMED_EVENTS: usize = 20;
const MIN_DECISIONS: usize = 8;
const MIN_DECISION_EVENTS: usize = 8;
const MIN_DECISION_SYMBOLS: usize = 6;
const MIN_DECISION_MONTHS: usize = 4;
const MIN_DECISION_RATIO_PERCENT: f64 = 15.0;
const MAX_DECISION_RATIO_PERCENT: f64 = 45.0;

/// V5 只输出第二次连续失守收盘时已经可见的 L1 决策事件，不读取其后盈亏。
#[derive(Debug, Serialize)]
pub(crate) struct StrictVisualTwoCloseFailureL1Report {
    /// 独立 Research-only 身份，不覆盖 V3、V4 或冻结 V8。
    research_version: &'static str,
    /// 唯一退出门禁及其完成棒时序。
    definition: &'static str,
    /// L1 独立模拟只处理冻结止损/目标，反手和容量留到 L2 恢复。
    implementation_mode: &'static str,
    /// 明确禁止读取决策时点之后的 outcome 来选择规则。
    label_boundary: &'static str,
    /// 必须连续失守的完成 K 数；本轮固定为 2，不搜索邻域。
    failure_closes_required: usize,
    /// V8 中所有包含严格视觉横盘家族的已闭仓交易。
    strict_visual_family_trades: usize,
    /// 真正参与 L1 决策扫描的纯家族 Fixed 交易。
    eligible_pure_fixed_trades: usize,
    /// 混合其他家族的交易不参与本退出变量。
    excluded_mixed_family_trades: usize,
    /// 纯严格横盘但使用非 Fixed 退出的交易保持原样。
    excluded_non_fixed_pure_trades: usize,
    /// 当前样本可用于经验覆盖的多单数量。
    empirical_long_trades: usize,
    /// 当前样本可用于经验覆盖的空单数量；为零时只保留镜像测试结论。
    empirical_short_trades: usize,
    /// 零成本与成本报告的信号、成交和冻结风险漂移数。
    identity_changed_trades: usize,
    /// 在冻结止损/目标前由完成 K 极值达到 1H 的交易数。
    armed_trades: usize,
    /// armed 交易按 60 分钟同方向链式规则合并后的事件数。
    armed_effective_events_60m: usize,
    /// 在隔离存活路径中确认两次连续失守的交易数。
    decision_events: usize,
    /// 决策交易按 60 分钟同方向链式规则合并后的事件数。
    decision_effective_events_60m: usize,
    /// 决策事件覆盖的币种数。
    decision_symbols: usize,
    /// 决策事件覆盖的上海自然月数。
    decision_shanghai_months: usize,
    /// 决策交易占 armed 交易的比例，单位百分比。
    decision_share_of_armed_percent: f64,
    /// L1 机械覆盖门禁；通过只允许读取 outcome 进入 L2。
    l1_gate: L1Gate,
    /// 每笔记录只保存第二次失守收盘及以前可见的证据。
    records: Vec<DecisionRecord>,
}

/// 单笔 V5 L1 记录严格分离冻结风险、决策时证据与后续 outcome。
#[derive(Debug, Serialize)]
struct DecisionRecord {
    /// OKX 永续合约标识，用于检查决策覆盖是否集中在少数币种。
    symbol: String,
    /// 交易方向；当前经验样本全为多单，空单仅由镜像测试覆盖。
    direction: Direction,
    /// Unix 毫秒信号时间戳。
    signal_time_ms: i64,
    /// Unix 毫秒实际入场时间戳；用于从同一根开始因果扫描。
    entry_time_ms: i64,
    /// 实际成交价；L1 只用它计算 1H 与成本保本激活价。
    entry_price: f64,
    /// 信号成交时冻结的初始保护价。
    initial_stop: f64,
    /// 信号意图与实际入场共同还原的冻结目标价。
    frozen_target: f64,
    /// 信号时冻结的视觉横盘上沿，多单失守按该边界判断。
    frozen_range_upper: f64,
    /// 信号时冻结的视觉横盘下沿，供空单镜像与结构审计。
    frozen_range_lower: f64,
    /// `upper - lower`；入场后不得重新计算。
    frozen_range_height: f64,
    /// 1H 与成本地板中离入场更远的 armed 价。
    activation_price: f64,
    /// 按 8bps 单边成本向安全 tick 取整的净保本价。
    net_break_even_stop: f64,
    /// 完成 K 首次达到 1H 的 Unix 毫秒时间戳；`None` 表示先触及原止损/目标。
    armed_time_ms: Option<i64>,
    /// 当前连续失守序列第一根 K 的 Unix 毫秒时间戳；重新收复边界后会清空。
    first_failure_close_time_ms: Option<i64>,
    /// 第二根连续失守完成收盘时间；这是 L1 可见信息的截止点。
    decision_time_ms: Option<i64>,
}

/// L1 覆盖门禁不包含任何收益指标，避免结果反向决定两根确认规则。
#[derive(Debug, Serialize)]
struct L1Gate {
    /// 是否具备至少 30 笔可扫描的纯家族 Fixed 交易。
    eligible_trades_at_least_30: bool,
    /// 是否至少 30 笔在冻结止损/目标前达到 1H。
    armed_trades_at_least_30: bool,
    /// armed 交易是否覆盖至少 20 个 60 分钟事件。
    armed_events_at_least_20: bool,
    /// 两次连续失守是否至少形成 8 个决策。
    decisions_at_least_8: bool,
    /// 决策是否覆盖至少 8 个 60 分钟事件。
    decision_events_at_least_8: bool,
    /// 决策是否覆盖至少 6 个币种。
    decision_symbols_at_least_6: bool,
    /// 决策是否覆盖至少 4 个上海自然月。
    decision_months_at_least_4: bool,
    /// 实际影响是否落在结果前冻结的 15%～45% 区间。
    decision_share_between_15_and_45_percent: bool,
    /// 零成本与成本报告的成交身份和冻结风险是否一致。
    trade_identity_preserved: bool,
    /// 所有无标签覆盖门禁的合取；`true` 也只允许进入 L2。
    l1_gate_passed: bool,
}

/// 独立决策扫描输入只包含实际成交、冻结风险和信号时横盘，不含最终退出标签。
#[derive(Debug, Clone, Copy)]
struct DecisionScanInput {
    /// 决定 1H、结构失守和止损/目标镜像方向。
    direction: Direction,
    /// Unix 毫秒实际入场时间戳，是隔离扫描的起点。
    entry_time_ms: i64,
    /// 实际入场价，用于计算 armed 与成本保护价。
    entry_price: f64,
    /// 冻结初始止损；命中后不再读取该棒收盘。
    initial_stop: f64,
    /// 冻结原目标；命中后不再读取该棒收盘。
    frozen_target: f64,
    /// 多单两次失守使用的信号时冻结上沿。
    frozen_range_upper: f64,
    /// 空单镜像两次失守使用的信号时冻结下沿。
    frozen_range_lower: f64,
    /// 信号时冻结区间高度；入场后的扩大或收缩不能修改它。
    frozen_range_height: f64,
    /// 当前合约价格最小变动单位，用于安全侧成本取整。
    tick_size: f64,
}

/// 扫描结果只描述 armed 与连续失守决策，不描述此后交易输赢。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DecisionState {
    /// Unix 毫秒时间戳。
    armed_time_ms: Option<i64>,
    /// Unix 毫秒时间戳；若中途收复边界则被清空。
    first_failure_close_time_ms: Option<i64>,
    /// Unix 毫秒时间戳。
    decision_time_ms: Option<i64>,
}

/// 构建 V5 L1 无标签决策账本；本函数不计算或序列化最终退出与 R。
pub(crate) fn build_strict_visual_two_close_failure_l1(
    inputs: &[ExitCounterfactualInput<'_>],
) -> Result<StrictVisualTwoCloseFailureL1Report> {
    let mut records = Vec::new();
    let mut strict_visual_family_trades = 0;
    let mut excluded_mixed_family_trades = 0;
    let mut excluded_non_fixed_pure_trades = 0;

    for input in inputs {
        validate_input_identity(input)?;
        for (zero_trade, cost_trade) in input
            .zero_cost
            .trades
            .iter()
            .zip(&input.cost_adjusted.trades)
        {
            validate_l1_trade_identity(input.symbol, zero_trade, cost_trade)?;
            if !zero_trade
                .families
                .contains(&SignalFamily::StrictVisualConsolidationBreakLong)
            {
                continue;
            }
            strict_visual_family_trades += 1;
            if !is_pure_strict_visual(zero_trade) {
                excluded_mixed_family_trades += 1;
                continue;
            }
            if zero_trade.exit_policy != ExitPolicy::Fixed {
                excluded_non_fixed_pure_trades += 1;
                continue;
            }

            let intent = matching_intent(input.zero_cost, zero_trade)?;
            validate_cost_intent(input.cost_adjusted, zero_trade, intent)?;
            let range_height = intent.strict_visual_range_height.ok_or_else(|| {
                anyhow!(
                    "{} @ {} 缺少信号时冻结的严格视觉横盘高度",
                    input.symbol,
                    zero_trade.signal_time_ms
                )
            })?;
            let range_upper = intent.breakout_line.ok_or_else(|| {
                anyhow!(
                    "{} @ {} 缺少信号时冻结的严格视觉横盘上沿",
                    input.symbol,
                    zero_trade.signal_time_ms
                )
            })?;
            validate_range(
                input.symbol,
                zero_trade.signal_time_ms,
                range_upper,
                range_height,
            )?;
            let range_lower = range_upper - range_height;
            let frozen_target =
                actual_target_from_intent(intent, zero_trade.entry_price, input.tick_size)?;
            let scan_input = DecisionScanInput {
                direction: zero_trade.direction,
                entry_time_ms: zero_trade.entry_time_ms,
                entry_price: zero_trade.entry_price,
                initial_stop: zero_trade.initial_stop,
                frozen_target,
                frozen_range_upper: range_upper,
                frozen_range_lower: range_lower,
                frozen_range_height: range_height,
                tick_size: input.tick_size,
            };
            validate_scan_input(input.symbol, zero_trade.signal_time_ms, scan_input)?;
            let state = scan_decision(input.candles, scan_input)?;
            let net_break_even_stop = net_break_even_price(
                zero_trade.direction,
                zero_trade.entry_price,
                input.tick_size,
                NET_BREAK_EVEN_COST_BPS_PER_SIDE,
            );
            records.push(DecisionRecord {
                symbol: input.symbol.to_owned(),
                direction: zero_trade.direction,
                signal_time_ms: zero_trade.signal_time_ms,
                entry_time_ms: zero_trade.entry_time_ms,
                entry_price: zero_trade.entry_price,
                initial_stop: zero_trade.initial_stop,
                frozen_target,
                frozen_range_upper: range_upper,
                frozen_range_lower: range_lower,
                frozen_range_height: range_height,
                activation_price: activation_price(
                    zero_trade.direction,
                    zero_trade.entry_price,
                    range_height,
                    net_break_even_stop,
                ),
                net_break_even_stop,
                armed_time_ms: state.armed_time_ms,
                first_failure_close_time_ms: state.first_failure_close_time_ms,
                decision_time_ms: state.decision_time_ms,
            });
        }
    }

    let armed_trades = records
        .iter()
        .filter(|record| record.armed_time_ms.is_some())
        .count();
    let decision_events = records
        .iter()
        .filter(|record| record.decision_time_ms.is_some())
        .count();
    let armed_effective_events_60m =
        event_count_with(&records, |record| record.armed_time_ms.is_some());
    let decision_effective_events_60m =
        event_count_with(&records, |record| record.decision_time_ms.is_some());
    let decision_symbols = records
        .iter()
        .filter(|record| record.decision_time_ms.is_some())
        .map(|record| record.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let decision_shanghai_months = records
        .iter()
        .filter(|record| record.decision_time_ms.is_some())
        .map(|record| shanghai_month(record.signal_time_ms))
        .collect::<BTreeSet<_>>()
        .len();
    let decision_share_of_armed_percent = if armed_trades == 0 {
        0.0
    } else {
        decision_events as f64 / armed_trades as f64 * 100.0
    };
    let l1_gate = l1_gate(
        records.len(),
        armed_trades,
        armed_effective_events_60m,
        decision_events,
        decision_effective_events_60m,
        decision_symbols,
        decision_shanghai_months,
        decision_share_of_armed_percent,
    );

    Ok(StrictVisualTwoCloseFailureL1Report {
        research_version:
            "strict_visual_breakout_one_height_two_close_range_upper_failure_15m_research_v5",
        definition: "pure strict_visual_consolidation_break_long Fixed trades; a completed candle first reaches 1.0 frozen range height, then two later consecutive completed closes strictly lose the frozen range upper before net break-even protection may start on the next candle; short is mirrored",
        implementation_mode: "L1 isolated frozen-stop/target decision scan; no reverse, capacity release, replacement entry, or post-decision outcome is evaluated",
        label_boundary: "records end at the second failure close and contain no actual exit time, exit reason, exit price, MFE, MAE, R, win, or loss",
        failure_closes_required: FAILURE_CLOSES_REQUIRED,
        strict_visual_family_trades,
        eligible_pure_fixed_trades: records.len(),
        excluded_mixed_family_trades,
        excluded_non_fixed_pure_trades,
        empirical_long_trades: records
            .iter()
            .filter(|record| record.direction == Direction::Long)
            .count(),
        empirical_short_trades: records
            .iter()
            .filter(|record| record.direction == Direction::Short)
            .count(),
        identity_changed_trades: 0,
        armed_trades,
        armed_effective_events_60m,
        decision_events,
        decision_effective_events_60m,
        decision_symbols,
        decision_shanghai_months,
        decision_share_of_armed_percent,
        l1_gate,
        records,
    })
}

/// 零成本与成本报告在 L1 只能比较成交身份和冻结风险，不能读取最终退出标签。
fn validate_l1_trade_identity(symbol: &str, zero: &Trade, cost: &Trade) -> Result<()> {
    let same = zero.direction == cost.direction
        && zero.families == cost.families
        && zero.exit_policy == cost.exit_policy
        && zero.signal_time_ms == cost.signal_time_ms
        && zero.entry_time_ms == cost.entry_time_ms
        && nearly_equal(zero.entry_price, cost.entry_price)
        && nearly_equal(zero.initial_stop, cost.initial_stop)
        && nearly_equal(zero.initial_risk, cost.initial_risk);
    if !same {
        bail!(
            "{} @ {} 的零成本与成本后 L1 成交身份漂移",
            symbol,
            zero.signal_time_ms
        );
    }
    Ok(())
}

/// 拒绝不能还原正价格上下沿的冻结横盘，防止无效 H 进入覆盖统计。
fn validate_range(symbol: &str, signal_time_ms: i64, upper: f64, height: f64) -> Result<()> {
    if !upper.is_finite() || upper <= 0.0 || !height.is_finite() || height <= 0.0 {
        bail!(
            "{} @ {} 的冻结视觉横盘无效：upper={} height={}",
            symbol,
            signal_time_ms,
            upper,
            height
        );
    }
    if upper - height <= 0.0 {
        bail!("{} @ {} 的冻结横盘下沿不为正", symbol, signal_time_ms);
    }
    Ok(())
}

/// 验证止损、入场和目标的方向顺序，保证隔离存活判断与原交易风险一致。
fn validate_scan_input(symbol: &str, signal_time_ms: i64, input: DecisionScanInput) -> Result<()> {
    let ordered = match input.direction {
        Direction::Long => {
            input.initial_stop < input.entry_price && input.entry_price < input.frozen_target
        }
        Direction::Short => {
            input.initial_stop > input.entry_price && input.entry_price > input.frozen_target
        }
    };
    if !ordered {
        bail!(
            "{} @ {} 的冻结止损/入场/目标顺序无效",
            symbol,
            signal_time_ms
        );
    }
    if input.tick_size <= 0.0 || !input.tick_size.is_finite() {
        bail!("{} @ {} 的 tick size 无效", symbol, signal_time_ms);
    }
    Ok(())
}

/// 独立路径在冻结止损/目标前寻找决策时点；命中后立即停止，绝不读取更晚 K 线。
fn scan_decision(candles: &[Candle], input: DecisionScanInput) -> Result<DecisionState> {
    let entry_index = candles
        .binary_search_by_key(&input.entry_time_ms, |candle| candle.timestamp_ms)
        .map_err(|_| anyhow!("找不到 V5 L1 入场 K 线：{}", input.entry_time_ms))?;
    let net_break_even_stop = net_break_even_price(
        input.direction,
        input.entry_price,
        input.tick_size,
        NET_BREAK_EVEN_COST_BPS_PER_SIDE,
    );
    let activation = activation_price(
        input.direction,
        input.entry_price,
        input.frozen_range_height,
        net_break_even_stop,
    );
    let mut state = DecisionState::default();

    for candle in &candles[entry_index..] {
        // 原保护或目标在本棒路径中成交后，收盘证据已经没有持仓对象，不能计入 L1。
        if frozen_exit_hit(*candle, input) {
            break;
        }

        let armed_from_prior_close = state
            .armed_time_ms
            .is_some_and(|armed| candle.timestamp_ms > armed);
        if armed_from_prior_close {
            if range_boundary_lost_at_close(*candle, input) {
                if state.first_failure_close_time_ms.is_some() {
                    state.decision_time_ms = Some(candle.timestamp_ms);
                    return Ok(state);
                }
                state.first_failure_close_time_ms = Some(candle.timestamp_ms);
            } else {
                // 单棒跌回后快速收复属于正常回踩；必须重新出现完整的连续两棒序列。
                state.first_failure_close_time_ms = None;
            }
            continue;
        }

        if state.armed_time_ms.is_none() && activation_reached(input.direction, *candle, activation)
        {
            // armed 棒收盘才确认 1H，故它自身即使收回区间也不能充当第一根失守棒。
            state.armed_time_ms = Some(candle.timestamp_ms);
        }
    }
    Ok(state)
}

/// 只用完成收盘判断失守；影线越界不能推进连续计数。
fn range_boundary_lost_at_close(candle: Candle, input: DecisionScanInput) -> bool {
    match input.direction {
        Direction::Long => candle.close < input.frozen_range_upper,
        Direction::Short => candle.close > input.frozen_range_lower,
    }
}

/// L1 只需知道冻结 stop/target 是否已结束仓位，不计算成交价或收益标签。
fn frozen_exit_hit(candle: Candle, input: DecisionScanInput) -> bool {
    let open_hit = match input.direction {
        Direction::Long => candle.open <= input.initial_stop || candle.open >= input.frozen_target,
        Direction::Short => candle.open >= input.initial_stop || candle.open <= input.frozen_target,
    };
    open_hit
        || broker_path(candle).windows(2).any(|segment| {
            between(input.initial_stop, segment[0], segment[1])
                || between(input.frozen_target, segment[0], segment[1])
        })
}

/// 同方向信号按相邻不超过 60 分钟链式归并，防止同步市场波动放大样本数。
fn event_count_with(
    records: &[DecisionRecord],
    predicate: impl Fn(&DecisionRecord) -> bool,
) -> usize {
    [Direction::Long, Direction::Short]
        .iter()
        .map(|&direction| {
            let mut times = records
                .iter()
                .filter(|record| record.direction == direction && predicate(record))
                .map(|record| record.signal_time_ms)
                .collect::<Vec<_>>();
            times.sort_unstable();
            times
                .iter()
                .enumerate()
                .filter(|(index, time)| {
                    *index == 0 || **time - times[*index - 1] > EVENT_CLUSTER_MS
                })
                .count()
        })
        .sum()
}

/// 把信号 Unix 毫秒时间映射到上海自然月，用于检查时间集中度。
fn shanghai_month(timestamp_ms: i64) -> String {
    let shifted = timestamp_ms + 8 * 60 * 60 * 1_000;
    let time = Utc
        .timestamp_millis_opt(shifted)
        .single()
        .expect("valid unix milliseconds");
    format!("{:04}-{:02}", time.year(), time.month())
}

#[allow(clippy::too_many_arguments)]
/// 汇总结果前冻结的 L1 门禁，不接收任何收益或最终退出字段。
fn l1_gate(
    eligible_trades: usize,
    armed_trades: usize,
    armed_events: usize,
    decisions: usize,
    decision_events: usize,
    decision_symbols: usize,
    decision_months: usize,
    decision_share_percent: f64,
) -> L1Gate {
    let eligible_trades_at_least_30 = eligible_trades >= 30;
    let armed_trades_at_least_30 = armed_trades >= MIN_ARMED_TRADES;
    let armed_events_at_least_20 = armed_events >= MIN_ARMED_EVENTS;
    let decisions_at_least_8 = decisions >= MIN_DECISIONS;
    let decision_events_at_least_8 = decision_events >= MIN_DECISION_EVENTS;
    let decision_symbols_at_least_6 = decision_symbols >= MIN_DECISION_SYMBOLS;
    let decision_months_at_least_4 = decision_months >= MIN_DECISION_MONTHS;
    let decision_share_between_15_and_45_percent =
        (MIN_DECISION_RATIO_PERCENT..=MAX_DECISION_RATIO_PERCENT).contains(&decision_share_percent);
    let trade_identity_preserved = true;
    let l1_gate_passed = eligible_trades_at_least_30
        && armed_trades_at_least_30
        && armed_events_at_least_20
        && decisions_at_least_8
        && decision_events_at_least_8
        && decision_symbols_at_least_6
        && decision_months_at_least_4
        && decision_share_between_15_and_45_percent
        && trade_identity_preserved;
    L1Gate {
        eligible_trades_at_least_30,
        armed_trades_at_least_30,
        armed_events_at_least_20,
        decisions_at_least_8,
        decision_events_at_least_8,
        decision_symbols_at_least_6,
        decision_months_at_least_4,
        decision_share_between_15_and_45_percent,
        trade_identity_preserved,
        l1_gate_passed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造不依赖指标的已完成 K 线，专门验证退出决策时序。
    fn candle(timestamp_ms: i64, open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle {
            timestamp_ms,
            open,
            high,
            low,
            close,
            volume: 1.0,
        }
    }

    /// 返回具有清晰 10 点风险和 10 点横盘高度的多单测试输入。
    fn long_input() -> DecisionScanInput {
        DecisionScanInput {
            direction: Direction::Long,
            entry_time_ms: 0,
            entry_price: 100.0,
            initial_stop: 90.0,
            frozen_target: 130.0,
            frozen_range_upper: 100.0,
            frozen_range_lower: 90.0,
            frozen_range_height: 10.0,
            tick_size: 0.1,
        }
    }

    #[test]
    fn single_failure_close_resets_after_reclaim_before_two_new_failures() {
        let candles = [
            candle(0, 100.0, 111.0, 99.0, 109.0),
            candle(1, 109.0, 109.0, 98.0, 99.0),
            candle(2, 99.0, 102.0, 98.0, 101.0),
            candle(3, 101.0, 102.0, 98.0, 99.0),
            candle(4, 99.0, 100.0, 97.0, 98.0),
        ];
        let state = scan_decision(&candles, long_input()).expect("scan succeeds");
        assert_eq!(state.armed_time_ms, Some(0));
        assert_eq!(state.first_failure_close_time_ms, Some(3));
        assert_eq!(state.decision_time_ms, Some(4));
    }

    #[test]
    fn activation_candle_cannot_count_as_the_first_failure_close() {
        let candles = [
            candle(0, 100.0, 111.0, 98.0, 99.0),
            candle(1, 99.0, 101.0, 98.0, 99.0),
        ];
        let state = scan_decision(&candles, long_input()).expect("scan succeeds");
        assert_eq!(state.armed_time_ms, Some(0));
        assert_eq!(state.first_failure_close_time_ms, Some(1));
        assert_eq!(state.decision_time_ms, None);
    }

    #[test]
    fn frozen_target_before_second_close_ends_the_l1_position() {
        let candles = [
            candle(0, 100.0, 111.0, 99.0, 109.0),
            candle(1, 109.0, 109.0, 98.0, 99.0),
            candle(2, 99.0, 131.0, 97.0, 98.0),
        ];
        let state = scan_decision(&candles, long_input()).expect("scan succeeds");
        assert_eq!(state.first_failure_close_time_ms, Some(1));
        assert_eq!(state.decision_time_ms, None);
    }

    #[test]
    fn frozen_stop_before_second_close_ends_the_l1_position() {
        let candles = [
            candle(0, 100.0, 111.0, 99.0, 109.0),
            candle(1, 109.0, 109.0, 98.0, 99.0),
            candle(2, 99.0, 100.0, 89.0, 98.0),
        ];
        let state = scan_decision(&candles, long_input()).expect("scan succeeds");
        assert_eq!(state.first_failure_close_time_ms, Some(1));
        assert_eq!(state.decision_time_ms, None);
    }

    #[test]
    fn short_mirror_requires_two_closes_above_the_frozen_lower() {
        let input = DecisionScanInput {
            direction: Direction::Short,
            entry_time_ms: 0,
            entry_price: 100.0,
            initial_stop: 110.0,
            frozen_target: 70.0,
            frozen_range_upper: 110.0,
            frozen_range_lower: 100.0,
            frozen_range_height: 10.0,
            tick_size: 0.1,
        };
        let candles = [
            candle(0, 100.0, 101.0, 89.0, 91.0),
            candle(1, 91.0, 102.0, 90.0, 101.0),
            candle(2, 101.0, 103.0, 99.0, 102.0),
        ];
        let state = scan_decision(&candles, input).expect("scan succeeds");
        assert_eq!(state.armed_time_ms, Some(0));
        assert_eq!(state.first_failure_close_time_ms, Some(1));
        assert_eq!(state.decision_time_ms, Some(2));
    }

    #[test]
    fn one_height_waits_for_the_farther_cost_floor() {
        let input = DecisionScanInput {
            frozen_range_height: 0.01,
            ..long_input()
        };
        let candles = [candle(0, 100.0, 100.1, 99.0, 100.0)];
        let state = scan_decision(&candles, input).expect("scan succeeds");
        assert_eq!(state.armed_time_ms, None);
    }
}
