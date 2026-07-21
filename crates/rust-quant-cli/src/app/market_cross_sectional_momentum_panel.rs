use crate::app::okx_historical_universe::HistoricalUniverseManifest;
use anyhow::{bail, Context, Result};
use rust_quant_strategies::CandleItem;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MS_15M: i64 = 15 * 60 * 1_000;
const MS_8H: i64 = 8 * 60 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const LOOKBACK_BARS: usize = 24 * 4;
const FORWARD_8H_BARS: usize = 8 * 4;
const FORWARD_24H_BARS: usize = 24 * 4;
const MIN_FACTOR_COVERAGE: f64 = 0.80;
const RULE_VERSION: &str = "top1_bottom1_24h_return_equal_notional_8h_v1";

/// 冻结面板入口只接受 current-live 历史币池路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossSectionalMomentumArgs {
    /// 已完成 current-live crypto-only 审计的十二个月 manifest。
    pub manifest: PathBuf,
}

/// 记录横截面覆盖、排序、共同入场和 outcome 漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrossSectionalMomentumStages {
    /// 十二个月内的 UTC 8 小时决策时点数。
    pub decision_points: usize,
    /// 完整 24h 因子覆盖低于当月成员 80% 的时点数。
    pub coverage_blocked: usize,
    /// 未阻塞时点中成功计算的 24h 收益观察数。
    pub factor_observations: usize,
    /// 完成极端和中间分位四腿排序的时点数。
    pub selected_pairs: usize,
    /// 缺少任一共同入场或完整 24h outcome 的时点数。
    pub incomplete_outcomes: usize,
}

/// 一个预注册价差或单腿分组的 8h/24h 毛收益与命中率。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CrossSectionalMomentumSummary {
    /// 当前分组的有效观察数。
    pub observations: usize,
    /// 8h 平均毛收益。
    pub mean_forward_8h: Option<f64>,
    /// 24h 平均毛收益。
    pub mean_forward_24h: Option<f64>,
    /// 8h 收益为正的比例，单位百分比。
    pub positive_rate_8h_pct: Option<f64>,
    /// 24h 收益为正的比例，单位百分比。
    pub positive_rate_24h_pct: Option<f64>,
}

/// 横截面动量价差的覆盖、稳定性、方向贡献和集中度报告。
#[derive(Debug, Clone, PartialEq)]
pub struct CrossSectionalMomentumReport {
    /// 冻结横截面、对照与 outcome 规则身份。
    pub rule_version: String,
    /// 历史币池版本。
    pub universe_version: String,
    /// 实际加载的唯一 current-live 合约数。
    pub symbols: usize,
    /// 因果候选漏斗。
    pub stages: CrossSectionalMomentumStages,
    /// 相邻不超过 8h 的时点聚类数。
    pub effective_events_8h: usize,
    /// 全窗口 rank1/rankN 动量价差。
    pub momentum_overall: CrossSectionalMomentumSummary,
    /// 全窗口 25%/75% 中间分位对照价差。
    pub control_overall: CrossSectionalMomentumSummary,
    /// 前六个月动量价差。
    pub momentum_discovery: CrossSectionalMomentumSummary,
    /// 前六个月对照价差。
    pub control_discovery: CrossSectionalMomentumSummary,
    /// 后六个月动量价差。
    pub momentum_validation: CrossSectionalMomentumSummary,
    /// 后六个月对照价差。
    pub control_validation: CrossSectionalMomentumSummary,
    /// 动量多头腿自身收益。
    pub long_leg: CrossSectionalMomentumSummary,
    /// 动量空头腿按做空方向换算后的收益。
    pub short_leg: CrossSectionalMomentumSummary,
    /// 每个历史月份的动量价差。
    pub monthly: Vec<(i64, CrossSectionalMomentumSummary)>,
    /// 参与极端多空腿次数最多的合约。
    pub most_frequent_symbol: Option<String>,
    /// 该合约参与极端多空腿的次数。
    pub most_frequent_symbol_count: usize,
    /// 该次数除以价差观察数的比例，单位百分比。
    pub most_frequent_symbol_pct: Option<f64>,
    /// 是否通过全部预注册边际价值与集中度门槛。
    pub factor_gate_passed: bool,
}

/// 单个历史月份与其 current-live 成员。
#[derive(Debug, Clone, PartialEq, Eq)]
struct UniverseWindow {
    /// 生效起点，Unix 毫秒且包含。
    from_ms: i64,
    /// 生效终点，Unix 毫秒且不包含。
    to_ms: i64,
    /// 当月流动性 Top60 合约。
    members: BTreeSet<String>,
}

/// 连续十二个月的 point-in-time 币池。
#[derive(Debug, Clone, PartialEq, Eq)]
struct UniverseSchedule {
    /// manifest 中冻结的版本。
    version: String,
    /// 连续十二个月窗口。
    windows: Vec<UniverseWindow>,
}

/// 单个腿从下一开盘到两个固定期限的简单收益。
#[derive(Debug, Clone, Copy, PartialEq)]
struct LegOutcome {
    /// 下一共同开盘至 8h 的裸价格收益。
    forward_8h: f64,
    /// 下一共同开盘至 24h 的裸价格收益。
    forward_24h: f64,
}

/// 一个时点同时保存极端动量和中间分位对照的等名义结果。
#[derive(Debug, Clone, PartialEq)]
struct SpreadObservation {
    /// 因子决策时间。
    decision_ts: i64,
    /// rank1 动量多头合约。
    long_symbol: String,
    /// rankN 动量空头合约。
    short_symbol: String,
    /// 极端多头腿结果。
    long_outcome: LegOutcome,
    /// 极端空头腿裸价格结果；报告中取负号作为做空 PnL。
    short_outcome: LegOutcome,
    /// 25% 分位对照多头腿结果。
    control_long_outcome: LegOutcome,
    /// 75% 分位对照空头腿裸价格结果。
    control_short_outcome: LegOutcome,
}

/// 解析冻结参数；未知参数直接失败。
pub fn parse_cross_sectional_momentum_args<I>(values: I) -> Result<CrossSectionalMomentumArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut manifest = None;
    while let Some(arg) = values.next() {
        match arg.as_str() {
            "--manifest" => {
                manifest = Some(PathBuf::from(
                    values.next().context("--manifest requires a value")?,
                ));
            }
            "--help" | "-h" => bail!(cross_sectional_momentum_usage()),
            _ => bail!(
                "unknown argument: {arg}\n{}",
                cross_sectional_momentum_usage()
            ),
        }
    }
    Ok(CrossSectionalMomentumArgs {
        manifest: manifest.context("--manifest is required")?,
    })
}

/// 返回冻结因子面板的最小命令用法。
pub fn cross_sectional_momentum_usage() -> &'static str {
    "Usage: market_cross_sectional_momentum_panel --manifest PATH"
}

/// 运行等名义横截面动量价差因子面板，不写交易事实或触发执行。
pub async fn run_cross_sectional_momentum_panel(
    args: &CrossSectionalMomentumArgs,
    database_url: &str,
) -> Result<CrossSectionalMomentumReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode cross-sectional momentum universe manifest")?;
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
        .context("connect quant_core for cross-sectional momentum panel")?;
    let mut series = BTreeMap::<String, Vec<CandleItem>>::new();
    for symbol in schedule.union_symbols() {
        series.insert(
            symbol.clone(),
            load_symbol_candles(
                &pool,
                &symbol,
                first.from_ms.saturating_sub(2 * DAY_MS),
                last.to_ms.saturating_add(2 * DAY_MS),
            )
            .await?,
        );
    }
    let (observations, stages) = build_observations(&schedule, &series);
    let report = build_report(&schedule, series.len(), stages, &observations);
    print_report(&report);
    Ok(report)
}

impl UniverseSchedule {
    /// 从 current-live crypto-only manifest 构造连续十二个月窗口。
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
            bail!("cross-sectional momentum requires current-live crypto-only OKX 15m manifest");
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
                    || window.members.len() < 4
                    || window.members.iter().any(|symbol| !valid_symbol(symbol))
            })
            || windows
                .windows(2)
                .any(|pair| pair[0].to_ms != pair[1].from_ms)
        {
            bail!("cross-sectional momentum V1 requires twelve contiguous monthly windows");
        }
        Ok(Self {
            version: manifest.universe_version,
            windows,
        })
    }

    /// 返回全部窗口成员的确定性去重并集。
    fn union_symbols(&self) -> Vec<String> {
        self.windows
            .iter()
            .flat_map(|window| window.members.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// 查找指定决策时间所属的币池窗口。
    fn window_at(&self, ts: i64) -> Option<&UniverseWindow> {
        self.windows
            .iter()
            .find(|window| ts >= window.from_ms && ts < window.to_ms)
    }
}

/// 每个 UTC 8 小时按 24h 收益排序，并同时冻结极端与中间分位价差。
fn build_observations(
    schedule: &UniverseSchedule,
    series: &BTreeMap<String, Vec<CandleItem>>,
) -> (Vec<SpreadObservation>, CrossSectionalMomentumStages) {
    let mut stages = CrossSectionalMomentumStages::default();
    let mut observations = Vec::new();
    let Some(first) = schedule.windows.first() else {
        return (observations, stages);
    };
    let Some(last) = schedule.windows.last() else {
        return (observations, stages);
    };
    let mut decision_ts = first.from_ms;
    while decision_ts < last.to_ms {
        let Some(window) = schedule.window_at(decision_ts) else {
            decision_ts = decision_ts.saturating_add(MS_8H);
            continue;
        };
        stages.decision_points += 1;
        let minimum = (window.members.len() as f64 * MIN_FACTOR_COVERAGE).ceil() as usize;
        let mut ranked = Vec::<(String, f64)>::new();
        for symbol in &window.members {
            let Some(candles) = series.get(symbol) else {
                continue;
            };
            if let Some(value) = return_24h_at(candles, decision_ts.saturating_sub(MS_15M)) {
                ranked.push((symbol.clone(), value));
            }
        }
        if minimum == 0 || ranked.len() < minimum {
            stages.coverage_blocked += 1;
            decision_ts = decision_ts.saturating_add(MS_8H);
            continue;
        }
        stages.factor_observations += ranked.len();
        ranked.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        let control_long_index = ((ranked.len() - 1) as f64 * 0.25).floor() as usize;
        let control_short_index = ((ranked.len() - 1) as f64 * 0.75).floor() as usize;
        let symbols = [
            &ranked[0].0,
            &ranked[ranked.len() - 1].0,
            &ranked[control_long_index].0,
            &ranked[control_short_index].0,
        ];
        if symbols.iter().collect::<BTreeSet<_>>().len() != 4 {
            stages.coverage_blocked += 1;
            decision_ts = decision_ts.saturating_add(MS_8H);
            continue;
        }
        stages.selected_pairs += 1;
        let outcomes = symbols
            .iter()
            .map(|symbol| {
                series
                    .get(*symbol)
                    .and_then(|candles| leg_outcome(candles, decision_ts))
            })
            .collect::<Option<Vec<_>>>();
        if let Some(outcomes) = outcomes {
            observations.push(SpreadObservation {
                decision_ts,
                long_symbol: symbols[0].to_owned(),
                short_symbol: symbols[1].to_owned(),
                long_outcome: outcomes[0],
                short_outcome: outcomes[1],
                control_long_outcome: outcomes[2],
                control_short_outcome: outcomes[3],
            });
        } else {
            stages.incomplete_outcomes += 1;
        }
        decision_ts = decision_ts.saturating_add(MS_8H);
    }
    (observations, stages)
}

/// 只用连续且已完成的 24h 前缀计算排序收益。
fn return_24h_at(candles: &[CandleItem], decision_candle_ts: i64) -> Option<f64> {
    let index = candles
        .binary_search_by_key(&decision_candle_ts, |candle| candle.ts)
        .ok()?;
    if index < LOOKBACK_BARS {
        return None;
    }
    let window = &candles[index - LOOKBACK_BARS..=index];
    if window
        .windows(2)
        .any(|pair| pair[1].ts - pair[0].ts != MS_15M)
    {
        return None;
    }
    let value = candles[index].c / candles[index - LOOKBACK_BARS].c - 1.0;
    value.is_finite().then_some(value)
}

/// 从下一根 15m 开盘计算固定 8h 与 24h 单腿收益。
fn leg_outcome(candles: &[CandleItem], decision_ts: i64) -> Option<LegOutcome> {
    let entry_index = candles
        .binary_search_by_key(&decision_ts, |candle| candle.ts)
        .ok()?;
    let exit_8h_index = entry_index.checked_add(FORWARD_8H_BARS - 1)?;
    let exit_24h_index = entry_index.checked_add(FORWARD_24H_BARS - 1)?;
    let entry = candles.get(entry_index)?.o;
    let exit_8h = candles.get(exit_8h_index)?.c;
    let exit_24h = candles.get(exit_24h_index)?.c;
    let outcome_window = candles.get(entry_index..=exit_24h_index)?;
    if entry <= 0.0
        || exit_8h <= 0.0
        || exit_24h <= 0.0
        || outcome_window
            .windows(2)
            .any(|pair| pair[1].ts - pair[0].ts != MS_15M)
    {
        return None;
    }
    let forward_8h = exit_8h / entry - 1.0;
    let forward_24h = exit_24h / entry - 1.0;
    (forward_8h.is_finite() && forward_24h.is_finite()).then_some(LegOutcome {
        forward_8h,
        forward_24h,
    })
}

/// 构造时间段、月份、单腿贡献、有效事件与币种集中度报告。
fn build_report(
    schedule: &UniverseSchedule,
    symbols: usize,
    stages: CrossSectionalMomentumStages,
    observations: &[SpreadObservation],
) -> CrossSectionalMomentumReport {
    let split_ms = schedule.windows[6].from_ms;
    let discovery = observations
        .iter()
        .filter(|value| value.decision_ts < split_ms)
        .collect::<Vec<_>>();
    let validation = observations
        .iter()
        .filter(|value| value.decision_ts >= split_ms)
        .collect::<Vec<_>>();
    let momentum_overall = summarize_spread(observations, false);
    let control_overall = summarize_spread(observations, true);
    let momentum_discovery = summarize_refs(&discovery, false);
    let control_discovery = summarize_refs(&discovery, true);
    let momentum_validation = summarize_refs(&validation, false);
    let control_validation = summarize_refs(&validation, true);
    let long_leg = summarize_legs(observations.iter().map(|value| value.long_outcome), false);
    let short_leg = summarize_legs(observations.iter().map(|value| value.short_outcome), true);
    let monthly = schedule
        .windows
        .iter()
        .map(|window| {
            let values = observations
                .iter()
                .filter(|value| {
                    value.decision_ts >= window.from_ms && value.decision_ts < window.to_ms
                })
                .collect::<Vec<_>>();
            (window.from_ms, summarize_refs(&values, false))
        })
        .collect::<Vec<_>>();
    let effective_events_8h = effective_events(observations);
    let (most_frequent_symbol, most_frequent_symbol_count) = concentration(observations);
    let most_frequent_symbol_pct = (!observations.is_empty())
        .then_some(most_frequent_symbol_count as f64 / observations.len() as f64 * 100.0);
    let factor_gate_passed = observations.len() >= 1_000
        && discovery.len() >= 500
        && validation.len() >= 500
        && effective_events_8h >= 500
        && segment_passed(&momentum_discovery, &control_discovery)
        && segment_passed(&momentum_validation, &control_validation)
        && momentum_overall
            .mean_forward_8h
            .is_some_and(|value| value > 0.0)
        && most_frequent_symbol_pct.is_some_and(|value| value <= 20.0);
    CrossSectionalMomentumReport {
        rule_version: RULE_VERSION.to_owned(),
        universe_version: schedule.version.clone(),
        symbols,
        stages,
        effective_events_8h,
        momentum_overall,
        control_overall,
        momentum_discovery,
        control_discovery,
        momentum_validation,
        control_validation,
        long_leg,
        short_leg,
        monthly,
        most_frequent_symbol,
        most_frequent_symbol_count,
        most_frequent_symbol_pct,
        factor_gate_passed,
    }
}

/// 判断封存时间段是否满足 24h 收益、命中率和对照增量。
fn segment_passed(
    momentum: &CrossSectionalMomentumSummary,
    control: &CrossSectionalMomentumSummary,
) -> bool {
    momentum
        .mean_forward_24h
        .zip(control.mean_forward_24h)
        .is_some_and(|(momentum_mean, control_mean)| {
            momentum_mean >= 0.005 && momentum_mean - control_mean >= 0.0025
        })
        && momentum
            .positive_rate_24h_pct
            .is_some_and(|value| value >= 55.0)
}

/// 汇总 owned 观察的极端或对照价差。
fn summarize_spread(values: &[SpreadObservation], control: bool) -> CrossSectionalMomentumSummary {
    let values = values.iter().collect::<Vec<_>>();
    summarize_refs(&values, control)
}

/// 汇总引用观察的极端或对照价差。
fn summarize_refs(values: &[&SpreadObservation], control: bool) -> CrossSectionalMomentumSummary {
    let legs = values.iter().map(|value| {
        if control {
            (
                value.control_long_outcome.forward_8h - value.control_short_outcome.forward_8h,
                value.control_long_outcome.forward_24h - value.control_short_outcome.forward_24h,
            )
        } else {
            (
                value.long_outcome.forward_8h - value.short_outcome.forward_8h,
                value.long_outcome.forward_24h - value.short_outcome.forward_24h,
            )
        }
    });
    summarize_values(legs)
}

/// 汇总多头腿或按做空方向转换后的空头腿。
fn summarize_legs(
    values: impl Iterator<Item = LegOutcome>,
    invert: bool,
) -> CrossSectionalMomentumSummary {
    summarize_values(values.map(|value| {
        if invert {
            (-value.forward_8h, -value.forward_24h)
        } else {
            (value.forward_8h, value.forward_24h)
        }
    }))
}

/// 汇总有限的 8h/24h 收益二元组。
fn summarize_values(values: impl Iterator<Item = (f64, f64)>) -> CrossSectionalMomentumSummary {
    let values = values.collect::<Vec<_>>();
    if values.is_empty()
        || values
            .iter()
            .any(|(forward_8h, forward_24h)| !forward_8h.is_finite() || !forward_24h.is_finite())
    {
        return CrossSectionalMomentumSummary::default();
    }
    CrossSectionalMomentumSummary {
        observations: values.len(),
        mean_forward_8h: Some(
            values.iter().map(|(forward_8h, _)| forward_8h).sum::<f64>() / values.len() as f64,
        ),
        mean_forward_24h: Some(
            values
                .iter()
                .map(|(_, forward_24h)| forward_24h)
                .sum::<f64>()
                / values.len() as f64,
        ),
        positive_rate_8h_pct: Some(
            values
                .iter()
                .filter(|(forward_8h, _)| *forward_8h > 0.0)
                .count() as f64
                / values.len() as f64
                * 100.0,
        ),
        positive_rate_24h_pct: Some(
            values
                .iter()
                .filter(|(_, forward_24h)| *forward_24h > 0.0)
                .count() as f64
                / values.len() as f64
                * 100.0,
        ),
    }
}

/// 将距事件起点不超过 8h 的决策归并，避免相邻时点链式吞并全年。
fn effective_events(values: &[SpreadObservation]) -> usize {
    let mut count = 0usize;
    let mut event_start = None::<i64>;
    for value in values {
        if event_start.is_none_or(|point| value.decision_ts - point > MS_8H) {
            count += 1;
            event_start = Some(value.decision_ts);
        }
    }
    count
}

/// 统计极端多空腿中参与次数最多的合约。
fn concentration(values: &[SpreadObservation]) -> (Option<String>, usize) {
    let mut counts = BTreeMap::<String, usize>::new();
    for value in values {
        *counts.entry(value.long_symbol.clone()).or_default() += 1;
        *counts.entry(value.short_symbol.clone()).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map_or((None, 0), |(symbol, count)| (Some(symbol), count))
}

/// 输出候选漏斗、价差、单腿、月份和集中度报告。
fn print_report(report: &CrossSectionalMomentumReport) {
    println!(
        "cross_sectional_momentum_panel\trule={}\tuniverse={}\tsymbols={}\tdecision_points={}\tcoverage_blocked={}\tfactor_observations={}\tselected_pairs={}\tincomplete={}\teffective_events_8h={}\tmost_frequent_symbol={}\tmost_frequent_count={}\tmost_frequent_pct={}\tfactor_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.symbols,
        report.stages.decision_points,
        report.stages.coverage_blocked,
        report.stages.factor_observations,
        report.stages.selected_pairs,
        report.stages.incomplete_outcomes,
        report.effective_events_8h,
        report.most_frequent_symbol.as_deref().unwrap_or("NA"),
        report.most_frequent_symbol_count,
        optional(report.most_frequent_symbol_pct),
        report.factor_gate_passed,
    );
    for (label, value) in [
        ("momentum_overall", &report.momentum_overall),
        ("control_overall", &report.control_overall),
        ("momentum_discovery", &report.momentum_discovery),
        ("control_discovery", &report.control_discovery),
        ("momentum_validation", &report.momentum_validation),
        ("control_validation", &report.control_validation),
        ("long_leg", &report.long_leg),
        ("short_leg", &report.short_leg),
    ] {
        print_summary(label, value);
    }
    for (from_ms, value) in &report.monthly {
        print_summary(&format!("month_{from_ms}"), value);
    }
}

/// 输出单个价差或腿的样本、收益与命中率。
fn print_summary(label: &str, value: &CrossSectionalMomentumSummary) {
    println!(
        "cross_sectional_momentum_summary\tgroup={}\tobservations={}\tmean_8h={}\tmean_24h={}\tpositive_8h_pct={}\tpositive_24h_pct={}",
        label,
        value.observations,
        optional(value.mean_forward_8h),
        optional(value.mean_forward_24h),
        optional(value.positive_rate_8h_pct),
        optional(value.positive_rate_24h_pct),
    );
}

/// 将缺失浮点指标稳定格式化为 `NA`。
fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_owned(), |number| number.to_string())
}

/// 从本地 quant_core 读取已确认且严格排序的 OKX 15m K 线。
async fn load_symbol_candles(
    pool: &PgPool,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<CandleItem>> {
    if !valid_symbol(symbol) {
        bail!("invalid cross-sectional momentum manifest symbol {symbol}");
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
        .with_context(|| format!("load cross-sectional momentum candles from {table}"))?
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

/// 解析数据库数值并拒绝非有限值。
fn parse_number(value: String) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse cross-sectional momentum number {value}"))?;
    if !parsed.is_finite() {
        bail!("non-finite cross-sectional momentum number {value}");
    }
    Ok(parsed)
}

/// 限制动态表名只能来自规范 OKX USDT 永续标识。
fn valid_symbol(symbol: &str) -> bool {
    symbol.ends_with("-USDT-SWAP")
        && symbol
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests;
