use super::binance_klines::{BinanceCandle, BinanceKlineAudit};
use super::{
    load_binance_klines, load_okx_candles, CrossExchangeBasisPanelArgs, HistoricalUniverseManifest,
    UniverseSchedule, DAY_MS, MS_15M,
};
use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use rust_quant_strategies::CandleItem;
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const RULE_VERSION: &str = "okx15m_binance15m_taker_delta_divergence_one_shot_v1";
const TREND_NET_BARS: usize = 192;
const TREND_REGRESSION_BARS: usize = 96;
const TREND_NET_MOVE_PCT: f64 = 8.0;
const TREND_MIN_R_SQUARED: f64 = 0.60;
const SETUP_AVERAGE_BARS: usize = 20;
const MIN_VOLUME_RATIO: f64 = 2.0;
const MIN_RANGE_RATIO: f64 = 1.4;
const MIN_BODY_RATIO: f64 = 0.20;
const LONG_MIN_TAKER_BUY_SHARE: f64 = 0.60;
const SHORT_MAX_TAKER_BUY_SHARE: f64 = 0.40;
const RESET_CONFIRM_BARS: usize = 8;
const ATR_BARS: usize = 14;
const STOP_PCT: f64 = 0.03;
const TARGET_SCALE: f64 = 4.0;
const MIN_TARGET_R: f64 = 1.8;
const MAX_TARGET_R: f64 = 3.0;
const MAX_HOLDING_BARS: usize = 48 * 4;
const COST_RATE_PER_SIDE: f64 = 0.0008;
const EVENT_CLUSTER_MS: i64 = 30 * 60 * 1_000;

/// 记录 15m Taker Delta 背离从趋势背景到完整 outcome 的因果漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TakerDeltaStages {
    pub armed_episodes: usize,
    pub neutral_resets: usize,
    pub trend_context_bars: usize,
    pub extreme_price_setups: usize,
    pub synchronized_flow_setups: usize,
    pub divergence_setups_before_dedup: usize,
    pub emitted_setups: usize,
    pub overlap_blocked: usize,
    pub risk_blocked: usize,
    pub incomplete_outcomes: usize,
}

/// V1 的方向只允许明确做多或做空，不接受运行时 both 推断。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TakerDeltaDirection {
    Long,
    Short,
}

/// 单笔交易保留 setup 时可见的价格与主动成交证据。
#[derive(Debug, Clone, PartialEq)]
pub struct TakerDeltaTrade {
    pub symbol: String,
    pub direction: TakerDeltaDirection,
    pub setup_ts: i64,
    pub decision_ts: i64,
    pub entry_ts: i64,
    pub exit_ts: i64,
    pub volume_ratio: f64,
    pub range_ratio: f64,
    pub taker_buy_share: f64,
    pub entry: f64,
    pub stop: f64,
    pub target: f64,
    pub target_r: f64,
    pub gross_r: f64,
    pub cost_r: f64,
    pub net_r: f64,
    pub exit_reason: &'static str,
}

/// 固定初始风险 R 的交易级统计，不冒充统一资金组合权益。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TakerDeltaMetrics {
    pub trades: usize,
    pub net_sum_r: f64,
    pub net_expectancy_r: Option<f64>,
    pub profit_factor: Option<f64>,
    pub win_rate_pct: Option<f64>,
    pub trade_sharpe: Option<f64>,
    pub max_drawdown_r: f64,
    pub recovery_factor: Option<f64>,
}

/// 单个冻结年度窗口的完整覆盖、稳定性、成本与集中度结果。
#[derive(Debug, Clone, PartialEq)]
pub struct TakerDeltaResearchReport {
    pub rule_version: String,
    pub universe_version: String,
    pub symbols: usize,
    pub binance_audit: BinanceKlineAudit,
    pub stages: TakerDeltaStages,
    pub trades: Vec<TakerDeltaTrade>,
    pub effective_events: usize,
    pub gross_zero_cost: TakerDeltaMetrics,
    pub overall: TakerDeltaMetrics,
    pub first_half: TakerDeltaMetrics,
    pub second_half: TakerDeltaMetrics,
    pub long: TakerDeltaMetrics,
    pub short: TakerDeltaMetrics,
    pub double_cost: TakerDeltaMetrics,
    pub quartiles: Vec<TakerDeltaMetrics>,
    pub monthly: Vec<(i64, TakerDeltaMetrics)>,
    pub positive_months: usize,
    pub top_three_positive_symbols: Vec<String>,
    pub net_r_without_top_three_symbols: f64,
    pub exit_reasons: BTreeMap<String, usize>,
    pub minimum_gate_passed: bool,
    pub professional_trade_gate_passed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// 当前 setup 之前的互斥价格趋势背景。
enum TrendContext {
    Neutral,
    PriorUp,
    PriorDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// 一次性趋势状态在 armed、consumed 与等待重置之间的阶段。
enum Lifecycle {
    Neutral,
    Armed(TrendContext),
    Consumed(TrendContext),
    AwaitNeutral,
}

/// 单币独享的趋势消费状态，禁止跨币或跨回放共享。
struct OneShotState {
    lifecycle: Lifecycle,
    neutral_streak: usize,
}

#[derive(Debug, Clone, Copy)]
/// 192 根净变化窗口所需的最小开收盘样本。
struct HistoricalPoint {
    open: f64,
    close: f64,
}

/// 固定 96 根窗口的常数时间线性回归统计。
struct RollingRegression {
    closes: VecDeque<f64>,
    sum_y: f64,
    sum_y2: f64,
    sum_xy: f64,
}

/// 同时维护 192 根净变化与 96 根回归趋势。
struct TrendTracker {
    history: VecDeque<HistoricalPoint>,
    regression: RollingRegression,
}

#[derive(Debug, Clone, Copy)]
/// setup 时点已完成且可审计的价格与主动量证据。
struct SetupEvidence {
    direction: TakerDeltaDirection,
    volume_ratio: f64,
    range_ratio: f64,
    taker_buy_share: f64,
}

/// 区分已成交、风险计算失败与未来路径不完整。
enum Settlement {
    Trade(TakerDeltaTrade, i64),
    RiskBlocked,
    Incomplete,
}

impl OneShotState {
    /// 创建尚未武装的单币趋势状态。
    fn new() -> Self {
        Self {
            lifecycle: Lifecycle::Neutral,
            neutral_streak: 0,
        }
    }

    /// 直接换向不能重新 armed；必须连续八根中性棒完成后才解除冷却。
    fn observe(&mut self, context: TrendContext) -> (bool, bool) {
        let previous = self.lifecycle;
        let mut reset = false;
        self.lifecycle = match (previous, context) {
            (Lifecycle::Neutral, TrendContext::Neutral) => {
                self.neutral_streak = 0;
                Lifecycle::Neutral
            }
            (Lifecycle::Neutral, directional) => {
                self.neutral_streak = 0;
                Lifecycle::Armed(directional)
            }
            (Lifecycle::Armed(current), next) | (Lifecycle::Consumed(current), next)
                if current == next =>
            {
                self.neutral_streak = 0;
                previous
            }
            (Lifecycle::Armed(_), TrendContext::Neutral)
            | (Lifecycle::Consumed(_), TrendContext::Neutral)
            | (Lifecycle::AwaitNeutral, TrendContext::Neutral) => {
                self.neutral_streak = self.neutral_streak.saturating_add(1);
                if self.neutral_streak >= RESET_CONFIRM_BARS {
                    self.neutral_streak = 0;
                    reset = true;
                    Lifecycle::Neutral
                } else {
                    previous
                }
            }
            (Lifecycle::Armed(_), _)
            | (Lifecycle::Consumed(_), _)
            | (Lifecycle::AwaitNeutral, _) => {
                self.neutral_streak = 0;
                Lifecycle::AwaitNeutral
            }
        };
        (
            matches!(
                (previous, self.lifecycle),
                (Lifecycle::Neutral, Lifecycle::Armed(_))
            ),
            reset,
        )
    }

    /// 只有当前 armed 方向与 setup 背景一致时才能消费一次。
    fn consume(&mut self, context: TrendContext) -> bool {
        if self.lifecycle == Lifecycle::Armed(context) {
            self.lifecycle = Lifecycle::Consumed(context);
            true
        } else {
            false
        }
    }
}

impl RollingRegression {
    /// 创建空的 96 根回归窗口。
    fn new() -> Self {
        Self {
            closes: VecDeque::with_capacity(TREND_REGRESSION_BARS + 1),
            sum_y: 0.0,
            sum_y2: 0.0,
            sum_xy: 0.0,
        }
    }

    /// 数据出现 15m 缺口时清空窗口，禁止跨缺口拼接趋势。
    fn clear(&mut self) {
        self.closes.clear();
        self.sum_y = 0.0;
        self.sum_y2 = 0.0;
        self.sum_xy = 0.0;
    }

    /// 推入一根已完成收盘价并常数时间移除最早样本。
    fn push(&mut self, close: f64) {
        if self.closes.len() == TREND_REGRESSION_BARS {
            let previous_sum_y = self.sum_y;
            let removed = self.closes.pop_front().unwrap_or(0.0);
            self.sum_y -= removed;
            self.sum_y2 -= removed * removed;
            self.sum_xy -= previous_sum_y - removed;
        }
        let x = self.closes.len() as f64;
        self.closes.push_back(close);
        self.sum_y += close;
        self.sum_y2 += close * close;
        self.sum_xy += x * close;
    }

    /// 仅在斜率、首尾方向和 R² 同时成立时返回趋势方向。
    fn direction(&self) -> Option<std::cmp::Ordering> {
        if self.closes.len() != TREND_REGRESSION_BARS {
            return None;
        }
        let n = TREND_REGRESSION_BARS as f64;
        let sum_x = n * (n - 1.0) / 2.0;
        let sum_x2 = n * (n - 1.0) * (2.0 * n - 1.0) / 6.0;
        let covariance = self.sum_xy - sum_x * self.sum_y / n;
        let variance_x = sum_x2 - sum_x * sum_x / n;
        let variance_y = self.sum_y2 - self.sum_y * self.sum_y / n;
        if variance_x <= 0.0 || variance_y <= 0.0 {
            return None;
        }
        let r_squared = covariance * covariance / (variance_x * variance_y);
        if !r_squared.is_finite() || r_squared < TREND_MIN_R_SQUARED {
            return None;
        }
        let first = *self.closes.front()?;
        let last = *self.closes.back()?;
        if covariance > 0.0 && last > first {
            Some(std::cmp::Ordering::Greater)
        } else if covariance < 0.0 && last < first {
            Some(std::cmp::Ordering::Less)
        } else {
            None
        }
    }
}

impl TrendTracker {
    /// 创建空的净变化与回归双窗口。
    fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(TREND_NET_BARS + 1),
            regression: RollingRegression::new(),
        }
    }

    /// 同步清空两个趋势窗口，避免价格缺口污染状态。
    fn clear(&mut self) {
        self.history.clear();
        self.regression.clear();
    }

    /// 只读取当前 setup 之前已经完成的历史，避免同棒趋势泄漏。
    fn context(&self) -> TrendContext {
        let mut prior_up = false;
        let mut prior_down = false;
        if self.history.len() == TREND_NET_BARS {
            if let (Some(first), Some(last)) = (self.history.front(), self.history.back()) {
                if first.open > 0.0 {
                    let move_pct = (last.close - first.open) / first.open * 100.0;
                    prior_up |= move_pct >= TREND_NET_MOVE_PCT;
                    prior_down |= move_pct <= -TREND_NET_MOVE_PCT;
                }
            }
        }
        match self.regression.direction() {
            Some(std::cmp::Ordering::Greater) => prior_up = true,
            Some(std::cmp::Ordering::Less) => prior_down = true,
            _ => {}
        }
        match (prior_up, prior_down) {
            (true, false) => TrendContext::PriorUp,
            (false, true) => TrendContext::PriorDown,
            _ => TrendContext::Neutral,
        }
    }

    /// 当前 setup 判断完成后才推入该棒，维持严格因果顺序。
    fn push(&mut self, candle: &CandleItem) {
        if self.history.len() == TREND_NET_BARS {
            self.history.pop_front();
        }
        self.history.push_back(HistoricalPoint {
            open: candle.o,
            close: candle.c,
        });
        self.regression.push(candle.c);
    }
}

/// 运行一个预冻结年度窗口；调用两次即可完成开发窗口与独立旧窗口检查。
pub async fn run_taker_delta_reversal_research(
    args: &CrossExchangeBasisPanelArgs,
    database_url: &str,
) -> Result<TakerDeltaResearchReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode taker-delta universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let first = schedule
        .windows
        .first()
        .context("missing first universe window")?;
    let last = schedule
        .windows
        .last()
        .context("missing last universe window")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for taker-delta reversal research")?;
    let mut okx = BTreeMap::<String, Vec<CandleItem>>::new();
    for symbol in schedule.union_symbols() {
        okx.insert(
            symbol.clone(),
            load_okx_candles(
                &pool,
                &symbol,
                first.from_ms.saturating_sub(8 * DAY_MS),
                last.to_ms.saturating_add(3 * DAY_MS),
            )
            .await?,
        );
    }
    let (binance, binance_audit) = load_binance_klines(args, &schedule).await?;
    let (mut trades, stages) = scan_all_symbols(&schedule, &okx, &binance);
    trades.sort_by(|left, right| {
        left.entry_ts
            .cmp(&right.entry_ts)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    let split_ms = schedule.windows[6].from_ms;
    let first_half = subset(&trades, |trade| trade.entry_ts < split_ms);
    let second_half = subset(&trades, |trade| trade.entry_ts >= split_ms);
    let long = subset(&trades, |trade| {
        trade.direction == TakerDeltaDirection::Long
    });
    let short = subset(&trades, |trade| {
        trade.direction == TakerDeltaDirection::Short
    });
    let quartiles = (0..4)
        .map(|index| {
            let start = schedule.windows[index * 3].from_ms;
            let end = schedule.windows[(index + 1) * 3 - 1].to_ms;
            metrics(
                &subset(&trades, |trade| {
                    trade.entry_ts >= start && trade.entry_ts < end
                }),
                1.0,
            )
        })
        .collect::<Vec<_>>();
    let monthly = schedule
        .windows
        .iter()
        .map(|window| {
            (
                window.from_ms,
                metrics(
                    &subset(&trades, |trade| {
                        trade.entry_ts >= window.from_ms && trade.entry_ts < window.to_ms
                    }),
                    1.0,
                ),
            )
        })
        .collect::<Vec<_>>();
    let positive_months = monthly
        .iter()
        .filter(|(_, value)| value.net_sum_r > 0.0)
        .count();
    let (top_three_positive_symbols, net_r_without_top_three_symbols) =
        concentration_without_top_three(&trades);
    let mut exit_reasons = BTreeMap::new();
    for trade in &trades {
        *exit_reasons
            .entry(trade.exit_reason.to_owned())
            .or_default() += 1;
    }
    let overall = metrics(&trades, 1.0);
    let first_half_metrics = metrics(&first_half, 1.0);
    let second_half_metrics = metrics(&second_half, 1.0);
    let minimum_gate_passed = positive_edge(&overall)
        && positive_edge(&first_half_metrics)
        && positive_edge(&second_half_metrics);
    let professional_trade_gate_passed = overall.net_expectancy_r.is_some_and(|value| value >= 0.6)
        && overall.profit_factor.is_some_and(|value| value >= 2.2)
        && overall.trade_sharpe.is_some_and(|value| value >= 1.5);
    let report = TakerDeltaResearchReport {
        rule_version: RULE_VERSION.to_owned(),
        universe_version: schedule.version.clone(),
        symbols: okx.len(),
        binance_audit,
        stages,
        effective_events: effective_event_count(&trades),
        gross_zero_cost: metrics(&trades, 0.0),
        overall,
        first_half: first_half_metrics,
        second_half: second_half_metrics,
        long: metrics(&long, 1.0),
        short: metrics(&short, 1.0),
        double_cost: metrics(&trades, 2.0),
        quartiles,
        monthly,
        positive_months,
        top_three_positive_symbols,
        net_r_without_top_three_symbols,
        exit_reasons,
        minimum_gate_passed,
        professional_trade_gate_passed,
        trades,
    };
    print_report(&report);
    Ok(report)
}

/// 逐币回放固定币池并汇总全市场漏斗。
fn scan_all_symbols(
    schedule: &UniverseSchedule,
    okx: &BTreeMap<String, Vec<CandleItem>>,
    binance: &BTreeMap<String, Vec<BinanceCandle>>,
) -> (Vec<TakerDeltaTrade>, TakerDeltaStages) {
    let mut all_trades = Vec::new();
    let mut stages = TakerDeltaStages::default();
    for symbol in schedule.union_symbols() {
        let (Some(candles), Some(flow)) = (okx.get(&symbol), binance.get(&symbol)) else {
            continue;
        };
        all_trades.extend(scan_symbol(&symbol, candles, flow, schedule, &mut stages));
    }
    (all_trades, stages)
}

/// 单币按时间推进趋势、一次性状态、flow gate 和非重叠交易。
fn scan_symbol(
    symbol: &str,
    candles: &[CandleItem],
    flow: &[BinanceCandle],
    schedule: &UniverseSchedule,
    stages: &mut TakerDeltaStages,
) -> Vec<TakerDeltaTrade> {
    let mut trend = TrendTracker::new();
    let mut state = OneShotState::new();
    let mut trades = Vec::new();
    let mut locked_until = i64::MIN;
    let mut previous_ts = None::<i64>;
    for index in 0..candles.len() {
        let candle = &candles[index];
        if previous_ts.is_some_and(|ts| ts.saturating_add(MS_15M) != candle.ts) {
            trend.clear();
            state = OneShotState::new();
        }
        previous_ts = Some(candle.ts);
        let decision_ts = candle.ts.saturating_add(MS_15M);
        let context = trend.context();
        let in_window = schedule
            .window_at(decision_ts)
            .is_some_and(|window| window.members.contains(symbol));
        let (armed, reset) = state.observe(context);
        if in_window {
            stages.armed_episodes += usize::from(armed);
            stages.neutral_resets += usize::from(reset);
            stages.trend_context_bars += usize::from(context != TrendContext::Neutral);
        }
        let setup = setup_evidence(candles, index, flow, context, stages, in_window);
        trend.push(candle);
        let Some(evidence) = setup else {
            continue;
        };
        if in_window {
            stages.divergence_setups_before_dedup += 1;
        }
        // 窗口外的已知 setup 也消费状态，禁止边界处伪造重新 armed。
        if !state.consume(context) || !in_window {
            continue;
        }
        stages.emitted_setups += 1;
        if decision_ts <= locked_until {
            stages.overlap_blocked += 1;
            continue;
        }
        match settle_trade(symbol, candles, index, evidence) {
            Settlement::Trade(trade, exit_ts) => {
                locked_until = exit_ts;
                trades.push(trade);
            }
            Settlement::RiskBlocked => stages.risk_blocked += 1,
            Settlement::Incomplete => stages.incomplete_outcomes += 1,
        }
    }
    trades
}

/// 检查趋势同向极端量与精确同步的 Taker Delta 背离。
fn setup_evidence(
    candles: &[CandleItem],
    index: usize,
    flow: &[BinanceCandle],
    context: TrendContext,
    stages: &mut TakerDeltaStages,
    count_stages: bool,
) -> Option<SetupEvidence> {
    if context == TrendContext::Neutral || index < SETUP_AVERAGE_BARS {
        return None;
    }
    let candle = &candles[index];
    let range = candle.h - candle.l;
    let body = (candle.c - candle.o).abs();
    let average_volume = candles[index - SETUP_AVERAGE_BARS..index]
        .iter()
        .map(|item| item.v)
        .sum::<f64>()
        / SETUP_AVERAGE_BARS as f64;
    let average_range = candles[index - SETUP_AVERAGE_BARS..index]
        .iter()
        .map(|item| item.h - item.l)
        .sum::<f64>()
        / SETUP_AVERAGE_BARS as f64;
    if range <= 0.0 || average_volume <= 0.0 || average_range <= 0.0 {
        return None;
    }
    let volume_ratio = candle.v / average_volume;
    let range_ratio = range / average_range;
    let price_direction_matches = match context {
        TrendContext::PriorDown => candle.c < candle.o,
        TrendContext::PriorUp => candle.c > candle.o,
        TrendContext::Neutral => false,
    };
    if !price_direction_matches
        || body / range < MIN_BODY_RATIO
        || volume_ratio < MIN_VOLUME_RATIO
        || range_ratio < MIN_RANGE_RATIO
    {
        return None;
    }
    if count_stages {
        stages.extreme_price_setups += 1;
    }
    let flow_bar = exact_flow_bar(flow, candle.ts)?;
    if flow_bar.quote_volume <= 0.0 {
        return None;
    }
    let taker_buy_share = flow_bar.taker_buy_quote_volume / flow_bar.quote_volume;
    if !taker_buy_share.is_finite() || !(0.0..=1.0).contains(&taker_buy_share) {
        return None;
    }
    if count_stages {
        stages.synchronized_flow_setups += 1;
    }
    let direction = match context {
        TrendContext::PriorDown if taker_buy_share >= LONG_MIN_TAKER_BUY_SHARE => {
            TakerDeltaDirection::Long
        }
        TrendContext::PriorUp if taker_buy_share <= SHORT_MAX_TAKER_BUY_SHARE => {
            TakerDeltaDirection::Short
        }
        _ => return None,
    };
    Some(SetupEvidence {
        direction,
        volume_ratio,
        range_ratio,
        taker_buy_share,
    })
}

/// 只接受完全相同开盘时间的 Binance 原生 15m 棒。
fn exact_flow_bar(flow: &[BinanceCandle], ts: i64) -> Option<&BinanceCandle> {
    flow.binary_search_by_key(&ts, |candle| candle.ts)
        .ok()
        .and_then(|index| flow.get(index))
}

/// 下一根 OKX 开盘入场，并以同棒双触发止损优先的保守规则回放。
fn settle_trade(
    symbol: &str,
    candles: &[CandleItem],
    setup_index: usize,
    evidence: SetupEvidence,
) -> Settlement {
    let Some(entry_index) = setup_index.checked_add(1) else {
        return Settlement::Incomplete;
    };
    let Some(entry_candle) = candles.get(entry_index) else {
        return Settlement::Incomplete;
    };
    if entry_candle.ts != candles[setup_index].ts.saturating_add(MS_15M) {
        return Settlement::Incomplete;
    }
    let Some(atr) = atr_at(candles, setup_index) else {
        return Settlement::RiskBlocked;
    };
    let entry = entry_candle.o;
    let risk = entry * STOP_PCT;
    if !entry.is_finite() || !risk.is_finite() || entry <= 0.0 || risk <= 0.0 {
        return Settlement::RiskBlocked;
    }
    let atr_multiplier = if evidence.volume_ratio >= 3.0 {
        3.0
    } else {
        2.0
    };
    let target_r = (atr * atr_multiplier / risk * TARGET_SCALE).clamp(MIN_TARGET_R, MAX_TARGET_R);
    if !target_r.is_finite() {
        return Settlement::RiskBlocked;
    }
    let (stop, target) = match evidence.direction {
        TakerDeltaDirection::Long => (entry - risk, entry + risk * target_r),
        TakerDeltaDirection::Short => (entry + risk, entry - risk * target_r),
    };
    let Some(last_index) = entry_index.checked_add(MAX_HOLDING_BARS - 1) else {
        return Settlement::Incomplete;
    };
    if last_index >= candles.len() {
        return Settlement::Incomplete;
    }
    let mut previous = entry_candle.ts.saturating_sub(MS_15M);
    for index in entry_index..=last_index {
        let candle = &candles[index];
        if candle.ts != previous.saturating_add(MS_15M) {
            return Settlement::Incomplete;
        }
        previous = candle.ts;
        let hit_stop = match evidence.direction {
            TakerDeltaDirection::Long => candle.l <= stop,
            TakerDeltaDirection::Short => candle.h >= stop,
        };
        let hit_target = match evidence.direction {
            TakerDeltaDirection::Long => candle.h >= target,
            TakerDeltaDirection::Short => candle.l <= target,
        };
        if hit_stop || hit_target {
            let (exit, gross_r, reason) = if hit_stop {
                (stop, -1.0, "stop_first")
            } else {
                (target, target_r, "target")
            };
            return Settlement::Trade(
                build_trade(
                    symbol,
                    candles,
                    setup_index,
                    index,
                    evidence,
                    entry,
                    stop,
                    target,
                    target_r,
                    exit,
                    gross_r,
                    reason,
                ),
                candle.ts.saturating_add(MS_15M),
            );
        }
    }
    let exit_candle = &candles[last_index];
    let exit = exit_candle.c;
    let gross_r = match evidence.direction {
        TakerDeltaDirection::Long => (exit - entry) / risk,
        TakerDeltaDirection::Short => (entry - exit) / risk,
    };
    Settlement::Trade(
        build_trade(
            symbol,
            candles,
            setup_index,
            last_index,
            evidence,
            entry,
            stop,
            target,
            target_r,
            exit,
            gross_r,
            "max_holding_timeout",
        ),
        exit_candle.ts.saturating_add(MS_15M),
    )
}

/// 把固定结算价格换算为含双边成本的可审计交易记录。
#[allow(clippy::too_many_arguments)]
fn build_trade(
    symbol: &str,
    candles: &[CandleItem],
    setup_index: usize,
    exit_index: usize,
    evidence: SetupEvidence,
    entry: f64,
    stop: f64,
    target: f64,
    target_r: f64,
    exit: f64,
    gross_r: f64,
    exit_reason: &'static str,
) -> TakerDeltaTrade {
    let cost_r = (entry + exit) * COST_RATE_PER_SIDE / (entry * STOP_PCT);
    TakerDeltaTrade {
        symbol: symbol.to_owned(),
        direction: evidence.direction,
        setup_ts: candles[setup_index].ts,
        decision_ts: candles[setup_index].ts.saturating_add(MS_15M),
        entry_ts: candles[setup_index + 1].ts,
        exit_ts: candles[exit_index].ts.saturating_add(MS_15M),
        volume_ratio: evidence.volume_ratio,
        range_ratio: evidence.range_ratio,
        taker_buy_share: evidence.taker_buy_share,
        entry,
        stop,
        target,
        target_r,
        gross_r,
        cost_r,
        net_r: gross_r - cost_r,
        exit_reason,
    }
}

/// 计算截至 setup 的 14 根真实波幅均值，不读取入场后数据。
fn atr_at(candles: &[CandleItem], index: usize) -> Option<f64> {
    if index + 1 < ATR_BARS {
        return None;
    }
    let start = index + 1 - ATR_BARS;
    let mut total = 0.0;
    for current in start..=index {
        let candle = &candles[current];
        let previous_close = current
            .checked_sub(1)
            .map(|previous| candles[previous].c)
            .unwrap_or(candle.c);
        total += (candle.h - candle.l)
            .max((candle.h - previous_close).abs())
            .max((candle.l - previous_close).abs());
    }
    let atr = total / ATR_BARS as f64;
    (atr.is_finite() && atr > 0.0).then_some(atr)
}

/// 按预定义时间或方向条件复制一组交易用于分段统计。
fn subset<F>(trades: &[TakerDeltaTrade], mut predicate: F) -> Vec<TakerDeltaTrade>
where
    F: FnMut(&TakerDeltaTrade) -> bool,
{
    trades
        .iter()
        .filter(|trade| predicate(trade))
        .cloned()
        .collect()
}

/// 按给定成本倍数汇总固定风险 R 指标。
fn metrics(trades: &[TakerDeltaTrade], cost_multiplier: f64) -> TakerDeltaMetrics {
    if trades.is_empty() {
        return TakerDeltaMetrics::default();
    }
    let values = trades
        .iter()
        .map(|trade| trade.gross_r - trade.cost_r * cost_multiplier)
        .collect::<Vec<_>>();
    let net_sum_r = values.iter().sum::<f64>();
    let profit = values.iter().filter(|value| **value > 0.0).sum::<f64>();
    let loss = values
        .iter()
        .filter(|value| **value < 0.0)
        .map(|value| value.abs())
        .sum::<f64>();
    let mean = net_sum_r / values.len() as f64;
    let variance = if values.len() > 1 {
        values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (values.len() - 1) as f64
    } else {
        0.0
    };
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for value in &values {
        equity += value;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    TakerDeltaMetrics {
        trades: values.len(),
        net_sum_r,
        net_expectancy_r: Some(mean),
        profit_factor: (loss > 0.0).then_some(profit / loss),
        win_rate_pct: Some(
            values.iter().filter(|value| **value > 0.0).count() as f64 / values.len() as f64
                * 100.0,
        ),
        trade_sharpe: (variance > 0.0)
            .then_some(mean / variance.sqrt() * (values.len() as f64).sqrt()),
        max_drawdown_r: max_drawdown,
        recovery_factor: (max_drawdown > 0.0).then_some(net_sum_r / max_drawdown),
    }
}

/// 最低门禁要求成本后 EV 与 PF 同时为正向。
fn positive_edge(value: &TakerDeltaMetrics) -> bool {
    value.net_expectancy_r.is_some_and(|metric| metric > 0.0)
        && value.profit_factor.is_some_and(|metric| metric > 1.0)
}

/// 同方向 30 分钟内的同步触发归并为一个有效市场事件。
fn effective_event_count(trades: &[TakerDeltaTrade]) -> usize {
    let mut latest = BTreeMap::<TakerDeltaDirection, i64>::new();
    let mut count = 0usize;
    for trade in trades {
        if latest
            .get(&trade.direction)
            .is_none_or(|point| trade.entry_ts.saturating_sub(*point) > EVENT_CLUSTER_MS)
        {
            count += 1;
        }
        latest.insert(trade.direction, trade.entry_ts);
    }
    count
}

/// 移除净贡献最高三个盈利币种后重新计算总 R。
fn concentration_without_top_three(trades: &[TakerDeltaTrade]) -> (Vec<String>, f64) {
    let mut by_symbol = BTreeMap::<String, f64>::new();
    for trade in trades {
        *by_symbol.entry(trade.symbol.clone()).or_default() += trade.net_r;
    }
    let mut positive = by_symbol
        .into_iter()
        .filter(|(_, value)| *value > 0.0)
        .collect::<Vec<_>>();
    positive.sort_by(|left, right| right.1.total_cmp(&left.1));
    let top = positive
        .iter()
        .take(3)
        .map(|(symbol, _)| symbol.clone())
        .collect::<BTreeSet<_>>();
    let remaining = trades
        .iter()
        .filter(|trade| !top.contains(&trade.symbol))
        .map(|trade| trade.net_r)
        .sum();
    (top.into_iter().collect(), remaining)
}

/// 以稳定制表符格式打印年度覆盖、分段与集中度结果。
fn print_report(report: &TakerDeltaResearchReport) {
    println!(
        "taker_delta_reversal\trule={}\tuniverse={}\tsymbols={}\teffective_events={}\tminimum_gate={}\tprofessional_trade_gate={}",
        report.rule_version,
        report.universe_version,
        report.symbols,
        report.effective_events,
        report.minimum_gate_passed,
        report.professional_trade_gate_passed,
    );
    println!(
        "binance_audit\tmapped={}\tblocked={}\trequested={}\tavailable={}\tmissing={}\tinvalid={}\trows={}",
        report.binance_audit.mapped_symbols,
        report.binance_audit.mapping_blocked_symbols,
        report.binance_audit.requested_files,
        report.binance_audit.available_files,
        report.binance_audit.missing_files,
        report.binance_audit.invalid_files,
        report.binance_audit.parsed_rows,
    );
    println!(
        "stages\tarmed={}\tresets={}\ttrend_bars={}\textreme_price={}\tsynchronized_flow={}\tdivergence_before_dedup={}\temitted={}\toverlap_blocked={}\trisk_blocked={}\tincomplete={}",
        report.stages.armed_episodes,
        report.stages.neutral_resets,
        report.stages.trend_context_bars,
        report.stages.extreme_price_setups,
        report.stages.synchronized_flow_setups,
        report.stages.divergence_setups_before_dedup,
        report.stages.emitted_setups,
        report.stages.overlap_blocked,
        report.stages.risk_blocked,
        report.stages.incomplete_outcomes,
    );
    print_metrics("gross_zero_cost", &report.gross_zero_cost);
    print_metrics("overall", &report.overall);
    print_metrics("first_half", &report.first_half);
    print_metrics("second_half", &report.second_half);
    print_metrics("long", &report.long);
    print_metrics("short", &report.short);
    print_metrics("double_cost", &report.double_cost);
    for (index, value) in report.quartiles.iter().enumerate() {
        print_metrics(&format!("q{}", index + 1), value);
    }
    for (month, value) in &report.monthly {
        let label = Utc
            .timestamp_millis_opt(*month)
            .single()
            .map(|date| date.format("%Y-%m").to_string())
            .unwrap_or_else(|| month.to_string());
        print_metrics(&format!("month_{label}"), value);
    }
    println!(
        "concentration\tpositive_months={}\ttop_three={}\tnet_r_without_top_three={:.6}\texit_reasons={:?}",
        report.positive_months,
        report.top_three_positive_symbols.join(","),
        report.net_r_without_top_three_symbols,
        report.exit_reasons,
    );
}

/// 打印一个预注册分段的核心交易级指标。
fn print_metrics(label: &str, value: &TakerDeltaMetrics) {
    println!(
        "metrics\tsegment={}\ttrades={}\tnet_sum_r={:.6}\tev={}\tpf={}\twin_pct={}\tsharpe={}\tmax_dd_r={:.6}\trecovery={}",
        label,
        value.trades,
        value.net_sum_r,
        optional(value.net_expectancy_r),
        optional(value.profit_factor),
        optional(value.win_rate_pct),
        optional(value.trade_sharpe),
        value.max_drawdown_r,
        optional(value.recovery_factor),
    );
}

/// 稳定格式化可能因无交易或无亏损而缺失的指标。
fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_owned(), |number| format!("{number:.6}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造用于状态与结算测试的已完成 15m K 线。
    fn candle(ts: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> CandleItem {
        CandleItem {
            ts,
            o: open,
            h: high,
            l: low,
            c: close,
            v: volume,
            confirm: 1,
        }
    }

    #[test]
    /// 已消费状态必须完成八根连续中性棒才能再次武装。
    fn one_shot_state_requires_eight_neutral_bars_before_rearming() {
        let mut state = OneShotState::new();
        state.observe(TrendContext::PriorDown);
        assert!(state.consume(TrendContext::PriorDown));
        for _ in 0..7 {
            assert!(!state.observe(TrendContext::Neutral).1);
        }
        assert!(state.observe(TrendContext::Neutral).1);
        assert!(state.observe(TrendContext::PriorDown).0);
        assert!(state.consume(TrendContext::PriorDown));
    }

    #[test]
    /// flow 数据禁止使用相邻或未来 15m 棒填补缺口。
    fn flow_lookup_requires_exact_native_15m_timestamp() {
        let flow = vec![BinanceCandle {
            ts: MS_15M,
            open: 100.0,
            close: 99.0,
            quote_volume: 1_000.0,
            taker_buy_quote_volume: 650.0,
        }];
        assert_eq!(exact_flow_bar(&flow, MS_15M).unwrap().quote_volume, 1_000.0);
        assert!(exact_flow_bar(&flow, MS_15M + 1).is_none());
    }

    #[test]
    /// 同一棒同时穿越止盈止损时按保守止损先发生处理。
    fn same_bar_stop_and_target_uses_stop_first() {
        let mut candles = (0..MAX_HOLDING_BARS + ATR_BARS + 1)
            .map(|index| candle(index as i64 * MS_15M, 100.0, 100.5, 99.5, 100.0, 10.0))
            .collect::<Vec<_>>();
        let setup_index = ATR_BARS - 1;
        let entry_index = setup_index + 1;
        candles[entry_index] = candle(entry_index as i64 * MS_15M, 100.0, 120.0, 80.0, 100.0, 10.0);
        let settled = settle_trade(
            "BTC-USDT-SWAP",
            &candles,
            setup_index,
            SetupEvidence {
                direction: TakerDeltaDirection::Long,
                volume_ratio: 2.0,
                range_ratio: 1.4,
                taker_buy_share: 0.6,
            },
        );
        assert!(matches!(
            settled,
            Settlement::Trade(TakerDeltaTrade { gross_r, .. }, _) if gross_r == -1.0
        ));
    }

    #[test]
    /// 统计口径必须从毛 R 中扣除开平双边手续费与滑点。
    fn metrics_charge_round_trip_cost_in_r() {
        let trade = TakerDeltaTrade {
            symbol: "BTC-USDT-SWAP".to_owned(),
            direction: TakerDeltaDirection::Long,
            setup_ts: 0,
            decision_ts: MS_15M,
            entry_ts: MS_15M,
            exit_ts: 2 * MS_15M,
            volume_ratio: 2.0,
            range_ratio: 1.4,
            taker_buy_share: 0.6,
            entry: 100.0,
            stop: 97.0,
            target: 106.0,
            target_r: 2.0,
            gross_r: 2.0,
            cost_r: 0.05493333333333334,
            net_r: 1.9450666666666666,
            exit_reason: "target",
        };
        assert!((metrics(&[trade], 1.0).net_expectancy_r.unwrap() - 1.9450666667).abs() < 1e-9);
    }
}
