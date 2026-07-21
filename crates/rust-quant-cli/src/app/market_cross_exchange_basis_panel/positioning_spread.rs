use super::binance_klines::{load_binance_klines, BinanceCandle, BinanceKlineAudit};
use super::binance_positioning::{
    load_binance_positioning, BinancePositioningAudit, BinancePositioningPoint,
};
use super::{CrossExchangeBasisPanelArgs, HistoricalUniverseManifest, UniverseSchedule, MS_15M};
use anyhow::{Context, Result};
use std::collections::BTreeMap;

const MS_5M: i64 = 5 * 60 * 1_000;
const MS_8H: i64 = 8 * 60 * 60 * 1_000;
const MS_24H: i64 = 24 * 60 * 60 * 1_000;
const MIN_CROSS_SECTION: usize = 30;
const RULE_VERSION_TOP_SIZE: &str = "top_position_over_account_ratio_rank1_rankN_8h_v1";
const RULE_VERSION_VS_CROWD: &str = "top_position_over_global_account_ratio_rank1_rankN_8h_v1";

/// 显式区分已淘汰的头部内部规模因子与第三字段 crowd 分歧因子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PositioningScoreRule {
    /// 头部持仓金额方向除以头部账户数量方向。
    TopSizeConviction,
    /// 头部持仓金额方向除以全市场账户数量方向。
    TopPositionVsCrowd,
}

impl PositioningScoreRule {
    /// 只使用当前点计算冻结 score；缺少所需字段即阻塞该成员。
    fn score(self, point: BinancePositioningPoint) -> Option<f64> {
        let denominator = match self {
            Self::TopSizeConviction => point.account_ratio,
            Self::TopPositionVsCrowd => point.global_account_ratio?,
        };
        let value = (point.position_ratio / denominator).ln();
        value.is_finite().then_some(value)
    }

    /// 返回与字段组合绑定的可审计版本。
    fn rule_version(self) -> &'static str {
        match self {
            Self::TopSizeConviction => RULE_VERSION_TOP_SIZE,
            Self::TopPositionVsCrowd => RULE_VERSION_VS_CROWD,
        }
    }
}

/// Top-trader 定位因子从 5m 覆盖到固定 15m outcome 的漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TopTraderPositioningStages {
    /// 十二个月内 UTC 8 小时决策点数。
    pub decision_points: usize,
    /// 精确 T-5m 指标成员少于 30 的时点数。
    pub coverage_blocked: usize,
    /// 未阻塞时点成功计算的规模确信度分数数。
    pub factor_observations: usize,
    /// 完成极端和中间分位四腿排序的时点数。
    pub selected_pairs: usize,
    /// 选中四腿缺少严格连续 8h/24h outcome 的时点数。
    pub incomplete_outcomes: usize,
}

/// 一个极端或对照价差的样本、收益和命中率。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TopTraderPositioningSummary {
    /// 完整观察数。
    pub observations: usize,
    /// 多空 signal score 差的平均值。
    pub mean_score_spread: Option<f64>,
    /// 8h 平均毛价差。
    pub mean_forward_8h: Option<f64>,
    /// 24h 平均毛价差。
    pub mean_forward_24h: Option<f64>,
    /// 8h 毛价差为正的比例，单位百分比。
    pub positive_rate_8h_pct: Option<f64>,
    /// 24h 毛价差为正的比例，单位百分比。
    pub positive_rate_24h_pct: Option<f64>,
}

/// Top-trader 规模确信度面板的官方文件、稳定性和集中度报告。
#[derive(Debug, Clone, PartialEq)]
pub struct TopTraderPositioningReport {
    /// 冻结因子与 outcome 规则身份。
    pub rule_version: String,
    /// 历史币池版本。
    pub universe_version: String,
    /// OKX 历史币池唯一 current-live 成员数。
    pub okx_symbols: usize,
    /// Binance regular 15m 文件审计。
    pub kline_audit: BinanceKlineAudit,
    /// Binance daily metrics 文件审计。
    pub positioning_audit: BinancePositioningAudit,
    /// 因果候选漏斗。
    pub stages: TopTraderPositioningStages,
    /// 极端 rank1/rankN 全窗口价差。
    pub factor_overall: TopTraderPositioningSummary,
    /// 中间 25%/75% 全窗口对照。
    pub control_overall: TopTraderPositioningSummary,
    /// 前六个月极端价差。
    pub factor_discovery: TopTraderPositioningSummary,
    /// 前六个月对照价差。
    pub control_discovery: TopTraderPositioningSummary,
    /// 后六个月极端价差。
    pub factor_validation: TopTraderPositioningSummary,
    /// 后六个月对照价差。
    pub control_validation: TopTraderPositioningSummary,
    /// 每个历史月份的极端价差。
    pub monthly: Vec<(i64, TopTraderPositioningSummary)>,
    /// 24h 平均毛价差为正的月份数。
    pub positive_months: usize,
    /// 参与极端腿最多的合约。
    pub most_frequent_symbol: Option<String>,
    /// 该合约参与极端腿次数。
    pub most_frequent_symbol_count: usize,
    /// 参与次数占全部观察的比例，单位百分比。
    pub most_frequent_symbol_pct: Option<f64>,
    /// 是否通过全部预注册边际价值门槛。
    pub factor_gate_passed: bool,
}

/// 单腿从决策开盘到固定 8h/24h 开盘的收益。
#[derive(Debug, Clone, Copy, PartialEq)]
struct LegOutcome {
    /// 8h 开盘对开盘收益。
    forward_8h: f64,
    /// 24h 开盘对开盘收益。
    forward_24h: f64,
}

/// 同一决策点保存极端价差和中间分位对照。
#[derive(Debug, Clone, PartialEq)]
struct PositioningObservation {
    /// 决策时间。
    decision_ts: i64,
    /// 极端多头 symbol。
    long_symbol: String,
    /// 极端空头 symbol。
    short_symbol: String,
    /// 极端多头 score 减空头 score。
    factor_score_spread: f64,
    /// 对照多头 score 减空头 score。
    control_score_spread: f64,
    /// 极端多头 outcome。
    long_outcome: LegOutcome,
    /// 极端空头裸价格 outcome。
    short_outcome: LegOutcome,
    /// 对照多头 outcome。
    control_long_outcome: LegOutcome,
    /// 对照空头裸价格 outcome。
    control_short_outcome: LegOutcome,
}

/// 运行完整 point-in-time top-trader 因子面板，不写交易事实。
pub async fn run_top_trader_positioning_spread_panel(
    args: &CrossExchangeBasisPanelArgs,
) -> Result<TopTraderPositioningReport> {
    run_panel(args, PositioningScoreRule::TopSizeConviction).await
}

/// 运行头部持仓方向相对全市场账户方向的独立分歧面板。
pub async fn run_top_trader_vs_crowd_spread_panel(
    args: &CrossExchangeBasisPanelArgs,
) -> Result<TopTraderPositioningReport> {
    run_panel(args, PositioningScoreRule::TopPositionVsCrowd).await
}

/// 共享官方档案与 outcome，仅由显式枚举选择预注册字段组合。
async fn run_panel(
    args: &CrossExchangeBasisPanelArgs,
    score_rule: PositioningScoreRule,
) -> Result<TopTraderPositioningReport> {
    let manifest: HistoricalUniverseManifest =
        serde_json::from_slice(&std::fs::read(&args.manifest).with_context(|| {
            format!(
                "read top-trader positioning manifest {}",
                args.manifest.display()
            )
        })?)
        .context("decode top-trader positioning universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let (klines, kline_audit) = load_binance_klines(args, &schedule).await?;
    let (positioning, positioning_audit) = load_binance_positioning(args, &schedule).await?;
    let (observations, stages) = build_observations(&schedule, &positioning, &klines, score_rule);
    let report = build_report(
        &schedule,
        score_rule.rule_version(),
        kline_audit,
        positioning_audit,
        stages,
        &observations,
    );
    print_report(&report);
    Ok(report)
}

/// 每 8h 只读精确 T-5m 指标，完成确定性极端与对照排序。
fn build_observations(
    schedule: &UniverseSchedule,
    positioning: &BTreeMap<String, Vec<BinancePositioningPoint>>,
    klines: &BTreeMap<String, Vec<BinanceCandle>>,
    score_rule: PositioningScoreRule,
) -> (Vec<PositioningObservation>, TopTraderPositioningStages) {
    let mut stages = TopTraderPositioningStages::default();
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
        let factor_ts = decision_ts.saturating_sub(MS_5M);
        let mut ranked = window
            .members
            .iter()
            .filter_map(|symbol| {
                let point = positioning_at(positioning.get(symbol)?, factor_ts)?;
                score_rule.score(point).map(|score| (symbol.clone(), score))
            })
            .collect::<Vec<_>>();
        if ranked.len() < MIN_CROSS_SECTION {
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
        let selected = [
            (&ranked[0].0, ranked[0].1),
            (&ranked[ranked.len() - 1].0, ranked[ranked.len() - 1].1),
            (&ranked[control_long_index].0, ranked[control_long_index].1),
            (
                &ranked[control_short_index].0,
                ranked[control_short_index].1,
            ),
        ];
        if selected
            .iter()
            .map(|(symbol, _)| *symbol)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != 4
        {
            stages.coverage_blocked += 1;
            decision_ts = decision_ts.saturating_add(MS_8H);
            continue;
        }
        stages.selected_pairs += 1;
        let outcomes = selected
            .iter()
            .map(|(symbol, _)| {
                klines
                    .get(*symbol)
                    .and_then(|rows| leg_outcome(rows, decision_ts))
            })
            .collect::<Option<Vec<_>>>();
        if let Some(outcomes) = outcomes {
            observations.push(PositioningObservation {
                decision_ts,
                long_symbol: selected[0].0.to_owned(),
                short_symbol: selected[1].0.to_owned(),
                factor_score_spread: selected[0].1 - selected[1].1,
                control_score_spread: selected[2].1 - selected[3].1,
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

/// 精确读取决策前五分钟点，不允许使用未来或较早最近值。
fn positioning_at(points: &[BinancePositioningPoint], ts: i64) -> Option<BinancePositioningPoint> {
    let index = points.binary_search_by_key(&ts, |point| point.ts).ok()?;
    points.get(index).copied()
}

/// 从决策 15m 开盘计算固定 8h/24h 开盘收益并拒绝内部缺口。
fn leg_outcome(candles: &[BinanceCandle], decision_ts: i64) -> Option<LegOutcome> {
    let entry_index = candles
        .binary_search_by_key(&decision_ts, |candle| candle.ts)
        .ok()?;
    let exit_8h_ts = decision_ts.checked_add(MS_8H)?;
    let exit_24h_ts = decision_ts.checked_add(MS_24H)?;
    let exit_8h_index = candles
        .binary_search_by_key(&exit_8h_ts, |candle| candle.ts)
        .ok()?;
    let exit_24h_index = candles
        .binary_search_by_key(&exit_24h_ts, |candle| candle.ts)
        .ok()?;
    let window = candles.get(entry_index..=exit_24h_index)?;
    if window
        .windows(2)
        .any(|pair| pair[1].ts - pair[0].ts != MS_15M)
    {
        return None;
    }
    let entry = candles[entry_index].open;
    let forward_8h = candles[exit_8h_index].open / entry - 1.0;
    let forward_24h = candles[exit_24h_index].open / entry - 1.0;
    (entry > 0.0 && forward_8h.is_finite() && forward_24h.is_finite()).then_some(LegOutcome {
        forward_8h,
        forward_24h,
    })
}

/// 构造半年、月份、集中度和预注册门禁报告。
fn build_report(
    schedule: &UniverseSchedule,
    rule_version: &str,
    kline_audit: BinanceKlineAudit,
    positioning_audit: BinancePositioningAudit,
    stages: TopTraderPositioningStages,
    observations: &[PositioningObservation],
) -> TopTraderPositioningReport {
    let split_ms = schedule.windows[6].from_ms;
    let discovery = observations
        .iter()
        .filter(|value| value.decision_ts < split_ms)
        .collect::<Vec<_>>();
    let validation = observations
        .iter()
        .filter(|value| value.decision_ts >= split_ms)
        .collect::<Vec<_>>();
    let factor_overall = summarize_owned(observations, false);
    let control_overall = summarize(&observations.iter().collect::<Vec<_>>(), true);
    let factor_discovery = summarize(&discovery, false);
    let control_discovery = summarize(&discovery, true);
    let factor_validation = summarize(&validation, false);
    let control_validation = summarize(&validation, true);
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
            (window.from_ms, summarize(&values, false))
        })
        .collect::<Vec<_>>();
    let positive_months = monthly
        .iter()
        .filter(|(_, summary)| summary.mean_forward_24h.is_some_and(|value| value > 0.0))
        .count();
    let (most_frequent_symbol, most_frequent_symbol_count) = concentration(observations);
    let most_frequent_symbol_pct = (!observations.is_empty())
        .then_some(most_frequent_symbol_count as f64 / observations.len() as f64 * 100.0);
    let factor_gate_passed = observations.len() >= 1_000
        && discovery.len() >= 500
        && validation.len() >= 500
        && segment_passed(&factor_discovery, &control_discovery)
        && segment_passed(&factor_validation, &control_validation)
        && factor_overall
            .mean_forward_8h
            .is_some_and(|value| value > 0.0)
        && positive_months >= 8
        && most_frequent_symbol_pct.is_some_and(|value| value <= 20.0);
    TopTraderPositioningReport {
        rule_version: rule_version.to_owned(),
        universe_version: schedule.version.clone(),
        okx_symbols: schedule.union_symbols().len(),
        kline_audit,
        positioning_audit,
        stages,
        factor_overall,
        control_overall,
        factor_discovery,
        control_discovery,
        factor_validation,
        control_validation,
        monthly,
        positive_months,
        most_frequent_symbol,
        most_frequent_symbol_count,
        most_frequent_symbol_pct,
        factor_gate_passed,
    }
}

/// 判断半年是否同时满足经济幅度、命中与对照增量。
fn segment_passed(
    factor: &TopTraderPositioningSummary,
    control: &TopTraderPositioningSummary,
) -> bool {
    factor
        .mean_forward_24h
        .zip(control.mean_forward_24h)
        .is_some_and(|(candidate, baseline)| candidate >= 0.005 && candidate - baseline >= 0.0025)
        && factor
            .positive_rate_24h_pct
            .is_some_and(|value| value >= 55.0)
}

/// 汇总 owned 观察的极端或对照价差。
fn summarize_owned(
    values: &[PositioningObservation],
    control: bool,
) -> TopTraderPositioningSummary {
    summarize(&values.iter().collect::<Vec<_>>(), control)
}

/// 汇总引用观察的 score spread 与两个固定期限价差。
fn summarize(values: &[&PositioningObservation], control: bool) -> TopTraderPositioningSummary {
    if values.is_empty() {
        return TopTraderPositioningSummary::default();
    }
    let length = values.len() as f64;
    let score = |value: &&PositioningObservation| {
        if control {
            value.control_score_spread
        } else {
            value.factor_score_spread
        }
    };
    let returns = values
        .iter()
        .map(|value| {
            if control {
                (
                    value.control_long_outcome.forward_8h - value.control_short_outcome.forward_8h,
                    value.control_long_outcome.forward_24h
                        - value.control_short_outcome.forward_24h,
                )
            } else {
                (
                    value.long_outcome.forward_8h - value.short_outcome.forward_8h,
                    value.long_outcome.forward_24h - value.short_outcome.forward_24h,
                )
            }
        })
        .collect::<Vec<_>>();
    TopTraderPositioningSummary {
        observations: values.len(),
        mean_score_spread: Some(values.iter().map(score).sum::<f64>() / length),
        mean_forward_8h: Some(returns.iter().map(|value| value.0).sum::<f64>() / length),
        mean_forward_24h: Some(returns.iter().map(|value| value.1).sum::<f64>() / length),
        positive_rate_8h_pct: Some(
            returns.iter().filter(|value| value.0 > 0.0).count() as f64 / length * 100.0,
        ),
        positive_rate_24h_pct: Some(
            returns.iter().filter(|value| value.1 > 0.0).count() as f64 / length * 100.0,
        ),
    }
}

/// 统计极端两腿的最大单币参与次数。
fn concentration(values: &[PositioningObservation]) -> (Option<String>, usize) {
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

/// 输出官方文件、候选漏斗、半年、月份和集中度。
fn print_report(report: &TopTraderPositioningReport) {
    println!(
        "top_trader_positioning_panel\trule={}\tuniverse={}\tokx_symbols={}\tkline_mapped={}\tkline_requested={}\tkline_available={}\tkline_missing={}\tkline_invalid={}\tkline_rows={}\tpositioning_mapped={}\tpositioning_requested={}\tpositioning_available={}\tpositioning_missing={}\tpositioning_invalid={}\tpositioning_points={}\tdecision_points={}\tcoverage_blocked={}\tfactor_observations={}\tselected_pairs={}\tincomplete={}\tpositive_months={}\tmost_frequent_symbol={}\tmost_frequent_count={}\tmost_frequent_pct={}\tfactor_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.okx_symbols,
        report.kline_audit.mapped_symbols,
        report.kline_audit.requested_files,
        report.kline_audit.available_files,
        report.kline_audit.missing_files,
        report.kline_audit.invalid_files,
        report.kline_audit.parsed_rows,
        report.positioning_audit.mapped_symbols,
        report.positioning_audit.requested_files,
        report.positioning_audit.available_files,
        report.positioning_audit.missing_files,
        report.positioning_audit.invalid_files,
        report.positioning_audit.retained_points,
        report.stages.decision_points,
        report.stages.coverage_blocked,
        report.stages.factor_observations,
        report.stages.selected_pairs,
        report.stages.incomplete_outcomes,
        report.positive_months,
        report.most_frequent_symbol.as_deref().unwrap_or("NA"),
        report.most_frequent_symbol_count,
        optional(report.most_frequent_symbol_pct),
        report.factor_gate_passed,
    );
    for (label, summary) in [
        ("factor_overall", &report.factor_overall),
        ("control_overall", &report.control_overall),
        ("factor_discovery", &report.factor_discovery),
        ("control_discovery", &report.control_discovery),
        ("factor_validation", &report.factor_validation),
        ("control_validation", &report.control_validation),
    ] {
        print_summary(label, summary);
    }
    for (from_ms, summary) in &report.monthly {
        print_summary(&format!("month_{from_ms}"), summary);
    }
}

/// 输出单个 top-trader 分组的分数与价差结果。
fn print_summary(label: &str, summary: &TopTraderPositioningSummary) {
    println!(
        "top_trader_positioning_summary\tgroup={}\tobservations={}\tscore_spread={}\tmean_8h={}\tmean_24h={}\tpositive_8h_pct={}\tpositive_24h_pct={}",
        label,
        summary.observations,
        optional(summary.mean_score_spread),
        optional(summary.mean_forward_8h),
        optional(summary.mean_forward_24h),
        optional(summary.positive_rate_8h_pct),
        optional(summary.positive_rate_24h_pct),
    );
}

/// 稳定格式化缺失浮点指标。
fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_owned(), |number| number.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造连续 Binance 15m 开盘序列。
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
    fn conviction_score_is_position_ratio_over_account_ratio() {
        let point = BinancePositioningPoint {
            ts: 0,
            account_ratio: 0.5,
            position_ratio: 2.0,
            global_account_ratio: Some(0.25),
        };
        assert!(((point.position_ratio / point.account_ratio).ln() - 4.0_f64.ln()).abs() < 1e-12);
    }

    #[test]
    fn positioning_lookup_requires_exact_prior_five_minute_point() {
        let points = [BinancePositioningPoint {
            ts: 10,
            account_ratio: 1.0,
            position_ratio: 1.0,
            global_account_ratio: Some(1.0),
        }];
        assert!(positioning_at(&points, 9).is_none());
        assert_eq!(positioning_at(&points, 10), Some(points[0]));
    }

    #[test]
    fn price_outcome_uses_decision_open_and_rejects_internal_gap() {
        let mut values = candles(0, &vec![100.0; 97]);
        values[32].open = 104.0;
        values[96].open = 110.0;
        let outcome = leg_outcome(&values, 0).unwrap();
        assert!((outcome.forward_8h - 0.04).abs() < 1e-12);
        assert!((outcome.forward_24h - 0.10).abs() < 1e-12);
        values[20].ts += 1;
        assert!(leg_outcome(&values, 0).is_none());
    }

    #[test]
    fn summary_is_equal_notional_long_minus_short() {
        let observation = PositioningObservation {
            decision_ts: 0,
            long_symbol: "AAA-USDT-SWAP".to_owned(),
            short_symbol: "BBB-USDT-SWAP".to_owned(),
            factor_score_spread: 2.0,
            control_score_spread: 0.5,
            long_outcome: LegOutcome {
                forward_8h: 0.03,
                forward_24h: 0.05,
            },
            short_outcome: LegOutcome {
                forward_8h: -0.02,
                forward_24h: -0.04,
            },
            control_long_outcome: LegOutcome {
                forward_8h: 0.0,
                forward_24h: 0.0,
            },
            control_short_outcome: LegOutcome {
                forward_8h: 0.0,
                forward_24h: 0.0,
            },
        };
        let summary = summarize_owned(&[observation], false);
        assert_eq!(summary.mean_score_spread, Some(2.0));
        assert!((summary.mean_forward_8h.unwrap() - 0.05).abs() < 1e-12);
        assert!((summary.mean_forward_24h.unwrap() - 0.09).abs() < 1e-12);
    }

    #[test]
    fn crowd_score_uses_global_account_ratio_and_requires_it() {
        let point = BinancePositioningPoint {
            ts: 0,
            account_ratio: 4.0,
            position_ratio: 2.0,
            global_account_ratio: Some(0.5),
        };
        let score = PositioningScoreRule::TopPositionVsCrowd
            .score(point)
            .unwrap();
        assert!((score - 4.0_f64.ln()).abs() < 1e-12);
        assert!(PositioningScoreRule::TopPositionVsCrowd
            .score(BinancePositioningPoint {
                global_account_ratio: None,
                ..point
            })
            .is_none());
    }
}
