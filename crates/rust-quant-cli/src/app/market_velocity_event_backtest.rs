use super::env_parse::first_non_empty_env;
use anyhow::{Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use rust_quant_domain::entities::BacktestLog;
use rust_quant_domain::traits::BacktestLogRepository;
use rust_quant_infrastructure::SqlxBacktestRepository;
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::collections::{BTreeMap, HashMap};
mod args;
mod data;
mod equity;
mod equity_stats;
mod exit;
mod fvg;
mod manifest;
mod paper_outcome;
mod paper_signal;
mod reentry;
mod report;
mod short_entry;
mod stop_loss;
#[cfg(test)]
pub(crate) use args::market_velocity_paper_observation_usage;
use args::{format_entry_trigger_filter_list, normalize_entry_trigger, normalize_symbol};
pub use args::{
    parse_cli_args_from, parse_paper_observation_args_from, parse_paper_observation_command_from,
    print_market_velocity_event_backtest_usage, print_market_velocity_paper_observation_usage,
    FvgEntryMode, MarketVelocityEventBacktestArgs, MarketVelocityEventSource,
    MarketVelocityPaperObservationCommand, MarketVelocityPaperOutcomeSink,
    MarketVelocityPaperStrategySignalSink, MarketVelocityStopLossMode,
    MarketVelocityTradeDirection, MarketVelocityTrendTimeframe, StopReentryMode,
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
use fvg::{
    find_15m_impulse_fvg_retrace_after_signal, find_15m_self_fvg_entry_after_signal,
    find_fvg_entry, FvgEntrySearch,
};
pub use manifest::{market_velocity_paper_strategy_preset_manifest, MarketVelocityPresetManifest};
pub use paper_outcome::build_market_velocity_paper_outcomes;
use paper_outcome::{
    print_market_velocity_paper_outcomes_jsonl, submit_market_velocity_paper_outcomes,
};
pub use paper_signal::build_market_velocity_paper_strategy_signal_request;
use paper_signal::submit_market_velocity_paper_strategy_signals;
use reentry::maybe_apply_stop_reentry;
use report::{
    print_effective_entry_report, print_result_report, print_stage_report,
    print_trigger_quality_report, print_trigger_variant_quality_report,
};
use short_entry::sideways_range_breakdown_candidate;
pub(crate) use stop_loss::select_stop_loss_for_confirmed_signal;
pub const MS_15M: i64 = 15 * 60 * 1_000;
pub const MS_1H: i64 = 60 * 60 * 1_000;
pub const MS_4H: i64 = 4 * 60 * 60 * 1_000;
const TOUCH_THRESHOLD_PCT: f64 = 0.3;
const FAST_MOMENTUM_RSI_PERIOD: usize = 14;
const FAST_MOMENTUM_BOLLINGER_PERIOD: usize = 20;
const FAST_MOMENTUM_BOLLINGER_STDDEV: f64 = 2.0;
const PAPER_OUTCOME_HORIZONS: &[(i32, i64)] =
    &[(24, 24 * 60 * 60 * 1_000), (48, 48 * 60 * 60 * 1_000)];
#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityEventBacktestConfig {
    /// databaseURL，用于配置运行参数。
    pub database_url: String,
    /// args，用于配置运行参数。
    pub args: MarketVelocityEventBacktestArgs,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BacktestCandle {
    /// 事件时间戳。
    pub ts: i64,
    /// 开盘价。
    pub open: f64,
    /// 最高价。
    pub high: f64,
    /// 最低价。
    pub low: f64,
    /// 收盘价。
    pub close: f64,
    /// 成交量。
    pub volume: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedCandle {
    /// K 线。
    pub candle: BacktestCandle,
    /// SMA 指标值；为空时表示未计算。
    pub sma: Option<f64>,
    /// EMA；为空时使用默认值或表示不限制。
    pub ema: Option<f64>,
    /// previous成交量平均；为空时使用默认值或表示不限制。
    pub previous_volume_avg: Option<f64>,
    /// RSI14；为空时表示样本不足或价格无效。
    pub rsi14: Option<f64>,
    /// 布林带20期中轨；为空时表示样本不足或价格无效。
    pub bollinger_middle: Option<f64>,
    /// 布林带20期上轨；为空时表示样本不足或价格无效。
    pub bollinger_upper: Option<f64>,
    /// 布林带20期下轨；为空时表示样本不足或价格无效。
    pub bollinger_lower: Option<f64>,
    /// 布林带宽度百分比；为空时表示中轨无效。
    pub bollinger_bandwidth_pct: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
struct CandlePair {
    /// 交易对或资产符号。
    symbol: String,
    /// K 线15m，用于行情、K 线或市场扫描。
    candles_15m: String,
    /// 1 小时 K 线集合；为空时表示未加载。
    candles_1h: Option<String>,
    /// K 线4h，用于行情、K 线或市场扫描。
    candles_4h: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct RadarEvent {
    /// 唯一标识。
    pub id: i64,
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 事件时间戳。
    pub ts: i64,
    /// 时间字段。
    pub detected_at: String,
    /// new排名，用于行情、K 线或市场扫描。
    pub new_rank: i32,
    /// delta排名，用于行情、K 线或市场扫描。
    pub delta_rank: i32,
    /// 价格数值。
    pub current_price: f64,
    /// 价格涨跌幅百分比。
    pub price_change_pct: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmedEvent {
    /// event，用于行情、K 线或市场扫描。
    pub event: RadarEvent,
    /// 时间戳。
    pub entry_ts: i64,
    /// 入场价格。
    pub entry_price: f64,
    /// 入场idx，用于行情、K 线或市场扫描。
    pub entry_idx: usize,
    /// trigger，用于行情、K 线或市场扫描。
    pub trigger: String,
    /// 结构止损价格；为空时表示只能回退到固定百分比止损。
    pub structure_stop_loss_price: Option<f64>,
    /// 结构止损来源；为空时表示没有结构锚点。
    pub structure_stop_loss_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityLiveEntryShellSelection {
    /// 原始 signal 在 15m 计算序列中的索引。
    pub signal_idx: usize,
    /// 原始 signal trigger。
    pub signal_trigger: String,
    /// 实际 live entry 在 15m 计算序列中的索引。
    pub entry_idx: usize,
    /// 实际 live entry 时间戳。
    pub entry_ts: i64,
    /// 实际 live entry 价格。
    pub entry_price: f64,
    /// 实际 live entry trigger。
    pub entry_trigger: String,
    /// 结构止损价格；为空时表示没有结构锚点。
    pub structure_stop_loss_price: Option<f64>,
    /// 结构止损来源；为空时表示没有结构锚点。
    pub structure_stop_loss_source: Option<String>,
}
/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
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
    /// 提供标签的集中实现，避免回测策略调用方重复处理相同细节。
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
    /// outcome，用于记录交易或执行状态。
    pub outcome: TradeOutcome,
    /// 原因说明。
    pub reason: String,
    /// 时间戳。
    pub exit_ts: i64,
    /// R 倍数；为空时表示无有效风险单位。
    pub r: Option<f64>,
    /// complete，用于记录交易或执行状态。
    pub complete: bool,
    /// 交易对或资产符号。
    pub symbol: Option<String>,
    /// event ID；为空时使用默认值或表示不限制。
    pub event_id: Option<i64>,
    /// 时间字段。
    pub detected_at: Option<String>,
    /// 时间戳。
    pub entry_ts: i64,
    /// 入场价格。
    pub entry_price: f64,
    /// 触发原因；为空时表示无触发来源。
    pub trigger: Option<String>,
    /// 再次入场标记；为空时表示非再次入场。
    pub reentry: Option<StopReentryDetails>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct StopReentryDetails {
    /// 模式。
    pub mode: StopReentryMode,
    /// 时间戳。
    pub original_entry_ts: i64,
    /// original入场价格。
    pub original_entry_price: f64,
    /// 时间戳。
    pub original_exit_ts: i64,
    /// 原因说明。
    pub original_reason: String,
    /// 原始 R 倍数；为空时表示无原始风险单位。
    pub original_r: Option<f64>,
    /// 时间戳。
    pub signal_ts: i64,
    /// 价格数值。
    pub reclaim_price: f64,
    /// 原因说明。
    pub reentry_exit_reason: String,
    /// 再次入场 R 倍数；为空时表示未再次入场。
    pub reentry_r: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BacktestDataSet {
    /// 列表数据。
    pairs: Vec<CandlePair>,
    /// 列表数据。
    candles_15m: HashMap<String, Vec<BacktestCandle>>,
    /// 列表数据。
    candles_1h: HashMap<String, Vec<BacktestCandle>>,
    /// 列表数据。
    candles_4h: HashMap<String, Vec<BacktestCandle>>,
    /// 列表数据。
    candles_15m_computed: HashMap<String, Vec<ComputedCandle>>,
    /// 列表数据。
    candles_4h_computed: HashMap<String, Vec<ComputedCandle>>,
    /// 列表数据。
    events: Vec<RadarEvent>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationReport {
    /// 列表数据。
    pub confirmed: Vec<ConfirmedEvent>,
    /// 键值扩展数据。
    pub stage_counts: BTreeMap<String, usize>,
    /// 键值扩展数据。
    pub blockers: BTreeMap<String, BTreeMap<String, usize>>,
}
/// 提供配置from环境变量andargs的集中实现，避免回测策略调用方重复处理相同细节。
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
/// 执行 回测与策略研究 主流程，并把外部依赖调用、状态推进和错误返回串起来。
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
    let trigger_filtered = filter_confirmed_events_by_entry_trigger(&symbol_filtered, &config.args);
    print_entry_trigger_filter_report(&symbol_filtered, &trigger_filtered, &config.args);
    let confirmed = filter_confirmed_events_by_symbol_cooldown(&trigger_filtered, &config.args);
    print_symbol_cooldown_filter_report(&trigger_filtered, &confirmed, &config.args);
    print_effective_entry_report(data.events.len(), &evaluation, &symbol_filtered, &confirmed);
    print_trigger_quality_report(
        "before_trigger_filter",
        &symbol_filtered,
        &data.candles_15m,
        &config.args,
    );
    print_trigger_variant_quality_report(
        "after_trigger_filter",
        &confirmed,
        &data.candles_15m,
        &config.args,
    );
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
    submit_market_velocity_paper_strategy_signals(&confirmed, &config.args).await?;
    Ok(())
}
/// 持久化 回测与策略研究 结果，保证写入路径和幂等语义集中处理。
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
/// 提供市场动量finalfund的集中实现，避免回测策略调用方重复处理相同细节。
fn market_velocity_final_fund(report: &FrameworkEquityReport) -> f64 {
    report
        .symbols
        .iter()
        .map(|symbol| symbol.final_fund)
        .sum::<f64>()
}
/// 提供市场动量K 线窗口的集中实现，避免回测策略调用方重复处理相同细节。
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
/// 提供市场动量策略detail的集中实现，避免回测策略调用方重复处理相同细节。
fn market_velocity_strategy_detail(args: &MarketVelocityEventBacktestArgs) -> serde_json::Value {
    json!({
        "source": "market_velocity_event_backtest",
        "event_source": match args.event_source {
            MarketVelocityEventSource::Episodes => "episodes",
            MarketVelocityEventSource::RawEvents => "raw_events",
            MarketVelocityEventSource::RawState => "raw_state",
            MarketVelocityEventSource::Kline15m => "kline_15m",
        },
        "trade_direction": args.trade_direction.label(),
        "entry_rule_version": &args.paper_outcome_entry_rule_version,
        "entry_period": args.entry_period,
        "entry_max_distance_pct": args.entry_max_distance_pct,
        "entry_min_volume_ratio": args.entry_min_volume_ratio,
        "entry_min_rsi": args.entry_min_rsi,
        "entry_max_rsi": args.entry_max_rsi,
        "entry_min_rsi_delta": args.entry_min_rsi_delta,
        "entry_rsi_delta_lookback_candles": args.entry_rsi_delta_lookback_candles,
        "entry_bollinger_breakout": args.entry_bollinger_breakout,
        "entry_min_bollinger_bandwidth_expansion_pct": args.entry_min_bollinger_bandwidth_expansion_pct,
        "entry_min_recent_drawdown_pct": args.entry_min_recent_drawdown_pct,
        "entry_recent_drawdown_lookback_candles": args.entry_recent_drawdown_lookback_candles,
        "entry_symbol_cooldown_candles": args.entry_symbol_cooldown_candles,
        "entry_max_signal_pullback_pct": args.entry_max_signal_pullback_pct,
        "entry_max_gap_without_retest_pct": args.entry_max_gap_without_retest_pct,
        "entry_retest_tolerance_pct": args.entry_retest_tolerance_pct,
        "entry_retest_after_signal": args.entry_retest_after_signal,
        "entry_retest_max_wait_candles": args.entry_retest_max_wait_candles,
        "entry_retest_min_entry_open_gap_pct": args.entry_retest_min_entry_open_gap_pct,
        "entry_retest_open_fade_min_volume_ratio": args.entry_retest_open_fade_min_volume_ratio,
        "fvg_impulse_retrace_fill_pct": args.fvg_impulse_retrace_fill_pct,
        "fvg_impulse_retrace_min_wait_candles": args.fvg_impulse_retrace_min_wait_candles,
        "trend_timeframe": args.trend_timeframe.label(),
        "trend_min_average_distance_pct": args.trend_min_average_distance_pct,
        "min_delta_rank": args.min_delta_rank,
        "max_delta_rank": args.max_delta_rank,
        "min_price_change_pct": args.min_price_change_pct,
        "entry_trigger_allowlist": &args.entry_trigger_allowlist,
        "entry_trigger_blocklist": &args.entry_trigger_blocklist,
        "symbol_blocklist": &args.symbol_blocklist,
    })
}
/// 提供市场动量风控配置detail的集中实现，避免回测策略调用方重复处理相同细节。
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
/// 解析过滤confirmed事件by入场触发，把外部输入转换成回测策略可用的内部值。
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
/// 解析过滤confirmed事件by交易对，把外部输入转换成回测策略可用的内部值。
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
/// 按同交易对冷却窗口过滤已确认信号，用于研究连续追同一币对的集中度风险。
pub fn filter_confirmed_events_by_symbol_cooldown(
    confirmed: &[ConfirmedEvent],
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<ConfirmedEvent> {
    let Some(cooldown_candles) = args.entry_symbol_cooldown_candles else {
        return confirmed.to_vec();
    };
    let cooldown_ms = MS_15M.saturating_mul(cooldown_candles as i64);
    let mut last_kept_entry_ts_by_symbol = HashMap::new();
    let mut filtered = Vec::with_capacity(confirmed.len());
    for event in confirmed {
        let symbol = normalize_symbol(&event.event.symbol);
        let keep = last_kept_entry_ts_by_symbol
            .get(&symbol)
            .is_none_or(|last_entry_ts| {
                event.entry_ts.saturating_sub(*last_entry_ts) >= cooldown_ms
            });
        if keep {
            last_kept_entry_ts_by_symbol.insert(symbol, event.entry_ts);
            filtered.push(event.clone());
        }
    }
    filtered
}
/// 提供交易对allowed的集中实现，避免回测策略调用方重复处理相同细节。
fn symbol_allowed(symbol: &str, args: &MarketVelocityEventBacktestArgs) -> bool {
    let normalized = normalize_symbol(symbol);
    !args
        .symbol_blocklist
        .iter()
        .any(|blocked| normalize_symbol(blocked) == normalized)
}
/// 提供入场触发allowed的集中实现，避免回测策略调用方重复处理相同细节。
fn entry_trigger_allowed(event: &ConfirmedEvent, args: &MarketVelocityEventBacktestArgs) -> bool {
    let normalized = base_entry_trigger(&event.trigger);
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
    true
}
fn base_entry_trigger(trigger: &str) -> String {
    normalize_entry_trigger(trigger)
        .split_once('+')
        .map_or_else(
            || normalize_entry_trigger(trigger),
            |(base, _)| base.to_string(),
        )
}
/// 执行输出交易对过滤报告步骤，串起回测策略需要的状态推进和错误处理。
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
/// 执行输出入场触发过滤报告步骤，串起回测策略需要的状态推进和错误处理。
fn print_entry_trigger_filter_report(
    before: &[ConfirmedEvent],
    after: &[ConfirmedEvent],
    args: &MarketVelocityEventBacktestArgs,
) {
    if args.entry_trigger_allowlist.is_empty() && args.entry_trigger_blocklist.is_empty() {
        return;
    }
    println!(
        "entry_trigger_filter\tbefore={}\tafter={}\tallowlist={}\tblocklist={}",
        before.len(),
        after.len(),
        format_entry_trigger_filter_list(&args.entry_trigger_allowlist),
        format_entry_trigger_filter_list(&args.entry_trigger_blocklist)
    );
}
/// 输出同交易对冷却窗口的过滤效果，便于判断 15m 快进快出是否过度追同一币种。
fn print_symbol_cooldown_filter_report(
    before: &[ConfirmedEvent],
    after: &[ConfirmedEvent],
    args: &MarketVelocityEventBacktestArgs,
) {
    let Some(cooldown_candles) = args.entry_symbol_cooldown_candles else {
        return;
    };
    println!(
        "symbol_cooldown_filter\tbefore={}\tafter={}\tcooldown_candles={}",
        before.len(),
        after.len(),
        cooldown_candles
    );
}
/// 构建 回测与策略研究 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_computed_candles(candles: Vec<BacktestCandle>, period: usize) -> Vec<ComputedCandle> {
    let mut computed = Vec::with_capacity(candles.len());
    let mut ema: Option<f64> = None;
    let mut rsi_average_gain_loss: Option<(f64, f64)> = None;
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
        let rsi14 = rsi_average_gain_loss_at(&candles, i, &mut rsi_average_gain_loss).and_then(
            |(average_gain, average_loss)| rsi_from_average_gain_loss(average_gain, average_loss),
        );
        let (bollinger_middle, bollinger_upper, bollinger_lower, bollinger_bandwidth_pct) =
            bollinger_bands_at(&candles, i)
                .map(|bands| (Some(bands.0), Some(bands.1), Some(bands.2), bands.3))
                .unwrap_or((None, None, None, None));
        computed.push(ComputedCandle {
            candle: candles[i].clone(),
            sma,
            ema,
            previous_volume_avg,
            rsi14,
            bollinger_middle,
            bollinger_upper,
            bollinger_lower,
            bollinger_bandwidth_pct,
        });
    }
    computed
}
/// 按 Wilder RSI 的平滑方式维护 RSI14 的平均涨跌幅，避免每根 K 线重复扫描历史。
fn rsi_average_gain_loss_at(
    candles: &[BacktestCandle],
    idx: usize,
    previous_average: &mut Option<(f64, f64)>,
) -> Option<(f64, f64)> {
    if idx < FAST_MOMENTUM_RSI_PERIOD {
        return None;
    }
    if idx == FAST_MOMENTUM_RSI_PERIOD {
        let mut gain_sum = 0.0;
        let mut loss_sum = 0.0;
        for window_idx in 1..=FAST_MOMENTUM_RSI_PERIOD {
            let delta = candles[window_idx].close - candles[window_idx - 1].close;
            if !delta.is_finite() {
                return None;
            }
            if delta >= 0.0 {
                gain_sum += delta;
            } else {
                loss_sum += delta.abs();
            }
        }
        let average = (
            gain_sum / FAST_MOMENTUM_RSI_PERIOD as f64,
            loss_sum / FAST_MOMENTUM_RSI_PERIOD as f64,
        );
        *previous_average = Some(average);
        return Some(average);
    }
    let (previous_gain, previous_loss) = (*previous_average)?;
    let delta = candles[idx].close - candles[idx - 1].close;
    if !delta.is_finite() {
        *previous_average = None;
        return None;
    }
    let gain = delta.max(0.0);
    let loss = (-delta).max(0.0);
    let period = FAST_MOMENTUM_RSI_PERIOD as f64;
    let average = (
        (previous_gain * (period - 1.0) + gain) / period,
        (previous_loss * (period - 1.0) + loss) / period,
    );
    *previous_average = Some(average);
    Some(average)
}
/// 将 RSI 的平均涨跌幅转换为 0-100 分值，并处理单边上涨或无波动样本。
fn rsi_from_average_gain_loss(average_gain: f64, average_loss: f64) -> Option<f64> {
    if !average_gain.is_finite()
        || !average_loss.is_finite()
        || average_gain < 0.0
        || average_loss < 0.0
    {
        return None;
    }
    if average_loss == 0.0 {
        return Some(if average_gain == 0.0 { 50.0 } else { 100.0 });
    }
    let relative_strength = average_gain / average_loss;
    Some(100.0 - 100.0 / (1.0 + relative_strength))
}
/// 计算 20 期布林带和带宽，用于 15m 内生突破过滤而不是依赖高周期均线。
fn bollinger_bands_at(
    candles: &[BacktestCandle],
    idx: usize,
) -> Option<(f64, f64, f64, Option<f64>)> {
    if idx + 1 < FAST_MOMENTUM_BOLLINGER_PERIOD {
        return None;
    }
    let start = idx + 1 - FAST_MOMENTUM_BOLLINGER_PERIOD;
    let closes = candles[start..=idx]
        .iter()
        .map(|candle| candle.close)
        .collect::<Vec<_>>();
    let middle = simple_average(closes.iter().copied())?;
    let variance = closes
        .iter()
        .map(|close| (close - middle).powi(2))
        .sum::<f64>()
        / FAST_MOMENTUM_BOLLINGER_PERIOD as f64;
    if !variance.is_finite() || variance < 0.0 {
        return None;
    }
    let deviation = variance.sqrt() * FAST_MOMENTUM_BOLLINGER_STDDEV;
    let upper = middle + deviation;
    let lower = middle - deviation;
    let bandwidth = valid_positive(middle).then_some((upper - lower) / middle * 100.0);
    Some((middle, upper, lower, bandwidth))
}
/// 提供趋势确认的集中实现，避免回测策略调用方重复处理相同细节。
pub fn trend_confirmation(
    candles: &[ComputedCandle],
    event_ts: i64,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> (bool, String) {
    trend_confirmation_for_timeframe(
        candles,
        event_ts,
        direction,
        args,
        MS_4H,
        "4h",
        args.max_4h_staleness_min,
    )
}
/// 复用原 4H 趋势确认逻辑，以显式周期参数支持 1H 对照但不改变默认 4H 行为。
fn trend_confirmation_for_timeframe(
    candles: &[ComputedCandle],
    event_ts: i64,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
    candle_ms: i64,
    timeframe_label: &str,
    max_staleness_min: i64,
) -> (bool, String) {
    let idx = completed_candle_count(candles, event_ts, candle_ms);
    if idx == 0 {
        return (false, format!("no_completed_{timeframe_label}"));
    }
    let latest_completed_at = candles[idx - 1].candle.ts + candle_ms;
    if event_ts - latest_completed_at > max_staleness_min * 60 * 1_000 {
        return (false, format!("stale_{timeframe_label}"));
    }
    if idx < args.entry_period {
        return (false, format!("insufficient_{timeframe_label}"));
    }
    let latest = &candles[idx - 1];
    let Some(sma) = latest.sma else {
        return (false, format!("invalid_{timeframe_label}_average"));
    };
    let Some(ema) = latest.ema else {
        return (false, format!("invalid_{timeframe_label}_average"));
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
            return (false, format!("invalid_{timeframe_label}_distance"));
        };
        let Some(ema_distance) = moving_average_distance_pct(latest.candle.close, ema) else {
            return (false, format!("invalid_{timeframe_label}_distance"));
        };
        if sma_distance.abs() < args.trend_min_average_distance_pct
            || ema_distance.abs() < args.trend_min_average_distance_pct
        {
            return (false, format!("weak_{timeframe_label}_average_distance"));
        }
    }
    (
        confirmed,
        format!("{timeframe_label}_{sma_state}_{ema_state}"),
    )
}
/// 提供入场确认的集中实现，避免回测策略调用方重复处理相同细节。
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
    let volume_ratio = latest
        .previous_volume_avg
        .filter(|average| *average > 0.0)
        .map(|average| latest.candle.volume / average);
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
    let reclaim_ema_candidate = matches!(direction, MarketVelocityTradeDirection::Long)
        && previous.ema.is_some_and(|previous_ema| {
            previous.candle.close <= previous_ema && latest.candle.close > ema
        });
    let reclaim_ma_candidate = matches!(direction, MarketVelocityTradeDirection::Long)
        && previous.sma.is_some_and(|previous_sma| {
            previous.candle.close <= previous_sma && latest.candle.close > sma
        });
    let breakout_previous_high_candidate = matches!(direction, MarketVelocityTradeDirection::Long)
        && latest.candle.close > previous.candle.high;
    let pullback_hold_ema_candidate = matches!(direction, MarketVelocityTradeDirection::Long)
        && latest.candle.low <= ema
        && latest.candle.close > latest.candle.open
        && latest.candle.close > ema;
    let reject_ema_candidate = matches!(direction, MarketVelocityTradeDirection::Short)
        && previous.ema.is_some_and(|previous_ema| {
            previous.candle.close >= previous_ema && latest.candle.close < ema
        });
    let reject_ma_candidate = matches!(direction, MarketVelocityTradeDirection::Short)
        && previous.sma.is_some_and(|previous_sma| {
            previous.candle.close >= previous_sma && latest.candle.close < sma
        });
    let breakdown_previous_low_candidate = matches!(direction, MarketVelocityTradeDirection::Short)
        && latest.candle.close < previous.candle.low;
    let pullback_reject_ema_candidate = matches!(direction, MarketVelocityTradeDirection::Short)
        && latest.candle.high >= ema
        && latest.candle.close < latest.candle.open
        && latest.candle.close < ema;
    let breakdown_range_low_candidate = matches!(direction, MarketVelocityTradeDirection::Short)
        && sideways_range_breakdown_candidate(candles, idx - 1);
    if args.entry_min_volume_ratio > 0.0
        && !volume_ratio.is_some_and(|ratio| ratio >= args.entry_min_volume_ratio)
    {
        return (false, "volume_not_confirmed".to_string());
    }
    if let Some(reason) = fast_momentum_entry_filter_reason(candles, idx, direction, args) {
        return (false, reason.to_string());
    }
    match direction {
        MarketVelocityTradeDirection::Long => {
            if reclaim_ema_candidate {
                return (true, "reclaim_ema".to_string());
            }
            if reclaim_ma_candidate {
                return (true, "reclaim_ma".to_string());
            }
            if breakout_previous_high_candidate {
                return (true, "breakout_previous_high".to_string());
            }
            if pullback_hold_ema_candidate {
                return (true, "pullback_hold_ema".to_string());
            }
        }
        MarketVelocityTradeDirection::Short => {
            if breakdown_range_low_candidate {
                return (true, "breakdown_range_low".to_string());
            }
            if reject_ema_candidate {
                return (true, "reject_ema".to_string());
            }
            if reject_ma_candidate {
                return (true, "reject_ma".to_string());
            }
            if breakdown_previous_low_candidate {
                return (true, "breakdown_previous_low".to_string());
            }
            if pullback_reject_ema_candidate {
                return (true, "pullback_reject_ema".to_string());
            }
        }
        MarketVelocityTradeDirection::Both => {}
    }
    (false, "timing_not_confirmed".to_string())
}

/// 聚合 15m 快动量研究过滤：RSI、布林突破和入场前跌幅均只在显式配置时启用。
fn fast_momentum_entry_filter_reason(
    candles: &[ComputedCandle],
    completed_count: usize,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<&'static str> {
    let latest_idx = completed_count.checked_sub(1)?;
    let latest = candles.get(latest_idx)?;
    if args.entry_min_rsi.is_some()
        || args.entry_max_rsi.is_some()
        || args.entry_min_rsi_delta.is_some()
    {
        let Some(latest_rsi) = latest.rsi14 else {
            return Some("rsi_not_ready");
        };
        if args
            .entry_min_rsi
            .is_some_and(|min_rsi| latest_rsi < min_rsi)
        {
            return Some("rsi_below_min");
        }
        if args
            .entry_max_rsi
            .is_some_and(|max_rsi| latest_rsi > max_rsi)
        {
            return Some("rsi_above_max");
        }
        if let Some(min_delta) = args.entry_min_rsi_delta {
            let Some(previous_idx) = latest_idx.checked_sub(args.entry_rsi_delta_lookback_candles)
            else {
                return Some("rsi_delta_not_ready");
            };
            let Some(previous_rsi) = candles.get(previous_idx).and_then(|candle| candle.rsi14)
            else {
                return Some("rsi_delta_not_ready");
            };
            if latest_rsi - previous_rsi < min_delta {
                return Some("rsi_delta_not_confirmed");
            }
        }
    }
    if args.entry_bollinger_breakout {
        let breakout_ok = match direction {
            MarketVelocityTradeDirection::Long => latest
                .bollinger_upper
                .is_some_and(|upper| latest.candle.close > upper),
            MarketVelocityTradeDirection::Short => latest
                .bollinger_lower
                .is_some_and(|lower| latest.candle.close < lower),
            MarketVelocityTradeDirection::Both => false,
        };
        if !breakout_ok {
            return Some("bollinger_breakout_not_confirmed");
        }
    }
    if let Some(min_expansion_pct) = args.entry_min_bollinger_bandwidth_expansion_pct {
        let Some(previous_idx) = latest_idx.checked_sub(1) else {
            return Some("bollinger_bandwidth_not_ready");
        };
        let Some(latest_bandwidth) = latest.bollinger_bandwidth_pct else {
            return Some("bollinger_bandwidth_not_ready");
        };
        let Some(previous_bandwidth) = candles
            .get(previous_idx)
            .and_then(|candle| candle.bollinger_bandwidth_pct)
        else {
            return Some("bollinger_bandwidth_not_ready");
        };
        if !valid_positive(previous_bandwidth) {
            return Some("bollinger_bandwidth_not_ready");
        }
        let expansion_pct = (latest_bandwidth - previous_bandwidth) / previous_bandwidth * 100.0;
        if expansion_pct < min_expansion_pct {
            return Some("bollinger_bandwidth_expansion_not_confirmed");
        }
    }
    if let Some(min_drawdown_pct) = args.entry_min_recent_drawdown_pct {
        let Some(drawdown_pct) = recent_entry_drawdown_pct(
            candles,
            latest_idx,
            args.entry_recent_drawdown_lookback_candles,
        ) else {
            return Some("recent_drawdown_not_ready");
        };
        if drawdown_pct < min_drawdown_pct {
            return Some("recent_drawdown_not_confirmed");
        }
    }
    None
}

/// 计算当前突破 K 线之前的回看跌幅，避免把已经连续拉升的末端动量当作首轮机会。
fn recent_entry_drawdown_pct(
    candles: &[ComputedCandle],
    latest_idx: usize,
    lookback_candles: usize,
) -> Option<f64> {
    let start = latest_idx.checked_sub(lookback_candles)?;
    let history = candles.get(start..latest_idx)?;
    if history.is_empty() {
        return None;
    }
    let mut highest_high = f64::NEG_INFINITY;
    let mut lowest_low = f64::INFINITY;
    for candle in history {
        if !candle.candle.high.is_finite() || !candle.candle.low.is_finite() {
            return None;
        }
        highest_high = highest_high.max(candle.candle.high);
        lowest_low = lowest_low.min(candle.candle.low);
    }
    valid_positive(highest_high).then_some((highest_high - lowest_low) / highest_high * 100.0)
}

fn entry_signal_pullback_block_reason(
    event: &RadarEvent,
    entry_price: f64,
    direction: MarketVelocityTradeDirection,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<String> {
    let max_pullback_pct = args.entry_max_signal_pullback_pct?;
    if event.current_price <= 0.0 || entry_price <= 0.0 {
        return None;
    }
    let pullback_pct = match direction {
        MarketVelocityTradeDirection::Long if entry_price < event.current_price => {
            (event.current_price - entry_price) / event.current_price * 100.0
        }
        MarketVelocityTradeDirection::Short if entry_price > event.current_price => {
            (entry_price - event.current_price) / event.current_price * 100.0
        }
        _ => 0.0,
    };
    (pullback_pct > max_pullback_pct).then(|| "entry_signal_pullback_too_deep".to_string())
}

pub fn select_live_entry_from_signal_shell(
    event_ts: i64,
    current_price: f64,
    candles_15m: &[BacktestCandle],
    args: &MarketVelocityEventBacktestArgs,
) -> Result<MarketVelocityLiveEntryShellSelection, String> {
    let computed = build_computed_candles(candles_15m.to_vec(), args.entry_period);
    let direction = MarketVelocityTradeDirection::Long;
    let (entry_ok, signal_trigger) = entry_confirmation(&computed, event_ts, direction, args);
    if !entry_ok {
        return Err(signal_trigger);
    }
    let signal_idx = completed_candle_count(&computed, event_ts, MS_15M)
        .checked_sub(1)
        .ok_or_else(|| "no_completed_15m".to_string())?;
    let event = RadarEvent {
        id: 0,
        exchange: "okx".to_string(),
        symbol: String::new(),
        ts: event_ts,
        detected_at: String::new(),
        new_rank: 0,
        delta_rank: 0,
        current_price,
        price_change_pct: 0.0,
    };
    let finalize = |entry_idx: usize,
                    entry_ts: i64,
                    entry_price: f64,
                    entry_trigger: String,
                    structure_stop_loss_price: Option<f64>,
                    structure_stop_loss_source: Option<String>|
     -> Result<MarketVelocityLiveEntryShellSelection, String> {
        if let Some(reason) =
            entry_signal_pullback_block_reason(&event, entry_price, direction, args)
        {
            return Err(reason);
        }
        Ok(MarketVelocityLiveEntryShellSelection {
            signal_idx,
            signal_trigger: signal_trigger.clone(),
            entry_idx,
            entry_ts,
            entry_price,
            entry_trigger,
            structure_stop_loss_price,
            structure_stop_loss_source,
        })
    };
    match args.fvg_entry_mode {
        FvgEntryMode::M15ImpulseRetrace => {
            match find_15m_impulse_fvg_retrace_after_signal(
                candles_15m,
                &computed,
                event_ts,
                &signal_trigger,
                args,
            ) {
                FvgEntrySearch::Found(entry) => finalize(
                    entry.entry_15m_idx,
                    entry.entry_ts,
                    entry.entry_price,
                    entry.trigger,
                    entry.structure_stop_loss_price,
                    entry.structure_stop_loss_source,
                ),
                FvgEntrySearch::Blocked(reason) if args.entry_retest_after_signal => {
                    let fallback = find_retest_entry_after_signal(
                        &computed,
                        signal_idx,
                        direction,
                        &signal_trigger,
                        args,
                    )
                    .map_err(|fallback_reason| format!("{reason}_then_{fallback_reason}"))?;
                    finalize(
                        fallback.entry_idx,
                        fallback.entry_ts,
                        fallback.entry_price,
                        format!("{}+fvg_fallback", fallback.trigger),
                        fallback.structure_stop_loss_price,
                        fallback.structure_stop_loss_source,
                    )
                }
                FvgEntrySearch::Blocked(reason) => Err(reason),
            }
        }
        FvgEntryMode::Off if args.entry_retest_after_signal => {
            let fallback = find_retest_entry_after_signal(
                &computed,
                signal_idx,
                direction,
                &signal_trigger,
                args,
            )?;
            finalize(
                fallback.entry_idx,
                fallback.entry_ts,
                fallback.entry_price,
                fallback.trigger,
                fallback.structure_stop_loss_price,
                fallback.structure_stop_loss_source,
            )
        }
        _ => Err("live_signal_shell_requires_hybrid_retest_mode".to_string()),
    }
}
/// 封装评估events，减少回测策略调用方重复实现相同细节。
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
    let candles_1h_computed: HashMap<String, Vec<ComputedCandle>> =
        if args.trend_timeframe == MarketVelocityTrendTimeframe::OneHour {
            raw_candles_1h
                .iter()
                .filter(|(_, candles)| !candles.is_empty())
                .map(|(symbol, candles)| {
                    (
                        symbol.clone(),
                        build_computed_candles(candles.clone(), args.entry_period),
                    )
                })
                .collect()
        } else {
            HashMap::new()
        };
    for event in events {
        increment(&mut stage_counts, "raw");
        let Some(symbol_15m) = candles_15m
            .get(&event.symbol)
            .filter(|candles| !candles.is_empty())
        else {
            increment(&mut stage_counts, "no_15m_rows");
            increment_nested(&mut blockers, &event.symbol, "no_15m_rows");
            continue;
        };
        let direction = trade_direction_for_event(event);
        let (trend_ok, trend_reason) = match args.trend_timeframe {
            MarketVelocityTrendTimeframe::FourHour => {
                let Some(symbol_4h) = candles_4h
                    .get(&event.symbol)
                    .filter(|candles| !candles.is_empty())
                else {
                    increment(&mut stage_counts, "no_4h_rows");
                    increment_nested(&mut blockers, &event.symbol, "no_4h_rows");
                    continue;
                };
                trend_confirmation(symbol_4h, event.ts, direction, args)
            }
            MarketVelocityTrendTimeframe::OneHour => {
                let Some(symbol_1h) = candles_1h_computed
                    .get(&event.symbol)
                    .filter(|candles| !candles.is_empty())
                else {
                    increment(&mut stage_counts, "no_1h_rows");
                    increment_nested(&mut blockers, &event.symbol, "no_1h_rows");
                    continue;
                };
                trend_confirmation_for_timeframe(
                    symbol_1h, event.ts, direction, args, MS_1H, "1h", 60,
                )
            }
            MarketVelocityTrendTimeframe::Off => (true, "trend_off".to_string()),
        };
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
                    increment(&mut stage_counts, "entry_signal_blocked");
                    increment_nested(&mut blockers, &event.symbol, &entry_reason);
                    continue;
                }
                increment(&mut stage_counts, "entry_signal_pass");
                let signal_idx = completed_candle_count(symbol_15m, event.ts, MS_15M) - 1;
                if args.entry_retest_after_signal {
                    match find_retest_entry_after_signal(
                        symbol_15m,
                        signal_idx,
                        direction,
                        &entry_reason,
                        args,
                    ) {
                        Ok(entry) => {
                            if let Some(reason) = entry_signal_pullback_block_reason(
                                event,
                                entry.entry_price,
                                direction,
                                args,
                            ) {
                                increment(&mut stage_counts, "entry_blocked");
                                increment(&mut stage_counts, "entry_execution_blocked");
                                increment_nested(&mut blockers, &event.symbol, &reason);
                                continue;
                            }
                            increment(&mut stage_counts, "entry_pass");
                            increment(&mut stage_counts, "entry_execution_pass");
                            confirmed.push(ConfirmedEvent {
                                event: event.clone(),
                                entry_ts: entry.entry_ts,
                                entry_price: entry.entry_price,
                                entry_idx: entry.entry_idx,
                                trigger: entry.trigger,
                                structure_stop_loss_price: entry.structure_stop_loss_price,
                                structure_stop_loss_source: entry.structure_stop_loss_source,
                            });
                        }
                        Err(reason) => {
                            increment(&mut stage_counts, "entry_blocked");
                            increment(&mut stage_counts, "entry_execution_blocked");
                            increment_nested(&mut blockers, &event.symbol, &reason);
                        }
                    }
                    continue;
                }
                let Some(entry_idx) = next_entry_candle_idx(symbol_15m, event.ts) else {
                    increment(&mut stage_counts, "no_next_entry_candle");
                    increment(&mut stage_counts, "entry_execution_blocked");
                    increment_nested(&mut blockers, &event.symbol, "no_next_entry_candle");
                    continue;
                };
                if let Some(reason) =
                    entry_gap_without_retest_block_reason(symbol_15m, signal_idx, entry_idx, args)
                {
                    increment(&mut stage_counts, "entry_blocked");
                    increment(&mut stage_counts, "entry_execution_blocked");
                    increment_nested(&mut blockers, &event.symbol, &reason);
                    continue;
                }
                let entry = &symbol_15m[entry_idx].candle;
                if let Some(reason) =
                    entry_signal_pullback_block_reason(event, entry.open, direction, args)
                {
                    increment(&mut stage_counts, "entry_blocked");
                    increment(&mut stage_counts, "entry_execution_blocked");
                    increment_nested(&mut blockers, &event.symbol, &reason);
                    continue;
                }
                increment(&mut stage_counts, "entry_pass");
                increment(&mut stage_counts, "entry_execution_pass");
                confirmed.push(ConfirmedEvent {
                    event: event.clone(),
                    entry_ts: entry.ts,
                    entry_price: entry.open,
                    entry_idx,
                    trigger: entry_reason,
                    structure_stop_loss_price: None,
                    structure_stop_loss_source: None,
                });
            }
            FvgEntryMode::M15SelfAfterSignal => {
                let (entry_ok, entry_reason) =
                    entry_confirmation(symbol_15m, event.ts, direction, args);
                if !entry_ok {
                    increment(&mut stage_counts, "entry_blocked");
                    increment(&mut stage_counts, "entry_signal_blocked");
                    increment_nested(&mut blockers, &event.symbol, &entry_reason);
                    continue;
                }
                increment(&mut stage_counts, "entry_signal_pass");
                let Some(symbol_15m_raw) = raw_candles_15m
                    .get(&event.symbol)
                    .filter(|candles| !candles.is_empty())
                else {
                    increment(&mut stage_counts, "no_15m_rows");
                    increment_nested(&mut blockers, &event.symbol, "no_15m_rows");
                    continue;
                };
                if direction == MarketVelocityTradeDirection::Short {
                    increment(&mut stage_counts, "entry_blocked");
                    increment_nested(&mut blockers, &event.symbol, "short_fvg_not_supported");
                    continue;
                }
                match find_15m_self_fvg_entry_after_signal(
                    symbol_15m_raw,
                    event.ts,
                    &entry_reason,
                    args,
                ) {
                    FvgEntrySearch::Found(entry) => {
                        if let Some(reason) = entry_signal_pullback_block_reason(
                            event,
                            entry.entry_price,
                            direction,
                            args,
                        ) {
                            increment(&mut stage_counts, "entry_blocked");
                            increment(&mut stage_counts, "entry_execution_blocked");
                            increment_nested(&mut blockers, &event.symbol, &reason);
                            continue;
                        }
                        increment(&mut stage_counts, "entry_pass");
                        increment(&mut stage_counts, "entry_execution_pass");
                        confirmed.push(ConfirmedEvent {
                            event: event.clone(),
                            entry_ts: entry.entry_ts,
                            entry_price: entry.entry_price,
                            entry_idx: entry.entry_15m_idx,
                            trigger: entry.trigger,
                            structure_stop_loss_price: entry.structure_stop_loss_price,
                            structure_stop_loss_source: entry.structure_stop_loss_source,
                        });
                    }
                    FvgEntrySearch::Blocked(reason) => {
                        increment(&mut stage_counts, "entry_blocked");
                        increment(&mut stage_counts, "entry_execution_blocked");
                        increment_nested(&mut blockers, &event.symbol, &reason);
                    }
                }
            }
            FvgEntryMode::M15ImpulseRetrace => {
                let (entry_ok, entry_reason) =
                    entry_confirmation(symbol_15m, event.ts, direction, args);
                if !entry_ok {
                    increment(&mut stage_counts, "entry_blocked");
                    increment(&mut stage_counts, "entry_signal_blocked");
                    increment_nested(&mut blockers, &event.symbol, &entry_reason);
                    continue;
                }
                increment(&mut stage_counts, "entry_signal_pass");
                let signal_idx = completed_candle_count(symbol_15m, event.ts, MS_15M) - 1;
                let Some(symbol_15m_raw) = raw_candles_15m
                    .get(&event.symbol)
                    .filter(|candles| !candles.is_empty())
                else {
                    increment(&mut stage_counts, "no_15m_rows");
                    increment_nested(&mut blockers, &event.symbol, "no_15m_rows");
                    continue;
                };
                if direction == MarketVelocityTradeDirection::Short {
                    increment(&mut stage_counts, "entry_blocked");
                    increment_nested(&mut blockers, &event.symbol, "short_fvg_not_supported");
                    continue;
                }
                match find_15m_impulse_fvg_retrace_after_signal(
                    symbol_15m_raw,
                    symbol_15m,
                    event.ts,
                    &entry_reason,
                    args,
                ) {
                    FvgEntrySearch::Found(entry) => {
                        if let Some(reason) = entry_signal_pullback_block_reason(
                            event,
                            entry.entry_price,
                            direction,
                            args,
                        ) {
                            increment(&mut stage_counts, "entry_blocked");
                            increment(&mut stage_counts, "entry_execution_blocked");
                            increment_nested(&mut blockers, &event.symbol, &reason);
                            continue;
                        }
                        increment(&mut stage_counts, "entry_pass");
                        increment(&mut stage_counts, "entry_execution_pass");
                        confirmed.push(ConfirmedEvent {
                            event: event.clone(),
                            entry_ts: entry.entry_ts,
                            entry_price: entry.entry_price,
                            entry_idx: entry.entry_15m_idx,
                            trigger: entry.trigger,
                            structure_stop_loss_price: entry.structure_stop_loss_price,
                            structure_stop_loss_source: entry.structure_stop_loss_source,
                        });
                    }
                    FvgEntrySearch::Blocked(reason) => {
                        if args.entry_retest_after_signal {
                            match find_retest_entry_after_signal(
                                symbol_15m,
                                signal_idx,
                                direction,
                                &entry_reason,
                                args,
                            ) {
                                Ok(entry) => {
                                    if let Some(reason) = entry_signal_pullback_block_reason(
                                        event,
                                        entry.entry_price,
                                        direction,
                                        args,
                                    ) {
                                        increment(&mut stage_counts, "entry_blocked");
                                        increment(&mut stage_counts, "entry_execution_blocked");
                                        increment_nested(&mut blockers, &event.symbol, &reason);
                                        continue;
                                    }
                                    increment(&mut stage_counts, "entry_pass");
                                    increment(&mut stage_counts, "entry_execution_pass");
                                    confirmed.push(ConfirmedEvent {
                                        event: event.clone(),
                                        entry_ts: entry.entry_ts,
                                        entry_price: entry.entry_price,
                                        entry_idx: entry.entry_idx,
                                        trigger: format!("{}+fvg_fallback", entry.trigger),
                                        structure_stop_loss_price: entry.structure_stop_loss_price,
                                        structure_stop_loss_source: entry
                                            .structure_stop_loss_source,
                                    });
                                }
                                Err(fallback_reason) => {
                                    increment(&mut stage_counts, "entry_blocked");
                                    increment(&mut stage_counts, "entry_execution_blocked");
                                    increment_nested(
                                        &mut blockers,
                                        &event.symbol,
                                        &format!("{reason}_then_{fallback_reason}"),
                                    );
                                }
                            }
                        } else {
                            increment(&mut stage_counts, "entry_blocked");
                            increment(&mut stage_counts, "entry_execution_blocked");
                            increment_nested(&mut blockers, &event.symbol, &reason);
                        }
                    }
                }
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
                        if let Some(reason) = entry_signal_pullback_block_reason(
                            event,
                            entry.entry_price,
                            direction,
                            args,
                        ) {
                            increment(&mut stage_counts, "entry_blocked");
                            increment(&mut stage_counts, "entry_execution_blocked");
                            increment_nested(&mut blockers, &event.symbol, &reason);
                            continue;
                        }
                        increment(&mut stage_counts, "entry_pass");
                        increment(&mut stage_counts, "entry_execution_pass");
                        confirmed.push(ConfirmedEvent {
                            event: event.clone(),
                            entry_ts: entry.entry_ts,
                            entry_price: entry.entry_price,
                            entry_idx: entry.entry_15m_idx,
                            trigger: entry.trigger,
                            structure_stop_loss_price: entry.structure_stop_loss_price,
                            structure_stop_loss_source: entry.structure_stop_loss_source,
                        });
                    }
                    FvgEntrySearch::Blocked(reason) => {
                        increment(&mut stage_counts, "entry_blocked");
                        increment(&mut stage_counts, "entry_execution_blocked");
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
#[derive(Debug, Clone, PartialEq)]
struct RetestEntrySignal {
    entry_ts: i64,
    entry_price: f64,
    entry_idx: usize,
    trigger: String,
    structure_stop_loss_price: Option<f64>,
    structure_stop_loss_source: Option<String>,
}
fn find_retest_entry_after_signal(
    candles: &[ComputedCandle],
    signal_idx: usize,
    direction: MarketVelocityTradeDirection,
    original_trigger: &str,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<RetestEntrySignal, String> {
    if direction == MarketVelocityTradeDirection::Short {
        return Err("entry_retest_short_not_supported".to_string());
    }
    let signal = candles
        .get(signal_idx)
        .ok_or_else(|| "entry_retest_missing_signal".to_string())?;
    let base_trigger = base_entry_trigger(original_trigger);
    let retest_level = match base_trigger.as_str() {
        "breakout_previous_high" => signal_idx
            .checked_sub(1)
            .and_then(|previous_idx| candles.get(previous_idx))
            .map(|previous| previous.candle.high),
        "reclaim_ema" => signal.ema,
        _ => return Err("entry_retest_unsupported_trigger".to_string()),
    }
    .filter(|level| level.is_finite() && *level > 0.0)
    .ok_or_else(|| "entry_retest_invalid_level".to_string())?;
    let last_confirmation_idx =
        (signal_idx + args.entry_retest_max_wait_candles).min(candles.len().saturating_sub(1));
    for confirmation_idx in signal_idx + 1..=last_confirmation_idx {
        let confirmation = &candles[confirmation_idx];
        if !retest_confirmation_matches(confirmation, retest_level, args) {
            continue;
        }
        let entry_idx = confirmation_idx + 1;
        let Some(entry) = candles.get(entry_idx) else {
            return Err("entry_retest_no_next_entry_candle".to_string());
        };
        let volume_ratio = confirmation
            .previous_volume_avg
            .filter(|average| *average > 0.0)
            .map(|average| confirmation.candle.volume / average);
        if let Some(min_gap_pct) = args.entry_retest_min_entry_open_gap_pct {
            let gap_pct = moving_average_distance_pct(entry.candle.open, confirmation.candle.close)
                .ok_or_else(|| "entry_retest_invalid_entry_gap".to_string())?;
            if gap_pct < min_gap_pct {
                let rescued =
                    args.entry_retest_open_fade_min_volume_ratio
                        .is_some_and(|min_volume_ratio| {
                            volume_ratio.is_some_and(|ratio| ratio >= min_volume_ratio)
                        });
                if !rescued {
                    return Err("entry_retest_entry_open_faded_confirmation".to_string());
                }
            }
        }
        return Ok(RetestEntrySignal {
            entry_ts: entry.candle.ts,
            entry_price: entry.candle.open,
            entry_idx,
            trigger: format!("{base_trigger}+retest_after_signal"),
            structure_stop_loss_price: Some(retest_level),
            structure_stop_loss_source: Some(match base_trigger.as_str() {
                "reclaim_ema" => "entry_confirmation_ema".to_string(),
                "breakout_previous_high" => "entry_confirmation_previous_high".to_string(),
                _ => "entry_confirmation_structure".to_string(),
            }),
        });
    }
    Err("entry_retest_no_pullback_confirmation".to_string())
}
fn retest_confirmation_matches(
    confirmation: &ComputedCandle,
    retest_level: f64,
    args: &MarketVelocityEventBacktestArgs,
) -> bool {
    let candle = &confirmation.candle;
    let tolerance = 1.0 + args.entry_retest_tolerance_pct / 100.0;
    if candle.low > retest_level * tolerance
        || candle.close < retest_level
        || candle.close <= candle.open
    {
        return false;
    }
    let (Some(sma), Some(ema)) = (confirmation.sma, confirmation.ema) else {
        return false;
    };
    if candle.close <= sma || candle.close <= ema {
        return false;
    }
    let Some(sma_distance) = moving_average_distance_pct(candle.close, sma) else {
        return false;
    };
    let Some(ema_distance) = moving_average_distance_pct(candle.close, ema) else {
        return false;
    };
    if args.entry_max_distance_pct > 0.0
        && (sma_distance.abs() > args.entry_max_distance_pct
            || ema_distance.abs() > args.entry_max_distance_pct)
    {
        return false;
    }
    let volume_ratio = confirmation
        .previous_volume_avg
        .filter(|average| *average > 0.0)
        .map(|average| candle.volume / average);
    args.entry_min_volume_ratio <= 0.0
        || volume_ratio.is_some_and(|ratio| ratio >= args.entry_min_volume_ratio)
}
fn entry_gap_without_retest_block_reason(
    candles: &[ComputedCandle],
    signal_idx: usize,
    entry_idx: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<String> {
    let max_gap_pct = args.entry_max_gap_without_retest_pct?;
    let signal = candles.get(signal_idx)?;
    let previous = signal_idx
        .checked_sub(1)
        .and_then(|previous_idx| candles.get(previous_idx))?;
    let entry = candles.get(entry_idx)?;
    let gap_pct = moving_average_distance_pct(entry.candle.open, signal.candle.close)?;
    if gap_pct <= max_gap_pct {
        return None;
    }
    let retest_level = previous.candle.high;
    let tolerance = 1.0 + args.entry_retest_tolerance_pct / 100.0;
    let has_known_retest = candles
        .get(signal_idx + 1..entry_idx)
        .unwrap_or(&[])
        .iter()
        .any(|candle| {
            candle.candle.low <= retest_level * tolerance && candle.candle.close >= retest_level
        });
    (!has_known_retest).then(|| "entry_gap_without_retest".to_string())
}
/// 生成 回测与策略研究 需要的派生数据，供后续执行、展示或审计使用。
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
        let selected_stop_loss = select_stop_loss_for_confirmed_signal(signal, args);
        let mut result = simulate_trade(
            candles,
            signal.entry_idx,
            signal.entry_ts,
            signal.entry_price,
            trade_direction_for_event(&signal.event),
            selected_stop_loss.stop_loss_pct,
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
/// 提供盈利保护for目标的集中实现，避免回测策略调用方重复处理相同细节。
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
/// 执行 Runner离场for目标步骤，串起回测策略需要的状态推进和错误处理。
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
/// 提供early离场的集中实现，避免回测策略调用方重复处理相同细节。
pub(crate) fn early_exit(args: &MarketVelocityEventBacktestArgs) -> Option<EarlyExit> {
    args.early_exit_no_profit_candles
        .map(|no_profit_candles| EarlyExit { no_profit_candles })
}
/// 提供已完成K 线数量的集中实现，避免回测策略调用方重复处理相同细节。
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
/// 封装推进entryK 线idx，减少回测策略调用方重复实现相同细节。
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
/// 提供movingaverage状态的集中实现，避免回测策略调用方重复处理相同细节。
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
/// 提供movingaveragedistancepct的集中实现，避免回测策略调用方重复处理相同细节。
fn moving_average_distance_pct(close: f64, average: f64) -> Option<f64> {
    if average <= 0.0 || !average.is_finite() || !close.is_finite() {
        return None;
    }
    Some((close - average) / average * 100.0)
}
/// 提供simpleaverage的集中实现，避免回测策略调用方重复处理相同细节。
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
/// 提供timestampmstorfc3339的集中实现，避免回测策略调用方重复处理相同细节。
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
