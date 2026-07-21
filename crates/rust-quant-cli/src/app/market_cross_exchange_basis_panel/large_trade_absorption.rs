use super::binance_aggtrades::{
    load_binance_aggtrades, BinanceAggTradesAudit, BinanceAggTradesData,
};
use super::binance_klines::{load_binance_klines, BinanceCandle, BinanceKlineAudit};
use super::{CrossExchangeBasisPanelArgs, UniverseSchedule, UniverseWindow, MS_15M};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MS_6H: i64 = 6 * 60 * 60 * 1_000;
const MS_24H: i64 = 24 * 60 * 60 * 1_000;
const BASELINE_WINDOWS: usize = 120;
const STANDARD_PAIR_COST: f64 = 0.0032;
const DOUBLE_PAIR_COST: f64 = 0.0064;
const RULE_VERSION: &str = "aggtrade_tail_pressure_price_residual_rank_6h_v1";
const UNIVERSE_VERSION: &str = "binance_current_live_btc_eth_202407_202506";
const DEFAULT_BINANCE_REST_BASE: &str = "https://fapi.binance.com";
const DEFAULT_BINANCE_DATA_BASE: &str = "https://data.binance.vision";

/// 大体积 aggTrades 面板只允许数据位置与最多两个并发文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeTradePanelArgs {
    /// 官方月包的可复用缓存目录。
    pub cache_dir: PathBuf,
    /// 大体积 ZIP 最大并发下载数，固定限制为一到二。
    pub download_concurrency: usize,
    /// Binance 当前 USD-M 合约元数据 API 根地址。
    pub binance_rest_base: String,
    /// Binance 官方公开历史数据根地址。
    pub binance_data_base: String,
}

/// 大单吸收因子从完整历史前缀到固定 outcome 的漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LargeTradeStages {
    /// 十二个月内所有 UTC 6h 决策点数。
    pub decision_points: usize,
    /// 价格或尾部主动流的当前/120 窗口历史不可计算数。
    pub input_blocked: usize,
    /// 成功计算两个资产 score 并确定方向的时点数。
    pub computable_signals: usize,
    /// 两个 score 完全相同而跳过的时点数。
    pub tied_scores: usize,
    /// 缺少严格连续 6h/24h outcome 的时点数。
    pub incomplete_outcomes: usize,
}

/// 一个时间分组的大单吸收价差收益与成本结果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LargeTradeSummary {
    /// 完整观察数。
    pub observations: usize,
    /// 平均多空吸收 score 差。
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

/// BTC—ETH 大单吸收因子面板的完整只读报告。
#[derive(Debug, Clone, PartialEq)]
pub struct LargeTradePanelReport {
    /// 冻结因子与方向身份。
    pub rule_version: String,
    /// 固定 BTC/ETH current-live 币池身份。
    pub universe_version: String,
    /// Binance regular 15m 月包审计。
    pub kline_audit: BinanceKlineAudit,
    /// Binance aggTrades 月包审计。
    pub aggtrades_audit: BinanceAggTradesAudit,
    /// 覆盖与 outcome 漏斗。
    pub stages: LargeTradeStages,
    /// 是否通过先验覆盖门禁。
    pub coverage_gate_passed: bool,
    /// 全窗口结果。
    pub overall: LargeTradeSummary,
    /// 2024-07～12 Discovery。
    pub discovery: LargeTradeSummary,
    /// 2025-01～06 Validation。
    pub validation: LargeTradeSummary,
    /// 每个完整月结果。
    pub monthly: Vec<(i64, LargeTradeSummary)>,
    /// 6h 毛收益为正的月份数。
    pub positive_months: usize,
    /// score spread 从低到高五分位摘要。
    pub score_quintiles: Vec<LargeTradeSummary>,
    /// 五分位相邻均值非下降次数。
    pub nondecreasing_quintile_steps: usize,
    /// 是否通过全部预注册边际价值门槛。
    pub factor_gate_passed: bool,
}

/// outcome 前只保存 point-in-time score 排名。
#[derive(Debug, Clone, Copy, PartialEq)]
struct LargeTradeSignal {
    /// 6h UTC 决策时间。
    decision_ts: i64,
    /// `true` 表示 BTC 吸收 score 更高。
    long_btc: bool,
    /// 高 score 减低 score。
    score_spread: f64,
}

/// 冻结方向的双腿固定期限 outcome。
#[derive(Debug, Clone, Copy, PartialEq)]
struct LargeTradeObservation {
    /// 决策时间。
    decision_ts: i64,
    /// 入场时可见的 score 差。
    score_spread: f64,
    /// 6h 等名义多空价差。
    forward_6h: f64,
    /// 24h 等名义多空价差。
    forward_24h: f64,
}

/// 解析冻结大单面板参数；未知参数直接失败。
pub fn parse_large_trade_panel_args<I>(values: I) -> Result<LargeTradePanelArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut cache_dir = None;
    let mut download_concurrency = 1usize;
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
            "--help" | "-h" => bail!(large_trade_panel_usage()),
            _ => bail!("unknown argument: {arg}\n{}", large_trade_panel_usage()),
        }
    }
    if !(1..=2).contains(&download_concurrency) {
        bail!("--download-concurrency must be between 1 and 2 for large aggTrades files");
    }
    Ok(LargeTradePanelArgs {
        cache_dir: cache_dir.context("--cache-dir is required")?,
        download_concurrency,
        binance_rest_base,
        binance_data_base,
    })
}

/// 返回冻结大单面板的最小命令用法。
pub fn large_trade_panel_usage() -> &'static str {
    "Usage: market_btc_eth_large_trade_absorption_panel --cache-dir PATH [--download-concurrency 1]"
}

/// 运行只读大单吸收面板；覆盖失败时不读取任何 forward outcome。
pub async fn run_large_trade_absorption_panel(
    args: &LargeTradePanelArgs,
) -> Result<LargeTradePanelReport> {
    let (history_start_ms, signal_start_ms, split_ms, end_ms) = frozen_boundaries()?;
    let schedule = fixed_schedule(history_start_ms, end_ms.saturating_add(MS_24H));
    let kline_args = CrossExchangeBasisPanelArgs {
        manifest: PathBuf::new(),
        cache_dir: args.cache_dir.join("regular_15m"),
        download_concurrency: 2,
        binance_rest_base: args.binance_rest_base.clone(),
        binance_data_base: args.binance_data_base.clone(),
    };
    let (klines, kline_audit) = load_binance_klines(&kline_args, &schedule).await?;
    let first_month =
        NaiveDate::from_ymd_opt(2024, 6, 1).context("invalid first aggTrades month")?;
    let last_month = NaiveDate::from_ymd_opt(2025, 6, 1).context("invalid last aggTrades month")?;
    let (aggtrades, aggtrades_audit) =
        load_binance_aggtrades(args, first_month, last_month).await?;
    let (signals, mut stages) = build_signals(signal_start_ms, end_ms, &aggtrades, &klines);
    let coverage_gate_passed = aggtrades_audit.requested_files == 26
        && aggtrades_audit.available_files == 26
        && aggtrades_audit.missing_files == 0
        && aggtrades_audit.invalid_files == 0
        && kline_audit.requested_files == kline_audit.available_files
        && kline_audit.missing_files == 0
        && kline_audit.invalid_files == 0
        && signals.len() >= 1_400;
    let observations = if coverage_gate_passed {
        build_observations(&signals, &klines, &mut stages)
    } else {
        Vec::new()
    };
    let report = build_report(
        split_ms,
        kline_audit,
        aggtrades_audit,
        stages,
        coverage_gate_passed,
        &observations,
    );
    print_report(&report);
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

/// 生成 30 天历史、研究、Discovery/Validation 和结束边界。
fn frozen_boundaries() -> Result<(i64, i64, i64, i64)> {
    let timestamp = |year, month, day| {
        NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|time| time.and_utc().timestamp_millis())
            .context("invalid frozen large-trade boundary")
    };
    Ok((
        timestamp(2024, 6, 1)?,
        timestamp(2024, 7, 1)?,
        timestamp(2025, 1, 1)?,
        timestamp(2025, 7, 1)?,
    ))
}

/// 只用 T 前当前与 120 个历史 6h 桶构造吸收 score 排名。
fn build_signals(
    start_ms: i64,
    end_ms: i64,
    aggtrades: &BinanceAggTradesData,
    klines: &BTreeMap<String, Vec<BinanceCandle>>,
) -> (Vec<LargeTradeSignal>, LargeTradeStages) {
    let mut stages = LargeTradeStages::default();
    let mut signals = Vec::new();
    let mut decision_ts = start_ms;
    while decision_ts < end_ms {
        stages.decision_points += 1;
        let btc_score = absorption_score("BTC", "BTC-USDT-SWAP", decision_ts, aggtrades, klines);
        let eth_score = absorption_score("ETH", "ETH-USDT-SWAP", decision_ts, aggtrades, klines);
        let Some((btc_score, eth_score)) = btc_score.zip(eth_score) else {
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
        signals.push(LargeTradeSignal {
            decision_ts,
            long_btc: btc_score > eth_score,
            score_spread: (btc_score - eth_score).abs(),
        });
        decision_ts = decision_ts.saturating_add(MS_6H);
    }
    (signals, stages)
}

/// 分别标准化价格与二阶主动流，再取价格强度减尾部压力。
fn absorption_score(
    asset: &'static str,
    price_symbol: &str,
    decision_ts: i64,
    aggtrades: &BinanceAggTradesData,
    klines: &BTreeMap<String, Vec<BinanceCandle>>,
) -> Option<f64> {
    let flow = aggtrades.get(asset)?;
    let candles = klines.get(price_symbol)?;
    let current_start = decision_ts.checked_sub(MS_6H)?;
    let current_price = window_price_return(candles, current_start)?;
    let current_pressure = flow.get(&current_start)?.pressure()?;
    let mut historical_prices = Vec::with_capacity(BASELINE_WINDOWS);
    let mut historical_pressures = Vec::with_capacity(BASELINE_WINDOWS);
    for offset in 1..=BASELINE_WINDOWS {
        let start = current_start.checked_sub(offset as i64 * MS_6H)?;
        historical_prices.push(window_price_return(candles, start)?);
        historical_pressures.push(flow.get(&start)?.pressure()?);
    }
    let price_z = point_in_time_zscore(current_price, &historical_prices)?;
    let pressure_z = point_in_time_zscore(current_pressure, &historical_pressures)?;
    Some(price_z - pressure_z)
}

/// 用窗口首个开盘和最后一个已完成收盘计算 6h 对数收益。
fn window_price_return(candles: &[BinanceCandle], start_ts: i64) -> Option<f64> {
    let start_index = candle_index(candles, start_ts)?;
    let last_ts = start_ts.checked_add(MS_6H)?.checked_sub(MS_15M)?;
    let last_index = candle_index(candles, last_ts)?;
    if last_index.checked_sub(start_index)? != 23 {
        return None;
    }
    Some((candles[last_index].close / candles[start_index].open).ln())
}

/// 只用此前 120 个值计算样本标准差 z-score。
fn point_in_time_zscore(current: f64, history: &[f64]) -> Option<f64> {
    if history.len() != BASELINE_WINDOWS {
        return None;
    }
    let mean = history.iter().sum::<f64>() / history.len() as f64;
    let variance = history
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (history.len() - 1) as f64;
    let standard_deviation = variance.sqrt();
    (standard_deviation.is_finite() && standard_deviation > 0.0)
        .then_some((current - mean) / standard_deviation)
}

/// 覆盖门禁通过后才读取 T 之后的 6h/24h 开盘。
fn build_observations(
    signals: &[LargeTradeSignal],
    klines: &BTreeMap<String, Vec<BinanceCandle>>,
    stages: &mut LargeTradeStages,
) -> Vec<LargeTradeObservation> {
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
            Some(LargeTradeObservation {
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
    aggtrades_audit: BinanceAggTradesAudit,
    stages: LargeTradeStages,
    coverage_gate_passed: bool,
    observations: &[LargeTradeObservation],
) -> LargeTradePanelReport {
    let overall = summarize(observations);
    let discovery = subset_summary(observations, |row| row.decision_ts < split_ms);
    let validation = subset_summary(observations, |row| row.decision_ts >= split_ms);
    let mut by_month = BTreeMap::<(i32, u32), Vec<LargeTradeObservation>>::new();
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
    let counts_passed = overall.observations >= 1_400
        && discovery.observations >= 680
        && validation.observations >= 680;
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
    LargeTradePanelReport {
        rule_version: RULE_VERSION.to_owned(),
        universe_version: UNIVERSE_VERSION.to_owned(),
        kline_audit,
        aggtrades_audit,
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
fn subset_summary<F>(observations: &[LargeTradeObservation], predicate: F) -> LargeTradeSummary
where
    F: Fn(&LargeTradeObservation) -> bool,
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
fn quintile_summaries(observations: &[LargeTradeObservation]) -> Vec<LargeTradeSummary> {
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
fn summarize(observations: &[LargeTradeObservation]) -> LargeTradeSummary {
    if observations.is_empty() {
        return LargeTradeSummary::default();
    }
    let count = observations.len() as f64;
    let mean_6h = observations.iter().map(|row| row.forward_6h).sum::<f64>() / count;
    let mean_24h = observations.iter().map(|row| row.forward_24h).sum::<f64>() / count;
    LargeTradeSummary {
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
fn print_report(report: &LargeTradePanelReport) {
    println!(
        "large_trade_absorption_panel\trule={}\tuniverse={}\tkline_requested={}\tkline_available={}\tkline_missing={}\tkline_invalid={}\tkline_rows={}\tagg_requested={}\tagg_available={}\tagg_missing={}\tagg_invalid={}\tagg_rows={}\tdecision_points={}\tinput_blocked={}\tcomputable={}\ttied={}\tincomplete={}\tcoverage_gate_passed={}\tpositive_months={}\tnondecreasing_quintile_steps={}\tfactor_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.kline_audit.requested_files,
        report.kline_audit.available_files,
        report.kline_audit.missing_files,
        report.kline_audit.invalid_files,
        report.kline_audit.parsed_rows,
        report.aggtrades_audit.requested_files,
        report.aggtrades_audit.available_files,
        report.aggtrades_audit.missing_files,
        report.aggtrades_audit.invalid_files,
        report.aggtrades_audit.parsed_rows,
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
    for example in &report.aggtrades_audit.invalid_examples {
        println!("aggtrades_invalid_example\t{example}");
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
fn print_summary(group: &str, summary: &LargeTradeSummary) {
    println!(
        "large_trade_summary\tgroup={}\tobservations={}\tscore_spread={:?}\tgross_6h={:?}\tgross_24h={:?}\tstandard_net_6h={:?}\tdouble_net_6h={:?}\tpositive_6h_pct={:?}\tpositive_24h_pct={:?}",
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
    fn zscore_excludes_the_current_value() {
        let history = (0..BASELINE_WINDOWS)
            .map(|value| value as f64)
            .collect::<Vec<_>>();
        let score = point_in_time_zscore(200.0, &history).unwrap();
        assert!(score > 4.0);
    }

    #[test]
    fn summary_deducts_all_four_pair_fills() {
        let rows = vec![LargeTradeObservation {
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
    fn args_limit_large_file_concurrency() {
        let args = parse_large_trade_panel_args(vec![
            "--cache-dir".to_owned(),
            "/tmp/large-trade".to_owned(),
        ])
        .unwrap();
        assert_eq!(args.download_concurrency, 1);
        assert!(parse_large_trade_panel_args(vec![
            "--cache-dir".to_owned(),
            "/tmp/large-trade".to_owned(),
            "--download-concurrency".to_owned(),
            "3".to_owned(),
        ])
        .is_err());
    }
}
