mod context;

use self::context::{
    load_context_states, ContextAudit, ContextStates, OiState, RatioState, TakerState,
};
use crate::app::okx_historical_universe::HistoricalUniverseManifest;
use anyhow::{anyhow, bail, Context, Result};
use rust_quant_strategies::CandleItem;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MS_15M: i64 = 15 * 60 * 1_000;
const MS_30M: i64 = 30 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const HISTORY_BARS: usize = 96;
const ATR_PERIOD: usize = 14;
const PRICE_BOTTOM_RATIO: f64 = 0.20;
const PRICE_COVERAGE_MIN_RATIO: f64 = 0.80;
const SWEEP_CLOSE_POSITION_MIN: f64 = 0.50;
const SWEEP_LOWER_WICK_BODY_MULTIPLIER: f64 = 0.50;
const STOP_ATR_BUFFER: f64 = 0.25;
const MIN_RISK_PCT: f64 = 0.5;
const MAX_RISK_PCT: f64 = 3.0;
const TARGET_R: f64 = 3.0;
const MAX_HOLDING_BARS: usize = 48 * 4;
const COST_RATE_PER_SIDE: f64 = 0.0008;
const DEFAULT_OKX_BASE: &str = "https://www.okx.com";

/// 冻结 V1 只暴露 manifest、上下文缓存和下载并发，不允许从命令行扫描策略阈值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleveragingResearchArgs {
    pub manifest: PathBuf,
    pub context_cache: PathBuf,
    pub download_concurrency: usize,
    pub okx_base: String,
}

/// 严格时序信号漏斗，区分价格横截面、三类去杠杆证据和成交风控。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeleveragingStageCounts {
    pub price_tail_pass: usize,
    pub oi_pass: usize,
    pub taker_pass: usize,
    pub ratio_pass: usize,
    pub sweep_pass: usize,
    pub confirmation_pass: usize,
    pub risk_blocked: usize,
    pub incomplete_outcomes: usize,
}

/// 单笔 15m 交易及其在决策时点可见的去杠杆证据。
#[derive(Debug, Clone, PartialEq)]
pub struct DeleveragingTrade {
    pub symbol: String,
    pub sweep_ts: i64,
    pub decision_ts: i64,
    pub entry_ts: i64,
    pub exit_ts: i64,
    pub oi_change: f64,
    pub taker_sell_share: f64,
    pub long_short_ratio: f64,
    pub long_short_change: f64,
    pub entry: f64,
    pub stop: f64,
    pub target: f64,
    pub gross_r: f64,
    pub cost_r: f64,
    pub net_r: f64,
    pub exit_reason: &'static str,
}

/// 交易级固定 R 指标；统一资金组合审计前不等于可部署权益结果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeleveragingMetrics {
    pub trades: usize,
    pub net_sum_r: f64,
    pub net_expectancy_r: Option<f64>,
    pub profit_factor: Option<f64>,
    pub win_rate_pct: Option<f64>,
    pub trade_sharpe: Option<f64>,
    pub max_drawdown_r: f64,
    pub recovery_factor: Option<f64>,
}

/// 冻结 V1 的完整数据覆盖、信号漏斗、稳定性与集中度报告。
#[derive(Debug, Clone, PartialEq)]
pub struct DeleveragingResearchReport {
    pub universe_version: String,
    pub symbols: usize,
    pub context: ContextAudit,
    pub price_coverage_blocked: usize,
    pub stages: DeleveragingStageCounts,
    pub trades: Vec<DeleveragingTrade>,
    pub effective_events: usize,
    pub gross_zero_cost: DeleveragingMetrics,
    pub overall: DeleveragingMetrics,
    pub discovery: DeleveragingMetrics,
    pub validation: DeleveragingMetrics,
    pub double_cost: DeleveragingMetrics,
    pub monthly: Vec<(i64, DeleveragingMetrics)>,
    pub positive_months: usize,
    pub top_three_positive_symbols: Vec<String>,
    pub net_r_without_top_three_symbols: f64,
    pub exit_reasons: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UniverseWindow {
    pub from_ms: i64,
    pub to_ms: i64,
    pub members: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UniverseSchedule {
    pub version: String,
    pub windows: Vec<UniverseWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct EntryEvidence {
    oi: OiState,
    taker: TakerState,
    ratio: RatioState,
    atr: f64,
}

enum Settlement {
    Trade(DeleveragingTrade, usize),
    RiskBlocked,
    Incomplete,
}

/// 解析只读研究参数；未知参数失败，避免拼写错误改变冻结数据集。
pub fn parse_deleveraging_research_args<I>(values: I) -> Result<DeleveragingResearchArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut manifest = None;
    let mut context_cache = None;
    let mut download_concurrency = 8usize;
    let mut okx_base = DEFAULT_OKX_BASE.to_owned();
    while let Some(arg) = values.next() {
        let value = |values: &mut I::IntoIter| {
            values
                .next()
                .ok_or_else(|| anyhow!("{arg} requires a value"))
        };
        match arg.as_str() {
            "--manifest" => manifest = Some(PathBuf::from(value(&mut values)?)),
            "--context-cache" => context_cache = Some(PathBuf::from(value(&mut values)?)),
            "--download-concurrency" => {
                download_concurrency = value(&mut values)?
                    .parse()
                    .context("parse --download-concurrency")?;
            }
            "--okx-base" => okx_base = value(&mut values)?.trim_end_matches('/').to_owned(),
            "--help" | "-h" => bail!(deleveraging_research_usage()),
            _ => bail!("unknown argument: {arg}\n{}", deleveraging_research_usage()),
        }
    }
    if !(1..=12).contains(&download_concurrency) {
        bail!("--download-concurrency must be between 1 and 12");
    }
    Ok(DeleveragingResearchArgs {
        manifest: manifest.context("--manifest is required")?,
        context_cache: context_cache.context("--context-cache is required")?,
        download_concurrency,
        okx_base,
    })
}

/// 返回冻结 V1 的最小命令用法。
pub fn deleveraging_research_usage() -> &'static str {
    "Usage: market_deleveraging_reversal_research --manifest PATH --context-cache PATH [--download-concurrency 8]"
}

impl UniverseSchedule {
    /// 只接受连续的当前 live 加密 USDT 永续月度币池。
    fn from_manifest(manifest: HistoricalUniverseManifest) -> Result<Self> {
        if manifest.schema_version != 1
            || manifest.exchange != "okx"
            || manifest.market_type != "perpetual_swap"
            || manifest.quote_currency != "USDT"
            || manifest.timeframe != "15m"
            || !manifest
                .selection_rule
                .starts_with("current-live OKX USDT swaps only")
            || !manifest
                .source
                .classification_boundary
                .contains("instCategory=1")
        {
            bail!("deleveraging research requires current-live crypto-only OKX 15m manifest");
        }
        let mut windows = manifest
            .months
            .into_iter()
            .map(|month| UniverseWindow {
                from_ms: month.effective_from_ms,
                to_ms: month.effective_to_ms,
                members: month
                    .members
                    .into_iter()
                    .map(|member| member.symbol.to_ascii_uppercase())
                    .collect(),
            })
            .collect::<Vec<_>>();
        windows.sort_by_key(|window| window.from_ms);
        if windows.len() < 4
            || windows.iter().any(|window| {
                window.from_ms >= window.to_ms
                    || window.members.is_empty()
                    || window.members.iter().any(|symbol| !valid_symbol(symbol))
            })
            || windows
                .windows(2)
                .any(|pair| pair[0].to_ms != pair[1].from_ms)
        {
            bail!("deleveraging research requires at least four contiguous monthly windows");
        }
        Ok(Self {
            version: manifest.universe_version,
            windows,
        })
    }

    pub(super) fn union_symbols(&self) -> Vec<String> {
        self.windows
            .iter()
            .flat_map(|window| window.members.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub(super) fn window_at(&self, ts: i64) -> Option<&UniverseWindow> {
        self.windows
            .iter()
            .find(|window| ts >= window.from_ms && ts < window.to_ms)
    }
}

/// 拉取或读取冻结外部上下文、读取本机 quant_core 15m，并执行一次严格回放。
pub async fn run_deleveraging_research(
    args: &DeleveragingResearchArgs,
    database_url: &str,
) -> Result<DeleveragingResearchReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode deleveraging universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let (context_states, context_audit) = load_context_states(args, &schedule).await?;
    let first = schedule.windows.first().context("missing first window")?;
    let last = schedule.windows.last().context("missing last window")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for deleveraging research")?;
    let mut candles_by_symbol = BTreeMap::<String, Vec<CandleItem>>::new();
    for symbol in schedule.union_symbols() {
        let candles = load_symbol_candles(
            &pool,
            &symbol,
            first.from_ms.saturating_sub(32 * DAY_MS),
            last.to_ms.saturating_add(2 * DAY_MS),
        )
        .await?;
        candles_by_symbol.insert(symbol, candles);
    }
    let (price_tail, price_coverage_blocked) =
        build_price_tail_states(&schedule, &candles_by_symbol);
    let mut stages = DeleveragingStageCounts::default();
    let mut trades = Vec::new();
    for (symbol, candles) in &candles_by_symbol {
        trades.extend(scan_symbol(
            symbol,
            candles,
            &schedule,
            &context_states,
            price_tail.get(symbol),
            &mut stages,
        ));
    }
    trades.sort_by(|left, right| {
        left.entry_ts
            .cmp(&right.entry_ts)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    let split_ms = schedule.windows[schedule.windows.len() / 2].from_ms;
    let discovery = trades
        .iter()
        .filter(|trade| trade.entry_ts < split_ms)
        .cloned()
        .collect::<Vec<_>>();
    let validation = trades
        .iter()
        .filter(|trade| trade.entry_ts >= split_ms)
        .cloned()
        .collect::<Vec<_>>();
    let monthly = schedule
        .windows
        .iter()
        .map(|window| {
            let values = trades
                .iter()
                .filter(|trade| trade.entry_ts >= window.from_ms && trade.entry_ts < window.to_ms)
                .cloned()
                .collect::<Vec<_>>();
            (window.from_ms, metrics(&values, 1.0))
        })
        .collect::<Vec<_>>();
    let positive_months = monthly
        .iter()
        .filter(|(_, value)| value.net_sum_r > 0.0)
        .count();
    let (top_three_positive_symbols, net_r_without_top_three_symbols) =
        concentration_without_top_three(&trades);
    let mut exit_reasons = BTreeMap::<String, usize>::new();
    for trade in &trades {
        *exit_reasons
            .entry(trade.exit_reason.to_owned())
            .or_default() += 1;
    }
    let report = DeleveragingResearchReport {
        universe_version: schedule.version.clone(),
        symbols: candles_by_symbol.len(),
        context: context_audit,
        price_coverage_blocked,
        stages,
        effective_events: effective_event_count(&trades),
        gross_zero_cost: metrics(&trades, 0.0),
        overall: metrics(&trades, 1.0),
        discovery: metrics(&discovery, 1.0),
        validation: metrics(&validation, 1.0),
        double_cost: metrics(&trades, 2.0),
        monthly,
        positive_months,
        top_three_positive_symbols,
        net_r_without_top_three_symbols,
        exit_reasons,
        trades,
    };
    print_report(&report);
    Ok(report)
}

fn build_price_tail_states(
    schedule: &UniverseSchedule,
    candles_by_symbol: &BTreeMap<String, Vec<CandleItem>>,
) -> (BTreeMap<String, BTreeSet<i64>>, usize) {
    let mut grouped = BTreeMap::<i64, Vec<(String, f64)>>::new();
    for (symbol, candles) in candles_by_symbol {
        for index in HISTORY_BARS..candles.len() {
            if candles[index].ts - candles[index - HISTORY_BARS].ts != HISTORY_BARS as i64 * MS_15M
            {
                continue;
            }
            let event_ts = candles[index].ts.saturating_add(MS_15M);
            if !schedule
                .window_at(event_ts)
                .is_some_and(|window| window.members.contains(symbol))
            {
                continue;
            }
            let value = candles[index].c / candles[index - HISTORY_BARS].c - 1.0;
            if value.is_finite() {
                grouped
                    .entry(event_ts)
                    .or_default()
                    .push((symbol.clone(), value));
            }
        }
    }
    let mut eligible = BTreeMap::<String, BTreeSet<i64>>::new();
    let mut coverage_blocked = 0usize;
    for (event_ts, mut values) in grouped {
        let Some(window) = schedule.window_at(event_ts) else {
            continue;
        };
        values.retain(|(symbol, _)| window.members.contains(symbol));
        let minimum = (window.members.len() as f64 * PRICE_COVERAGE_MIN_RATIO).ceil() as usize;
        if values.len() < minimum {
            coverage_blocked += 1;
            continue;
        }
        values.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        let bottom = (values.len() as f64 * PRICE_BOTTOM_RATIO).ceil() as usize;
        for (rank, (symbol, change)) in values.into_iter().enumerate() {
            if change < 0.0 && rank < bottom {
                eligible.entry(symbol).or_default().insert(event_ts);
            }
        }
    }
    (eligible, coverage_blocked)
}

fn scan_symbol(
    symbol: &str,
    candles: &[CandleItem],
    schedule: &UniverseSchedule,
    context: &ContextStates,
    price_tail: Option<&BTreeSet<i64>>,
    stages: &mut DeleveragingStageCounts,
) -> Vec<DeleveragingTrade> {
    let mut trades = Vec::new();
    let mut locked_until = None::<usize>;
    for sweep_index in HISTORY_BARS..candles.len().saturating_sub(2) {
        if locked_until.is_some_and(|exit_index| sweep_index <= exit_index) {
            continue;
        }
        let sweep_event_ts = candles[sweep_index].ts.saturating_add(MS_15M);
        let confirmation_index = sweep_index + 1;
        let entry_index = sweep_index + 2;
        if candles[confirmation_index].ts != sweep_event_ts
            || candles[entry_index].ts != sweep_event_ts.saturating_add(MS_15M)
            || !schedule
                .window_at(sweep_event_ts)
                .is_some_and(|window| window.members.contains(symbol))
            || !price_tail.is_some_and(|points| points.contains(&sweep_event_ts))
        {
            continue;
        }
        stages.price_tail_pass += 1;
        let decision_ts = candles[confirmation_index].ts.saturating_add(MS_15M);
        let Some(oi) = context
            .oi_at(symbol, decision_ts)
            .filter(|state| state.eligible)
        else {
            continue;
        };
        stages.oi_pass += 1;
        let Some(taker) = context
            .taker_at(symbol, decision_ts)
            .filter(|state| state.eligible)
        else {
            continue;
        };
        stages.taker_pass += 1;
        let Some(ratio) = context
            .ratio_at(symbol, decision_ts)
            .filter(|state| state.eligible)
        else {
            continue;
        };
        stages.ratio_pass += 1;
        if !sweep_reclaim_shape(candles, sweep_index) {
            continue;
        }
        stages.sweep_pass += 1;
        if !confirmation_passes(&candles[sweep_index], &candles[confirmation_index]) {
            continue;
        }
        stages.confirmation_pass += 1;
        let Some(atr) = atr_at(candles, sweep_index) else {
            stages.risk_blocked += 1;
            continue;
        };
        match settle_trade(
            symbol,
            candles,
            sweep_index,
            EntryEvidence {
                oi,
                taker,
                ratio,
                atr,
            },
        ) {
            Settlement::Trade(trade, exit_index) => {
                trades.push(trade);
                locked_until = Some(exit_index);
            }
            Settlement::RiskBlocked => stages.risk_blocked += 1,
            Settlement::Incomplete => stages.incomplete_outcomes += 1,
        }
    }
    trades
}

/// 扫破此前 24h 低点后收回，允许收阴，但要求收盘回到上半区且下影有意义。
fn sweep_reclaim_shape(candles: &[CandleItem], index: usize) -> bool {
    if index < HISTORY_BARS || index >= candles.len() {
        return false;
    }
    let candle = &candles[index];
    let prior_low = candles[index - HISTORY_BARS..index]
        .iter()
        .map(|item| item.l)
        .reduce(f64::min)
        .unwrap_or(f64::NAN);
    let range = candle.h - candle.l;
    let body = (candle.c - candle.o).abs();
    let lower_wick = candle.o.min(candle.c) - candle.l;
    range.is_finite()
        && range > 0.0
        && candle.l < prior_low
        && candle.c > prior_low
        && (candle.c - candle.l) / range >= SWEEP_CLOSE_POSITION_MIN
        && lower_wick >= body * SWEEP_LOWER_WICK_BODY_MULTIPLIER
}

/// 第二根必须真实收阳并站上扫低实体中点，避免在长下影尚未确认时接刀。
fn confirmation_passes(sweep: &CandleItem, confirmation: &CandleItem) -> bool {
    let sweep_body_mid = (sweep.o + sweep.c) / 2.0;
    confirmation.c > confirmation.o && confirmation.c > sweep_body_mid
}

fn settle_trade(
    symbol: &str,
    candles: &[CandleItem],
    sweep_index: usize,
    evidence: EntryEvidence,
) -> Settlement {
    let entry_index = sweep_index + 2;
    if entry_index + MAX_HOLDING_BARS > candles.len() {
        return Settlement::Incomplete;
    }
    let entry = candles[entry_index].o;
    let stop = candles[sweep_index].l - evidence.atr * STOP_ATR_BUFFER;
    let risk = entry - stop;
    let risk_pct = risk / entry * 100.0;
    if !entry.is_finite()
        || !stop.is_finite()
        || risk <= 0.0
        || !(MIN_RISK_PCT..=MAX_RISK_PCT).contains(&risk_pct)
    {
        return Settlement::RiskBlocked;
    }
    let target = entry + risk * TARGET_R;
    let last_index = entry_index + MAX_HOLDING_BARS - 1;
    let mut exit_index = last_index;
    let mut exit = candles[last_index].c;
    let mut gross_r = (exit - entry) / risk;
    let mut exit_reason = "max_holding_timeout";
    for (offset, candle) in candles[entry_index..=last_index].iter().enumerate() {
        let current = entry_index + offset;
        if candle.l <= stop {
            exit_index = current;
            exit = stop;
            gross_r = -1.0;
            exit_reason = "sweep_stop";
            break;
        }
        if candle.h >= target {
            exit_index = current;
            exit = target;
            gross_r = TARGET_R;
            exit_reason = "target_3r";
            break;
        }
    }
    let cost_r = (entry + exit) * COST_RATE_PER_SIDE / risk;
    Settlement::Trade(
        DeleveragingTrade {
            symbol: symbol.to_owned(),
            sweep_ts: candles[sweep_index].ts.saturating_add(MS_15M),
            decision_ts: candles[sweep_index + 1].ts.saturating_add(MS_15M),
            entry_ts: candles[entry_index].ts,
            exit_ts: candles[exit_index].ts.saturating_add(MS_15M),
            oi_change: evidence.oi.change,
            taker_sell_share: evidence.taker.sell_share,
            long_short_ratio: evidence.ratio.ratio,
            long_short_change: evidence.ratio.change,
            entry,
            stop,
            target,
            gross_r,
            cost_r,
            net_r: gross_r - cost_r,
            exit_reason,
        },
        exit_index,
    )
}

async fn load_symbol_candles(
    pool: &PgPool,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<CandleItem>> {
    if !valid_symbol(symbol) {
        bail!("invalid manifest symbol {symbol}");
    }
    let table = format!("{}_candles_15m", symbol.to_ascii_lowercase());
    let query = format!(
        "SELECT ts, o, h, l, c, vol FROM \"{table}\" WHERE confirm = '1' AND ts >= $1 AND ts < $2 ORDER BY ts"
    );
    sqlx::query(&query)
        .bind(start_ms)
        .bind(end_ms)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load deleveraging candles from {table}"))?
        .into_iter()
        .map(|row| {
            Ok(CandleItem {
                ts: row.get("ts"),
                o: parse_number(row.get::<String, _>("o"))?,
                h: parse_number(row.get::<String, _>("h"))?,
                l: parse_number(row.get::<String, _>("l"))?,
                c: parse_number(row.get::<String, _>("c"))?,
                v: parse_number(row.get::<String, _>("vol"))?,
                confirm: 1,
            })
        })
        .collect()
}

fn atr_at(candles: &[CandleItem], index: usize) -> Option<f64> {
    if index + 1 < ATR_PERIOD {
        return None;
    }
    let start = index + 1 - ATR_PERIOD;
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
    let atr = total / ATR_PERIOD as f64;
    (atr.is_finite() && atr > 0.0).then_some(atr)
}

fn metrics(trades: &[DeleveragingTrade], cost_multiplier: f64) -> DeleveragingMetrics {
    if trades.is_empty() {
        return DeleveragingMetrics::default();
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
    DeleveragingMetrics {
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

fn effective_event_count(trades: &[DeleveragingTrade]) -> usize {
    let mut count = 0usize;
    let mut latest = None::<i64>;
    for trade in trades {
        if latest.is_none_or(|point| trade.entry_ts - point > MS_30M) {
            count += 1;
        }
        latest = Some(trade.entry_ts);
    }
    count
}

fn concentration_without_top_three(trades: &[DeleveragingTrade]) -> (Vec<String>, f64) {
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
        .collect::<Vec<_>>();
    let removed = positive
        .iter()
        .take(3)
        .map(|(_, value)| *value)
        .sum::<f64>();
    (
        top,
        trades.iter().map(|trade| trade.net_r).sum::<f64>() - removed,
    )
}

fn print_report(report: &DeleveragingResearchReport) {
    println!(
        "deleveraging_research\tuniverse={}\tsymbols={}\toi_rows={}\ttaker_rows={}\tratio_rows={}\toi_coverage_blocked={}\ttaker_coverage_blocked={}\tratio_coverage_blocked={}\tprice_coverage_blocked={}\tprice_tail={}\toi_pass={}\ttaker_pass={}\tratio_pass={}\tsweep_pass={}\tconfirmation_pass={}\trisk_blocked={}\tincomplete={}\ttrades={}\teffective_events={}\tpositive_months={}",
        report.universe_version,
        report.symbols,
        report.context.oi_rows,
        report.context.taker_rows,
        report.context.ratio_rows,
        report.context.oi_coverage_blocked,
        report.context.taker_coverage_blocked,
        report.context.ratio_coverage_blocked,
        report.price_coverage_blocked,
        report.stages.price_tail_pass,
        report.stages.oi_pass,
        report.stages.taker_pass,
        report.stages.ratio_pass,
        report.stages.sweep_pass,
        report.stages.confirmation_pass,
        report.stages.risk_blocked,
        report.stages.incomplete_outcomes,
        report.trades.len(),
        report.effective_events,
        report.positive_months,
    );
    for (label, value) in [
        ("gross_zero_cost", &report.gross_zero_cost),
        ("overall", &report.overall),
        ("discovery", &report.discovery),
        ("validation", &report.validation),
        ("double_cost", &report.double_cost),
    ] {
        print_metrics(label, value);
    }
    for (from_ms, value) in &report.monthly {
        print_metrics(&format!("month_{from_ms}"), value);
    }
    println!(
        "deleveraging_concentration\ttop_three={}\tnet_r_without_top_three={}\texit_reasons={}",
        report.top_three_positive_symbols.join(","),
        report.net_r_without_top_three_symbols,
        report
            .exit_reasons
            .iter()
            .map(|(reason, count)| format!("{reason}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    );
}

fn print_metrics(label: &str, value: &DeleveragingMetrics) {
    println!(
        "deleveraging_metrics\twindow={}\ttrades={}\tnet_sum_r={}\tnet_ev_r={}\tpf={}\twin_rate_pct={}\ttrade_sharpe={}\tmax_drawdown_r={}\trecovery={}",
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

fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_owned(), |number| number.to_string())
}

fn parse_number(value: String) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse candle number {value}"))?;
    if !parsed.is_finite() {
        bail!("non-finite candle number {value}");
    }
    Ok(parsed)
}

fn valid_symbol(symbol: &str) -> bool {
    symbol.ends_with("-USDT-SWAP")
        && symbol
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests;
