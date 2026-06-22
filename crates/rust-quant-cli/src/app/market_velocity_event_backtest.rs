use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use rust_quant_domain::entities::BacktestLog;
use rust_quant_domain::traits::BacktestLogRepository;
use rust_quant_infrastructure::SqlxBacktestRepository;
use rust_quant_services::rust_quan_web::{
    ExecutionTaskClient, ExecutionTaskConfig, MarketVelocityPaperOutcomeRequest,
};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::collections::{BTreeMap, HashMap};

mod args;
mod data;
mod equity;
mod exit;
mod fvg;
mod reentry;
mod report;
use args::{
    entry_trigger_filter_version_label, format_entry_trigger_filter_list,
    format_entry_trigger_rank_blocklist, normalize_entry_trigger, normalize_symbol,
};
pub use args::{
    parse_cli_args_from, parse_paper_observation_args_from, parse_paper_observation_command_from,
    print_market_velocity_event_backtest_usage, print_market_velocity_paper_observation_usage,
    EntryTriggerRankBlock, FvgEntryMode, MarketVelocityEventBacktestArgs,
    MarketVelocityEventSource, MarketVelocityPaperObservationCommand,
    MarketVelocityPaperOutcomeSink, MarketVelocityTradeDirection, StopReentryMode,
};
use data::load_backtest_data;
pub use equity::{
    build_framework_equity_concentration_reports, build_framework_equity_quartile_reports,
    build_framework_equity_report, build_framework_equity_split_reports,
    build_framework_equity_trade_reports, build_framework_equity_trigger_reports,
    build_market_velocity_backtest_details, market_velocity_strategy_type,
    print_framework_equity_reports, FrameworkEquityCloseLegReport, FrameworkEquityReport,
    FrameworkEquitySymbolReport, FrameworkEquityTradeReport,
};
pub use exit::{simulate_trade, EarlyExit, ProfitProtection, RunnerExit};
use fvg::{find_fvg_entry, FvgEntrySearch};
use reentry::maybe_apply_stop_reentry;
use report::{print_result_report, print_stage_report};

pub const MS_15M: i64 = 15 * 60 * 1_000;
pub const MS_1H: i64 = 60 * 60 * 1_000;
pub const MS_4H: i64 = 4 * 60 * 60 * 1_000;

const TOUCH_THRESHOLD_PCT: f64 = 0.3;
const PAPER_OUTCOME_HORIZONS: &[(i32, i64)] =
    &[(24, 24 * 60 * 60 * 1_000), (48, 48 * 60 * 60 * 1_000)];

#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityEventBacktestConfig {
    pub database_url: String,
    pub args: MarketVelocityEventBacktestArgs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BacktestCandle {
    pub ts: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedCandle {
    pub candle: BacktestCandle,
    pub sma: Option<f64>,
    pub ema: Option<f64>,
    pub previous_volume_avg: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
struct CandlePair {
    symbol: String,
    candles_15m: String,
    candles_1h: Option<String>,
    candles_4h: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadarEvent {
    pub id: i64,
    pub exchange: String,
    pub symbol: String,
    pub ts: i64,
    pub detected_at: String,
    pub new_rank: i32,
    pub delta_rank: i32,
    pub current_price: f64,
    pub price_change_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmedEvent {
    pub event: RadarEvent,
    pub entry_ts: i64,
    pub entry_price: f64,
    pub entry_idx: usize,
    pub trigger: String,
}

pub(super) fn trade_direction_for_event(event: &RadarEvent) -> MarketVelocityTradeDirection {
    if event.price_change_pct < 0.0 {
        MarketVelocityTradeDirection::Short
    } else {
        MarketVelocityTradeDirection::Long
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TradeOutcome {
    Win,
    Loss,
    Flat,
    Timeout,
    Incomplete,
}

impl TradeOutcome {
    fn label(self) -> &'static str {
        match self {
            Self::Win => "win",
            Self::Loss => "loss",
            Self::Flat => "flat",
            Self::Timeout => "timeout",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradeResult {
    pub outcome: TradeOutcome,
    pub reason: String,
    pub exit_ts: i64,
    pub r: Option<f64>,
    pub complete: bool,
    pub symbol: Option<String>,
    pub event_id: Option<i64>,
    pub detected_at: Option<String>,
    pub entry_ts: i64,
    pub entry_price: f64,
    pub trigger: Option<String>,
    pub reentry: Option<StopReentryDetails>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StopReentryDetails {
    pub mode: StopReentryMode,
    pub original_entry_ts: i64,
    pub original_entry_price: f64,
    pub original_exit_ts: i64,
    pub original_reason: String,
    pub original_r: Option<f64>,
    pub signal_ts: i64,
    pub reclaim_price: f64,
    pub reentry_exit_reason: String,
    pub reentry_r: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BacktestDataSet {
    pairs: Vec<CandlePair>,
    candles_15m: HashMap<String, Vec<BacktestCandle>>,
    candles_1h: HashMap<String, Vec<BacktestCandle>>,
    candles_4h: HashMap<String, Vec<BacktestCandle>>,
    candles_15m_computed: HashMap<String, Vec<ComputedCandle>>,
    candles_4h_computed: HashMap<String, Vec<ComputedCandle>>,
    events: Vec<RadarEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationReport {
    pub confirmed: Vec<ConfirmedEvent>,
    pub stage_counts: BTreeMap<String, usize>,
    pub blockers: BTreeMap<String, BTreeMap<String, usize>>,
}

pub fn config_from_env_and_args(
    args: MarketVelocityEventBacktestArgs,
) -> Result<MarketVelocityEventBacktestConfig> {
    let database_url = first_non_empty_env(&[
        "QUANT_CORE_DATABASE_URL",
        "POSTGRES_QUANT_CORE_DATABASE_URL",
    ])
    .context("market velocity event backtest requires QUANT_CORE_DATABASE_URL")?;
    Ok(MarketVelocityEventBacktestConfig { database_url, args })
}

pub async fn run_market_velocity_event_backtest(
    config: MarketVelocityEventBacktestConfig,
) -> Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for market velocity event backtest")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let evaluation = evaluate_events(
        &data.events,
        &data.candles_4h_computed,
        &data.candles_15m_computed,
        &data.candles_4h,
        &data.candles_1h,
        &data.candles_15m,
        &config.args,
    );
    print_stage_report(&data, &evaluation);
    let symbol_filtered = filter_confirmed_events_by_symbol(&evaluation.confirmed, &config.args);
    print_symbol_filter_report(&evaluation.confirmed, &symbol_filtered, &config.args);
    let confirmed = filter_confirmed_events_by_entry_trigger(&symbol_filtered, &config.args);
    print_entry_trigger_filter_report(&symbol_filtered, &confirmed, &config.args);
    print_result_report(&confirmed, &data.candles_15m, &config.args);
    if config.args.equity_report
        || config.args.equity_split_report
        || config.args.equity_quartile_report
        || config.args.equity_trigger_report
        || config.args.equity_concentration_report
        || config.args.equity_feature_report
        || config.args.equity_symbol_window_report
        || config.args.equity_trade_report
    {
        print_framework_equity_reports(&confirmed, &data.candles_15m, &config.args);
    }
    if config.args.save_backtest_detail {
        save_market_velocity_backtest_detail(&pool, &confirmed, &data.candles_15m, &config.args)
            .await?;
    }
    match config.args.paper_outcome_sink {
        MarketVelocityPaperOutcomeSink::Off => {}
        MarketVelocityPaperOutcomeSink::Jsonl => {
            let outcomes =
                build_market_velocity_paper_outcomes(&confirmed, &data.candles_15m, &config.args);
            print_market_velocity_paper_outcomes_jsonl(&outcomes)?;
        }
        MarketVelocityPaperOutcomeSink::Web => {
            let outcomes =
                build_market_velocity_paper_outcomes(&confirmed, &data.candles_15m, &config.args);
            submit_market_velocity_paper_outcomes(&outcomes).await?;
        }
    }
    Ok(())
}

async fn save_market_velocity_backtest_detail(
    pool: &PgPool,
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<()> {
    let repository = SqlxBacktestRepository::new(pool.clone());
    let (kline_start_time, kline_end_time, kline_nums) = market_velocity_kline_window(candles_15m);
    let strategy_type = market_velocity_strategy_type(args).to_string();

    for target_r in &args.target_rs {
        let report = build_framework_equity_report(confirmed, candles_15m, *target_r, args);
        let trade_reports =
            build_framework_equity_trade_reports(confirmed, candles_15m, *target_r, args);
        let details = trade_reports
            .iter()
            .map(|trade| build_market_velocity_backtest_details(trade, 0, args))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let backtest_log = BacktestLog::new(
            strategy_type.clone(),
            "MULTI_SYMBOL".to_string(),
            "15m".to_string(),
            report
                .win_rate
                .map(|value| value.to_string())
                .unwrap_or_else(|| "NA".to_string()),
            market_velocity_final_fund(&report).to_string(),
            report.total_open_trades as i32,
            Some(market_velocity_strategy_detail(args).to_string()),
            market_velocity_risk_config_detail(args, *target_r).to_string(),
            report.total_profit.to_string(),
            kline_start_time,
            kline_end_time,
            kline_nums,
        );
        let back_test_id = repository.insert_log(&backtest_log).await?;
        let details = details
            .into_iter()
            .map(|mut detail| {
                detail.back_test_id = back_test_id;
                detail
            })
            .collect::<Vec<_>>();
        let inserted = repository.insert_details(&details).await?;
        println!(
            "market_velocity_backtest_detail_saved\ttarget={}R\tback_test_log_id={}\ttrades={}\tdetails_inserted={}",
            target_r,
            back_test_id,
            trade_reports.len(),
            inserted,
        );
    }

    Ok(())
}

fn market_velocity_final_fund(report: &FrameworkEquityReport) -> f64 {
    report
        .symbols
        .iter()
        .map(|symbol| symbol.final_fund)
        .sum::<f64>()
}

fn market_velocity_kline_window(
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
) -> (i64, i64, i32) {
    let mut start = i64::MAX;
    let mut end = i64::MIN;
    let mut nums = 0i32;

    for candle in candles_15m.values().flatten() {
        start = start.min(candle.ts);
        end = end.max(candle.ts);
        nums = nums.saturating_add(1);
    }

    if nums == 0 {
        (0, 0, 0)
    } else {
        (start, end, nums)
    }
}

fn market_velocity_strategy_detail(args: &MarketVelocityEventBacktestArgs) -> serde_json::Value {
    json!({
        "source": "market_velocity_event_backtest",
        "event_source": match args.event_source {
            MarketVelocityEventSource::Episodes => "episodes",
            MarketVelocityEventSource::RawEvents => "raw_events",
            MarketVelocityEventSource::RawState => "raw_state",
        },
        "trade_direction": args.trade_direction.label(),
        "entry_rule_version": &args.paper_outcome_entry_rule_version,
        "entry_period": args.entry_period,
        "entry_max_distance_pct": args.entry_max_distance_pct,
        "entry_min_volume_ratio": args.entry_min_volume_ratio,
        "trend_min_average_distance_pct": args.trend_min_average_distance_pct,
        "min_delta_rank": args.min_delta_rank,
        "max_delta_rank": args.max_delta_rank,
        "max_new_rank": args.max_new_rank,
        "min_price_change_pct": args.min_price_change_pct,
        "tail_new_rank_threshold": args.tail_new_rank_threshold,
        "tail_rank_min_price_change_pct": args.tail_rank_min_price_change_pct,
        "chase_top_rank": args.chase_top_rank,
        "chase_price_change_pct": args.chase_price_change_pct,
        "entry_trigger_allowlist": &args.entry_trigger_allowlist,
        "entry_trigger_blocklist": &args.entry_trigger_blocklist,
        "entry_trigger_rank_blocklist": args.entry_trigger_rank_blocklist.iter().map(|block| {
            json!({
                "trigger": &block.trigger,
                "min_new_rank": block.min_new_rank,
                "max_new_rank": block.max_new_rank,
            })
        }).collect::<Vec<_>>(),
        "symbol_blocklist": &args.symbol_blocklist,
    })
}

fn market_velocity_risk_config_detail(
    args: &MarketVelocityEventBacktestArgs,
    target_r: f64,
) -> serde_json::Value {
    json!({
        "mode": "symbol_isolated_100u",
        "trade_direction": args.trade_direction.label(),
        "stop_loss_pct": args.stop_loss_pct,
        "target_r": target_r,
        "profit_protect_after_r": args.profit_protect_after_r,
        "profit_protect_stop_r": args.profit_protect_stop_r,
        "runner_target_r": args.runner_target_r,
        "runner_fraction": args.runner_fraction,
        "runner_stop_r": args.runner_stop_r,
        "early_exit_no_profit_candles": args.early_exit_no_profit_candles,
        "stop_reentry_mode": args.stop_reentry_mode.label(),
        "fvg_entry_mode": args.fvg_entry_mode.label(),
        "fvg_lookback_candles": args.fvg_lookback_candles,
        "fvg_max_wait_candles": args.fvg_max_wait_candles,
    })
}

pub fn filter_confirmed_events_by_entry_trigger(
    confirmed: &[ConfirmedEvent],
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<ConfirmedEvent> {
    confirmed
        .iter()
        .filter(|event| entry_trigger_allowed(event, args))
        .cloned()
        .collect()
}

pub fn filter_confirmed_events_by_symbol(
    confirmed: &[ConfirmedEvent],
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<ConfirmedEvent> {
    confirmed
        .iter()
        .filter(|event| symbol_allowed(&event.event.symbol, args))
        .cloned()
        .collect()
}

fn symbol_allowed(symbol: &str, args: &MarketVelocityEventBacktestArgs) -> bool {
    let normalized = normalize_symbol(symbol);
    !args
        .symbol_blocklist
        .iter()
        .any(|blocked| normalize_symbol(blocked) == normalized)
}

fn entry_trigger_allowed(event: &ConfirmedEvent, args: &MarketVelocityEventBacktestArgs) -> bool {
    let normalized = normalize_entry_trigger(&event.trigger);
    if !args.entry_trigger_allowlist.is_empty()
        && !args
            .entry_trigger_allowlist
            .iter()
            .any(|allowed| allowed == &normalized)
    {
        return false;
    }
    if args
        .entry_trigger_blocklist
        .iter()
        .any(|blocked| blocked == &normalized)
    {
        return false;
    }
    !args.entry_trigger_rank_blocklist.iter().any(|blocked| {
        blocked.trigger == normalized
            && event.event.new_rank >= blocked.min_new_rank
            && event.event.new_rank <= blocked.max_new_rank
    })
}

fn print_symbol_filter_report(
    before: &[ConfirmedEvent],
    after: &[ConfirmedEvent],
    args: &MarketVelocityEventBacktestArgs,
) {
    if args.symbol_blocklist.is_empty() {
        return;
    }
    println!(
        "symbol_filter\tbefore={}\tafter={}\tblocklist={}",
        before.len(),
        after.len(),
        args.symbol_blocklist.join(",")
    );
}

fn print_entry_trigger_filter_report(
    before: &[ConfirmedEvent],
    after: &[ConfirmedEvent],
    args: &MarketVelocityEventBacktestArgs,
) {
    if args.entry_trigger_allowlist.is_empty()
        && args.entry_trigger_blocklist.is_empty()
        && args.entry_trigger_rank_blocklist.is_empty()
    {
        return;
    }
    println!(
        "entry_trigger_filter\tbefore={}\tafter={}\tallowlist={}\tblocklist={}\trank_blocklist={}",
        before.len(),
        after.len(),
        format_entry_trigger_filter_list(&args.entry_trigger_allowlist),
        format_entry_trigger_filter_list(&args.entry_trigger_blocklist),
        format_entry_trigger_rank_blocklist(&args.entry_trigger_rank_blocklist)
    );
}

pub fn build_computed_candles(candles: Vec<BacktestCandle>, period: usize) -> Vec<ComputedCandle> {
    let mut computed = Vec::with_capacity(candles.len());
    let mut ema: Option<f64> = None;
    let multiplier = 2.0 / (period as f64 + 1.0);

    for i in 0..candles.len() {
        let sma = if i + 1 >= period {
            simple_average(
                candles[i + 1 - period..=i]
                    .iter()
                    .map(|candle| candle.close),
            )
        } else {
            None
        };
        ema = match (i + 1, ema, sma) {
            (count, _, Some(value)) if count == period => Some(value),
            (count, Some(previous), _) if count > period && valid_positive(candles[i].close) => {
                Some((candles[i].close - previous) * multiplier + previous)
            }
            (count, previous, _) if count > period => previous.and(None),
            _ => None,
        };
        let previous_volume_avg = if i >= period {
            simple_average(candles[i - period..i].iter().map(|candle| candle.volume))
        } else {
            None
        };

        computed.push(ComputedCandle {
            candle: candles[i].clone(),
            sma,
            ema,
            previous_volume_avg,
        });
    }

    computed
}

pub fn trend_confirmation(
    candles: &[ComputedCandle],
    event_ts: i64,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> (bool, String) {
    let idx = completed_candle_count(candles, event_ts, MS_4H);
    if idx == 0 {
        return (false, "no_completed_4h".to_string());
    }

    let latest_completed_at = candles[idx - 1].candle.ts + MS_4H;
    if event_ts - latest_completed_at > args.max_4h_staleness_min * 60 * 1_000 {
        return (false, "stale_4h".to_string());
    }
    if idx < args.entry_period {
        return (false, "insufficient_4h".to_string());
    }

    let latest = &candles[idx - 1];
    let Some(sma) = latest.sma else {
        return (false, "invalid_4h_average".to_string());
    };
    let Some(ema) = latest.ema else {
        return (false, "invalid_4h_average".to_string());
    };
    let previous = idx
        .checked_sub(2)
        .and_then(|previous_idx| candles.get(previous_idx));
    let previous_close = previous.map(|candle| candle.candle.close);
    let previous_sma = previous.and_then(|candle| candle.sma);
    let previous_ema = previous.and_then(|candle| candle.ema);
    let sma_state = moving_average_state(latest.candle.close, sma, previous_close, previous_sma);
    let ema_state = moving_average_state(latest.candle.close, ema, previous_close, previous_ema);
    let confirmed = match direction {
        MarketVelocityTradeDirection::Long => {
            matches!(sma_state, "above" | "breakout_up")
                && matches!(ema_state, "above" | "breakout_up")
        }
        MarketVelocityTradeDirection::Short => {
            matches!(sma_state, "below" | "breakdown_down")
                && matches!(ema_state, "below" | "breakdown_down")
        }
        MarketVelocityTradeDirection::Both => false,
    };
    if confirmed && args.trend_min_average_distance_pct > 0.0 {
        let Some(sma_distance) = moving_average_distance_pct(latest.candle.close, sma) else {
            return (false, "invalid_4h_distance".to_string());
        };
        let Some(ema_distance) = moving_average_distance_pct(latest.candle.close, ema) else {
            return (false, "invalid_4h_distance".to_string());
        };
        if sma_distance.abs() < args.trend_min_average_distance_pct
            || ema_distance.abs() < args.trend_min_average_distance_pct
        {
            return (false, "weak_4h_average_distance".to_string());
        }
    }

    (confirmed, format!("4h_{sma_state}_{ema_state}"))
}

pub fn entry_confirmation(
    candles: &[ComputedCandle],
    event_ts: i64,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> (bool, String) {
    let idx = completed_candle_count(candles, event_ts, MS_15M);
    if idx == 0 {
        return (false, "no_completed_15m".to_string());
    }

    let latest_completed_at = candles[idx - 1].candle.ts + MS_15M;
    if event_ts - latest_completed_at > args.max_15m_staleness_min * 60 * 1_000 {
        return (false, "stale_15m".to_string());
    }
    if idx <= args.entry_period {
        return (false, "insufficient_15m".to_string());
    }

    let latest = &candles[idx - 1];
    let previous = &candles[idx - 2];
    let Some(sma) = latest.sma else {
        return (false, "invalid_15m_average".to_string());
    };
    let Some(ema) = latest.ema else {
        return (false, "invalid_15m_average".to_string());
    };
    match direction {
        MarketVelocityTradeDirection::Long
            if latest.candle.close <= sma || latest.candle.close <= ema =>
        {
            return (false, "price_below_15m_average".to_string());
        }
        MarketVelocityTradeDirection::Short
            if latest.candle.close >= sma || latest.candle.close >= ema =>
        {
            return (false, "price_above_15m_average".to_string());
        }
        MarketVelocityTradeDirection::Both => {
            return (false, "invalid_trade_direction".to_string())
        }
        _ => {}
    }

    let Some(sma_distance) = moving_average_distance_pct(latest.candle.close, sma) else {
        return (false, "invalid_15m_distance".to_string());
    };
    let Some(ema_distance) = moving_average_distance_pct(latest.candle.close, ema) else {
        return (false, "invalid_15m_distance".to_string());
    };
    if args.entry_max_distance_pct > 0.0
        && (sma_distance.abs() > args.entry_max_distance_pct
            || ema_distance.abs() > args.entry_max_distance_pct)
    {
        return (false, "overextended_15m_average".to_string());
    }

    let volume_ratio = latest
        .previous_volume_avg
        .filter(|average| *average > 0.0)
        .map(|average| latest.candle.volume / average);
    if args.entry_min_volume_ratio > 0.0
        && !volume_ratio.is_some_and(|ratio| ratio >= args.entry_min_volume_ratio)
    {
        return (false, "volume_not_confirmed".to_string());
    }

    match direction {
        MarketVelocityTradeDirection::Long => {
            if previous.ema.is_some_and(|previous_ema| {
                previous.candle.close <= previous_ema && latest.candle.close > ema
            }) {
                return (true, "reclaim_ema".to_string());
            }
            if previous.sma.is_some_and(|previous_sma| {
                previous.candle.close <= previous_sma && latest.candle.close > sma
            }) {
                return (true, "reclaim_ma".to_string());
            }
            if latest.candle.close > previous.candle.high {
                return (true, "breakout_previous_high".to_string());
            }
            if latest.candle.low <= ema
                && latest.candle.close > latest.candle.open
                && latest.candle.close > ema
            {
                return (true, "pullback_hold_ema".to_string());
            }
        }
        MarketVelocityTradeDirection::Short => {
            if previous.ema.is_some_and(|previous_ema| {
                previous.candle.close >= previous_ema && latest.candle.close < ema
            }) {
                return (true, "reject_ema".to_string());
            }
            if previous.sma.is_some_and(|previous_sma| {
                previous.candle.close >= previous_sma && latest.candle.close < sma
            }) {
                return (true, "reject_ma".to_string());
            }
            if latest.candle.close < previous.candle.low {
                return (true, "breakdown_previous_low".to_string());
            }
            if latest.candle.high >= ema
                && latest.candle.close < latest.candle.open
                && latest.candle.close < ema
            {
                return (true, "pullback_reject_ema".to_string());
            }
        }
        MarketVelocityTradeDirection::Both => {}
    }

    (false, "timing_not_confirmed".to_string())
}

pub fn evaluate_events(
    events: &[RadarEvent],
    candles_4h: &HashMap<String, Vec<ComputedCandle>>,
    candles_15m: &HashMap<String, Vec<ComputedCandle>>,
    raw_candles_4h: &HashMap<String, Vec<BacktestCandle>>,
    raw_candles_1h: &HashMap<String, Vec<BacktestCandle>>,
    raw_candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    args: &MarketVelocityEventBacktestArgs,
) -> EvaluationReport {
    let mut stage_counts = BTreeMap::new();
    let mut blockers: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut confirmed = Vec::new();

    for event in events {
        increment(&mut stage_counts, "raw");
        let Some(symbol_4h) = candles_4h
            .get(&event.symbol)
            .filter(|candles| !candles.is_empty())
        else {
            increment(&mut stage_counts, "no_4h_rows");
            increment_nested(&mut blockers, &event.symbol, "no_4h_rows");
            continue;
        };
        let Some(symbol_15m) = candles_15m
            .get(&event.symbol)
            .filter(|candles| !candles.is_empty())
        else {
            increment(&mut stage_counts, "no_15m_rows");
            increment_nested(&mut blockers, &event.symbol, "no_15m_rows");
            continue;
        };

        let direction = trade_direction_for_event(event);
        let (trend_ok, trend_reason) = trend_confirmation(symbol_4h, event.ts, direction, args);
        if !trend_ok {
            increment(&mut stage_counts, "trend_blocked");
            increment_nested(&mut blockers, &event.symbol, &trend_reason);
            continue;
        }
        increment(&mut stage_counts, "trend_pass");

        match args.fvg_entry_mode {
            FvgEntryMode::Off => {
                let (entry_ok, entry_reason) =
                    entry_confirmation(symbol_15m, event.ts, direction, args);
                if !entry_ok {
                    increment(&mut stage_counts, "entry_blocked");
                    increment_nested(&mut blockers, &event.symbol, &entry_reason);
                    continue;
                }
                increment(&mut stage_counts, "entry_pass");

                let Some(entry_idx) = next_entry_candle_idx(symbol_15m, event.ts) else {
                    increment(&mut stage_counts, "no_next_entry_candle");
                    increment_nested(&mut blockers, &event.symbol, "no_next_entry_candle");
                    continue;
                };
                let entry = &symbol_15m[entry_idx].candle;
                confirmed.push(ConfirmedEvent {
                    event: event.clone(),
                    entry_ts: entry.ts,
                    entry_price: entry.open,
                    entry_idx,
                    trigger: entry_reason,
                });
            }
            fvg_mode => {
                let Some(symbol_15m_raw) = raw_candles_15m
                    .get(&event.symbol)
                    .filter(|candles| !candles.is_empty())
                else {
                    increment(&mut stage_counts, "no_15m_rows");
                    increment_nested(&mut blockers, &event.symbol, "no_15m_rows");
                    continue;
                };
                let Some(symbol_1h_raw) = raw_candles_1h
                    .get(&event.symbol)
                    .filter(|candles| !candles.is_empty())
                else {
                    increment(&mut stage_counts, "no_1h_rows");
                    increment_nested(&mut blockers, &event.symbol, "no_1h_rows");
                    continue;
                };
                let Some(symbol_4h_raw) = raw_candles_4h
                    .get(&event.symbol)
                    .filter(|candles| !candles.is_empty())
                else {
                    increment(&mut stage_counts, "no_4h_rows");
                    increment_nested(&mut blockers, &event.symbol, "no_4h_rows");
                    continue;
                };

                if direction == MarketVelocityTradeDirection::Short {
                    increment(&mut stage_counts, "entry_blocked");
                    increment_nested(&mut blockers, &event.symbol, "short_fvg_not_supported");
                    continue;
                }

                match find_fvg_entry(
                    fvg_mode,
                    symbol_4h_raw,
                    symbol_1h_raw,
                    symbol_15m_raw,
                    event.ts,
                    args,
                ) {
                    FvgEntrySearch::Found(entry) => {
                        increment(&mut stage_counts, "entry_pass");
                        confirmed.push(ConfirmedEvent {
                            event: event.clone(),
                            entry_ts: entry.entry_ts,
                            entry_price: entry.entry_price,
                            entry_idx: entry.entry_15m_idx,
                            trigger: entry.trigger,
                        });
                    }
                    FvgEntrySearch::Blocked(reason) => {
                        increment(&mut stage_counts, "entry_blocked");
                        increment_nested(&mut blockers, &event.symbol, &reason);
                    }
                }
            }
        }
    }

    EvaluationReport {
        confirmed,
        stage_counts,
        blockers,
    }
}

fn summarize_target(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    target_r: f64,
    horizon_ms: i64,
    args: &MarketVelocityEventBacktestArgs,
) -> (Vec<TradeResult>, usize) {
    let mut lock_until: HashMap<String, i64> = HashMap::new();
    let mut results = Vec::new();
    let mut skipped_lock = 0;

    for signal in confirmed {
        let symbol = &signal.event.symbol;
        if signal.event.ts <= *lock_until.get(symbol).unwrap_or(&-1) {
            skipped_lock += 1;
            continue;
        }
        let Some(candles) = candles_15m.get(symbol) else {
            continue;
        };
        let mut result = simulate_trade(
            candles,
            signal.entry_idx,
            signal.entry_ts,
            signal.entry_price,
            trade_direction_for_event(&signal.event),
            args.stop_loss_pct,
            target_r,
            horizon_ms,
            profit_protection_for_target(args, target_r),
            runner_exit_for_target(args, target_r),
            early_exit(args),
        );
        result = maybe_apply_stop_reentry(candles, signal, result, target_r, horizon_ms, args);
        result.symbol = Some(symbol.clone());
        result.event_id = Some(signal.event.id);
        result.detected_at = Some(signal.event.detected_at.clone());
        result.trigger = Some(
            match (
                result.reentry.as_ref(),
                args.stop_reentry_mode.trigger_suffix(),
            ) {
                (Some(_), Some(suffix)) => format!("{}+{}", signal.trigger, suffix),
                _ => signal.trigger.clone(),
            },
        );
        lock_until.insert(
            symbol.clone(),
            if result.complete {
                result.exit_ts
            } else {
                signal.entry_ts + horizon_ms
            },
        );
        results.push(result);
    }

    (results, skipped_lock)
}

pub fn build_market_velocity_paper_outcomes(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<MarketVelocityPaperOutcomeRequest> {
    let confirmed_by_event_id = confirmed
        .iter()
        .map(|signal| (signal.event.id, signal))
        .collect::<HashMap<_, _>>();
    let mut outcomes = Vec::new();
    let entry_trigger_filter_version = entry_trigger_filter_version(args);

    for target_r in &args.target_rs {
        for (horizon_hours, horizon_ms) in PAPER_OUTCOME_HORIZONS {
            let (results, skipped_lock) =
                summarize_target(confirmed, candles_15m, *target_r, *horizon_ms, args);
            for result in results {
                let Some(event_id) = result.event_id else {
                    continue;
                };
                let Some(signal) = confirmed_by_event_id.get(&event_id) else {
                    continue;
                };
                let symbol = result
                    .symbol
                    .clone()
                    .unwrap_or_else(|| signal.event.symbol.clone());
                let entry_trigger = result.trigger.clone();
                outcomes.push(MarketVelocityPaperOutcomeRequest {
                    rank_event_id: event_id,
                    exchange: signal.event.exchange.trim().to_ascii_lowercase(),
                    symbol,
                    target_r: *target_r,
                    horizon_hours: *horizon_hours,
                    entry_rule_version: args.paper_outcome_entry_rule_version.clone(),
                    entry_trigger: entry_trigger.clone(),
                    entry_price: result.entry_price,
                    entry_at: timestamp_ms_to_rfc3339(result.entry_ts),
                    outcome_status: result.outcome.label().to_string(),
                    exit_reason: result.reason.clone(),
                    result_r: result.r,
                    evaluated_at: timestamp_ms_to_rfc3339(result.exit_ts),
                    evaluation_payload: json!({
                        "source": "market_velocity_event_backtest",
                        "rank_event_id": event_id,
                        "detected_at": signal.event.detected_at,
                        "target_r": target_r,
                        "horizon_hours": horizon_hours,
                        "trade_direction": trade_direction_for_event(&signal.event).label(),
                        "stop_loss_pct": args.stop_loss_pct,
                        "entry_period": args.entry_period,
                        "entry_trigger": entry_trigger,
                        "entry_trigger_filter_version": entry_trigger_filter_version,
                        "trade_complete": result.complete,
                        "exit_ts": result.exit_ts,
                        "skipped_lock_count": skipped_lock,
                        "entry_rule_version": &args.paper_outcome_entry_rule_version,
                        "stop_reentry": stop_reentry_payload(&result, args),
                        "fvg_entry": fvg_entry_payload(args),
                        "profit_protection": profit_protection_payload(args),
                        "runner_exit": runner_exit_payload(args),
                        "early_exit": early_exit_payload(args),
                        "entry_filter": {
                            "entry_trigger_filter_version": entry_trigger_filter_version,
                            "entry_trigger_allowlist": &args.entry_trigger_allowlist,
                            "entry_trigger_blocklist": &args.entry_trigger_blocklist,
                            "entry_trigger_rank_blocklist": format_entry_trigger_rank_blocklist(&args.entry_trigger_rank_blocklist),
                        },
                        "filters": {
                            "min_delta_rank": args.min_delta_rank,
                            "max_delta_rank": args.max_delta_rank,
                            "max_new_rank": args.max_new_rank,
                            "min_price_change_pct": args.min_price_change_pct,
                            "tail_new_rank_threshold": args.tail_new_rank_threshold,
                            "tail_rank_min_price_change_pct": args.tail_rank_min_price_change_pct,
                            "chase_top_rank": args.chase_top_rank,
                            "chase_price_change_pct": args.chase_price_change_pct,
                            "entry_max_distance_pct": args.entry_max_distance_pct,
                            "entry_min_volume_ratio": args.entry_min_volume_ratio,
                            "trend_min_average_distance_pct": args.trend_min_average_distance_pct,
                            "max_15m_staleness_min": args.max_15m_staleness_min,
                            "max_4h_staleness_min": args.max_4h_staleness_min
                        }
                    }),
                });
            }
        }
    }

    outcomes
}

fn fvg_entry_payload(args: &MarketVelocityEventBacktestArgs) -> serde_json::Value {
    json!({
        "mode": args.fvg_entry_mode.label(),
        "lookback_candles": args.fvg_lookback_candles,
        "max_wait_candles": args.fvg_max_wait_candles,
    })
}

fn profit_protection_payload(args: &MarketVelocityEventBacktestArgs) -> serde_json::Value {
    json!({
        "enabled": args.profit_protect_after_r.is_some(),
        "activate_after_r": args.profit_protect_after_r,
        "stop_r": args.profit_protect_stop_r,
    })
}

fn runner_exit_payload(args: &MarketVelocityEventBacktestArgs) -> serde_json::Value {
    json!({
        "enabled": args.runner_target_r.is_some(),
        "target_r": args.runner_target_r,
        "fraction": args.runner_fraction,
        "stop_r": args.runner_stop_r,
    })
}

fn early_exit_payload(args: &MarketVelocityEventBacktestArgs) -> serde_json::Value {
    json!({
        "enabled": args.early_exit_no_profit_candles.is_some(),
        "no_profit_candles": args.early_exit_no_profit_candles,
    })
}

pub(crate) fn profit_protection_for_target(
    args: &MarketVelocityEventBacktestArgs,
    target_r: f64,
) -> Option<ProfitProtection> {
    let activate_after_r = args.profit_protect_after_r?;
    (activate_after_r < target_r).then_some(ProfitProtection {
        activate_after_r,
        stop_r: args.profit_protect_stop_r,
    })
}

pub(crate) fn runner_exit_for_target(
    args: &MarketVelocityEventBacktestArgs,
    target_r: f64,
) -> Option<RunnerExit> {
    let runner_target_r = args.runner_target_r?;
    (runner_target_r > target_r).then_some(RunnerExit {
        target_r: runner_target_r,
        fraction: args.runner_fraction,
        stop_r: args.runner_stop_r,
    })
}

pub(crate) fn early_exit(args: &MarketVelocityEventBacktestArgs) -> Option<EarlyExit> {
    args.early_exit_no_profit_candles
        .map(|no_profit_candles| EarlyExit { no_profit_candles })
}

fn stop_reentry_payload(
    result: &TradeResult,
    args: &MarketVelocityEventBacktestArgs,
) -> serde_json::Value {
    let Some(reentry) = &result.reentry else {
        return json!({
            "mode": args.stop_reentry_mode.label(),
            "triggered": false,
        });
    };
    json!({
        "mode": reentry.mode.label(),
        "triggered": true,
        "original_entry_ts": reentry.original_entry_ts,
        "original_entry_price": reentry.original_entry_price,
        "original_exit_ts": reentry.original_exit_ts,
        "original_reason": reentry.original_reason,
        "original_r": reentry.original_r,
        "signal_ts": reentry.signal_ts,
        "reclaim_price": reentry.reclaim_price,
        "reentry_exit_reason": reentry.reentry_exit_reason,
        "reentry_r": reentry.reentry_r,
    })
}

fn entry_trigger_filter_version(args: &MarketVelocityEventBacktestArgs) -> &'static str {
    entry_trigger_filter_version_label(
        !args.entry_trigger_allowlist.is_empty(),
        !args.entry_trigger_blocklist.is_empty(),
        !args.entry_trigger_rank_blocklist.is_empty(),
    )
}

fn print_market_velocity_paper_outcomes_jsonl(
    outcomes: &[MarketVelocityPaperOutcomeRequest],
) -> Result<()> {
    for outcome in outcomes {
        println!(
            "paper_outcome_json\t{}",
            serde_json::to_string(outcome).context("serialize market velocity paper outcome")?
        );
    }
    println!("paper_outcomes_generated={}", outcomes.len());
    Ok(())
}

async fn submit_market_velocity_paper_outcomes(
    outcomes: &[MarketVelocityPaperOutcomeRequest],
) -> Result<usize> {
    if outcomes.is_empty() {
        println!("paper_outcomes_submitted=0");
        return Ok(0);
    }

    let client = ExecutionTaskClient::new(quant_web_execution_task_config_from_env()?)?;
    let mut submitted = 0;
    for outcome in outcomes {
        let response = client
            .submit_market_velocity_paper_outcome(outcome.clone())
            .await
            .with_context(|| {
                format!(
                    "submit market velocity paper outcome rank_event_id={} target={}R horizon={}h",
                    outcome.rank_event_id, outcome.target_r, outcome.horizon_hours
                )
            })?;
        if response.generated_execution_task_count != 0 {
            bail!(
                "market velocity paper outcome endpoint generated {} execution tasks; expected observation-only",
                response.generated_execution_task_count
            );
        }
        submitted += 1;
    }
    println!("paper_outcomes_submitted={submitted}");
    Ok(submitted)
}

fn completed_candle_count(candles: &[ComputedCandle], event_ts: i64, candle_ms: i64) -> usize {
    let mut left = 0;
    let mut right = candles.len();
    while left < right {
        let mid = left + (right - left) / 2;
        if candles[mid].candle.ts + candle_ms <= event_ts {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    left
}

fn next_entry_candle_idx(candles: &[ComputedCandle], event_ts: i64) -> Option<usize> {
    let mut left = 0;
    let mut right = candles.len();
    while left < right {
        let mid = left + (right - left) / 2;
        if candles[mid].candle.ts <= event_ts {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    (left < candles.len()).then_some(left)
}

fn moving_average_state(
    close: f64,
    average: f64,
    previous_close: Option<f64>,
    previous_average: Option<f64>,
) -> &'static str {
    if let (Some(previous_close), Some(previous_average)) = (previous_close, previous_average) {
        if close > average && previous_close <= previous_average {
            return "breakout_up";
        }
        if close < average && previous_close >= previous_average {
            return "breakdown_down";
        }
    }

    if moving_average_distance_pct(close, average)
        .is_some_and(|distance_pct| distance_pct.abs() <= TOUCH_THRESHOLD_PCT)
    {
        return "touching";
    }
    if close > average {
        "above"
    } else {
        "below"
    }
}

fn moving_average_distance_pct(close: f64, average: f64) -> Option<f64> {
    if average <= 0.0 || !average.is_finite() || !close.is_finite() {
        return None;
    }
    Some((close - average) / average * 100.0)
}

fn simple_average(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut count = 0;
    let mut sum = 0.0;
    for value in values {
        if !valid_positive(value) {
            return None;
        }
        count += 1;
        sum += value;
    }
    (count > 0).then_some(sum / count as f64)
}

fn valid_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn timestamp_ms_to_rfc3339(ts: i64) -> String {
    Utc.timestamp_millis_opt(ts)
        .single()
        .unwrap_or_else(|| {
            Utc.timestamp_millis_opt(0)
                .single()
                .expect("unix epoch timestamp should be valid")
        })
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn quant_web_execution_task_config_from_env() -> Result<ExecutionTaskConfig> {
    let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
        .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
        .context("--paper-outcome-sink web requires RUST_QUAN_WEB_BASE_URL/QUANT_WEB_BASE_URL")?;
    let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .or_else(|_| std::env::var("ALPHA_EXECUTION_INTERNAL_SECRET"))
        .context(
            "--paper-outcome-sink web requires EXECUTION_EVENT_SECRET/RUST_QUAN_WEB_INTERNAL_SECRET/ALPHA_EXECUTION_INTERNAL_SECRET",
        )?;
    Ok(ExecutionTaskConfig {
        base_url,
        internal_secret,
    })
}

fn first_non_empty_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn increment(counter: &mut BTreeMap<String, usize>, key: &str) {
    *counter.entry(key.to_string()).or_default() += 1;
}

fn increment_nested(
    counters: &mut BTreeMap<String, BTreeMap<String, usize>>,
    symbol: &str,
    reason: &str,
) {
    increment(counters.entry(symbol.to_string()).or_default(), reason);
}

#[cfg(test)]
mod tests;
