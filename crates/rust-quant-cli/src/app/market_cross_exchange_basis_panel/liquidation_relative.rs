use super::binance_klines::{load_binance_klines, BinanceCandle, BinanceKlineAudit};
use super::binance_liquidation::{
    load_binance_liquidations, BinanceLiquidationAudit, BinanceLiquidationData,
};
use super::{CrossExchangeBasisPanelArgs, UniverseSchedule, UniverseWindow};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, Duration as ChronoDuration, NaiveDate, TimeZone, Utc};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MS_6H: i64 = 6 * 60 * 60 * 1_000;
const MS_24H: i64 = 24 * 60 * 60 * 1_000;
const BASELINE_WINDOWS: usize = 120;
const STANDARD_PAIR_COST: f64 = 0.0032;
const DOUBLE_PAIR_COST: f64 = 0.0064;
const RULE_VERSION: &str = "coinm_liquidation_6h_vs_prior30d_zscore_rank_6h_v1";
const UNIVERSE_VERSION: &str = "binance_current_live_btc_eth_202407_202506";
const DEFAULT_BINANCE_REST_BASE: &str = "https://fapi.binance.com";
const DEFAULT_BINANCE_DATA_BASE: &str = "https://data.binance.vision";

/// 冻结强平面板只允许数据位置与下载并发，不暴露研究阈值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiquidationRelativePanelArgs {
    /// 官方月包和日包的可复用缓存目录。
    pub cache_dir: PathBuf,
    /// 官方文件最大并发下载数。
    pub download_concurrency: usize,
    /// Binance 当前 USD-M 合约元数据 API 根地址。
    pub binance_rest_base: String,
    /// Binance 官方公开历史数据根地址。
    pub binance_data_base: String,
}

/// 强平因子从完整历史窗口到固定 outcome 的漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiquidationRelativeStages {
    /// 十二个月内所有 UTC 6h 决策点数。
    pub decision_points: usize,
    /// 30 天历史经过缺失日、标准差为零或输入缺失的时点数。
    pub input_blocked: usize,
    /// 成功计算 BTC/ETH z-score 并确定方向的时点数。
    pub computable_signals: usize,
    /// 两个 score 完全相同而不交易的时点数。
    pub tied_scores: usize,
    /// 缺少严格连续 6h/24h outcome 的时点数。
    pub incomplete_outcomes: usize,
}

/// 一个时间分组的强平价差收益、成本和命中率。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LiquidationRelativeSummary {
    /// 完整观察数。
    pub observations: usize,
    /// 平均多空 z-score 差。
    pub mean_score_spread: Option<f64>,
    /// 6h 平均毛配对收益。
    pub mean_gross_6h: Option<f64>,
    /// 24h 平均毛配对收益。
    pub mean_gross_24h: Option<f64>,
    /// 扣除 32bps 标准成本后的 6h 平均净收益。
    pub mean_standard_net_6h: Option<f64>,
    /// 扣除 64bps 双倍成本后的 6h 平均净收益。
    pub mean_double_net_6h: Option<f64>,
    /// 6h 毛收益为正的比例，单位百分比。
    pub positive_rate_6h_pct: Option<f64>,
    /// 24h 毛收益为正的比例，单位百分比。
    pub positive_rate_24h_pct: Option<f64>,
}

/// BTC—ETH 强平耗竭因子面板的完整只读报告。
#[derive(Debug, Clone, PartialEq)]
pub struct LiquidationRelativePanelReport {
    /// 冻结因子与方向规则身份。
    pub rule_version: String,
    /// 固定 BTC/ETH current-live 币池身份。
    pub universe_version: String,
    /// Binance USD-M regular 15m 月包审计。
    pub kline_audit: BinanceKlineAudit,
    /// Binance COIN-M liquidationSnapshot 日包审计。
    pub liquidation_audit: BinanceLiquidationAudit,
    /// 覆盖与 outcome 漏斗。
    pub stages: LiquidationRelativeStages,
    /// 是否通过先验覆盖门禁。
    pub coverage_gate_passed: bool,
    /// 全窗口结果。
    pub overall: LiquidationRelativeSummary,
    /// 2024-07～12 Discovery。
    pub discovery: LiquidationRelativeSummary,
    /// 2025-01～06 Validation。
    pub validation: LiquidationRelativeSummary,
    /// 每个完整月结果。
    pub monthly: Vec<(i64, LiquidationRelativeSummary)>,
    /// 6h 毛收益为正的月份数。
    pub positive_months: usize,
    /// score spread 从低到高的五分位摘要。
    pub score_quintiles: Vec<LiquidationRelativeSummary>,
    /// 五分位相邻均值非下降的次数。
    pub nondecreasing_quintile_steps: usize,
    /// 是否通过全部预注册边际价值门槛。
    pub factor_gate_passed: bool,
}

/// outcome 前只保存 point-in-time score 排名。
#[derive(Debug, Clone, Copy, PartialEq)]
struct LiquidationSignal {
    /// 6h UTC 决策时间。
    decision_ts: i64,
    /// `true` 表示 BTC 强平卖压 z-score 更高。
    long_btc: bool,
    /// 高 score 减低 score。
    score_spread: f64,
}

/// 冻结方向的双腿固定期限 outcome。
#[derive(Debug, Clone, Copy, PartialEq)]
struct LiquidationObservation {
    /// 决策时间。
    decision_ts: i64,
    /// 入场时可见的 score 差。
    score_spread: f64,
    /// 6h 等名义多空价差。
    forward_6h: f64,
    /// 24h 等名义多空价差。
    forward_24h: f64,
}

/// 解析冻结强平面板参数；未知参数直接失败。
pub fn parse_liquidation_relative_panel_args<I>(values: I) -> Result<LiquidationRelativePanelArgs>
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
            "--help" | "-h" => bail!(liquidation_relative_panel_usage()),
            _ => bail!(
                "unknown argument: {arg}\n{}",
                liquidation_relative_panel_usage()
            ),
        }
    }
    if !(1..=32).contains(&download_concurrency) {
        bail!("--download-concurrency must be between 1 and 32");
    }
    Ok(LiquidationRelativePanelArgs {
        cache_dir: cache_dir.context("--cache-dir is required")?,
        download_concurrency,
        binance_rest_base,
        binance_data_base,
    })
}

/// 返回冻结强平面板的最小命令用法。
pub fn liquidation_relative_panel_usage() -> &'static str {
    "Usage: market_btc_eth_liquidation_exhaustion_panel --cache-dir PATH [--download-concurrency 16]"
}

/// 运行只读强平耗竭面板；覆盖失败时不读取任何 forward outcome。
pub async fn run_liquidation_relative_panel(
    args: &LiquidationRelativePanelArgs,
) -> Result<LiquidationRelativePanelReport> {
    let (start_ms, split_ms, end_ms) = frozen_boundaries()?;
    let kline_schedule = fixed_schedule(start_ms, end_ms.saturating_add(MS_24H));
    let kline_args = CrossExchangeBasisPanelArgs {
        manifest: PathBuf::new(),
        cache_dir: args.cache_dir.join("regular_15m"),
        download_concurrency: args.download_concurrency,
        binance_rest_base: args.binance_rest_base.clone(),
        binance_data_base: args.binance_data_base.clone(),
    };
    let (klines, kline_audit) = load_binance_klines(&kline_args, &kline_schedule).await?;
    let first_day = Utc
        .timestamp_millis_opt(start_ms.saturating_sub(121 * MS_6H))
        .single()
        .context("invalid first liquidation day")?
        .date_naive();
    let last_day = Utc
        .timestamp_millis_opt(end_ms.saturating_sub(1))
        .single()
        .context("invalid last liquidation day")?
        .date_naive();
    let requested_days_per_asset = inclusive_day_count(first_day, last_day)?;
    let (liquidations, liquidation_audit) =
        load_binance_liquidations(args, first_day, last_day).await?;
    let (signals, mut stages) = build_signals(start_ms, end_ms, &liquidations);
    let btc_coverage = liquidation_audit.btc_valid_days as f64 / requested_days_per_asset as f64;
    let eth_coverage = liquidation_audit.eth_valid_days as f64 / requested_days_per_asset as f64;
    let coverage_gate_passed = kline_audit.requested_files == kline_audit.available_files
        && kline_audit.missing_files == 0
        && kline_audit.invalid_files == 0
        && btc_coverage >= 0.95
        && eth_coverage >= 0.95
        && signals.len() >= 1_300;
    let observations = if coverage_gate_passed {
        build_observations(&signals, &klines, &mut stages)
    } else {
        Vec::new()
    };
    let report = build_report(
        split_ms,
        kline_audit,
        liquidation_audit,
        stages,
        coverage_gate_passed,
        &observations,
    );
    print_report(&report, requested_days_per_asset);
    Ok(report)
}

/// 构造仅含当前仍 live 的 BTC/ETH 的固定执行币池。
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

/// 生成固定研究和 Discovery/Validation 边界。
fn frozen_boundaries() -> Result<(i64, i64, i64)> {
    let timestamp = |year, month, day| {
        NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|time| time.and_utc().timestamp_millis())
            .context("invalid frozen liquidation boundary")
    };
    Ok((
        timestamp(2024, 7, 1)?,
        timestamp(2025, 1, 1)?,
        timestamp(2025, 7, 1)?,
    ))
}

/// 计算闭区间 UTC 日数。
fn inclusive_day_count(first: NaiveDate, last: NaiveDate) -> Result<usize> {
    if first > last {
        bail!("first day must not exceed last day");
    }
    Ok((last - first).num_days() as usize + 1)
}

/// 只用 T 前已完成强平桶和有效日集合构造 score 排名。
fn build_signals(
    start_ms: i64,
    end_ms: i64,
    data: &BinanceLiquidationData,
) -> (Vec<LiquidationSignal>, LiquidationRelativeStages) {
    let mut stages = LiquidationRelativeStages::default();
    let mut signals = Vec::new();
    let mut decision_ts = start_ms;
    while decision_ts < end_ms {
        stages.decision_points += 1;
        let Some(btc_score) = liquidation_score("BTC", decision_ts, data) else {
            stages.input_blocked += 1;
            decision_ts = decision_ts.saturating_add(MS_6H);
            continue;
        };
        let Some(eth_score) = liquidation_score("ETH", decision_ts, data) else {
            stages.input_blocked += 1;
            decision_ts = decision_ts.saturating_add(MS_6H);
            continue;
        };
        if btc_score == eth_score {
            stages.tied_scores += 1;
            decision_ts = decision_ts.saturating_add(MS_6H);
            continue;
        }
        stages.computable_signals += 1;
        signals.push(LiquidationSignal {
            decision_ts,
            long_btc: btc_score > eth_score,
            score_spread: (btc_score - eth_score).abs(),
        });
        decision_ts = decision_ts.saturating_add(MS_6H);
    }
    (signals, stages)
}

/// 当前 6h 净强平卖压相对紧邻此前 120 个 6h 窗口计算 z-score。
fn liquidation_score(
    asset: &'static str,
    decision_ts: i64,
    data: &BinanceLiquidationData,
) -> Option<f64> {
    let buckets = data.buckets.get(asset)?;
    let valid_days = data.valid_days.get(asset)?;
    let history_start = decision_ts.checked_sub((BASELINE_WINDOWS as i64 + 1) * MS_6H)?;
    if !all_days_valid(valid_days, history_start, decision_ts)? {
        return None;
    }
    let current = range_sum(buckets, decision_ts.checked_sub(MS_6H)?, decision_ts);
    let mut prior = Vec::with_capacity(BASELINE_WINDOWS);
    for offset in 1..=BASELINE_WINDOWS {
        let end = decision_ts.checked_sub(offset as i64 * MS_6H)?;
        prior.push(range_sum(buckets, end.checked_sub(MS_6H)?, end));
    }
    let mean = prior.iter().sum::<f64>() / prior.len() as f64;
    let variance = prior
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (prior.len() - 1) as f64;
    let standard_deviation = variance.sqrt();
    (standard_deviation.is_finite() && standard_deviation > 0.0)
        .then_some((current - mean) / standard_deviation)
}

/// 确认半开时间段涉及的每个 UTC 日都有有效官方文件。
fn all_days_valid(valid_days: &BTreeSet<NaiveDate>, start_ms: i64, end_ms: i64) -> Option<bool> {
    let mut day = Utc.timestamp_millis_opt(start_ms).single()?.date_naive();
    let last = Utc
        .timestamp_millis_opt(end_ms.checked_sub(1)?)
        .single()?
        .date_naive();
    while day <= last {
        if !valid_days.contains(&day) {
            return Some(false);
        }
        day = day.checked_add_signed(ChronoDuration::days(1))?;
    }
    Some(true)
}

/// 对半开 15m 桶范围求和；没有事件的有效桶自然为零。
fn range_sum(buckets: &BTreeMap<i64, f64>, start_ms: i64, end_ms: i64) -> f64 {
    buckets
        .range(start_ms..end_ms)
        .map(|(_, value)| value)
        .sum()
}

/// 覆盖门禁通过后才读取 T 之后的 6h/24h 开盘。
fn build_observations(
    signals: &[LiquidationSignal],
    klines: &BTreeMap<String, Vec<BinanceCandle>>,
    stages: &mut LiquidationRelativeStages,
) -> Vec<LiquidationObservation> {
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
            Some(LiquidationObservation {
                decision_ts: signal.decision_ts,
                score_spread: signal.score_spread,
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

/// 对 15m K 线做精确二分查找。
fn candle_index(candles: &[BinanceCandle], ts: i64) -> Option<usize> {
    candles.binary_search_by_key(&ts, |candle| candle.ts).ok()
}

/// 汇总时间切分、月份、score 单调性与预注册联合门槛。
fn build_report(
    split_ms: i64,
    kline_audit: BinanceKlineAudit,
    liquidation_audit: BinanceLiquidationAudit,
    stages: LiquidationRelativeStages,
    coverage_gate_passed: bool,
    observations: &[LiquidationObservation],
) -> LiquidationRelativePanelReport {
    let overall = summarize(observations);
    let discovery = subset_summary(observations, |row| row.decision_ts < split_ms);
    let validation = subset_summary(observations, |row| row.decision_ts >= split_ms);
    let mut by_month = BTreeMap::<(i32, u32), Vec<LiquidationObservation>>::new();
    for observation in observations {
        if let Some(time) = Utc.timestamp_millis_opt(observation.decision_ts).single() {
            by_month
                .entry((time.year(), time.month()))
                .or_default()
                .push(*observation);
        }
    }
    let monthly = by_month
        .into_iter()
        .filter_map(|((year, month), rows)| {
            let month_ts = NaiveDate::from_ymd_opt(year, month, 1)?
                .and_hms_opt(0, 0, 0)?
                .and_utc()
                .timestamp_millis();
            Some((month_ts, summarize(&rows)))
        })
        .collect::<Vec<_>>();
    let positive_months = monthly
        .iter()
        .filter(|(_, summary)| summary.mean_gross_6h.is_some_and(|value| value > 0.0))
        .count();
    let score_quintiles = quintile_summaries(observations);
    let nondecreasing_quintile_steps = score_quintiles
        .windows(2)
        .filter(|pair| {
            pair[0]
                .mean_gross_6h
                .zip(pair[1].mean_gross_6h)
                .is_some_and(|(left, right)| right >= left)
        })
        .count();
    let counts_passed = overall.observations >= 1_300
        && discovery.observations >= 600
        && validation.observations >= 600;
    let frequency_passed = monthly.len() == 12
        && monthly
            .iter()
            .all(|(_, summary)| (100..=124).contains(&summary.observations));
    let quality_passed = [&overall, &discovery, &validation]
        .into_iter()
        .all(|summary| {
            summary.mean_gross_6h.is_some_and(|value| {
                value > STANDARD_PAIR_COST && value - STANDARD_PAIR_COST >= 0.0020
            }) && summary
                .positive_rate_6h_pct
                .is_some_and(|value| value >= 55.0)
        });
    let factor_gate_passed = coverage_gate_passed
        && counts_passed
        && frequency_passed
        && positive_months >= 8
        && quality_passed
        && score_quintiles.len() == 5
        && nondecreasing_quintile_steps >= 4
        && overall.mean_double_net_6h.is_some_and(|value| value > 0.0);
    LiquidationRelativePanelReport {
        rule_version: RULE_VERSION.to_owned(),
        universe_version: UNIVERSE_VERSION.to_owned(),
        kline_audit,
        liquidation_audit,
        stages,
        coverage_gate_passed,
        overall,
        discovery,
        validation,
        monthly,
        positive_months,
        score_quintiles,
        nondecreasing_quintile_steps,
        factor_gate_passed,
    }
}

/// 选择时间子集并复用同一成本口径。
fn subset_summary<F>(
    observations: &[LiquidationObservation],
    predicate: F,
) -> LiquidationRelativeSummary
where
    F: Fn(&LiquidationObservation) -> bool,
{
    summarize(
        &observations
            .iter()
            .filter(|row| predicate(row))
            .copied()
            .collect::<Vec<_>>(),
    )
}

/// 按信号时点可见的 score spread 排序并确定性切成五组。
fn quintile_summaries(observations: &[LiquidationObservation]) -> Vec<LiquidationRelativeSummary> {
    if observations.len() < 5 {
        return Vec::new();
    }
    let mut sorted = observations.to_vec();
    sorted.sort_by(|left, right| {
        left.score_spread
            .total_cmp(&right.score_spread)
            .then_with(|| left.decision_ts.cmp(&right.decision_ts))
    });
    (0..5)
        .map(|bucket| {
            let start = sorted.len() * bucket / 5;
            let end = sorted.len() * (bucket + 1) / 5;
            summarize(&sorted[start..end])
        })
        .collect()
}

/// 计算固定成本下的均值和毛命中率。
fn summarize(observations: &[LiquidationObservation]) -> LiquidationRelativeSummary {
    if observations.is_empty() {
        return LiquidationRelativeSummary::default();
    }
    let count = observations.len() as f64;
    let mean_6h = observations.iter().map(|row| row.forward_6h).sum::<f64>() / count;
    let mean_24h = observations.iter().map(|row| row.forward_24h).sum::<f64>() / count;
    LiquidationRelativeSummary {
        observations: observations.len(),
        mean_score_spread: Some(
            observations.iter().map(|row| row.score_spread).sum::<f64>() / count,
        ),
        mean_gross_6h: Some(mean_6h),
        mean_gross_24h: Some(mean_24h),
        mean_standard_net_6h: Some(mean_6h - STANDARD_PAIR_COST),
        mean_double_net_6h: Some(mean_6h - DOUBLE_PAIR_COST),
        positive_rate_6h_pct: Some(
            observations
                .iter()
                .filter(|row| row.forward_6h > 0.0)
                .count() as f64
                / count
                * 100.0,
        ),
        positive_rate_24h_pct: Some(
            observations
                .iter()
                .filter(|row| row.forward_24h > 0.0)
                .count() as f64
                / count
                * 100.0,
        ),
    }
}

/// 输出机器可审计的一次性报告。
fn print_report(report: &LiquidationRelativePanelReport, requested_days_per_asset: usize) {
    println!(
        "liquidation_relative_panel\trule={}\tuniverse={}\tkline_requested={}\tkline_available={}\tkline_missing={}\tkline_invalid={}\tkline_rows={}\tliquidation_requested={}\tliquidation_available={}\tliquidation_missing={}\tliquidation_invalid={}\tbtc_valid_days={}\teth_valid_days={}\trequested_days_per_asset={}\traw_rows={}\tunique_orders={}\tdecision_points={}\tinput_blocked={}\tcomputable={}\ttied={}\tincomplete={}\tcoverage_gate_passed={}\tpositive_months={}\tnondecreasing_quintile_steps={}\tfactor_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.kline_audit.requested_files,
        report.kline_audit.available_files,
        report.kline_audit.missing_files,
        report.kline_audit.invalid_files,
        report.kline_audit.parsed_rows,
        report.liquidation_audit.requested_files,
        report.liquidation_audit.available_files,
        report.liquidation_audit.missing_files,
        report.liquidation_audit.invalid_files,
        report.liquidation_audit.btc_valid_days,
        report.liquidation_audit.eth_valid_days,
        requested_days_per_asset,
        report.liquidation_audit.raw_rows,
        report.liquidation_audit.unique_orders,
        report.stages.decision_points,
        report.stages.input_blocked,
        report.stages.computable_signals,
        report.stages.tied_scores,
        report.stages.incomplete_outcomes,
        report.coverage_gate_passed,
        report.positive_months,
        report.nondecreasing_quintile_steps,
        report.factor_gate_passed,
    );
    for example in &report.liquidation_audit.invalid_examples {
        println!("liquidation_invalid_example\t{example}");
    }
    for (group, summary) in [
        ("overall", &report.overall),
        ("discovery", &report.discovery),
        ("validation", &report.validation),
    ] {
        print_summary(group, summary);
    }
    for (month, summary) in &report.monthly {
        print_summary(&format!("month_{month}"), summary);
    }
    for (index, summary) in report.score_quintiles.iter().enumerate() {
        print_summary(&format!("score_quintile_{}", index + 1), summary);
    }
}

/// 以空值安全格式打印一个分组。
fn print_summary(group: &str, summary: &LiquidationRelativeSummary) {
    println!(
        "liquidation_relative_summary\tgroup={}\tobservations={}\tscore_spread={:?}\tgross_6h={:?}\tgross_24h={:?}\tstandard_net_6h={:?}\tdouble_net_6h={:?}\tpositive_6h_pct={:?}\tpositive_24h_pct={:?}",
        group,
        summary.observations,
        summary.mean_score_spread,
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

    #[test]
    fn range_sum_uses_half_open_completed_buckets() {
        let buckets = BTreeMap::from([(0, 1.0), (MS_6H, 2.0), (2 * MS_6H, 4.0)]);
        assert_eq!(range_sum(&buckets, 0, 2 * MS_6H), 3.0);
    }

    #[test]
    fn missing_day_blocks_the_entire_trailing_window() {
        let first = NaiveDate::from_ymd_opt(2024, 6, 1).unwrap();
        let days = BTreeSet::from([first]);
        let start = first
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        assert_eq!(all_days_valid(&days, start, start + MS_24H), Some(true));
        assert_eq!(
            all_days_valid(&days, start, start + 2 * MS_24H),
            Some(false)
        );
    }

    #[test]
    fn quintiles_are_sorted_only_by_signal_time_score() {
        let rows = (0..10)
            .rev()
            .map(|index| LiquidationObservation {
                decision_ts: index,
                score_spread: index as f64,
                forward_6h: index as f64 / 100.0,
                forward_24h: 0.0,
            })
            .collect::<Vec<_>>();
        let summaries = quintile_summaries(&rows);
        assert_eq!(summaries.len(), 5);
        assert!(summaries[0].mean_score_spread < summaries[4].mean_score_spread);
    }

    #[test]
    fn summary_deducts_all_four_pair_fills() {
        let rows = vec![LiquidationObservation {
            decision_ts: 0,
            score_spread: 1.0,
            forward_6h: 0.01,
            forward_24h: 0.02,
        }];
        let summary = summarize(&rows);
        assert!((summary.mean_standard_net_6h.unwrap() - 0.0068).abs() < 1e-12);
        assert!((summary.mean_double_net_6h.unwrap() - 0.0036).abs() < 1e-12);
    }

    #[test]
    fn args_do_not_expose_research_thresholds() {
        let args = parse_liquidation_relative_panel_args(vec![
            "--cache-dir".to_owned(),
            "/tmp/liquidation".to_owned(),
        ])
        .unwrap();
        assert_eq!(args.download_concurrency, 16);
        assert!(
            parse_liquidation_relative_panel_args(vec!["--zscore-threshold".to_owned()]).is_err()
        );
    }
}
