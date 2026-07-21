use crate::app::okx_historical_universe::HistoricalUniverseManifest;
use anyhow::{anyhow, bail, Context, Result};
use rust_quant_strategies::implementations::{
    CausalMarketStructureFeatures, SmartMoneyConceptsStrategy,
};
use rust_quant_strategies::CandleItem;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MS_15M: i64 = 15 * 60 * 1_000;
const MS_30M: i64 = 30 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const PIVOT_WING: usize = 5;
const CHOCH_TO_FVG_MAX_BARS: usize = 2;
const FVG_RETEST_MAX_BARS: usize = 8;
const ATR_PERIOD: usize = 14;
const STOP_ATR_BUFFER: f64 = 0.25;
const MIN_RISK_PCT: f64 = 0.5;
const MAX_RISK_PCT: f64 = 3.0;
const TARGET_R: f64 = 2.5;
const MAX_HOLDING_BARS: usize = 48 * 4;
const COST_RATE_PER_SIDE: f64 = 0.0008;

/// 只读结构反转研究参数；交易规则全部冻结，不暴露结果驱动调参面。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChochFvgResearchArgs {
    /// 版本化 current-live-only 月度币池。
    pub manifest: PathBuf,
}

/// 结构候选在逐根因果回放中的计数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChochFvgStageCounts {
    pub choch_events: usize,
    pub fvg_after_choch: usize,
    pub retests: usize,
    pub risk_blocked: usize,
    pub membership_blocked: usize,
    pub incomplete_outcomes: usize,
}

/// 单个已结算研究交易；R 固定为入场到初始结构止损的风险。
#[derive(Debug, Clone, PartialEq)]
pub struct ChochFvgTrade {
    pub symbol: String,
    pub choch_ts: i64,
    pub fvg_ts: i64,
    pub entry_ts: i64,
    pub exit_ts: i64,
    pub entry: f64,
    pub stop: f64,
    pub target: f64,
    pub exit: f64,
    pub gross_r: f64,
    pub cost_r: f64,
    pub net_r: f64,
    pub exit_reason: &'static str,
}

/// 一组交易的固定 R 统计；用于 discovery、validation、月份和成本压力同口径比较。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChochFvgMetrics {
    pub trades: usize,
    pub net_sum_r: f64,
    pub net_expectancy_r: Option<f64>,
    pub profit_factor: Option<f64>,
    pub win_rate_pct: Option<f64>,
    pub trade_sharpe: Option<f64>,
    pub max_drawdown_r: f64,
    pub recovery_factor: Option<f64>,
}

/// v1 全窗口报告；没有统一资金通过前只表示研究候选质量。
#[derive(Debug, Clone, PartialEq)]
pub struct ChochFvgResearchReport {
    pub universe_version: String,
    pub months: usize,
    pub symbols: usize,
    pub stages: ChochFvgStageCounts,
    pub trades: Vec<ChochFvgTrade>,
    pub effective_events: usize,
    pub gross_zero_cost: ChochFvgMetrics,
    pub overall: ChochFvgMetrics,
    pub discovery: ChochFvgMetrics,
    pub validation: ChochFvgMetrics,
    pub double_cost: ChochFvgMetrics,
    pub monthly: Vec<(i64, ChochFvgMetrics)>,
    pub positive_months: usize,
    pub top_three_positive_symbols: Vec<String>,
    pub net_r_without_top_three_symbols: f64,
    pub exit_reasons: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UniverseWindow {
    from_ms: i64,
    to_ms: i64,
    members: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UniverseSchedule {
    version: String,
    windows: Vec<UniverseWindow>,
}

#[derive(Debug, Clone, Copy)]
struct PendingChoch {
    index: usize,
    ts: i64,
    protected_low: f64,
    window_index: usize,
}

#[derive(Debug, Clone, Copy)]
struct PendingFvg {
    formation_index: usize,
    choch: PendingChoch,
    lower: f64,
    upper: f64,
    atr: f64,
}

/// CLI 只接受 manifest 路径，未知参数失败，避免把 v1 变成隐式参数扫描器。
pub fn parse_choch_fvg_research_args<I>(values: I) -> Result<ChochFvgResearchArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut manifest = None;
    while let Some(arg) = values.next() {
        match arg.as_str() {
            "--manifest" => {
                manifest = Some(PathBuf::from(
                    values
                        .next()
                        .ok_or_else(|| anyhow!("--manifest requires a path"))?,
                ));
            }
            "--help" | "-h" => bail!(choch_fvg_research_usage()),
            _ => bail!("unknown argument: {arg}\n{}", choch_fvg_research_usage()),
        }
    }
    Ok(ChochFvgResearchArgs {
        manifest: manifest.context("--manifest is required")?,
    })
}

/// 返回固定 v1 研究命令用法。
pub fn choch_fvg_research_usage() -> &'static str {
    "Usage: market_structure_choch_fvg_research --manifest PATH"
}

/// 加载本地 quant_core K 线，按冻结 v1 规则扫描并打印可审计结果。
pub async fn run_choch_fvg_research(
    args: &ChochFvgResearchArgs,
    database_url: &str,
) -> Result<ChochFvgResearchReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode CHoCH FVG universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let first_window = schedule
        .windows
        .first()
        .context("missing first universe month")?;
    let last_window = schedule
        .windows
        .last()
        .context("missing last universe month")?;
    let load_start_ms = first_window.from_ms.saturating_sub(32 * DAY_MS);
    let load_end_ms = last_window
        .to_ms
        .saturating_add(2 * DAY_MS)
        .saturating_add(FVG_RETEST_MAX_BARS as i64 * MS_15M);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for CHoCH FVG research")?;
    let symbols = schedule.union_symbols();
    let mut stages = ChochFvgStageCounts::default();
    let mut trades = Vec::new();
    for symbol in &symbols {
        let candles = load_symbol_candles(&pool, symbol, load_start_ms, load_end_ms).await?;
        let features = SmartMoneyConceptsStrategy::causal_market_structure_feature_series(
            &candles, PIVOT_WING,
        );
        trades.extend(scan_symbol(
            symbol,
            &candles,
            &features,
            &schedule,
            &mut stages,
        ));
    }
    trades.sort_by_key(|trade| (trade.entry_ts, trade.symbol.clone()));
    let split_ms = schedule
        .windows
        .get(schedule.windows.len() / 2)
        .map(|window| window.from_ms)
        .context("historical universe requires discovery and validation months")?;
    let discovery_trades = trades
        .iter()
        .filter(|trade| trade.entry_ts < split_ms)
        .cloned()
        .collect::<Vec<_>>();
    let validation_trades = trades
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
        .filter(|(_, metrics)| metrics.net_sum_r > 0.0)
        .count();
    let (top_three_positive_symbols, net_r_without_top_three_symbols) =
        concentration_without_top_three(&trades);
    let mut exit_reasons = BTreeMap::<String, usize>::new();
    for trade in &trades {
        *exit_reasons
            .entry(trade.exit_reason.to_string())
            .or_default() += 1;
    }
    let report = ChochFvgResearchReport {
        universe_version: schedule.version.clone(),
        months: schedule.windows.len(),
        symbols: symbols.len(),
        stages,
        effective_events: effective_event_count(&trades),
        gross_zero_cost: metrics(&trades, 0.0),
        overall: metrics(&trades, 1.0),
        discovery: metrics(&discovery_trades, 1.0),
        validation: metrics(&validation_trades, 1.0),
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

impl UniverseSchedule {
    fn from_manifest(manifest: HistoricalUniverseManifest) -> Result<Self> {
        if manifest.schema_version != 1
            || manifest.exchange != "okx"
            || manifest.market_type != "perpetual_swap"
            || manifest.quote_currency != "USDT"
            || manifest.timeframe != "15m"
            || !manifest
                .selection_rule
                .starts_with("current-live OKX USDT swaps only")
        {
            bail!("CHoCH FVG research requires current-live-only OKX USDT swap 15m manifest");
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
        if windows.len() < 2
            || windows.iter().any(|window| {
                window.from_ms >= window.to_ms
                    || window.members.is_empty()
                    || window.members.iter().any(|symbol| !valid_symbol(symbol))
            })
            || windows
                .windows(2)
                .any(|pair| pair[0].to_ms != pair[1].from_ms)
        {
            bail!("CHoCH FVG research requires contiguous non-empty monthly universe windows");
        }
        Ok(Self {
            version: manifest.universe_version,
            windows,
        })
    }

    fn union_symbols(&self) -> Vec<String> {
        self.windows
            .iter()
            .flat_map(|window| window.members.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn window_at(&self, ts: i64) -> Option<(usize, &UniverseWindow)> {
        self.windows
            .iter()
            .enumerate()
            .find(|(_, window)| ts >= window.from_ms && ts < window.to_ms)
    }
}

/// 单币状态机只在 CHoCH 后接受新 FVG，并在固定短窗内等待首次中点回踩。
fn scan_symbol(
    symbol: &str,
    candles: &[CandleItem],
    features: &[CausalMarketStructureFeatures],
    schedule: &UniverseSchedule,
    stages: &mut ChochFvgStageCounts,
) -> Vec<ChochFvgTrade> {
    if candles.len() != features.len() {
        return Vec::new();
    }
    let mut trades = Vec::new();
    let mut pending_choch = None::<PendingChoch>;
    let mut pending_fvg = None::<PendingFvg>;
    let mut locked_until = None::<usize>;
    for index in 0..candles.len() {
        if locked_until.is_some_and(|exit_index| index <= exit_index) {
            pending_choch = None;
            pending_fvg = None;
            continue;
        }
        let candle = &candles[index];
        if let Some(fvg) = pending_fvg {
            if candle.c < fvg.choch.protected_low
                || index > fvg.formation_index + FVG_RETEST_MAX_BARS
            {
                pending_fvg = None;
            } else if index > fvg.formation_index && candle.l <= (fvg.lower + fvg.upper) / 2.0 {
                stages.retests += 1;
                let entry_ts = candle.ts;
                let same_window =
                    schedule
                        .window_at(entry_ts)
                        .is_some_and(|(window_index, window)| {
                            window_index == fvg.choch.window_index
                                && window.members.contains(symbol)
                        });
                if !same_window {
                    stages.membership_blocked += 1;
                } else if let Some((trade, exit_index)) =
                    settle_retest_trade(symbol, candles, index, fvg)
                {
                    trades.push(trade);
                    locked_until = Some(exit_index);
                } else if index + MAX_HOLDING_BARS > candles.len() {
                    stages.incomplete_outcomes += 1;
                } else {
                    stages.risk_blocked += 1;
                }
                pending_fvg = None;
                pending_choch = None;
                continue;
            }
        }
        if let Some(choch) = pending_choch {
            if candle.c < choch.protected_low || index > choch.index + CHOCH_TO_FVG_MAX_BARS {
                pending_choch = None;
            } else if index > choch.index && features[index].bullish_fvg {
                if let (Some(lower), Some(upper), Some(atr)) = (
                    features[index].bullish_fvg_lower,
                    features[index].bullish_fvg_upper,
                    atr_at(candles, index),
                ) {
                    stages.fvg_after_choch += 1;
                    pending_fvg = Some(PendingFvg {
                        formation_index: index,
                        choch,
                        lower,
                        upper,
                        atr,
                    });
                }
                pending_choch = None;
            }
        }
        if features[index].bullish_choch {
            let event_ts = candle.ts.saturating_add(MS_15M);
            if let (Some(protected_low), Some((window_index, window))) = (
                features[index].latest_confirmed_swing_low,
                schedule.window_at(event_ts),
            ) {
                if window.members.contains(symbol) {
                    stages.choch_events += 1;
                    pending_choch = Some(PendingChoch {
                        index,
                        ts: event_ts,
                        protected_low,
                        window_index,
                    });
                    pending_fvg = None;
                }
            }
        }
    }
    trades
}

/// 在回踩 K 线上保守结算；同根止盈止损冲突按止损，避免 OHLC 内部顺序假设。
fn settle_retest_trade(
    symbol: &str,
    candles: &[CandleItem],
    entry_index: usize,
    fvg: PendingFvg,
) -> Option<(ChochFvgTrade, usize)> {
    if entry_index + MAX_HOLDING_BARS > candles.len() {
        return None;
    }
    let entry = (fvg.lower + fvg.upper) / 2.0;
    let stop = fvg.choch.protected_low - fvg.atr * STOP_ATR_BUFFER;
    let initial_risk = entry - stop;
    let risk_pct = initial_risk / entry * 100.0;
    if !entry.is_finite()
        || !stop.is_finite()
        || initial_risk <= 0.0
        || !(MIN_RISK_PCT..=MAX_RISK_PCT).contains(&risk_pct)
    {
        return None;
    }
    let target = entry + initial_risk * TARGET_R;
    let last_index = entry_index + MAX_HOLDING_BARS - 1;
    let mut exit_index = last_index;
    let mut exit = candles[last_index].c;
    let mut gross_r = (exit - entry) / initial_risk;
    let mut exit_reason = "max_holding_timeout";
    for (index, candle) in candles[entry_index..=last_index].iter().enumerate() {
        let absolute_index = entry_index + index;
        if candle.l <= stop {
            exit_index = absolute_index;
            exit = stop;
            gross_r = -1.0;
            exit_reason = "structure_stop";
            break;
        }
        // 入场根只有 OHLC，无法证明高点发生在中点回踩成交之后，因此禁止同根止盈。
        if absolute_index > entry_index && candle.h >= target {
            exit_index = absolute_index;
            exit = target;
            gross_r = TARGET_R;
            exit_reason = "target_2_5r";
            break;
        }
    }
    let cost_r = (entry + exit) * COST_RATE_PER_SIDE / initial_risk;
    Some((
        ChochFvgTrade {
            symbol: symbol.to_string(),
            choch_ts: fvg.choch.ts,
            fvg_ts: candles[fvg.formation_index].ts.saturating_add(MS_15M),
            entry_ts: candles[entry_index].ts,
            exit_ts: candles[exit_index].ts.saturating_add(MS_15M),
            entry,
            stop,
            target,
            exit,
            gross_r,
            cost_r,
            net_r: gross_r - cost_r,
            exit_reason,
        },
        exit_index,
    ))
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
        .with_context(|| format!("load CHoCH FVG candles from {table}"))?
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
    let mut sum = 0.0;
    for candle_index in start..=index {
        let candle = &candles[candle_index];
        let previous_close = candle_index
            .checked_sub(1)
            .map(|previous| candles[previous].c)
            .unwrap_or(candle.c);
        sum += (candle.h - candle.l)
            .max((candle.h - previous_close).abs())
            .max((candle.l - previous_close).abs());
    }
    let atr = sum / ATR_PERIOD as f64;
    (atr.is_finite() && atr > 0.0).then_some(atr)
}

fn metrics(trades: &[ChochFvgTrade], cost_multiplier: f64) -> ChochFvgMetrics {
    if trades.is_empty() {
        return ChochFvgMetrics::default();
    }
    let values = trades
        .iter()
        .map(|trade| trade.gross_r - trade.cost_r * cost_multiplier)
        .collect::<Vec<_>>();
    let net_sum_r = values.iter().sum::<f64>();
    let gross_profit = values.iter().filter(|value| **value > 0.0).sum::<f64>();
    let gross_loss = values
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
    ChochFvgMetrics {
        trades: values.len(),
        net_sum_r,
        net_expectancy_r: Some(mean),
        profit_factor: (gross_loss > 0.0).then_some(gross_profit / gross_loss),
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

fn effective_event_count(trades: &[ChochFvgTrade]) -> usize {
    let mut event_count = 0usize;
    let mut last_event_ts = None::<i64>;
    for trade in trades {
        if last_event_ts.is_none_or(|last| trade.entry_ts - last > MS_30M) {
            event_count += 1;
        }
        last_event_ts = Some(trade.entry_ts);
    }
    event_count
}

fn concentration_without_top_three(trades: &[ChochFvgTrade]) -> (Vec<String>, f64) {
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

fn print_report(report: &ChochFvgResearchReport) {
    println!(
        "choch_fvg_research\tuniverse={}\tmonths={}\tsymbols={}\tchoch={}\tfvg_after_choch={}\tretests={}\trisk_blocked={}\tmembership_blocked={}\tincomplete={}\ttrades={}\teffective_events={}\tpositive_months={}",
        report.universe_version,
        report.months,
        report.symbols,
        report.stages.choch_events,
        report.stages.fvg_after_choch,
        report.stages.retests,
        report.stages.risk_blocked,
        report.stages.membership_blocked,
        report.stages.incomplete_outcomes,
        report.trades.len(),
        report.effective_events,
        report.positive_months,
    );
    for (label, metrics) in [
        ("gross_zero_cost", &report.gross_zero_cost),
        ("overall", &report.overall),
        ("discovery", &report.discovery),
        ("validation", &report.validation),
        ("double_cost", &report.double_cost),
    ] {
        print_metrics(label, metrics);
    }
    for (from_ms, metrics) in &report.monthly {
        print_metrics(&format!("month_{from_ms}"), metrics);
    }
    println!(
        "choch_fvg_concentration\ttop_three={}\tnet_r_without_top_three={}\texit_reasons={}",
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

fn print_metrics(label: &str, metrics: &ChochFvgMetrics) {
    println!(
        "choch_fvg_metrics\twindow={}\ttrades={}\tnet_sum_r={}\tnet_ev_r={}\tpf={}\twin_rate_pct={}\ttrade_sharpe={}\tmax_drawdown_r={}\trecovery={}",
        label,
        metrics.trades,
        metrics.net_sum_r,
        optional(metrics.net_expectancy_r),
        optional(metrics.profit_factor),
        optional(metrics.win_rate_pct),
        optional(metrics.trade_sharpe),
        metrics.max_drawdown_r,
        optional(metrics.recovery_factor),
    );
}

fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_string(), |value| value.to_string())
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
mod tests {
    use super::*;

    fn candle(index: usize, low: f64, high: f64, close: f64) -> CandleItem {
        CandleItem {
            ts: index as i64 * MS_15M,
            o: 100.0,
            h: high,
            l: low,
            c: close,
            v: 1_000.0,
            confirm: 1,
        }
    }

    fn schedule() -> UniverseSchedule {
        UniverseSchedule {
            version: "fixture".to_string(),
            windows: vec![UniverseWindow {
                from_ms: 0,
                to_ms: 100 * MS_15M,
                members: BTreeSet::from(["BTC-USDT-SWAP".to_string()]),
            }],
        }
    }

    #[test]
    fn fvg_must_form_after_choch_and_retest_on_a_later_candle() {
        let mut candles = (0..20)
            .map(|index| candle(index, 99.0, 101.0, 100.0))
            .collect::<Vec<_>>();
        candles.extend([
            candle(20, 99.0, 103.0, 102.0),
            candle(21, 101.8, 104.0, 103.0),
            candle(22, 100.8, 103.0, 102.0),
            candle(23, 101.0, 108.0, 107.0),
        ]);
        candles.extend((24..220).map(|index| candle(index, 100.0, 102.0, 101.0)));
        let mut features = vec![CausalMarketStructureFeatures::default(); candles.len()];
        features[20] = CausalMarketStructureFeatures {
            bullish_choch: true,
            latest_confirmed_swing_low: Some(99.0),
            ..Default::default()
        };
        features[21] = CausalMarketStructureFeatures {
            bullish_fvg: true,
            bullish_fvg_lower: Some(100.0),
            bullish_fvg_upper: Some(102.0),
            ..Default::default()
        };
        let mut stages = ChochFvgStageCounts::default();
        let trades = scan_symbol(
            "BTC-USDT-SWAP",
            &candles,
            &features,
            &schedule(),
            &mut stages,
        );

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].entry_ts, candles[22].ts);
        assert_eq!(trades[0].exit_reason, "target_2_5r");
        assert_eq!(stages.fvg_after_choch, 1);
    }

    #[test]
    fn fvg_on_the_same_candle_as_choch_does_not_create_a_setup() {
        let candles = (0..220)
            .map(|index| candle(index, 99.0, 103.0, 101.0))
            .collect::<Vec<_>>();
        let mut features = vec![CausalMarketStructureFeatures::default(); candles.len()];
        features[20] = CausalMarketStructureFeatures {
            bullish_choch: true,
            bullish_fvg: true,
            bullish_fvg_lower: Some(100.0),
            bullish_fvg_upper: Some(102.0),
            latest_confirmed_swing_low: Some(99.0),
            ..Default::default()
        };
        let mut stages = ChochFvgStageCounts::default();

        assert!(scan_symbol(
            "BTC-USDT-SWAP",
            &candles,
            &features,
            &schedule(),
            &mut stages,
        )
        .is_empty());
    }

    #[test]
    fn entry_candle_high_cannot_be_counted_as_a_post_fill_target() {
        let mut candles = (0..220)
            .map(|index| candle(index, 100.0, 102.0, 101.0))
            .collect::<Vec<_>>();
        candles[20] = candle(20, 100.0, 108.0, 101.0);
        let fvg = PendingFvg {
            formation_index: 19,
            choch: PendingChoch {
                index: 18,
                ts: candles[18].ts + MS_15M,
                protected_low: 99.0,
                window_index: 0,
            },
            lower: 100.0,
            upper: 102.0,
            atr: 1.0,
        };

        let (trade, _) = settle_retest_trade("BTC-USDT-SWAP", &candles, 20, fvg).unwrap();

        assert_eq!(trade.exit_reason, "max_holding_timeout");
    }
}
