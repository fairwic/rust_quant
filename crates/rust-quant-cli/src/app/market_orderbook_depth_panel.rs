mod book_depth;

use self::book_depth::{load_book_depth, BookDepthAudit, DepthFactor};
use crate::app::okx_historical_universe::HistoricalUniverseManifest;
use anyhow::{anyhow, bail, Context, Result};
use rust_quant_strategies::CandleItem;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MS_15M: i64 = 15 * 60 * 1_000;
const MS_8H: i64 = 8 * 60 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const RETURN_6H_BARS: usize = 24;
const RETURN_24H_BARS: usize = 96;
const FORWARD_1H_BARS: usize = 4;
const FORWARD_4H_BARS: usize = 16;
const PRICE_COVERAGE_MIN_RATIO: f64 = 0.80;
const CANDIDATES_PER_DIRECTION: usize = 2;
const RULE_VERSION: &str = "top2_impulse_binance_depth_1pct_median_15m_8h_v1";
const ABSORPTION_RULE_VERSION: &str = "top2_impulse_opposed_1pct_depth_depletion_15m_8h_v1";
const DEFAULT_BINANCE_REST_BASE: &str = "https://fapi.binance.com";
const DEFAULT_BINANCE_DATA_BASE: &str = "https://data.binance.vision";

/// 因子面板只允许指定数据位置和下载并发，不暴露可调研究阈值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderbookDepthPanelArgs {
    pub manifest: PathBuf,
    pub cache: PathBuf,
    pub download_concurrency: usize,
    pub binance_rest_base: String,
    pub binance_data_base: String,
}

/// 价格候选与前瞻 outcome 的因果漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrderbookDepthPanelStages {
    pub decision_points: usize,
    pub price_coverage_blocked: usize,
    pub aligned_price_observations: usize,
    pub selected_price_candidates: usize,
    pub incomplete_outcomes: usize,
    pub depth_available: usize,
    pub depth_neutral: usize,
}

/// 单个预注册分组的样本、方向收益和正收益率。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FactorSummary {
    pub observations: usize,
    pub mean_forward_1h: Option<f64>,
    pub mean_forward_4h: Option<f64>,
    pub positive_rate_1h_pct: Option<f64>,
    pub positive_rate_4h_pct: Option<f64>,
}

/// 因子面板完整报告，不代表可执行策略结果。
#[derive(Debug, Clone, PartialEq)]
pub struct OrderbookDepthPanelReport {
    pub rule_version: String,
    pub universe_version: String,
    pub symbols: usize,
    pub depth_audit: BookDepthAudit,
    pub stages: OrderbookDepthPanelStages,
    pub aligned_overall: FactorSummary,
    pub opposed_overall: FactorSummary,
    pub aligned_discovery: FactorSummary,
    pub opposed_discovery: FactorSummary,
    pub aligned_validation: FactorSummary,
    pub opposed_validation: FactorSummary,
    pub aligned_long: FactorSummary,
    pub aligned_short: FactorSummary,
    pub factor_gate_passed: bool,
}

/// 对手盘消耗面板的 confirmed/control 稳定性报告。
#[derive(Debug, Clone, PartialEq)]
pub struct OrderbookAbsorptionPanelReport {
    pub rule_version: String,
    pub universe_version: String,
    pub symbols: usize,
    pub depth_audit: BookDepthAudit,
    pub stages: OrderbookDepthPanelStages,
    pub confirmed_overall: FactorSummary,
    pub control_overall: FactorSummary,
    pub confirmed_discovery: FactorSummary,
    pub control_discovery: FactorSummary,
    pub confirmed_validation: FactorSummary,
    pub control_validation: FactorSummary,
    pub confirmed_long: FactorSummary,
    pub confirmed_short: FactorSummary,
    pub factor_gate_passed: bool,
}

/// 单个历史币池生效窗口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UniverseWindow {
    pub from_ms: i64,
    pub to_ms: i64,
    pub members: BTreeSet<String>,
}

/// 按月份排列的 point-in-time 币池。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UniverseSchedule {
    pub version: String,
    pub windows: Vec<UniverseWindow>,
}

/// 信号时点冻结的价格候选及其后验诊断 outcome。
#[derive(Debug, Clone, PartialEq)]
struct PanelCandidate {
    symbol: String,
    decision_ts: i64,
    long: bool,
    forward_1h: f64,
    forward_4h: f64,
}

/// 绑定候选与信号前订单簿因子。
#[derive(Debug, Clone, PartialEq)]
struct PanelObservation {
    candidate: PanelCandidate,
    depth: DepthFactor,
}

/// 解析冻结面板参数；未知参数直接失败。
pub fn parse_orderbook_depth_panel_args<I>(values: I) -> Result<OrderbookDepthPanelArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut manifest = None;
    let mut cache = None;
    let mut download_concurrency = 16usize;
    let mut binance_rest_base = DEFAULT_BINANCE_REST_BASE.to_owned();
    let mut binance_data_base = DEFAULT_BINANCE_DATA_BASE.to_owned();
    while let Some(arg) = values.next() {
        let value = |values: &mut I::IntoIter| {
            values
                .next()
                .ok_or_else(|| anyhow!("{arg} requires a value"))
        };
        match arg.as_str() {
            "--manifest" => manifest = Some(PathBuf::from(value(&mut values)?)),
            "--cache" => cache = Some(PathBuf::from(value(&mut values)?)),
            "--download-concurrency" => {
                download_concurrency = value(&mut values)?
                    .parse()
                    .context("parse --download-concurrency")?
            }
            "--binance-rest-base" => {
                binance_rest_base = value(&mut values)?.trim_end_matches('/').to_owned()
            }
            "--binance-data-base" => {
                binance_data_base = value(&mut values)?.trim_end_matches('/').to_owned()
            }
            "--help" | "-h" => bail!(orderbook_depth_panel_usage()),
            _ => bail!("unknown argument: {arg}\n{}", orderbook_depth_panel_usage()),
        }
    }
    if !(1..=32).contains(&download_concurrency) {
        bail!("--download-concurrency must be between 1 and 32");
    }
    Ok(OrderbookDepthPanelArgs {
        manifest: manifest.context("--manifest is required")?,
        cache: cache.context("--cache is required")?,
        download_concurrency,
        binance_rest_base,
        binance_data_base,
    })
}

/// 返回冻结面板的最小命令用法。
pub fn orderbook_depth_panel_usage() -> &'static str {
    "Usage: market_orderbook_depth_panel --manifest PATH --cache PATH [--download-concurrency 16]"
}

impl UniverseSchedule {
    /// 从当前 live-only 历史 manifest 构造连续研究窗口。
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
            bail!("orderbook panel requires current-live crypto-only OKX 15m manifest");
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
        if windows.len() != 12
            || windows.iter().any(|window| {
                window.from_ms >= window.to_ms
                    || window.members.is_empty()
                    || window.members.iter().any(|symbol| !valid_symbol(symbol))
            })
            || windows
                .windows(2)
                .any(|pair| pair[0].to_ms != pair[1].from_ms)
        {
            bail!("orderbook panel requires twelve contiguous non-empty monthly windows");
        }
        Ok(Self {
            version: manifest.universe_version,
            windows,
        })
    }

    /// 返回全部窗口成员的去重并集。
    pub(super) fn union_symbols(&self) -> Vec<String> {
        self.windows
            .iter()
            .flat_map(|window| window.members.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// 查找指定决策时点所属的窗口。
    fn window_at(&self, ts: i64) -> Option<&UniverseWindow> {
        self.windows
            .iter()
            .find(|window| ts >= window.from_ms && ts < window.to_ms)
    }
}

/// 运行冻结面板并打印一次性因子结果。
pub async fn run_orderbook_depth_panel(
    args: &OrderbookDepthPanelArgs,
    database_url: &str,
) -> Result<OrderbookDepthPanelReport> {
    let (schedule, symbols, depth_audit, stages, observations) =
        load_panel_observations(args, database_url).await?;
    let report = build_report(&schedule, symbols, depth_audit, stages, &observations);
    print_report(&report);
    Ok(report)
}

/// 运行独立窗口的对手盘深度消耗因子面板。
pub async fn run_orderbook_absorption_panel(
    args: &OrderbookDepthPanelArgs,
    database_url: &str,
) -> Result<OrderbookAbsorptionPanelReport> {
    let (schedule, symbols, depth_audit, stages, observations) =
        load_panel_observations(args, database_url).await?;
    let report = build_absorption_report(&schedule, symbols, depth_audit, stages, &observations);
    print_absorption_report(&report);
    Ok(report)
}

/// 复用严格币池、价格候选、前瞻 outcome 与 bookDepth 完整性读取。
async fn load_panel_observations(
    args: &OrderbookDepthPanelArgs,
    database_url: &str,
) -> Result<(
    UniverseSchedule,
    usize,
    BookDepthAudit,
    OrderbookDepthPanelStages,
    Vec<PanelObservation>,
)> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode orderbook panel universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let first = schedule.windows.first().context("missing first window")?;
    let last = schedule.windows.last().context("missing last window")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for orderbook factor panel")?;
    let mut candles_by_symbol = BTreeMap::<String, Vec<CandleItem>>::new();
    for symbol in schedule.union_symbols() {
        candles_by_symbol.insert(
            symbol.clone(),
            load_symbol_candles(
                &pool,
                &symbol,
                first.from_ms.saturating_sub(2 * DAY_MS),
                last.to_ms.saturating_add(DAY_MS),
            )
            .await?,
        );
    }
    let (candidates, candidate_times, mut stages) = build_candidates(&schedule, &candles_by_symbol);
    let (depth, depth_audit) = load_book_depth(args, &schedule, &candidate_times).await?;
    let observations = candidates
        .into_iter()
        .filter_map(|candidate| {
            let factor = *depth.get(&(candidate.symbol.clone(), candidate.decision_ts))?;
            stages.depth_available += 1;
            if factor.imbalance == 0.0 {
                stages.depth_neutral += 1;
            }
            Some(PanelObservation {
                candidate,
                depth: factor,
            })
        })
        .collect::<Vec<_>>();
    Ok((
        schedule,
        candles_by_symbol.len(),
        depth_audit,
        stages,
        observations,
    ))
}

/// 先按完整因子计算覆盖，再确定性选择每个方向的两个价格候选。
fn build_candidates(
    schedule: &UniverseSchedule,
    candles_by_symbol: &BTreeMap<String, Vec<CandleItem>>,
) -> (
    Vec<PanelCandidate>,
    BTreeMap<String, Vec<i64>>,
    OrderbookDepthPanelStages,
) {
    let mut grouped = BTreeMap::<i64, Vec<(String, usize, f64, bool)>>::new();
    let mut coverage_by_time = BTreeMap::<i64, usize>::new();
    for (symbol, candles) in candles_by_symbol {
        for index in RETURN_24H_BARS..candles.len().saturating_sub(FORWARD_4H_BARS + 1) {
            let decision_ts = candles[index].ts.saturating_add(MS_15M);
            if decision_ts.rem_euclid(MS_8H) != 0
                || !schedule
                    .window_at(decision_ts)
                    .is_some_and(|window| window.members.contains(symbol))
                || candles[index].ts - candles[index - RETURN_24H_BARS].ts
                    != RETURN_24H_BARS as i64 * MS_15M
                || candles[index].ts - candles[index - RETURN_6H_BARS].ts
                    != RETURN_6H_BARS as i64 * MS_15M
            {
                continue;
            }
            let return_6h = candles[index].c / candles[index - RETURN_6H_BARS].c - 1.0;
            let return_24h = candles[index].c / candles[index - RETURN_24H_BARS].c - 1.0;
            if !return_6h.is_finite()
                || !return_24h.is_finite()
                || return_6h == 0.0
                || return_24h == 0.0
            {
                continue;
            }
            *coverage_by_time.entry(decision_ts).or_default() += 1;
            if return_6h.is_sign_positive() == return_24h.is_sign_positive() {
                grouped.entry(decision_ts).or_default().push((
                    symbol.clone(),
                    index,
                    return_6h,
                    return_6h > 0.0,
                ));
            }
        }
    }
    let mut candidates = Vec::new();
    let mut candidate_times = BTreeMap::<String, Vec<i64>>::new();
    let mut stages = OrderbookDepthPanelStages::default();
    for (decision_ts, coverage) in coverage_by_time {
        stages.decision_points += 1;
        let Some(window) = schedule.window_at(decision_ts) else {
            continue;
        };
        let minimum = (window.members.len() as f64 * PRICE_COVERAGE_MIN_RATIO).ceil() as usize;
        if coverage < minimum {
            stages.price_coverage_blocked += 1;
            continue;
        }
        let values = grouped.remove(&decision_ts).unwrap_or_default();
        stages.aligned_price_observations += values.len();
        for long in [true, false] {
            let mut directional = values
                .iter()
                .filter(|(_, _, _, candidate_long)| *candidate_long == long)
                .cloned()
                .collect::<Vec<_>>();
            directional.sort_by(|left, right| {
                let order = if long {
                    right.2.total_cmp(&left.2)
                } else {
                    left.2.total_cmp(&right.2)
                };
                order.then_with(|| left.0.cmp(&right.0))
            });
            directional.truncate(CANDIDATES_PER_DIRECTION);
            for (symbol, index, _, long) in directional {
                let candles = &candles_by_symbol[&symbol];
                let Some((forward_1h, forward_4h)) = forward_returns(candles, index, long) else {
                    stages.incomplete_outcomes += 1;
                    continue;
                };
                stages.selected_price_candidates += 1;
                candidate_times
                    .entry(symbol.clone())
                    .or_default()
                    .push(decision_ts);
                candidates.push(PanelCandidate {
                    symbol,
                    decision_ts,
                    long,
                    forward_1h,
                    forward_4h,
                });
            }
        }
    }
    candidates.sort_by(|left, right| {
        left.decision_ts
            .cmp(&right.decision_ts)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    (candidates, candidate_times, stages)
}

/// 从下一根 15m 开盘计算固定 1h 与 4h 方向收益，不参与候选排序。
fn forward_returns(
    candles: &[CandleItem],
    decision_index: usize,
    long: bool,
) -> Option<(f64, f64)> {
    let entry_index = decision_index + 1;
    let exit_1h_index = entry_index + FORWARD_1H_BARS - 1;
    let exit_4h_index = entry_index + FORWARD_4H_BARS - 1;
    let entry = candles.get(entry_index)?.o;
    let exit_1h = candles.get(exit_1h_index)?.c;
    let exit_4h = candles.get(exit_4h_index)?.c;
    if candles[exit_4h_index].ts - candles[entry_index].ts != (FORWARD_4H_BARS - 1) as i64 * MS_15M
        || !entry.is_finite()
        || entry <= 0.0
        || !exit_1h.is_finite()
        || !exit_4h.is_finite()
    {
        return None;
    }
    Some(if long {
        (exit_1h / entry - 1.0, exit_4h / entry - 1.0)
    } else {
        (entry / exit_1h - 1.0, entry / exit_4h - 1.0)
    })
}

/// 按 aligned/opposed、时间段和方向构造预注册面板并判断因子门槛。
fn build_report(
    schedule: &UniverseSchedule,
    symbols: usize,
    depth_audit: BookDepthAudit,
    stages: OrderbookDepthPanelStages,
    observations: &[PanelObservation],
) -> OrderbookDepthPanelReport {
    let split_ms = schedule.windows[6].from_ms;
    let aligned = |observation: &&PanelObservation| {
        (observation.candidate.long && observation.depth.imbalance > 0.0)
            || (!observation.candidate.long && observation.depth.imbalance < 0.0)
    };
    let opposed = |observation: &&PanelObservation| {
        (observation.candidate.long && observation.depth.imbalance < 0.0)
            || (!observation.candidate.long && observation.depth.imbalance > 0.0)
    };
    let select = |predicate: &dyn Fn(&&PanelObservation) -> bool| {
        observations.iter().filter(predicate).collect::<Vec<_>>()
    };
    let aligned_all = select(&aligned);
    let opposed_all = select(&opposed);
    let aligned_discovery_values = aligned_all
        .iter()
        .filter(|value| value.candidate.decision_ts < split_ms)
        .copied()
        .collect::<Vec<_>>();
    let opposed_discovery_values = opposed_all
        .iter()
        .filter(|value| value.candidate.decision_ts < split_ms)
        .copied()
        .collect::<Vec<_>>();
    let aligned_validation_values = aligned_all
        .iter()
        .filter(|value| value.candidate.decision_ts >= split_ms)
        .copied()
        .collect::<Vec<_>>();
    let opposed_validation_values = opposed_all
        .iter()
        .filter(|value| value.candidate.decision_ts >= split_ms)
        .copied()
        .collect::<Vec<_>>();
    let aligned_long_values = aligned_all
        .iter()
        .filter(|value| value.candidate.long)
        .copied()
        .collect::<Vec<_>>();
    let aligned_short_values = aligned_all
        .iter()
        .filter(|value| !value.candidate.long)
        .copied()
        .collect::<Vec<_>>();
    let aligned_overall = summarize(&aligned_all);
    let opposed_overall = summarize(&opposed_all);
    let aligned_discovery = summarize(&aligned_discovery_values);
    let opposed_discovery = summarize(&opposed_discovery_values);
    let aligned_validation = summarize(&aligned_validation_values);
    let opposed_validation = summarize(&opposed_validation_values);
    let aligned_long = summarize(&aligned_long_values);
    let aligned_short = summarize(&aligned_short_values);
    let factor_gate_passed = observations.len() >= 600
        && aligned_discovery.observations >= 100
        && opposed_discovery.observations >= 100
        && aligned_validation.observations >= 100
        && opposed_validation.observations >= 100
        && factor_segment_passed(&aligned_discovery, &opposed_discovery)
        && factor_segment_passed(&aligned_validation, &opposed_validation)
        && aligned_long.observations >= 100
        && aligned_short.observations >= 100
        && aligned_long
            .mean_forward_4h
            .is_some_and(|value| value > 0.0)
        && aligned_short
            .mean_forward_4h
            .is_some_and(|value| value > 0.0);
    OrderbookDepthPanelReport {
        rule_version: RULE_VERSION.to_owned(),
        universe_version: schedule.version.clone(),
        symbols,
        depth_audit,
        stages,
        aligned_overall,
        opposed_overall,
        aligned_discovery,
        opposed_discovery,
        aligned_validation,
        opposed_validation,
        aligned_long,
        aligned_short,
        factor_gate_passed,
    }
}

/// 判断一个封存时间段是否达到均值、命中率和相对增量门槛。
fn factor_segment_passed(aligned: &FactorSummary, opposed: &FactorSummary) -> bool {
    aligned
        .mean_forward_4h
        .zip(opposed.mean_forward_4h)
        .is_some_and(|(aligned_mean, opposed_mean)| {
            aligned_mean >= 0.002 && aligned_mean - opposed_mean >= 0.0015
        })
        && aligned
            .positive_rate_4h_pct
            .is_some_and(|value| value >= 55.0)
}

/// 按对手盘方向和对应深度下降构造 confirmed/control 面板。
fn build_absorption_report(
    schedule: &UniverseSchedule,
    symbols: usize,
    depth_audit: BookDepthAudit,
    stages: OrderbookDepthPanelStages,
    observations: &[PanelObservation],
) -> OrderbookAbsorptionPanelReport {
    let split_ms = schedule.windows[6].from_ms;
    let confirmed = |observation: &&PanelObservation| {
        let opposed = if observation.candidate.long {
            observation.depth.imbalance < 0.0
        } else {
            observation.depth.imbalance > 0.0
        };
        let obstacle_depleted = if observation.candidate.long {
            observation.depth.ask_change < 0.0
        } else {
            observation.depth.bid_change < 0.0
        };
        opposed && obstacle_depleted
    };
    let control = |observation: &&PanelObservation| {
        observation.depth.imbalance != 0.0 && !confirmed(observation)
    };
    let confirmed_all = observations.iter().filter(confirmed).collect::<Vec<_>>();
    let control_all = observations.iter().filter(control).collect::<Vec<_>>();
    let confirmed_discovery_values = confirmed_all
        .iter()
        .filter(|value| value.candidate.decision_ts < split_ms)
        .copied()
        .collect::<Vec<_>>();
    let control_discovery_values = control_all
        .iter()
        .filter(|value| value.candidate.decision_ts < split_ms)
        .copied()
        .collect::<Vec<_>>();
    let confirmed_validation_values = confirmed_all
        .iter()
        .filter(|value| value.candidate.decision_ts >= split_ms)
        .copied()
        .collect::<Vec<_>>();
    let control_validation_values = control_all
        .iter()
        .filter(|value| value.candidate.decision_ts >= split_ms)
        .copied()
        .collect::<Vec<_>>();
    let confirmed_long_values = confirmed_all
        .iter()
        .filter(|value| value.candidate.long)
        .copied()
        .collect::<Vec<_>>();
    let confirmed_short_values = confirmed_all
        .iter()
        .filter(|value| !value.candidate.long)
        .copied()
        .collect::<Vec<_>>();
    let confirmed_overall = summarize(&confirmed_all);
    let control_overall = summarize(&control_all);
    let confirmed_discovery = summarize(&confirmed_discovery_values);
    let control_discovery = summarize(&control_discovery_values);
    let confirmed_validation = summarize(&confirmed_validation_values);
    let control_validation = summarize(&control_validation_values);
    let confirmed_long = summarize(&confirmed_long_values);
    let confirmed_short = summarize(&confirmed_short_values);
    let factor_gate_passed = observations.len() >= 600
        && confirmed_discovery.observations >= 100
        && control_discovery.observations >= 100
        && confirmed_validation.observations >= 100
        && control_validation.observations >= 100
        && absorption_segment_passed(&confirmed_discovery, &control_discovery)
        && absorption_segment_passed(&confirmed_validation, &control_validation)
        && confirmed_long.observations >= 100
        && confirmed_short.observations >= 100
        && confirmed_long
            .mean_forward_4h
            .is_some_and(|value| value > 0.0)
        && confirmed_short
            .mean_forward_4h
            .is_some_and(|value| value > 0.0);
    OrderbookAbsorptionPanelReport {
        rule_version: ABSORPTION_RULE_VERSION.to_owned(),
        universe_version: schedule.version.clone(),
        symbols,
        depth_audit,
        stages,
        confirmed_overall,
        control_overall,
        confirmed_discovery,
        control_discovery,
        confirmed_validation,
        control_validation,
        confirmed_long,
        confirmed_short,
        factor_gate_passed,
    }
}

/// 判断动态对手盘消耗在一个封存时间段内是否通过门槛。
fn absorption_segment_passed(confirmed: &FactorSummary, control: &FactorSummary) -> bool {
    confirmed
        .mean_forward_4h
        .zip(control.mean_forward_4h)
        .is_some_and(|(confirmed_mean, control_mean)| {
            confirmed_mean >= 0.0025 && confirmed_mean - control_mean >= 0.0015
        })
        && confirmed
            .positive_rate_4h_pct
            .is_some_and(|value| value >= 55.0)
}

/// 汇总有限样本的均值与严格正收益率。
fn summarize(values: &[&PanelObservation]) -> FactorSummary {
    if values.is_empty() {
        return FactorSummary::default();
    }
    let count = values.len() as f64;
    FactorSummary {
        observations: values.len(),
        mean_forward_1h: Some(
            values
                .iter()
                .map(|value| value.candidate.forward_1h)
                .sum::<f64>()
                / count,
        ),
        mean_forward_4h: Some(
            values
                .iter()
                .map(|value| value.candidate.forward_4h)
                .sum::<f64>()
                / count,
        ),
        positive_rate_1h_pct: Some(
            values
                .iter()
                .filter(|value| value.candidate.forward_1h > 0.0)
                .count() as f64
                / count
                * 100.0,
        ),
        positive_rate_4h_pct: Some(
            values
                .iter()
                .filter(|value| value.candidate.forward_4h > 0.0)
                .count() as f64
                / count
                * 100.0,
        ),
    }
}

/// 打印数据审计、漏斗与全部预注册分组。
fn print_report(report: &OrderbookDepthPanelReport) {
    println!(
        "orderbook_depth_panel\trule={}\tuniverse={}\tsymbols={}\tmapped={}\tmapping_blocked={}\trequested_files={}\tavailable_files={}\tmissing_files={}\tinvalid_files={}\tincomplete_windows={}\tdecision_points={}\tcoverage_blocked={}\taligned_price_observations={}\tselected_price_candidates={}\tincomplete_outcomes={}\tdepth_available={}\tdepth_neutral={}\tfactor_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.symbols,
        report.depth_audit.mapped_symbols,
        report.depth_audit.mapping_blocked_symbols,
        report.depth_audit.requested_files,
        report.depth_audit.available_files,
        report.depth_audit.missing_files,
        report.depth_audit.invalid_files,
        report.depth_audit.incomplete_windows,
        report.stages.decision_points,
        report.stages.price_coverage_blocked,
        report.stages.aligned_price_observations,
        report.stages.selected_price_candidates,
        report.stages.incomplete_outcomes,
        report.stages.depth_available,
        report.stages.depth_neutral,
        report.factor_gate_passed,
    );
    for (label, summary) in [
        ("aligned_overall", &report.aligned_overall),
        ("opposed_overall", &report.opposed_overall),
        ("aligned_discovery", &report.aligned_discovery),
        ("opposed_discovery", &report.opposed_discovery),
        ("aligned_validation", &report.aligned_validation),
        ("opposed_validation", &report.opposed_validation),
        ("aligned_long", &report.aligned_long),
        ("aligned_short", &report.aligned_short),
    ] {
        println!(
            "orderbook_depth_factor\tgroup={}\tobservations={}\tmean_forward_1h={}\tmean_forward_4h={}\tpositive_rate_1h_pct={}\tpositive_rate_4h_pct={}",
            label,
            summary.observations,
            optional(summary.mean_forward_1h),
            optional(summary.mean_forward_4h),
            optional(summary.positive_rate_1h_pct),
            optional(summary.positive_rate_4h_pct),
        );
    }
}

/// 打印动态对手盘消耗面板的数据审计与全部预注册分组。
fn print_absorption_report(report: &OrderbookAbsorptionPanelReport) {
    println!(
        "orderbook_absorption_panel\trule={}\tuniverse={}\tsymbols={}\tmapped={}\tmapping_blocked={}\trequested_files={}\tavailable_files={}\tmissing_files={}\tinvalid_files={}\tincomplete_windows={}\tdecision_points={}\tcoverage_blocked={}\taligned_price_observations={}\tselected_price_candidates={}\tincomplete_outcomes={}\tdepth_available={}\tdepth_neutral={}\tfactor_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.symbols,
        report.depth_audit.mapped_symbols,
        report.depth_audit.mapping_blocked_symbols,
        report.depth_audit.requested_files,
        report.depth_audit.available_files,
        report.depth_audit.missing_files,
        report.depth_audit.invalid_files,
        report.depth_audit.incomplete_windows,
        report.stages.decision_points,
        report.stages.price_coverage_blocked,
        report.stages.aligned_price_observations,
        report.stages.selected_price_candidates,
        report.stages.incomplete_outcomes,
        report.stages.depth_available,
        report.stages.depth_neutral,
        report.factor_gate_passed,
    );
    for (label, summary) in [
        ("confirmed_overall", &report.confirmed_overall),
        ("control_overall", &report.control_overall),
        ("confirmed_discovery", &report.confirmed_discovery),
        ("control_discovery", &report.control_discovery),
        ("confirmed_validation", &report.confirmed_validation),
        ("control_validation", &report.control_validation),
        ("confirmed_long", &report.confirmed_long),
        ("confirmed_short", &report.confirmed_short),
    ] {
        println!(
            "orderbook_absorption_factor\tgroup={}\tobservations={}\tmean_forward_1h={}\tmean_forward_4h={}\tpositive_rate_1h_pct={}\tpositive_rate_4h_pct={}",
            label,
            summary.observations,
            optional(summary.mean_forward_1h),
            optional(summary.mean_forward_4h),
            optional(summary.positive_rate_1h_pct),
            optional(summary.positive_rate_4h_pct),
        );
    }
}

/// 将可选浮点统一为稳定文本。
fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

/// 从本地 quant_core 读取已确认且严格排序的 15m K 线。
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
        .with_context(|| format!("load orderbook-panel candles from {table}"))?
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

/// 解析数据库数值文本并拒绝非有限值。
fn parse_number(value: String) -> Result<f64> {
    let parsed = value.parse::<f64>().context("parse candle number")?;
    if !parsed.is_finite() {
        bail!("candle number must be finite");
    }
    Ok(parsed)
}

/// 只允许规范 OKX USDT 永续标识进入动态表名。
fn valid_symbol(symbol: &str) -> bool {
    symbol.ends_with("-USDT-SWAP")
        && symbol
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_returns_use_next_open_and_are_direction_symmetric() {
        let candles = (0..120)
            .map(|index| CandleItem {
                ts: index * MS_15M,
                o: 100.0,
                h: 102.0,
                l: 98.0,
                c: if index == 100 { 102.0 } else { 100.0 },
                v: 1.0,
                confirm: 1,
            })
            .collect::<Vec<_>>();

        let long = forward_returns(&candles, 84, true).unwrap();
        let short = forward_returns(&candles, 84, false).unwrap();

        assert!((long.1 - 0.02).abs() < 1e-12);
        assert!((short.1 - (100.0 / 102.0 - 1.0)).abs() < 1e-12);
    }

    #[test]
    fn factor_gate_requires_both_time_segments_and_directions() {
        let passed = FactorSummary {
            observations: 120,
            mean_forward_1h: Some(0.001),
            mean_forward_4h: Some(0.003),
            positive_rate_1h_pct: Some(52.0),
            positive_rate_4h_pct: Some(56.0),
        };
        let opposed = FactorSummary {
            observations: 120,
            mean_forward_1h: Some(0.0),
            mean_forward_4h: Some(0.001),
            positive_rate_1h_pct: Some(50.0),
            positive_rate_4h_pct: Some(50.0),
        };

        assert!(factor_segment_passed(&passed, &opposed));
        assert!(absorption_segment_passed(&passed, &opposed));
    }
}
