use super::binance_bvol::{load_binance_bvol, BinanceBvolAudit, BinanceBvolPoint};
use super::binance_klines::{load_binance_klines, BinanceCandle, BinanceKlineAudit};
use super::{CrossExchangeBasisPanelArgs, UniverseSchedule, UniverseWindow, MS_15M};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MS_6H: i64 = 6 * 60 * 60 * 1_000;
const MS_24H: i64 = 24 * 60 * 60 * 1_000;
const ONE_SECOND_MS: i64 = 1_000;
const STANDARD_PAIR_COST: f64 = 0.0032;
const DOUBLE_PAIR_COST: f64 = 0.0064;
const RULE_VERSION: &str = "price24h_bvol24h_opposite_rank_6h_v1";
const UNIVERSE_VERSION: &str = "binance_current_live_btc_eth_202306_202410";
const DEFAULT_BINANCE_REST_BASE: &str = "https://fapi.binance.com";
const DEFAULT_BINANCE_DATA_BASE: &str = "https://data.binance.vision";

/// 冻结 BVOL 面板只允许数据位置与下载并发，不暴露研究阈值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BvolRelativePanelArgs {
    /// 官方月包和日包的可复用缓存目录。
    pub cache_dir: PathBuf,
    /// 官方文件最大并发下载数。
    pub download_concurrency: usize,
    /// Binance 当前合约元数据 API 根地址。
    pub binance_rest_base: String,
    /// Binance 官方公开历史数据根地址。
    pub binance_data_base: String,
}

/// 因子从完整时点输入到固定 outcome 的漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BvolRelativeStages {
    /// 冻结窗口内所有 UTC 6h 决策点数。
    pub decision_points: usize,
    /// 缺少 T-1s/T-24h-1s BVOL 或已完成价格前缀的时点数。
    pub input_blocked: usize,
    /// outcome 计算前可确定因子/对照方向的时点数。
    pub computable_signals: usize,
    /// 价格强弱与 BVOL 风险定价相反的因子组数。
    pub factor_signals: usize,
    /// 两者同向的非确认对照组数。
    pub control_signals: usize,
    /// 缺少严格连续 6h/24h outcome 的时点数。
    pub incomplete_outcomes: usize,
}

/// 一个预注册分组的配对收益、成本和命中率。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BvolRelativeSummary {
    /// 完整观察数。
    pub observations: usize,
    /// 6h 平均毛配对收益。
    pub mean_gross_6h: Option<f64>,
    /// 24h 平均毛配对收益。
    pub mean_gross_24h: Option<f64>,
    /// 扣除 32bps 四腿标准成本后的 6h 平均净收益。
    pub mean_standard_net_6h: Option<f64>,
    /// 扣除 64bps 双倍成本后的 6h 平均净收益。
    pub mean_double_net_6h: Option<f64>,
    /// 6h 毛收益为正的比例，单位百分比。
    pub positive_rate_6h_pct: Option<f64>,
    /// 24h 毛收益为正的比例，单位百分比。
    pub positive_rate_24h_pct: Option<f64>,
}

/// BTC—ETH BVOL 确认相对动量面板的完整只读报告。
#[derive(Debug, Clone, PartialEq)]
pub struct BvolRelativePanelReport {
    /// 冻结因子与方向规则身份。
    pub rule_version: String,
    /// 固定 BTC/ETH current-live 币池身份。
    pub universe_version: String,
    /// Binance regular 15m 月包审计。
    pub kline_audit: BinanceKlineAudit,
    /// Binance BVOLIndex 日包审计。
    pub bvol_audit: BinanceBvolAudit,
    /// 覆盖与 outcome 漏斗。
    pub stages: BvolRelativeStages,
    /// 是否通过预注册的先验覆盖门禁。
    pub coverage_gate_passed: bool,
    /// 全窗口因子组。
    pub factor_overall: BvolRelativeSummary,
    /// 全窗口非确认对照组。
    pub control_overall: BvolRelativeSummary,
    /// Discovery 因子组。
    pub factor_discovery: BvolRelativeSummary,
    /// Discovery 对照组。
    pub control_discovery: BvolRelativeSummary,
    /// Validation 因子组。
    pub factor_validation: BvolRelativeSummary,
    /// Validation 对照组。
    pub control_validation: BvolRelativeSummary,
    /// 每个完整月的因子组摘要。
    pub monthly_factor: Vec<(i64, BvolRelativeSummary)>,
    /// 6h 毛收益为正的月份数。
    pub positive_months: usize,
    /// 是否通过全部预注册边际价值门槛。
    pub factor_gate_passed: bool,
}

/// outcome 前只保存已确认方向和因子/对照身份。
#[derive(Debug, Clone, Copy, PartialEq)]
struct DecisionSignal {
    /// 6h UTC 决策时间。
    decision_ts: i64,
    /// `true` 表示 BTC 为价格强者并作为多腿。
    long_btc: bool,
    /// `true` 表示期权隐波确认价格相对动量。
    factor_confirmed: bool,
}

/// 冻结方向的双腿固定期限 outcome。
#[derive(Debug, Clone, Copy, PartialEq)]
struct BvolRelativeObservation {
    /// 决策时间。
    decision_ts: i64,
    /// 因子确认或非确认对照身份。
    factor_confirmed: bool,
    /// 6h 等名义多空价差。
    forward_6h: f64,
    /// 24h 等名义多空价差。
    forward_24h: f64,
}

/// 解析冻结 BVOL 面板参数；未知参数直接失败。
pub fn parse_bvol_relative_panel_args<I>(values: I) -> Result<BvolRelativePanelArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut cache_dir = None;
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
            "--cache-dir" => cache_dir = Some(PathBuf::from(value(&mut values)?)),
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
            "--help" | "-h" => bail!(bvol_relative_panel_usage()),
            _ => bail!("unknown argument: {arg}\n{}", bvol_relative_panel_usage()),
        }
    }
    if !(1..=32).contains(&download_concurrency) {
        bail!("--download-concurrency must be between 1 and 32");
    }
    Ok(BvolRelativePanelArgs {
        cache_dir: cache_dir.context("--cache-dir is required")?,
        download_concurrency,
        binance_rest_base,
        binance_data_base,
    })
}

/// 返回冻结 BVOL 面板的最小命令用法。
pub fn bvol_relative_panel_usage() -> &'static str {
    "Usage: market_btc_eth_bvol_relative_momentum_panel --cache-dir PATH [--download-concurrency 16]"
}

/// 运行只读 BVOL 确认相对动量面板；覆盖失败时不计算任何 forward outcome。
pub async fn run_bvol_relative_momentum_panel(
    args: &BvolRelativePanelArgs,
) -> Result<BvolRelativePanelReport> {
    let (start_ms, split_ms, end_ms) = frozen_boundaries()?;
    let schedule = fixed_schedule(start_ms, end_ms);
    let kline_args = CrossExchangeBasisPanelArgs {
        manifest: PathBuf::new(),
        cache_dir: args.cache_dir.join("regular_15m"),
        download_concurrency: args.download_concurrency,
        binance_rest_base: args.binance_rest_base.clone(),
        binance_data_base: args.binance_data_base.clone(),
    };
    let (klines, kline_audit) = load_binance_klines(&kline_args, &schedule).await?;
    let first_bvol_day = Utc
        .timestamp_millis_opt(start_ms.saturating_sub(MS_24H))
        .single()
        .context("invalid first BVOL day")?
        .date_naive();
    let last_bvol_day = Utc
        .timestamp_millis_opt(end_ms.saturating_sub(1))
        .single()
        .context("invalid last BVOL day")?
        .date_naive();
    let (bvol, bvol_audit) = load_binance_bvol(args, first_bvol_day, last_bvol_day).await?;
    let (signals, mut stages) = build_signals(start_ms, end_ms, &bvol, &klines);
    let bvol_coverage = if bvol_audit.requested_files == 0 {
        0.0
    } else {
        bvol_audit.available_files as f64 / bvol_audit.requested_files as f64
    };
    let coverage_gate_passed = kline_audit.requested_files == kline_audit.available_files
        && kline_audit.missing_files == 0
        && kline_audit.invalid_files == 0
        && bvol_coverage >= 0.95
        && signals.len() >= 1_800;
    let observations = if coverage_gate_passed {
        build_observations(&signals, &klines, &mut stages)
    } else {
        Vec::new()
    };
    let report = build_report(
        split_ms,
        kline_audit,
        bvol_audit,
        stages,
        coverage_gate_passed,
        &observations,
    );
    print_report(&report);
    Ok(report)
}

/// 构造仅含当前仍 live 的 BTC/ETH 的固定时间币池。
fn fixed_schedule(start_ms: i64, end_ms: i64) -> UniverseSchedule {
    UniverseSchedule {
        version: UNIVERSE_VERSION.to_owned(),
        windows: vec![UniverseWindow {
            from_ms: start_ms,
            to_ms: end_ms,
            members: BTreeSet::from(["BTC-USDT-SWAP".to_owned(), "ETH-USDT-SWAP".to_owned()]),
        }],
    }
}

/// 生成固定研究、Discovery/Validation 边界。
fn frozen_boundaries() -> Result<(i64, i64, i64)> {
    let timestamp = |year, month, day| {
        NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|time| time.and_utc().timestamp_millis())
            .context("invalid frozen BVOL boundary")
    };
    Ok((
        timestamp(2023, 6, 1)?,
        timestamp(2024, 2, 1)?,
        timestamp(2024, 11, 1)?,
    ))
}

/// 只用决策前 BVOL 与已完成 15m 收盘确定方向，不读取 future outcome。
fn build_signals(
    start_ms: i64,
    end_ms: i64,
    bvol: &BTreeMap<&'static str, Vec<BinanceBvolPoint>>,
    klines: &BTreeMap<String, Vec<BinanceCandle>>,
) -> (Vec<DecisionSignal>, BvolRelativeStages) {
    let mut stages = BvolRelativeStages::default();
    let mut signals = Vec::new();
    let mut decision_ts = start_ms;
    while decision_ts < end_ms {
        stages.decision_points += 1;
        let Some((price_diff, bvol_diff)) = decision_factor(decision_ts, bvol, klines) else {
            stages.input_blocked += 1;
            decision_ts = decision_ts.saturating_add(MS_6H);
            continue;
        };
        if price_diff == 0.0 || bvol_diff == 0.0 {
            decision_ts = decision_ts.saturating_add(MS_6H);
            continue;
        }
        let factor_confirmed = price_diff * bvol_diff < 0.0;
        stages.computable_signals += 1;
        if factor_confirmed {
            stages.factor_signals += 1;
        } else {
            stages.control_signals += 1;
        }
        signals.push(DecisionSignal {
            decision_ts,
            long_btc: price_diff > 0.0,
            factor_confirmed,
        });
        decision_ts = decision_ts.saturating_add(MS_6H);
    }
    (signals, stages)
}

/// 计算价格相对强弱和期权隐波相对变化；所有输入严格早于决策时点。
fn decision_factor(
    decision_ts: i64,
    bvol: &BTreeMap<&'static str, Vec<BinanceBvolPoint>>,
    klines: &BTreeMap<String, Vec<BinanceCandle>>,
) -> Option<(f64, f64)> {
    let btc_return = trailing_price_return(klines.get("BTC-USDT-SWAP")?, decision_ts)?;
    let eth_return = trailing_price_return(klines.get("ETH-USDT-SWAP")?, decision_ts)?;
    let bvol_now_ts = decision_ts.checked_sub(ONE_SECOND_MS)?;
    let bvol_past_ts = bvol_now_ts.checked_sub(MS_24H)?;
    let btc_bvol = point_at(bvol.get("BTC")?, bvol_now_ts)?.value
        - point_at(bvol.get("BTC")?, bvol_past_ts)?.value;
    let eth_bvol = point_at(bvol.get("ETH")?, bvol_now_ts)?.value
        - point_at(bvol.get("ETH")?, bvol_past_ts)?.value;
    let price_diff = btc_return - eth_return;
    let bvol_diff = btc_bvol - eth_bvol;
    (price_diff.is_finite() && bvol_diff.is_finite()).then_some((price_diff, bvol_diff))
}

/// 使用 T-15m 与 T-24h-15m 的两个已完成收盘计算对数收益。
fn trailing_price_return(candles: &[BinanceCandle], decision_ts: i64) -> Option<f64> {
    let current_ts = decision_ts.checked_sub(MS_15M)?;
    let past_ts = current_ts.checked_sub(MS_24H)?;
    let current_index = candle_index(candles, current_ts)?;
    let past_index = candle_index(candles, past_ts)?;
    if current_index.checked_sub(past_index)? != 96 {
        return None;
    }
    Some((candles[current_index].close / candles[past_index].close).ln())
}

/// 覆盖门禁通过后才读取 T 之后的 6h/24h 开盘。
fn build_observations(
    signals: &[DecisionSignal],
    klines: &BTreeMap<String, Vec<BinanceCandle>>,
    stages: &mut BvolRelativeStages,
) -> Vec<BvolRelativeObservation> {
    let observations = signals
        .iter()
        .filter_map(|signal| {
            let btc = leg_outcome(klines.get("BTC-USDT-SWAP")?, signal.decision_ts)?;
            let eth = leg_outcome(klines.get("ETH-USDT-SWAP")?, signal.decision_ts)?;
            let (forward_6h, forward_24h) = if signal.long_btc {
                (btc.0 - eth.0, btc.1 - eth.1)
            } else {
                (eth.0 - btc.0, eth.1 - btc.1)
            };
            Some(BvolRelativeObservation {
                decision_ts: signal.decision_ts,
                factor_confirmed: signal.factor_confirmed,
                forward_6h,
                forward_24h,
            })
        })
        .collect::<Vec<_>>();
    stages.incomplete_outcomes = signals.len().saturating_sub(observations.len());
    observations
}

/// 从 T 开盘到固定 6h/24h 开盘计算单腿收益并验证连续索引。
fn leg_outcome(candles: &[BinanceCandle], decision_ts: i64) -> Option<(f64, f64)> {
    let entry_index = candle_index(candles, decision_ts)?;
    let exit_6h_index = candle_index(candles, decision_ts.checked_add(MS_6H)?)?;
    let exit_24h_index = candle_index(candles, decision_ts.checked_add(MS_24H)?)?;
    if exit_6h_index.checked_sub(entry_index)? != 24
        || exit_24h_index.checked_sub(entry_index)? != 96
    {
        return None;
    }
    let entry = candles[entry_index].open;
    Some((
        candles[exit_6h_index].open / entry - 1.0,
        candles[exit_24h_index].open / entry - 1.0,
    ))
}

/// 对 BVOL 保留点做精确二分查找，禁止陈旧值补齐。
fn point_at(points: &[BinanceBvolPoint], ts: i64) -> Option<BinanceBvolPoint> {
    points
        .binary_search_by_key(&ts, |point| point.ts)
        .ok()
        .map(|index| points[index])
}

/// 对 15m K 线做精确二分查找。
fn candle_index(candles: &[BinanceCandle], ts: i64) -> Option<usize> {
    candles.binary_search_by_key(&ts, |candle| candle.ts).ok()
}

/// 汇总全窗口、时间切分、月份与预注册联合门槛。
fn build_report(
    split_ms: i64,
    kline_audit: BinanceKlineAudit,
    bvol_audit: BinanceBvolAudit,
    stages: BvolRelativeStages,
    coverage_gate_passed: bool,
    observations: &[BvolRelativeObservation],
) -> BvolRelativePanelReport {
    let factor = observations
        .iter()
        .filter(|observation| observation.factor_confirmed)
        .copied()
        .collect::<Vec<_>>();
    let control = observations
        .iter()
        .filter(|observation| !observation.factor_confirmed)
        .copied()
        .collect::<Vec<_>>();
    let factor_discovery =
        subset_summary(&factor, |observation| observation.decision_ts < split_ms);
    let factor_validation =
        subset_summary(&factor, |observation| observation.decision_ts >= split_ms);
    let control_discovery =
        subset_summary(&control, |observation| observation.decision_ts < split_ms);
    let control_validation =
        subset_summary(&control, |observation| observation.decision_ts >= split_ms);
    let factor_overall = summarize(&factor);
    let control_overall = summarize(&control);
    let mut by_month = BTreeMap::<(i32, u32), Vec<BvolRelativeObservation>>::new();
    for observation in &factor {
        if let Some(time) = Utc.timestamp_millis_opt(observation.decision_ts).single() {
            by_month
                .entry((time.year(), time.month()))
                .or_default()
                .push(*observation);
        }
    }
    let monthly_factor = by_month
        .into_iter()
        .filter_map(|((year, month), rows)| {
            let month_ts = NaiveDate::from_ymd_opt(year, month, 1)?
                .and_hms_opt(0, 0, 0)?
                .and_utc()
                .timestamp_millis();
            Some((month_ts, summarize(&rows)))
        })
        .collect::<Vec<_>>();
    let positive_months = monthly_factor
        .iter()
        .filter(|(_, summary)| summary.mean_gross_6h.is_some_and(|value| value > 0.0))
        .count();
    let frequency_passed = monthly_factor.len() == 17
        && monthly_factor
            .iter()
            .all(|(_, summary)| (50..=120).contains(&summary.observations));
    let split_counts_passed = factor_overall.observations >= 1_000
        && factor_discovery.observations >= 450
        && factor_validation.observations >= 450;
    let quality_passed = [
        (&factor_overall, &control_overall),
        (&factor_discovery, &control_discovery),
        (&factor_validation, &control_validation),
    ]
    .into_iter()
    .all(|(factor_summary, control_summary)| {
        factor_summary
            .mean_gross_6h
            .zip(control_summary.mean_gross_6h)
            .is_some_and(|(factor_mean, control_mean)| {
                factor_mean > STANDARD_PAIR_COST
                    && factor_mean - STANDARD_PAIR_COST >= 0.0020
                    && factor_mean - control_mean >= 0.0025
            })
            && factor_summary
                .positive_rate_6h_pct
                .is_some_and(|value| value >= 55.0)
    });
    let factor_gate_passed = coverage_gate_passed
        && split_counts_passed
        && frequency_passed
        && positive_months >= 12
        && quality_passed
        && factor_overall
            .mean_double_net_6h
            .is_some_and(|value| value > 0.0);
    BvolRelativePanelReport {
        rule_version: RULE_VERSION.to_owned(),
        universe_version: UNIVERSE_VERSION.to_owned(),
        kline_audit,
        bvol_audit,
        stages,
        coverage_gate_passed,
        factor_overall,
        control_overall,
        factor_discovery,
        control_discovery,
        factor_validation,
        control_validation,
        monthly_factor,
        positive_months,
        factor_gate_passed,
    }
}

/// 选择时间子集并复用同一成本口径。
fn subset_summary<F>(observations: &[BvolRelativeObservation], predicate: F) -> BvolRelativeSummary
where
    F: Fn(&BvolRelativeObservation) -> bool,
{
    summarize(
        &observations
            .iter()
            .filter(|observation| predicate(observation))
            .copied()
            .collect::<Vec<_>>(),
    )
}

/// 计算固定成本下的均值和毛命中率。
fn summarize(observations: &[BvolRelativeObservation]) -> BvolRelativeSummary {
    if observations.is_empty() {
        return BvolRelativeSummary::default();
    }
    let count = observations.len() as f64;
    let mean_6h = observations
        .iter()
        .map(|observation| observation.forward_6h)
        .sum::<f64>()
        / count;
    let mean_24h = observations
        .iter()
        .map(|observation| observation.forward_24h)
        .sum::<f64>()
        / count;
    BvolRelativeSummary {
        observations: observations.len(),
        mean_gross_6h: Some(mean_6h),
        mean_gross_24h: Some(mean_24h),
        mean_standard_net_6h: Some(mean_6h - STANDARD_PAIR_COST),
        mean_double_net_6h: Some(mean_6h - DOUBLE_PAIR_COST),
        positive_rate_6h_pct: Some(
            observations
                .iter()
                .filter(|observation| observation.forward_6h > 0.0)
                .count() as f64
                / count
                * 100.0,
        ),
        positive_rate_24h_pct: Some(
            observations
                .iter()
                .filter(|observation| observation.forward_24h > 0.0)
                .count() as f64
                / count
                * 100.0,
        ),
    }
}

/// 输出机器可审计的一次性报告。
fn print_report(report: &BvolRelativePanelReport) {
    println!(
        "bvol_relative_panel\trule={}\tuniverse={}\tkline_requested={}\tkline_available={}\tkline_missing={}\tkline_invalid={}\tkline_rows={}\tbvol_requested={}\tbvol_available={}\tbvol_missing={}\tbvol_invalid={}\tbvol_points={}\tdecision_points={}\tinput_blocked={}\tcomputable={}\tfactor_signals={}\tcontrol_signals={}\tincomplete={}\tcoverage_gate_passed={}\tpositive_months={}\tfactor_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.kline_audit.requested_files,
        report.kline_audit.available_files,
        report.kline_audit.missing_files,
        report.kline_audit.invalid_files,
        report.kline_audit.parsed_rows,
        report.bvol_audit.requested_files,
        report.bvol_audit.available_files,
        report.bvol_audit.missing_files,
        report.bvol_audit.invalid_files,
        report.bvol_audit.retained_points,
        report.stages.decision_points,
        report.stages.input_blocked,
        report.stages.computable_signals,
        report.stages.factor_signals,
        report.stages.control_signals,
        report.stages.incomplete_outcomes,
        report.coverage_gate_passed,
        report.positive_months,
        report.factor_gate_passed,
    );
    for example in &report.bvol_audit.invalid_examples {
        println!("bvol_invalid_example\t{example}");
    }
    for (group, summary) in [
        ("factor_overall", &report.factor_overall),
        ("control_overall", &report.control_overall),
        ("factor_discovery", &report.factor_discovery),
        ("control_discovery", &report.control_discovery),
        ("factor_validation", &report.factor_validation),
        ("control_validation", &report.control_validation),
    ] {
        print_summary(group, summary);
    }
    for (month, summary) in &report.monthly_factor {
        print_summary(&format!("month_{month}"), summary);
    }
}

/// 以空值安全格式打印一个分组。
fn print_summary(group: &str, summary: &BvolRelativeSummary) {
    println!(
        "bvol_relative_summary\tgroup={}\tobservations={}\tgross_6h={:?}\tgross_24h={:?}\tstandard_net_6h={:?}\tdouble_net_6h={:?}\tpositive_6h_pct={:?}\tpositive_24h_pct={:?}",
        group,
        summary.observations,
        summary.mean_gross_6h,
        summary.mean_gross_24h,
        summary.mean_standard_net_6h,
        summary.mean_double_net_6h,
        summary.positive_rate_6h_pct,
        summary.positive_rate_24h_pct,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(ts: i64, value: f64) -> BinanceCandle {
        BinanceCandle {
            ts,
            open: value,
            close: value,
            quote_volume: 1.0,
            taker_buy_quote_volume: 0.5,
        }
    }

    #[test]
    fn exact_bvol_lookup_never_uses_a_later_point() {
        let points = vec![
            BinanceBvolPoint {
                ts: 1_000,
                value: 50.0,
            },
            BinanceBvolPoint {
                ts: 2_000,
                value: 51.0,
            },
        ];
        assert_eq!(point_at(&points, 1_000).unwrap().value, 50.0);
        assert_eq!(point_at(&points, 1_500), None);
    }

    #[test]
    fn price_prefix_uses_only_completed_candles() {
        let candles = (0..=96)
            .map(|index| candle(index * MS_15M, 100.0 + index as f64))
            .collect::<Vec<_>>();
        let decision_ts = 97 * MS_15M;
        let observed = trailing_price_return(&candles, decision_ts).unwrap();
        assert!((observed - (196.0_f64 / 100.0).ln()).abs() < 1e-12);
    }

    #[test]
    fn pair_outcome_follows_frozen_price_strength_direction() {
        let signal = DecisionSignal {
            decision_ts: 0,
            long_btc: true,
            factor_confirmed: true,
        };
        assert!(signal.long_btc);
        let btc = (0.02, 0.04);
        let eth = (-0.01, 0.01);
        assert_eq!(btc.0 - eth.0, 0.03);
        assert_eq!(btc.1 - eth.1, 0.03);
    }

    #[test]
    fn summary_deducts_all_four_pair_fills() {
        let rows = vec![BvolRelativeObservation {
            decision_ts: 0,
            factor_confirmed: true,
            forward_6h: 0.01,
            forward_24h: 0.02,
        }];
        let summary = summarize(&rows);
        assert!((summary.mean_standard_net_6h.unwrap() - 0.0068).abs() < 1e-12);
        assert!((summary.mean_double_net_6h.unwrap() - 0.0036).abs() < 1e-12);
    }

    #[test]
    fn args_do_not_expose_research_thresholds() {
        let args =
            parse_bvol_relative_panel_args(vec!["--cache-dir".to_owned(), "/tmp/bvol".to_owned()])
                .unwrap();
        assert_eq!(args.download_concurrency, 16);
        assert!(parse_bvol_relative_panel_args(vec!["--bvol-threshold".to_owned()]).is_err());
    }
}
