use super::binance_funding::{load_binance_funding, BinanceFundingAudit, BinanceFundingPoint};
use super::binance_klines::{load_binance_klines, BinanceCandle, BinanceKlineAudit};
use super::{CrossExchangeBasisPanelArgs, HistoricalUniverseManifest, UniverseSchedule, MS_15M};
use anyhow::{Context, Result};
use std::collections::BTreeMap;

const MS_8H: i64 = 8 * 60 * 60 * 1_000;
const MIN_FACTOR_COVERAGE: f64 = 0.80;
const CONTROL_SPREAD: f64 = 0.0016;
const EXECUTABLE_SPREAD: f64 = 0.0032;
const STANDARD_COST: f64 = 0.0032;
const RULE_VERSION_V1: &str = "post_settlement_bottom1_top1_hold_next_funding_8h_v1";
const RULE_VERSION_V2: &str = "post_settlement_common_min30_bottom1_top1_hold_next_funding_8h_v2";

/// 保留 V1 不可实现覆盖，并为 V2 使用冻结的共同可交易最小母集。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FundingCarryCoverage {
    /// V1：要求原 OKX Top60 成员达到 80%。
    TopSixtyEightyPercent,
    /// V2：要求信号时点至少有 30 个共同 8h funding 成员。
    CommonMinimumThirty,
}

impl FundingCarryCoverage {
    /// 返回当前版本的冻结最小横截面成员数。
    fn minimum_members(self, universe_members: usize) -> usize {
        match self {
            Self::TopSixtyEightyPercent => {
                (universe_members as f64 * MIN_FACTOR_COVERAGE).ceil() as usize
            }
            Self::CommonMinimumThirty => 30,
        }
    }

    /// 返回与覆盖规则绑定的可审计身份。
    fn rule_version(self) -> &'static str {
        match self {
            Self::TopSixtyEightyPercent => RULE_VERSION_V1,
            Self::CommonMinimumThirty => RULE_VERSION_V2,
        }
    }
}

/// Funding carry 从共同结算覆盖到固定下一结算 outcome 的漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FundingCarryStages {
    /// 研究窗口内出现的 8h funding 共同时间点。
    pub funding_timestamps: usize,
    /// 当月币池有效当前费率覆盖低于 80% 的时间点。
    pub coverage_blocked: usize,
    /// 单个时点实际可排序 8h funding 成员的最大数量。
    pub maximum_current_coverage: usize,
    /// 实际可排序成员达到 40 个的时点数。
    pub coverage_at_least_40: usize,
    /// 实际可排序成员达到 30 个的时点数。
    pub coverage_at_least_30: usize,
    /// 未阻塞时点参与排序的当前费率观察数。
    pub factor_observations: usize,
    /// 当前极差达到 32bps 的经济可执行候选。
    pub executable_candidates: usize,
    /// 当前极差位于 16bps 至 32bps 的近成本对照。
    pub control_candidates: usize,
    /// 当前极差低于 16bps、按清单不观察的时点。
    pub below_control: usize,
    /// 缺少下一费率或严格连续共同价格的候选。
    pub incomplete_outcomes: usize,
}

/// Funding carry 分组的费率持久性、价格和成本后收益摘要。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FundingCarrySummary {
    /// 完整观察数。
    pub observations: usize,
    /// 信号时点最高费率减最低费率的平均值。
    pub mean_current_funding_spread: Option<f64>,
    /// 下一实际结算给双腿带来的平均 funding PnL。
    pub mean_next_funding_pnl: Option<f64>,
    /// 等名义多头减空头的平均价格 PnL。
    pub mean_price_pnl: Option<f64>,
    /// 价格与 funding 合计的零成本平均 PnL。
    pub mean_gross_pnl: Option<f64>,
    /// 扣除 32bps 四次成交后的平均 PnL。
    pub mean_standard_pnl: Option<f64>,
    /// 标准成本后收益为正的比例，单位百分比。
    pub standard_positive_rate_pct: Option<f64>,
    /// 扣除 64bps 双倍成本后的平均 PnL。
    pub mean_double_cost_pnl: Option<f64>,
}

/// 跨币种 funding carry 因子面板完整审计报告。
#[derive(Debug, Clone, PartialEq)]
pub struct CrossSectionalFundingCarryReport {
    /// 冻结信号、发布缓冲和持有期身份。
    pub rule_version: String,
    /// 历史币池版本。
    pub universe_version: String,
    /// OKX current-live 历史币池唯一成员数。
    pub okx_symbols: usize,
    /// Binance regular 15m 映射和文件审计。
    pub kline_audit: BinanceKlineAudit,
    /// Binance funding 映射和文件审计。
    pub funding_audit: BinanceFundingAudit,
    /// 共同结算候选漏斗。
    pub stages: FundingCarryStages,
    /// 可执行组全窗口摘要。
    pub executable_overall: FundingCarrySummary,
    /// 近成本对照全窗口摘要。
    pub control_overall: FundingCarrySummary,
    /// 前六个月可执行组。
    pub executable_discovery: FundingCarrySummary,
    /// 前六个月近成本对照。
    pub control_discovery: FundingCarrySummary,
    /// 后六个月可执行组。
    pub executable_validation: FundingCarrySummary,
    /// 后六个月近成本对照。
    pub control_validation: FundingCarrySummary,
    /// 每个历史月份的可执行组摘要。
    pub monthly: Vec<(i64, FundingCarrySummary)>,
    /// 标准成本后平均为正的月份数。
    pub positive_months: usize,
    /// 参与可执行极端腿次数最多的合约。
    pub most_frequent_symbol: Option<String>,
    /// 该合约参与次数。
    pub most_frequent_symbol_count: usize,
    /// 该次数占可执行观察数的比例，单位百分比。
    pub most_frequent_symbol_pct: Option<f64>,
    /// 是否通过全部预注册因子门槛。
    pub factor_gate_passed: bool,
}

/// 一个结算周期的两腿价格、实际 funding 与成本后结果。
#[derive(Debug, Clone, PartialEq)]
struct FundingCarryObservation {
    /// 信号 funding 结算时点。
    signal_ts: i64,
    /// 信号时点费率最低、被做多的合约。
    long_symbol: String,
    /// 信号时点费率最高、被做空的合约。
    short_symbol: String,
    /// 当前最高费率减最低费率。
    current_funding_spread: f64,
    /// 下一结算中空头收到减多头支付的 funding PnL。
    next_funding_pnl: f64,
    /// 等名义多头价格收益减空头价格收益。
    price_pnl: f64,
    /// 价格与 funding 合计的零成本收益。
    gross_pnl: f64,
    /// 扣除四次标准成交成本后的收益。
    standard_pnl: f64,
    /// 扣除双倍成交成本后的收益。
    double_cost_pnl: f64,
    /// 是否属于当前费率差达到 32bps 的可执行组。
    executable: bool,
}

/// 运行只读 Binance 官方月包面板，不写数据库或触发交易执行。
pub async fn run_cross_sectional_funding_carry_panel(
    args: &CrossExchangeBasisPanelArgs,
) -> Result<CrossSectionalFundingCarryReport> {
    run_panel(args, FundingCarryCoverage::TopSixtyEightyPercent).await
}

/// 运行 V2 共同可交易最小 30 成员版本，其他规则保持 V1 不变。
pub async fn run_cross_sectional_funding_carry_panel_v2(
    args: &CrossExchangeBasisPanelArgs,
) -> Result<CrossSectionalFundingCarryReport> {
    run_panel(args, FundingCarryCoverage::CommonMinimumThirty).await
}

/// 共享官方文件加载，仅通过显式版本枚举切换冻结覆盖合同。
async fn run_panel(
    args: &CrossExchangeBasisPanelArgs,
    coverage: FundingCarryCoverage,
) -> Result<CrossSectionalFundingCarryReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read funding carry manifest {}", args.manifest.display()))?,
    )
    .context("decode funding carry universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let (klines, kline_audit) = load_binance_klines(args, &schedule).await?;
    let (funding, funding_audit) = load_binance_funding(args, &schedule).await?;
    let (observations, stages) = build_observations(&schedule, &funding, &klines, coverage);
    let report = build_report(
        &schedule,
        coverage.rule_version(),
        kline_audit,
        funding_audit,
        stages,
        &observations,
    );
    print_report(&report);
    Ok(report)
}

/// 只用当前 funding 排序，再读取下一结算和发布缓冲后的价格 outcome。
fn build_observations(
    schedule: &UniverseSchedule,
    funding: &BTreeMap<String, Vec<BinanceFundingPoint>>,
    klines: &BTreeMap<String, Vec<BinanceCandle>>,
    coverage: FundingCarryCoverage,
) -> (Vec<FundingCarryObservation>, FundingCarryStages) {
    let mut stages = FundingCarryStages::default();
    let mut observations = Vec::new();
    let Some(first) = schedule.windows.first() else {
        return (observations, stages);
    };
    let Some(last) = schedule.windows.last() else {
        return (observations, stages);
    };
    let mut by_time = BTreeMap::<i64, BTreeMap<String, f64>>::new();
    for (symbol, points) in funding {
        for point in points {
            if point.interval_hours == 8 && point.ts >= first.from_ms && point.ts < last.to_ms {
                by_time
                    .entry(point.ts)
                    .or_default()
                    .insert(symbol.clone(), point.rate);
            }
        }
    }
    for (signal_ts, rates) in by_time {
        let Some(window) = schedule.window_at(signal_ts) else {
            continue;
        };
        stages.funding_timestamps += 1;
        let mut ranked = window
            .members
            .iter()
            .filter_map(|symbol| rates.get(symbol).map(|rate| (symbol.clone(), *rate)))
            .collect::<Vec<_>>();
        let minimum = coverage.minimum_members(window.members.len());
        stages.maximum_current_coverage = stages.maximum_current_coverage.max(ranked.len());
        stages.coverage_at_least_40 += usize::from(ranked.len() >= 40);
        stages.coverage_at_least_30 += usize::from(ranked.len() >= 30);
        if ranked.len() < minimum {
            stages.coverage_blocked += 1;
            continue;
        }
        stages.factor_observations += ranked.len();
        ranked.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        let (long_symbol, low_rate) = &ranked[0];
        let (short_symbol, high_rate) = &ranked[ranked.len() - 1];
        let current_funding_spread = high_rate - low_rate;
        let executable = if current_funding_spread >= EXECUTABLE_SPREAD {
            stages.executable_candidates += 1;
            true
        } else if current_funding_spread >= CONTROL_SPREAD {
            stages.control_candidates += 1;
            false
        } else {
            stages.below_control += 1;
            continue;
        };
        let outcome = funding_outcome(
            signal_ts,
            long_symbol,
            short_symbol,
            current_funding_spread,
            executable,
            funding,
            klines,
        );
        if let Some(outcome) = outcome {
            observations.push(outcome);
        } else {
            stages.incomplete_outcomes += 1;
        }
    }
    (observations, stages)
}

/// 计算跨越下一结算的一组实际价格与 funding PnL。
fn funding_outcome(
    signal_ts: i64,
    long_symbol: &str,
    short_symbol: &str,
    current_funding_spread: f64,
    executable: bool,
    funding: &BTreeMap<String, Vec<BinanceFundingPoint>>,
    klines: &BTreeMap<String, Vec<BinanceCandle>>,
) -> Option<FundingCarryObservation> {
    let next_ts = signal_ts.checked_add(MS_8H)?;
    let next_long = funding_at(funding.get(long_symbol)?, next_ts)?;
    let next_short = funding_at(funding.get(short_symbol)?, next_ts)?;
    if next_long.interval_hours != 8 || next_short.interval_hours != 8 {
        return None;
    }
    let entry_ts = signal_ts.checked_add(MS_15M)?;
    let exit_ts = next_ts.checked_add(MS_15M)?;
    let long_return = price_return(klines.get(long_symbol)?, entry_ts, exit_ts)?;
    let short_return = price_return(klines.get(short_symbol)?, entry_ts, exit_ts)?;
    let next_funding_pnl = next_short.rate - next_long.rate;
    let price_pnl = long_return - short_return;
    let gross_pnl = price_pnl + next_funding_pnl;
    let standard_pnl = gross_pnl - STANDARD_COST;
    let double_cost_pnl = gross_pnl - 2.0 * STANDARD_COST;
    [
        current_funding_spread,
        next_funding_pnl,
        price_pnl,
        gross_pnl,
        standard_pnl,
        double_cost_pnl,
    ]
    .iter()
    .all(|value| value.is_finite())
    .then_some(FundingCarryObservation {
        signal_ts,
        long_symbol: long_symbol.to_owned(),
        short_symbol: short_symbol.to_owned(),
        current_funding_spread,
        next_funding_pnl,
        price_pnl,
        gross_pnl,
        standard_pnl,
        double_cost_pnl,
        executable,
    })
}

/// 精确读取指定下一结算时点，不允许未来最近值替代。
fn funding_at(points: &[BinanceFundingPoint], ts: i64) -> Option<BinanceFundingPoint> {
    let index = points.binary_search_by_key(&ts, |point| point.ts).ok()?;
    points.get(index).copied()
}

/// 使用发布缓冲后的共同 15m 开盘，拒绝持有窗口内任意缺口。
fn price_return(candles: &[BinanceCandle], entry_ts: i64, exit_ts: i64) -> Option<f64> {
    let entry_index = candles
        .binary_search_by_key(&entry_ts, |candle| candle.ts)
        .ok()?;
    let exit_index = candles
        .binary_search_by_key(&exit_ts, |candle| candle.ts)
        .ok()?;
    let window = candles.get(entry_index..=exit_index)?;
    if window
        .windows(2)
        .any(|pair| pair[1].ts - pair[0].ts != MS_15M)
    {
        return None;
    }
    let entry = window.first()?.open;
    let exit = window.last()?.open;
    let value = exit / entry - 1.0;
    (entry > 0.0 && exit > 0.0 && value.is_finite()).then_some(value)
}

/// 构造时间稳定性、月份、集中度与预注册门禁。
fn build_report(
    schedule: &UniverseSchedule,
    rule_version: &str,
    kline_audit: BinanceKlineAudit,
    funding_audit: BinanceFundingAudit,
    stages: FundingCarryStages,
    observations: &[FundingCarryObservation],
) -> CrossSectionalFundingCarryReport {
    let split_ms = schedule.windows[6].from_ms;
    let executable = observations
        .iter()
        .filter(|value| value.executable)
        .collect::<Vec<_>>();
    let control = observations
        .iter()
        .filter(|value| !value.executable)
        .collect::<Vec<_>>();
    let executable_discovery_values = executable
        .iter()
        .copied()
        .filter(|value| value.signal_ts < split_ms)
        .collect::<Vec<_>>();
    let executable_validation_values = executable
        .iter()
        .copied()
        .filter(|value| value.signal_ts >= split_ms)
        .collect::<Vec<_>>();
    let control_discovery_values = control
        .iter()
        .copied()
        .filter(|value| value.signal_ts < split_ms)
        .collect::<Vec<_>>();
    let control_validation_values = control
        .iter()
        .copied()
        .filter(|value| value.signal_ts >= split_ms)
        .collect::<Vec<_>>();
    let executable_overall = summarize(&executable);
    let control_overall = summarize(&control);
    let executable_discovery = summarize(&executable_discovery_values);
    let control_discovery = summarize(&control_discovery_values);
    let executable_validation = summarize(&executable_validation_values);
    let control_validation = summarize(&control_validation_values);
    let monthly = schedule
        .windows
        .iter()
        .map(|window| {
            let values = executable
                .iter()
                .copied()
                .filter(|value| value.signal_ts >= window.from_ms && value.signal_ts < window.to_ms)
                .collect::<Vec<_>>();
            (window.from_ms, summarize(&values))
        })
        .collect::<Vec<_>>();
    let positive_months = monthly
        .iter()
        .filter(|(_, summary)| summary.mean_standard_pnl.is_some_and(|value| value > 0.0))
        .count();
    let (most_frequent_symbol, most_frequent_symbol_count) = concentration(&executable);
    let most_frequent_symbol_pct = (!executable.is_empty())
        .then_some(most_frequent_symbol_count as f64 / executable.len() as f64 * 100.0);
    let factor_gate_passed = executable.len() >= 600
        && executable_discovery_values.len() >= 250
        && executable_validation_values.len() >= 250
        && segment_passed(&executable_discovery, &control_discovery)
        && segment_passed(&executable_validation, &control_validation)
        && positive_months >= 8
        && most_frequent_symbol_pct.is_some_and(|value| value <= 20.0)
        && executable_overall
            .mean_double_cost_pnl
            .is_some_and(|value| value > 0.0);
    CrossSectionalFundingCarryReport {
        rule_version: rule_version.to_owned(),
        universe_version: schedule.version.clone(),
        okx_symbols: schedule.union_symbols().len(),
        kline_audit,
        funding_audit,
        stages,
        executable_overall,
        control_overall,
        executable_discovery,
        control_discovery,
        executable_validation,
        control_validation,
        monthly,
        positive_months,
        most_frequent_symbol,
        most_frequent_symbol_count,
        most_frequent_symbol_pct,
        factor_gate_passed,
    }
}

/// 判断一个半年是否同时满足 carry 持久性、净收益、命中和对照增量。
fn segment_passed(executable: &FundingCarrySummary, control: &FundingCarrySummary) -> bool {
    executable
        .mean_next_funding_pnl
        .is_some_and(|value| value >= EXECUTABLE_SPREAD)
        && executable
            .mean_standard_pnl
            .zip(control.mean_standard_pnl)
            .is_some_and(|(candidate, baseline)| {
                candidate >= 0.005 && candidate - baseline >= 0.0025
            })
        && executable
            .standard_positive_rate_pct
            .is_some_and(|value| value >= 55.0)
}

/// 汇总有限 carry 观察的分解收益和命中率。
fn summarize(values: &[&FundingCarryObservation]) -> FundingCarrySummary {
    if values.is_empty() {
        return FundingCarrySummary::default();
    }
    let length = values.len() as f64;
    FundingCarrySummary {
        observations: values.len(),
        mean_current_funding_spread: Some(
            values
                .iter()
                .map(|value| value.current_funding_spread)
                .sum::<f64>()
                / length,
        ),
        mean_next_funding_pnl: Some(
            values
                .iter()
                .map(|value| value.next_funding_pnl)
                .sum::<f64>()
                / length,
        ),
        mean_price_pnl: Some(values.iter().map(|value| value.price_pnl).sum::<f64>() / length),
        mean_gross_pnl: Some(values.iter().map(|value| value.gross_pnl).sum::<f64>() / length),
        mean_standard_pnl: Some(
            values.iter().map(|value| value.standard_pnl).sum::<f64>() / length,
        ),
        standard_positive_rate_pct: Some(
            values
                .iter()
                .filter(|value| value.standard_pnl > 0.0)
                .count() as f64
                / length
                * 100.0,
        ),
        mean_double_cost_pnl: Some(
            values
                .iter()
                .map(|value| value.double_cost_pnl)
                .sum::<f64>()
                / length,
        ),
    }
}

/// 统计可执行极端两腿的最大单币参与次数。
fn concentration(values: &[&FundingCarryObservation]) -> (Option<String>, usize) {
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

/// 输出文件审计、候选漏斗、半年、月份与集中度。
fn print_report(report: &CrossSectionalFundingCarryReport) {
    println!(
        "cross_sectional_funding_carry\trule={}\tuniverse={}\tokx_symbols={}\tkline_mapped={}\tkline_requested={}\tkline_available={}\tkline_missing={}\tkline_invalid={}\tkline_rows={}\tfunding_mapped={}\tfunding_requested={}\tfunding_available={}\tfunding_missing={}\tfunding_invalid={}\tfunding_rows={}\tfunding_timestamps={}\tcoverage_blocked={}\tmaximum_current_coverage={}\tcoverage_at_least_40={}\tcoverage_at_least_30={}\tfactor_observations={}\texecutable_candidates={}\tcontrol_candidates={}\tbelow_control={}\tincomplete={}\tpositive_months={}\tmost_frequent_symbol={}\tmost_frequent_count={}\tmost_frequent_pct={}\tfactor_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.okx_symbols,
        report.kline_audit.mapped_symbols,
        report.kline_audit.requested_files,
        report.kline_audit.available_files,
        report.kline_audit.missing_files,
        report.kline_audit.invalid_files,
        report.kline_audit.parsed_rows,
        report.funding_audit.mapped_symbols,
        report.funding_audit.requested_files,
        report.funding_audit.available_files,
        report.funding_audit.missing_files,
        report.funding_audit.invalid_files,
        report.funding_audit.parsed_rows,
        report.stages.funding_timestamps,
        report.stages.coverage_blocked,
        report.stages.maximum_current_coverage,
        report.stages.coverage_at_least_40,
        report.stages.coverage_at_least_30,
        report.stages.factor_observations,
        report.stages.executable_candidates,
        report.stages.control_candidates,
        report.stages.below_control,
        report.stages.incomplete_outcomes,
        report.positive_months,
        report.most_frequent_symbol.as_deref().unwrap_or("NA"),
        report.most_frequent_symbol_count,
        optional(report.most_frequent_symbol_pct),
        report.factor_gate_passed,
    );
    for (label, summary) in [
        ("executable_overall", &report.executable_overall),
        ("control_overall", &report.control_overall),
        ("executable_discovery", &report.executable_discovery),
        ("control_discovery", &report.control_discovery),
        ("executable_validation", &report.executable_validation),
        ("control_validation", &report.control_validation),
    ] {
        print_summary(label, summary);
    }
    for (from_ms, summary) in &report.monthly {
        print_summary(&format!("month_{from_ms}"), summary);
    }
}

/// 输出一个 carry 分组的分解收益。
fn print_summary(label: &str, summary: &FundingCarrySummary) {
    println!(
        "cross_sectional_funding_carry_summary\tgroup={}\tobservations={}\tcurrent_funding_spread={}\tnext_funding_pnl={}\tprice_pnl={}\tgross_pnl={}\tstandard_pnl={}\tstandard_positive_pct={}\tdouble_cost_pnl={}",
        label,
        summary.observations,
        optional(summary.mean_current_funding_spread),
        optional(summary.mean_next_funding_pnl),
        optional(summary.mean_price_pnl),
        optional(summary.mean_gross_pnl),
        optional(summary.mean_standard_pnl),
        optional(summary.standard_positive_rate_pct),
        optional(summary.mean_double_cost_pnl),
    );
}

/// 稳定格式化缺失浮点指标。
fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_owned(), |number| number.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造严格连续的 Binance 15m 开盘序列。
    fn candles(start_ts: i64, opens: &[f64]) -> Vec<BinanceCandle> {
        opens
            .iter()
            .enumerate()
            .map(|(index, open)| BinanceCandle {
                ts: start_ts + index as i64 * MS_15M,
                open: *open,
                close: *open,
                quote_volume: 1.0,
                taker_buy_quote_volume: 0.5,
            })
            .collect()
    }

    #[test]
    fn price_outcome_uses_publication_buffer_and_rejects_internal_gap() {
        let mut values = candles(MS_15M, &vec![100.0; 33]);
        values[32].open = 105.0;
        assert!((price_return(&values, MS_15M, MS_8H + MS_15M).unwrap() - 0.05).abs() < 1e-12);
        values[10].ts += 1;
        assert!(price_return(&values, MS_15M, MS_8H + MS_15M).is_none());
    }

    #[test]
    fn outcome_adds_next_actual_carry_and_four_fill_cost() {
        let funding = BTreeMap::from([
            (
                "AAA-USDT-SWAP".to_owned(),
                vec![BinanceFundingPoint {
                    ts: MS_8H,
                    interval_hours: 8,
                    rate: -0.004,
                }],
            ),
            (
                "BBB-USDT-SWAP".to_owned(),
                vec![BinanceFundingPoint {
                    ts: MS_8H,
                    interval_hours: 8,
                    rate: 0.003,
                }],
            ),
        ]);
        let klines = BTreeMap::from([
            (
                "AAA-USDT-SWAP".to_owned(),
                candles(MS_15M, &vec![100.0; 33]),
            ),
            (
                "BBB-USDT-SWAP".to_owned(),
                candles(MS_15M, &vec![100.0; 33]),
            ),
        ]);
        let outcome = funding_outcome(
            0,
            "AAA-USDT-SWAP",
            "BBB-USDT-SWAP",
            0.008,
            true,
            &funding,
            &klines,
        )
        .unwrap();
        assert!((outcome.next_funding_pnl - 0.007).abs() < 1e-12);
        assert!((outcome.standard_pnl - 0.0038).abs() < 1e-12);
        assert!((outcome.double_cost_pnl - 0.0006).abs() < 1e-12);
    }

    #[test]
    fn summary_keeps_price_and_funding_components_separate() {
        let value = FundingCarryObservation {
            signal_ts: 0,
            long_symbol: "AAA-USDT-SWAP".to_owned(),
            short_symbol: "BBB-USDT-SWAP".to_owned(),
            current_funding_spread: 0.004,
            next_funding_pnl: 0.003,
            price_pnl: 0.002,
            gross_pnl: 0.005,
            standard_pnl: 0.0018,
            double_cost_pnl: -0.0014,
            executable: true,
        };
        let summary = summarize(&[&value]);
        assert_eq!(summary.observations, 1);
        assert_eq!(summary.mean_next_funding_pnl, Some(0.003));
        assert_eq!(summary.mean_price_pnl, Some(0.002));
        assert_eq!(summary.standard_positive_rate_pct, Some(100.0));
    }

    #[test]
    fn exact_funding_lookup_does_not_use_a_later_row() {
        let points = [BinanceFundingPoint {
            ts: MS_8H + 1,
            interval_hours: 8,
            rate: 0.01,
        }];
        assert!(funding_at(&points, MS_8H).is_none());
    }

    #[test]
    fn concentration_counts_each_extreme_leg_once() {
        let observation = FundingCarryObservation {
            signal_ts: 0,
            long_symbol: "AAA-USDT-SWAP".to_owned(),
            short_symbol: "BBB-USDT-SWAP".to_owned(),
            current_funding_spread: 0.004,
            next_funding_pnl: 0.0,
            price_pnl: 0.0,
            gross_pnl: 0.0,
            standard_pnl: -STANDARD_COST,
            double_cost_pnl: -2.0 * STANDARD_COST,
            executable: true,
        };
        assert_eq!(concentration(&[&observation]).1, 1);
    }

    #[test]
    fn coverage_versions_preserve_v1_and_freeze_v2_at_thirty() {
        assert_eq!(
            FundingCarryCoverage::TopSixtyEightyPercent.minimum_members(60),
            48
        );
        assert_eq!(
            FundingCarryCoverage::CommonMinimumThirty.minimum_members(60),
            30
        );
    }
}
