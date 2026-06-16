use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use rust_quant_services::rust_quan_web::{
    ExecutionTaskClient, ExecutionTaskConfig, MarketVelocityPaperOutcomeRequest,
};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::{BTreeMap, HashMap};

mod exit;
mod fvg;
mod reentry;
mod report;
pub use exit::{simulate_trade, ProfitProtection, RunnerExit};
use fvg::{find_fvg_entry, FvgEntrySearch};
use reentry::maybe_apply_stop_reentry;
use report::{print_result_report, print_stage_report};

pub const MS_15M: i64 = 15 * 60 * 1_000;
pub const MS_1H: i64 = 60 * 60 * 1_000;
pub const MS_4H: i64 = 4 * 60 * 60 * 1_000;

const DEFAULT_TARGET_RS: &[f64] = &[1.5, 2.0];
const TOUCH_THRESHOLD_PCT: f64 = 0.3;
const DEFAULT_PAPER_OUTCOME_ENTRY_RULE_VERSION: &str = "rank_radar_4h_trend_15m_timing_v1";
const ENTRY_TRIGGER_ALLOWLIST_FILTER_VERSION: &str = "entry_trigger_allowlist_v1";
const ENTRY_TRIGGER_BLOCKLIST_FILTER_VERSION: &str = "entry_trigger_blocklist_v1";
const ENTRY_TRIGGER_UNFILTERED_VERSION: &str = "unfiltered_v1";
const DEFAULT_WEB_PAPER_OUTCOME_ENTRY_TRIGGER_ALLOWLIST: &[&str] = &["breakout_previous_high"];
const DEFAULT_FVG_LOOKBACK_CANDLES: usize = 40;
const DEFAULT_FVG_MAX_WAIT_CANDLES: usize = 24;
const PAPER_OBSERVATION_LOOP_INTERVAL_FLAG: &str = "--loop-interval-seconds";
const PAPER_STRATEGY_PRESET_FLAG: &str = "--paper-strategy-preset";
const STOP_REENTRY_PROFIT_PRESET: &str = "stop_reentry_025sl_24r_v1";
const STOP_REENTRY_PROFIT_ENTRY_RULE_VERSION: &str =
    "rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1";
const PAPER_OBSERVATION_OWNED_FLAGS: &[&str] = &[
    "--paper-outcome-sink",
    "--paper-outcome-entry-rule-version",
    "--entry-trigger-allowlist",
    "--entry-trigger-blocklist",
    "--stop-reentry-mode",
    "--fvg-entry-mode",
    "--fvg-lookback-candles",
    "--fvg-max-wait-candles",
    "--profit-protect-after-r",
    "--profit-protect-stop-r",
    "--runner-target-r",
    "--runner-fraction",
    "--runner-stop-r",
];
const PAPER_STRATEGY_PRESET_LOCKED_FLAGS: &[&str] = &[
    "--target-rs",
    "--stop-loss-pct",
    "--entry-period",
    "--entry-max-distance-pct",
    "--entry-min-volume-ratio",
    "--trend-min-average-distance-pct",
    "--min-delta-rank",
    "--max-new-rank",
    "--chase-top-rank",
    "--chase-price-change-pct",
    "--max-15m-staleness-min",
    "--max-4h-staleness-min",
];
const PAPER_OUTCOME_HORIZONS: &[(i32, i64)] =
    &[(24, 24 * 60 * 60 * 1_000), (48, 48 * 60 * 60 * 1_000)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketVelocityPaperOutcomeSink {
    Off,
    Jsonl,
    Web,
}

impl MarketVelocityPaperOutcomeSink {
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "disabled" | "0" | "false" => Ok(Self::Off),
            "jsonl" | "stdout" | "print" => Ok(Self::Jsonl),
            "web" | "quant_web" | "submit" => Ok(Self::Web),
            other => bail!("unknown --paper-outcome-sink: {other}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReentryMode {
    Off,
    BreakoutReclaim,
}

impl StopReentryMode {
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "disabled" | "0" | "false" => Ok(Self::Off),
            "breakout_reclaim" | "reclaim_breakout" | "on" | "true" => Ok(Self::BreakoutReclaim),
            other => bail!("unknown --stop-reentry-mode: {other}"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::BreakoutReclaim => "breakout_reclaim",
        }
    }

    fn trigger_suffix(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::BreakoutReclaim => Some("stop_reentry_breakout_reclaim"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FvgEntryMode {
    Off,
    M15To1h,
    H1To4h,
}

impl FvgEntryMode {
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "disabled" | "0" | "false" => Ok(Self::Off),
            "m15_to_1h" | "15m_to_1h" | "15m-1h" => Ok(Self::M15To1h),
            "h1_to_4h" | "1h_to_4h" | "1h-4h" => Ok(Self::H1To4h),
            other => bail!("unknown --fvg-entry-mode: {other}"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::M15To1h => "m15_to_1h",
            Self::H1To4h => "h1_to_4h",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaperStrategyPreset {
    StopReentry025Sl24R,
}

impl PaperStrategyPreset {
    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            STOP_REENTRY_PROFIT_PRESET => Ok(Self::StopReentry025Sl24R),
            other => bail!("unknown {PAPER_STRATEGY_PRESET_FLAG}: {other}"),
        }
    }

    fn append_args(self, args: &mut Vec<String>) {
        match self {
            Self::StopReentry025Sl24R => {
                args.extend([
                    "--paper-outcome-entry-rule-version".to_string(),
                    STOP_REENTRY_PROFIT_ENTRY_RULE_VERSION.to_string(),
                    "--stop-reentry-mode".to_string(),
                    "breakout_reclaim".to_string(),
                    "--stop-loss-pct".to_string(),
                    "0.025".to_string(),
                    "--target-rs".to_string(),
                    "2.4".to_string(),
                ]);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityEventBacktestArgs {
    pub stop_loss_pct: f64,
    pub target_rs: Vec<f64>,
    pub entry_period: usize,
    pub entry_max_distance_pct: f64,
    pub entry_min_volume_ratio: f64,
    pub trend_min_average_distance_pct: f64,
    pub min_delta_rank: i32,
    pub max_new_rank: i32,
    pub chase_top_rank: i32,
    pub chase_price_change_pct: f64,
    pub max_15m_staleness_min: i64,
    pub max_4h_staleness_min: i64,
    pub sample_limit: usize,
    pub paper_outcome_sink: MarketVelocityPaperOutcomeSink,
    pub paper_outcome_entry_rule_version: String,
    pub entry_trigger_allowlist: Vec<String>,
    pub entry_trigger_blocklist: Vec<String>,
    pub stop_reentry_mode: StopReentryMode,
    pub fvg_entry_mode: FvgEntryMode,
    pub fvg_lookback_candles: usize,
    pub fvg_max_wait_candles: usize,
    pub profit_protect_after_r: Option<f64>,
    pub profit_protect_stop_r: f64,
    pub runner_target_r: Option<f64>,
    pub runner_fraction: f64,
    pub runner_stop_r: f64,
}

impl Default for MarketVelocityEventBacktestArgs {
    fn default() -> Self {
        Self {
            stop_loss_pct: 0.03,
            target_rs: DEFAULT_TARGET_RS.to_vec(),
            entry_period: 20,
            entry_max_distance_pct: 3.0,
            entry_min_volume_ratio: 1.0,
            trend_min_average_distance_pct: 0.0,
            min_delta_rank: 10,
            max_new_rank: 30,
            chase_top_rank: 10,
            chase_price_change_pct: 8.0,
            max_15m_staleness_min: 30,
            max_4h_staleness_min: 240,
            sample_limit: 5,
            paper_outcome_sink: MarketVelocityPaperOutcomeSink::Off,
            paper_outcome_entry_rule_version: DEFAULT_PAPER_OUTCOME_ENTRY_RULE_VERSION.to_string(),
            entry_trigger_allowlist: Vec::new(),
            entry_trigger_blocklist: Vec::new(),
            stop_reentry_mode: StopReentryMode::Off,
            fvg_entry_mode: FvgEntryMode::Off,
            fvg_lookback_candles: DEFAULT_FVG_LOOKBACK_CANDLES,
            fvg_max_wait_candles: DEFAULT_FVG_MAX_WAIT_CANDLES,
            profit_protect_after_r: None,
            profit_protect_stop_r: 0.0,
            runner_target_r: None,
            runner_fraction: 0.0,
            runner_stop_r: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityEventBacktestConfig {
    pub database_url: String,
    pub args: MarketVelocityEventBacktestArgs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityPaperObservationCommand {
    pub backtest_args: MarketVelocityEventBacktestArgs,
    pub loop_interval_seconds: Option<u64>,
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

pub fn parse_cli_args_from<I, S>(args: I) -> Result<MarketVelocityEventBacktestArgs>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut parsed = MarketVelocityEventBacktestArgs::default();
    let mut entry_trigger_allowlist_explicit = false;
    let mut entry_trigger_blocklist_explicit = false;
    let mut paper_outcome_entry_rule_version_explicit = false;
    let mut args = args.into_iter().map(Into::into);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stop-loss-pct" => parsed.stop_loss_pct = parse_next(&mut args, &arg)?,
            "--target-rs" => parsed.target_rs = parse_target_rs(&next_arg(&mut args, &arg)?)?,
            "--entry-period" => parsed.entry_period = parse_next(&mut args, &arg)?,
            "--entry-max-distance-pct" => {
                parsed.entry_max_distance_pct = parse_next(&mut args, &arg)?
            }
            "--entry-min-volume-ratio" => {
                parsed.entry_min_volume_ratio = parse_next(&mut args, &arg)?
            }
            "--trend-min-average-distance-pct" => {
                parsed.trend_min_average_distance_pct = parse_next(&mut args, &arg)?
            }
            "--min-delta-rank" => parsed.min_delta_rank = parse_next(&mut args, &arg)?,
            "--max-new-rank" => parsed.max_new_rank = parse_next(&mut args, &arg)?,
            "--chase-top-rank" => parsed.chase_top_rank = parse_next(&mut args, &arg)?,
            "--chase-price-change-pct" => {
                parsed.chase_price_change_pct = parse_next(&mut args, &arg)?
            }
            "--max-15m-staleness-min" => {
                parsed.max_15m_staleness_min = parse_next(&mut args, &arg)?
            }
            "--max-4h-staleness-min" => parsed.max_4h_staleness_min = parse_next(&mut args, &arg)?,
            "--sample-limit" => parsed.sample_limit = parse_next(&mut args, &arg)?,
            "--paper-outcome-sink" => {
                parsed.paper_outcome_sink =
                    MarketVelocityPaperOutcomeSink::from_str(&next_arg(&mut args, &arg)?)?
            }
            "--paper-outcome-entry-rule-version" => {
                paper_outcome_entry_rule_version_explicit = true;
                parsed.paper_outcome_entry_rule_version = next_arg(&mut args, &arg)?
            }
            "--entry-trigger-allowlist" => {
                entry_trigger_allowlist_explicit = true;
                parsed.entry_trigger_allowlist =
                    parse_entry_trigger_list(&next_arg(&mut args, &arg)?)?
            }
            "--entry-trigger-blocklist" => {
                entry_trigger_blocklist_explicit = true;
                parsed.entry_trigger_blocklist =
                    parse_entry_trigger_list(&next_arg(&mut args, &arg)?)?
            }
            "--stop-reentry-mode" => {
                parsed.stop_reentry_mode = StopReentryMode::from_str(&next_arg(&mut args, &arg)?)?
            }
            "--fvg-entry-mode" => {
                parsed.fvg_entry_mode = FvgEntryMode::from_str(&next_arg(&mut args, &arg)?)?
            }
            "--fvg-lookback-candles" => parsed.fvg_lookback_candles = parse_next(&mut args, &arg)?,
            "--fvg-max-wait-candles" => parsed.fvg_max_wait_candles = parse_next(&mut args, &arg)?,
            "--profit-protect-after-r" => {
                parsed.profit_protect_after_r = Some(parse_next(&mut args, &arg)?)
            }
            "--profit-protect-stop-r" => {
                parsed.profit_protect_stop_r = parse_next(&mut args, &arg)?
            }
            "--runner-target-r" => parsed.runner_target_r = Some(parse_next(&mut args, &arg)?),
            "--runner-fraction" => parsed.runner_fraction = parse_next(&mut args, &arg)?,
            "--runner-stop-r" => parsed.runner_stop_r = parse_next(&mut args, &arg)?,
            "--help" | "-h" => {
                print_market_velocity_event_backtest_usage();
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    if parsed.entry_period == 0 {
        bail!("--entry-period must be greater than 0");
    }
    if parsed.stop_loss_pct <= 0.0 {
        bail!("--stop-loss-pct must be greater than 0");
    }
    if parsed.trend_min_average_distance_pct < 0.0 {
        bail!("--trend-min-average-distance-pct must be zero or greater");
    }
    if parsed.fvg_lookback_candles == 0 {
        bail!("--fvg-lookback-candles must be greater than 0");
    }
    if parsed.fvg_max_wait_candles == 0 {
        bail!("--fvg-max-wait-candles must be greater than 0");
    }
    match parsed.profit_protect_after_r {
        Some(after_r) => {
            if after_r <= 0.0 {
                bail!("--profit-protect-after-r must be greater than 0");
            }
            if parsed.profit_protect_stop_r < 0.0 {
                bail!("--profit-protect-stop-r must be zero or greater");
            }
            if parsed.profit_protect_stop_r >= after_r {
                bail!("--profit-protect-stop-r must be lower than --profit-protect-after-r");
            }
        }
        None if parsed.profit_protect_stop_r != 0.0 => {
            bail!("--profit-protect-stop-r requires --profit-protect-after-r");
        }
        None => {}
    }
    match parsed.runner_target_r {
        Some(target_r) => {
            if target_r <= 0.0 {
                bail!("--runner-target-r must be greater than 0");
            }
            if parsed.runner_fraction <= 0.0 || parsed.runner_fraction >= 1.0 {
                bail!("--runner-fraction must be greater than 0 and lower than 1");
            }
            if parsed.runner_stop_r < 0.0 {
                bail!("--runner-stop-r must be zero or greater");
            }
            if parsed.runner_stop_r >= target_r {
                bail!("--runner-stop-r must be lower than --runner-target-r");
            }
        }
        None if parsed.runner_fraction != 0.0 || parsed.runner_stop_r != 0.0 => {
            bail!("--runner-fraction and --runner-stop-r require --runner-target-r");
        }
        None => {}
    }
    if parsed.paper_outcome_sink == MarketVelocityPaperOutcomeSink::Web
        && !entry_trigger_allowlist_explicit
        && !entry_trigger_blocklist_explicit
    {
        parsed.entry_trigger_allowlist = DEFAULT_WEB_PAPER_OUTCOME_ENTRY_TRIGGER_ALLOWLIST
            .iter()
            .map(|value| (*value).to_string())
            .collect();
    }
    if parsed.paper_outcome_sink == MarketVelocityPaperOutcomeSink::Web
        && parsed.stop_reentry_mode != StopReentryMode::Off
        && !paper_outcome_entry_rule_version_explicit
    {
        bail!("--stop-reentry-mode with --paper-outcome-sink web requires explicit --paper-outcome-entry-rule-version");
    }

    Ok(parsed)
}

pub fn parse_paper_observation_args_from<I, S>(args: I) -> Result<MarketVelocityEventBacktestArgs>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let user_args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let (preset, user_args) = extract_paper_strategy_preset(user_args)?;
    if preset.is_some() {
        reject_paper_strategy_preset_overrides(&user_args)?;
    }
    reject_paper_observation_owned_flags(&user_args)?;

    let mut parsed_args = Vec::with_capacity(user_args.len() + 10);
    parsed_args.push("--paper-outcome-sink".to_string());
    parsed_args.push("web".to_string());
    if let Some(preset) = preset {
        preset.append_args(&mut parsed_args);
    }
    parsed_args.extend(user_args);
    parse_cli_args_from(parsed_args)
}

pub fn parse_paper_observation_command_from<I, S>(
    args: I,
) -> Result<MarketVelocityPaperObservationCommand>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut backtest_args = Vec::new();
    let mut loop_interval_seconds = None;
    let mut args = args.into_iter().map(Into::into);

    while let Some(arg) = args.next() {
        if arg == PAPER_OBSERVATION_LOOP_INTERVAL_FLAG {
            set_paper_observation_loop_interval(
                &mut loop_interval_seconds,
                &next_arg(&mut args, PAPER_OBSERVATION_LOOP_INTERVAL_FLAG)?,
            )?;
        } else if let Some(value) = arg.strip_prefix("--loop-interval-seconds=") {
            set_paper_observation_loop_interval(&mut loop_interval_seconds, value)?;
        } else if arg == "--help" || arg == "-h" {
            print_market_velocity_paper_observation_usage();
            std::process::exit(0);
        } else {
            backtest_args.push(arg);
        }
    }

    Ok(MarketVelocityPaperObservationCommand {
        backtest_args: parse_paper_observation_args_from(backtest_args)?,
        loop_interval_seconds,
    })
}

fn set_paper_observation_loop_interval(
    loop_interval_seconds: &mut Option<u64>,
    value: &str,
) -> Result<()> {
    if loop_interval_seconds.is_some() {
        bail!("{PAPER_OBSERVATION_LOOP_INTERVAL_FLAG} can only be provided once");
    }
    let seconds = value
        .parse::<u64>()
        .with_context(|| format!("invalid value for {PAPER_OBSERVATION_LOOP_INTERVAL_FLAG}"))?;
    if seconds == 0 {
        bail!("{PAPER_OBSERVATION_LOOP_INTERVAL_FLAG} must be greater than 0");
    }
    *loop_interval_seconds = Some(seconds);
    Ok(())
}

fn reject_paper_observation_owned_flags(args: &[String]) -> Result<()> {
    for arg in args {
        let flag = normalized_arg_flag(arg);
        if PAPER_OBSERVATION_OWNED_FLAGS.contains(&flag) {
            bail!(
                "market_velocity_paper_observation owns {flag}; use market_velocity_event_backtest for experimental overrides"
            );
        }
    }
    Ok(())
}

fn extract_paper_strategy_preset(
    args: Vec<String>,
) -> Result<(Option<PaperStrategyPreset>, Vec<String>)> {
    let mut preset = None;
    let mut rest = Vec::with_capacity(args.len());
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        if arg == PAPER_STRATEGY_PRESET_FLAG {
            let value = iter
                .next()
                .with_context(|| format!("missing value for {PAPER_STRATEGY_PRESET_FLAG}"))?;
            set_paper_strategy_preset(&mut preset, &value)?;
        } else if let Some(value) = arg.strip_prefix("--paper-strategy-preset=") {
            set_paper_strategy_preset(&mut preset, value)?;
        } else {
            rest.push(arg);
        }
    }

    Ok((preset, rest))
}

fn set_paper_strategy_preset(preset: &mut Option<PaperStrategyPreset>, value: &str) -> Result<()> {
    if preset.is_some() {
        bail!("{PAPER_STRATEGY_PRESET_FLAG} can only be provided once");
    }
    *preset = Some(PaperStrategyPreset::from_str(value)?);
    Ok(())
}

fn reject_paper_strategy_preset_overrides(args: &[String]) -> Result<()> {
    for arg in args {
        let flag = normalized_arg_flag(arg);
        if PAPER_STRATEGY_PRESET_LOCKED_FLAGS.contains(&flag) {
            bail!("{PAPER_STRATEGY_PRESET_FLAG} locks {flag}; use market_velocity_event_backtest for parameter research");
        }
    }
    Ok(())
}

fn normalized_arg_flag(arg: &str) -> &str {
    arg.split_once('=').map(|(flag, _)| flag).unwrap_or(arg)
}

pub fn print_market_velocity_event_backtest_usage() {
    println!(
        "Usage: market_velocity_event_backtest [--target-rs 1.5,2.0] [--stop-loss-pct 0.02] [--entry-period 20] [--entry-trigger-allowlist breakout_previous_high,reclaim_ema] [--entry-trigger-blocklist pullback_hold_ema] [--stop-reentry-mode off|breakout_reclaim] [--profit-protect-after-r 1.0 --profit-protect-stop-r 0.0] [--runner-target-r 4.0 --runner-fraction 0.5 --runner-stop-r 0.0] [--fvg-entry-mode off|15m_to_1h|1h_to_4h] [--paper-outcome-sink off|jsonl|web]"
    );
}

pub fn print_market_velocity_paper_observation_usage() {
    println!(
        "Usage: market_velocity_paper_observation [--loop-interval-seconds 21600] [--paper-strategy-preset stop_reentry_025sl_24r_v1] [--target-rs 1.5,2.0] [--stop-loss-pct 0.03] [--entry-period 20]"
    );
}

pub fn config_from_env_and_args(
    args: MarketVelocityEventBacktestArgs,
) -> Result<MarketVelocityEventBacktestConfig> {
    let database_url = first_non_empty_env(&[
        "QUANT_CORE_DATABASE_URL",
        "POSTGRES_QUANT_CORE_DATABASE_URL",
        "DATABASE_URL",
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
    let confirmed = filter_confirmed_events_by_entry_trigger(&evaluation.confirmed, &config.args);
    print_entry_trigger_filter_report(&evaluation.confirmed, &confirmed, &config.args);
    print_result_report(&confirmed, &data.candles_15m, &config.args);
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

pub fn filter_confirmed_events_by_entry_trigger(
    confirmed: &[ConfirmedEvent],
    args: &MarketVelocityEventBacktestArgs,
) -> Vec<ConfirmedEvent> {
    confirmed
        .iter()
        .filter(|event| entry_trigger_allowed(&event.trigger, args))
        .cloned()
        .collect()
}

fn entry_trigger_allowed(trigger: &str, args: &MarketVelocityEventBacktestArgs) -> bool {
    let normalized = normalize_entry_trigger(trigger);
    if !args.entry_trigger_allowlist.is_empty()
        && !args
            .entry_trigger_allowlist
            .iter()
            .any(|allowed| allowed == &normalized)
    {
        return false;
    }
    !args
        .entry_trigger_blocklist
        .iter()
        .any(|blocked| blocked == &normalized)
}

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
    let positive = matches!(sma_state, "above" | "breakout_up")
        && matches!(ema_state, "above" | "breakout_up");
    if positive && args.trend_min_average_distance_pct > 0.0 {
        let Some(sma_distance) = moving_average_distance_pct(latest.candle.close, sma) else {
            return (false, "invalid_4h_distance".to_string());
        };
        let Some(ema_distance) = moving_average_distance_pct(latest.candle.close, ema) else {
            return (false, "invalid_4h_distance".to_string());
        };
        if sma_distance < args.trend_min_average_distance_pct
            || ema_distance < args.trend_min_average_distance_pct
        {
            return (false, "weak_4h_average_distance".to_string());
        }
    }

    (positive, format!("4h_{sma_state}_{ema_state}"))
}

pub fn entry_confirmation(
    candles: &[ComputedCandle],
    event_ts: i64,
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
    if latest.candle.close <= sma || latest.candle.close <= ema {
        return (false, "price_below_15m_average".to_string());
    }

    let Some(sma_distance) = moving_average_distance_pct(latest.candle.close, sma) else {
        return (false, "invalid_15m_distance".to_string());
    };
    let Some(ema_distance) = moving_average_distance_pct(latest.candle.close, ema) else {
        return (false, "invalid_15m_distance".to_string());
    };
    if args.entry_max_distance_pct > 0.0
        && (sma_distance > args.entry_max_distance_pct
            || ema_distance > args.entry_max_distance_pct)
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

        let (trend_ok, trend_reason) = trend_confirmation(symbol_4h, event.ts, args);
        if !trend_ok {
            increment(&mut stage_counts, "trend_blocked");
            increment_nested(&mut blockers, &event.symbol, &trend_reason);
            continue;
        }
        increment(&mut stage_counts, "trend_pass");

        match args.fvg_entry_mode {
            FvgEntryMode::Off => {
                let (entry_ok, entry_reason) = entry_confirmation(symbol_15m, event.ts, args);
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
            args.stop_loss_pct,
            target_r,
            horizon_ms,
            profit_protection_for_target(args, target_r),
            runner_exit_for_target(args, target_r),
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
                        "entry_filter": {
                            "entry_trigger_filter_version": entry_trigger_filter_version,
                            "entry_trigger_allowlist": &args.entry_trigger_allowlist,
                            "entry_trigger_blocklist": &args.entry_trigger_blocklist,
                        },
                        "filters": {
                            "min_delta_rank": args.min_delta_rank,
                            "max_new_rank": args.max_new_rank,
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
    if !args.entry_trigger_allowlist.is_empty() {
        ENTRY_TRIGGER_ALLOWLIST_FILTER_VERSION
    } else if !args.entry_trigger_blocklist.is_empty() {
        ENTRY_TRIGGER_BLOCKLIST_FILTER_VERSION
    } else {
        ENTRY_TRIGGER_UNFILTERED_VERSION
    }
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

async fn load_backtest_data(
    pool: &PgPool,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<BacktestDataSet> {
    let pairs = load_candle_pairs(pool, args).await?;
    let symbols = pairs
        .iter()
        .map(|pair| pair.symbol.clone())
        .collect::<Vec<_>>();
    let mut candles_15m = HashMap::new();
    let mut candles_1h = HashMap::new();
    let mut candles_4h = HashMap::new();
    let mut candles_15m_computed = HashMap::new();
    let mut candles_4h_computed = HashMap::new();

    for pair in &pairs {
        let raw_15m = load_candles(pool, &pair.candles_15m).await?;
        let raw_1h = match pair.candles_1h.as_deref() {
            Some(table_name) => load_candles(pool, table_name).await?,
            None => Vec::new(),
        };
        let raw_4h = load_candles(pool, &pair.candles_4h).await?;
        candles_15m_computed.insert(
            pair.symbol.clone(),
            build_computed_candles(raw_15m.clone(), args.entry_period),
        );
        candles_4h_computed.insert(
            pair.symbol.clone(),
            build_computed_candles(raw_4h.clone(), args.entry_period),
        );
        candles_15m.insert(pair.symbol.clone(), raw_15m);
        candles_1h.insert(pair.symbol.clone(), raw_1h);
        candles_4h.insert(pair.symbol.clone(), raw_4h);
    }

    let events = load_events(pool, &symbols, args).await?;
    Ok(BacktestDataSet {
        pairs,
        candles_15m,
        candles_1h,
        candles_4h,
        candles_15m_computed,
        candles_4h_computed,
        events,
    })
}

async fn load_candle_pairs(
    pool: &PgPool,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<Vec<CandlePair>> {
    let rows = sqlx::query(
        r#"
        WITH candidates AS (
          SELECT DISTINCT upper(symbol) AS symbol
          FROM market_rank_events
          WHERE event_type IN ('rank_velocity', 'top_entry')
            AND delta_rank >= $1
            AND new_rank BETWEEN 1 AND $2
            AND lower(price_direction) = 'up'
            AND current_price IS NOT NULL
            AND NOT (new_rank <= $3 AND COALESCE(price_change_pct, 0) >= $4)
        )
        SELECT
          candidates.symbol,
          t15.table_name AS candles_15m,
          t1.table_name AS candles_1h,
          t4.table_name AS candles_4h
        FROM candidates
        JOIN information_schema.tables t15
          ON t15.table_schema = 'public'
         AND t15.table_name = lower(candidates.symbol) || '_candles_15m'
        LEFT JOIN information_schema.tables t1
          ON t1.table_schema = 'public'
         AND t1.table_name = lower(candidates.symbol) || '_candles_1h'
        JOIN information_schema.tables t4
          ON t4.table_schema = 'public'
         AND t4.table_name = lower(candidates.symbol) || '_candles_4h'
        ORDER BY candidates.symbol
        "#,
    )
    .bind(args.min_delta_rank)
    .bind(args.max_new_rank)
    .bind(args.chase_top_rank)
    .bind(args.chase_price_change_pct)
    .fetch_all(pool)
    .await
    .context("load market velocity candle table pairs")?;

    Ok(rows
        .into_iter()
        .map(|row| CandlePair {
            symbol: row.get("symbol"),
            candles_15m: row.get("candles_15m"),
            candles_1h: row.try_get("candles_1h").ok(),
            candles_4h: row.get("candles_4h"),
        })
        .collect())
}

async fn load_candles(pool: &PgPool, table_name: &str) -> Result<Vec<BacktestCandle>> {
    let query = format!(
        "SELECT ts, o, h, l, c, vol FROM {} ORDER BY ts",
        quote_identifier(table_name)
    );
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load candles from {table_name}"))?;
    rows.into_iter()
        .map(|row| {
            Ok(BacktestCandle {
                ts: row.get::<i64, _>("ts"),
                open: parse_f64(row.get::<String, _>("o").as_str())?,
                high: parse_f64(row.get::<String, _>("h").as_str())?,
                low: parse_f64(row.get::<String, _>("l").as_str())?,
                close: parse_f64(row.get::<String, _>("c").as_str())?,
                volume: parse_f64(row.get::<String, _>("vol").as_str())?,
            })
        })
        .collect()
}

async fn load_events(
    pool: &PgPool,
    symbols: &[String],
    args: &MarketVelocityEventBacktestArgs,
) -> Result<Vec<RadarEvent>> {
    if symbols.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        r#"
        SELECT
          id::bigint AS id,
          lower(exchange) AS exchange,
          upper(symbol) AS symbol,
          floor(extract(epoch from detected_at) * 1000)::bigint AS detected_ms,
          detected_at::text AS detected_at,
          new_rank::int AS new_rank,
          delta_rank::int AS delta_rank,
          current_price::text AS current_price,
          COALESCE(price_change_pct, 0)::text AS price_change_pct
        FROM market_rank_events
        WHERE upper(symbol) = ANY($1)
          AND event_type IN ('rank_velocity', 'top_entry')
          AND delta_rank >= $2
          AND new_rank BETWEEN 1 AND $3
          AND lower(price_direction) = 'up'
          AND current_price IS NOT NULL
          AND NOT (new_rank <= $4 AND COALESCE(price_change_pct, 0) >= $5)
        ORDER BY detected_at, id
        "#,
    )
    .bind(symbols)
    .bind(args.min_delta_rank)
    .bind(args.max_new_rank)
    .bind(args.chase_top_rank)
    .bind(args.chase_price_change_pct)
    .fetch_all(pool)
    .await
    .context("load market velocity radar events")?;

    rows.into_iter()
        .map(|row| {
            Ok(RadarEvent {
                id: row.get("id"),
                exchange: row.get("exchange"),
                symbol: row.get("symbol"),
                ts: row.get("detected_ms"),
                detected_at: row.get("detected_at"),
                new_rank: row.get("new_rank"),
                delta_rank: row.get("delta_rank"),
                current_price: parse_f64(row.get::<String, _>("current_price").as_str())?,
                price_change_pct: parse_f64(row.get::<String, _>("price_change_pct").as_str())?,
            })
        })
        .collect()
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

fn parse_target_rs(value: &str) -> Result<Vec<f64>> {
    let targets = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.parse::<f64>().context("parse --target-rs value"))
        .collect::<Result<Vec<_>>>()?;
    if targets.is_empty() {
        return Ok(DEFAULT_TARGET_RS.to_vec());
    }
    Ok(targets)
}

fn parse_entry_trigger_list(value: &str) -> Result<Vec<String>> {
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "all" | "*" | "none") {
        return Ok(Vec::new());
    }
    let triggers = value
        .split(',')
        .map(normalize_entry_trigger)
        .filter(|trigger| !trigger.is_empty())
        .collect::<Vec<_>>();
    if triggers.is_empty() {
        bail!("entry trigger list must not be empty");
    }
    Ok(triggers)
}

fn normalize_entry_trigger(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn format_entry_trigger_filter_list(values: &[String]) -> String {
    if values.is_empty() {
        "all".to_string()
    } else {
        values.join(",")
    }
}

fn parse_f64(value: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .with_context(|| format!("parse numeric value {value}"))
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

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn next_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("missing value for {flag}"))
}

fn parse_next<T>(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    next_arg(args, flag)?
        .parse::<T>()
        .with_context(|| format!("invalid value for {flag}"))
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
