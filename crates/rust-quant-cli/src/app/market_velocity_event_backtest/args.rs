use anyhow::{bail, Context, Result};

const DEFAULT_TARGET_RS: &[f64] = &[1.5, 2.0];
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

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::BreakoutReclaim => "breakout_reclaim",
        }
    }

    pub(super) fn trigger_suffix(self) -> Option<&'static str> {
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

    pub(super) fn label(self) -> &'static str {
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
pub struct MarketVelocityPaperObservationCommand {
    pub backtest_args: MarketVelocityEventBacktestArgs,
    pub loop_interval_seconds: Option<u64>,
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

    if parsed.paper_outcome_sink == MarketVelocityPaperOutcomeSink::Web
        && !entry_trigger_allowlist_explicit
        && !entry_trigger_blocklist_explicit
    {
        parsed.entry_trigger_allowlist = DEFAULT_WEB_PAPER_OUTCOME_ENTRY_TRIGGER_ALLOWLIST
            .iter()
            .map(|value| (*value).to_string())
            .collect();
    }
    validate_args(&parsed, paper_outcome_entry_rule_version_explicit)?;
    Ok(parsed)
}

fn validate_args(
    parsed: &MarketVelocityEventBacktestArgs,
    paper_outcome_entry_rule_version_explicit: bool,
) -> Result<()> {
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
        && parsed.stop_reentry_mode != StopReentryMode::Off
        && !paper_outcome_entry_rule_version_explicit
    {
        bail!("--stop-reentry-mode with --paper-outcome-sink web requires explicit --paper-outcome-entry-rule-version");
    }
    Ok(())
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

pub(super) fn normalize_entry_trigger(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(super) fn format_entry_trigger_filter_list(values: &[String]) -> String {
    if values.is_empty() {
        "all".to_string()
    } else {
        values.join(",")
    }
}

pub(super) fn entry_trigger_filter_version_label(
    has_allowlist: bool,
    has_blocklist: bool,
) -> &'static str {
    if has_allowlist {
        ENTRY_TRIGGER_ALLOWLIST_FILTER_VERSION
    } else if has_blocklist {
        ENTRY_TRIGGER_BLOCKLIST_FILTER_VERSION
    } else {
        ENTRY_TRIGGER_UNFILTERED_VERSION
    }
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
