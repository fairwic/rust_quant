//! Bollinger 长影基础形态之上的“近期 EMA12 领先 EMA144/576”L1 无标签诊断。
//!
//! 本模块只读取信号 K 及此前已完成 K 的均线状态。EMA 只否决既有 V2 方向，
//! 不产生方向，也不读取成交、未来 K 线或任何结果标签。

use super::single_bar_ema_12_144_576_alignment::{
    ema576_close_series, ema_input_coverage, ClarifiedExitContract, EmaInputCoverage,
};
use super::{
    build_l1_report, frozen_l1_args, L1Candidate, L1Coverage, RESEARCH_CANDIDATE_KEY,
    RESEARCH_RULE_VERSION,
};
use crate::app::market_velocity_event_backtest::{
    config_from_env_and_args, load_backtest_data, BacktestDataSet, ComputedCandle,
    MarketVelocityEventBacktestArgs, MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_STRATEGY_KEY,
};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// 独立 Research 规则身份；旧的严格三线 `false -> true` 版本保持不变。
pub const RECENT_FAST_EMA_LEAD_RULE_VERSION: &str =
    "l1_bb20x2p5_wick_touch_reject_opposite_ema12_leads_144_576_age_le_192_v1";
/// 48 小时等于 192 根 15m K，来源于基础策略已冻结的最长持仓时限。
pub const RECENT_FAST_EMA_LEAD_MAX_AGE_BARS: usize = 192;
/// 用户要求单独验证的 24 小时版本，不覆盖 192 根版本的研究身份。
pub const RECENT_FAST_EMA_LEAD_96_RULE_VERSION: &str =
    "l1_bb20x2p5_wick_touch_reject_opposite_ema12_leads_144_576_age_le_96_v2";
/// 24 小时等于 96 根 15m K；只改变年龄上限，不改变趋势状态定义。
pub const RECENT_FAST_EMA_LEAD_96_MAX_AGE_BARS: usize = 96;
/// 同方向候选在 60 分钟内单链归并为同一市场事件。
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
/// 用户给出的失败样本只用于定义对齐，不能作为收益筛选标签。
const TARGET_SAMPLES: [(&str, i64, usize); 5] = [
    ("AGLD-USDT-SWAP", 1_783_444_500_000, 3),
    ("YFI-USDT-SWAP", 1_783_328_400_000, 62),
    ("NMR-USDT-SWAP", 1_782_962_100_000, 158),
    ("ORDI-USDT-SWAP", 1_782_738_900_000, 53),
    ("SATS-USDT-SWAP", 1_782_647_100_000, 29),
];

/// 一个可审计的年龄上限变体；仅承载机器身份和唯一阈值，不开放任意运行时调参。
#[derive(Debug, Clone, Copy)]
struct RecentFastEmaLeadVariant {
    /// 报告 schema 身份。
    schema_version: &'static str,
    /// 策略规则版本。
    rule_version: &'static str,
    /// 报告中记录的唯一变量。
    only_variable: &'static str,
    /// 对向状态仍被视为近期的最大连续 15m K 数。
    max_age_bars: usize,
}

/// 原 192 根变体，保留旧入口的可复现行为。
const AGE_192_VARIANT: RecentFastEmaLeadVariant = RecentFastEmaLeadVariant {
    schema_version: "momentum_bollinger_recent_fast_ema_lead_l1_v1",
    rule_version: RECENT_FAST_EMA_LEAD_RULE_VERSION,
    only_variable: "reject when the opposite EMA12 fast-lead state has remained continuously true for 1..=192 completed 15m candles at the signal close",
    max_age_bars: RECENT_FAST_EMA_LEAD_MAX_AGE_BARS,
};

/// 新 96 根变体只缩短年龄上限，其他研究与门禁合同全部复用。
const AGE_96_VARIANT: RecentFastEmaLeadVariant = RecentFastEmaLeadVariant {
    schema_version: "momentum_bollinger_recent_fast_ema_lead_96_l1_v2",
    rule_version: RECENT_FAST_EMA_LEAD_96_RULE_VERSION,
    only_variable: "change only the opposite EMA12 fast-lead rejection age ceiling from 192 to 96 completed 15m candles",
    max_age_bars: RECENT_FAST_EMA_LEAD_96_MAX_AGE_BARS,
};

/// 机器报告身份明确记录方向来源、唯一变量和时序边界。
#[derive(Debug, Clone, Serialize)]
pub struct RecentFastEmaLeadIdentity {
    /// 当前研究等级；L1 禁止读取结果标签。
    pub level: &'static str,
    /// Bollinger 长影研究候选键。
    pub candidate_key: &'static str,
    /// 本轮独立规则版本。
    pub rule_version: &'static str,
    /// 提供 Bollinger 外轨触碰 cohort 的规则版本。
    pub source_rule_version: &'static str,
    /// 真正产生多空方向的既有策略。
    pub source_strategy_key: &'static str,
    /// 既有策略的精确入场规则版本。
    pub source_entry_rule_version: &'static str,
    /// 从来源方向到新增否决的固定执行顺序。
    pub filter_pipeline: &'static str,
    /// 本轮唯一变量。
    pub only_variable: &'static str,
    /// 单根均线状态定义，不包含慢线之间的排序。
    pub fast_lead_definition: &'static str,
    /// 趋势年龄定义。
    pub age_definition: &'static str,
    /// EMA 的冻结计算口径。
    pub ema_calculation_policy: &'static str,
    /// 明确禁止读取的结果字段。
    pub label_boundary: &'static str,
}

/// 一条基础触轨 setup 在信号完成时可见的 EMA 趋势年龄。
#[derive(Debug, Clone, Serialize)]
pub struct RecentFastEmaLeadCandidate {
    /// OKX 永续合约标识。
    pub symbol: String,
    /// 信号 K 开始时间，Unix 毫秒。
    pub signal_ts_ms: i64,
    /// UTC 月份用于无标签分散性检查。
    pub signal_month_utc: String,
    /// `long` 或 `short`，继承既有 V2，不由 EMA 决定。
    pub direction: &'static str,
    /// 做多检查 `bearish`，做空检查 `bullish`。
    pub opposite_fast_lead: &'static str,
    /// 信号 K 收盘后的 EMA12。
    pub ema12: Option<f64>,
    /// 信号 K 收盘后的 EMA144。
    pub ema144: Option<f64>,
    /// 信号 K 收盘后的独立 EMA576。
    pub ema576: Option<f64>,
    /// `Some(0)` 表示当前未领先，正数表示连续领先根数，`None` 表示 EMA 不可判定。
    pub opposite_fast_lead_age_bars: Option<usize>,
    /// true 表示年龄位于当前规则版本冻结的近期窗口，应拒绝该反向候选。
    pub rejected_by_recent_fast_ema_lead: bool,
}

/// 用户样本的信号时定义复核；故意不携带胜负或 R。
#[derive(Debug, Clone, Serialize)]
pub struct TargetSampleAudit {
    /// 目标币种。
    pub symbol: &'static str,
    /// 目标信号时间戳。
    pub signal_ts_ms: i64,
    /// 预注册的连续领先年龄。
    pub expected_age_bars: usize,
    /// 机器从完整 EMA 序列复算的年龄；未找到候选时为 `None`。
    pub actual_age_bars: Option<usize>,
    /// 基础触轨账本是否包含该样本。
    pub found: bool,
    /// 复算年龄是否精确匹配预注册值。
    pub age_matches: bool,
    /// 新规则是否拒绝该样本。
    pub rejected: bool,
}

/// 近期 EMA12 领先过滤的无标签覆盖汇总。
#[derive(Debug, Clone, Serialize)]
pub struct RecentFastEmaLeadSummary {
    /// Bollinger 长影基础 setup 数。
    pub base_touch_setups: usize,
    /// 三条 EMA 均可判定的 setup 数。
    pub ema_ready_setups: usize,
    /// 任一 EMA 缺失或异常的 setup 数。
    pub ema_not_ready_setups: usize,
    /// 信号 K 已处于对向 EMA12 领先状态的 setup 数，不限制年龄。
    pub current_opposite_fast_lead_setups: usize,
    /// 年龄位于当前版本近期窗口、会被拒绝的 setup 数。
    pub rejected_setups: usize,
    /// 拒绝数占基础 setup 的百分比。
    pub impact_pct: f64,
    /// 被拒绝 setup 的多空分布。
    pub rejected_by_direction: BTreeMap<&'static str, usize>,
    /// 被拒绝 setup 覆盖币种数。
    pub rejected_symbol_count: usize,
    /// 被拒绝 setup 覆盖 UTC 月份数。
    pub rejected_month_count: usize,
    /// 被拒绝 setup 按方向与 60 分钟归并后的有效事件数。
    pub rejected_effective_market_events: usize,
    /// 全部基础 setup 的信号时年龄分布。
    pub age_distribution: BTreeMap<&'static str, usize>,
}

/// 预注册门槛与当前 L1 停止边界。
#[derive(Debug, Clone, Serialize)]
pub struct RecentFastEmaLeadDecision {
    /// `stop` 或 `coverage_pass_ready_for_l2_prereg`。
    pub status: &'static str,
    /// 查看结果前冻结的逐项门槛。
    pub gates: BTreeMap<&'static str, bool>,
    /// 当前停止或继续原因。
    pub reason: String,
    /// L1 必须恒为 false。
    pub outcome_evaluation_performed: bool,
    /// 用户 5 个目标样本是否全部完成定义对齐。
    pub target_sample_audit_completed: bool,
}

/// 近期 EMA12 领先双慢线 L1 的完整机器产物。
#[derive(Debug, Clone, Serialize)]
pub struct RecentFastEmaLeadReport {
    /// 报告 schema；字段语义变化必须升级。
    pub schema_version: &'static str,
    /// 生成时间不参与行情指纹。
    pub generated_at_utc: String,
    /// 冻结研究身份。
    pub identity: RecentFastEmaLeadIdentity,
    /// 基础 Bollinger cohort 的覆盖与局限。
    pub base_coverage: L1Coverage,
    /// EMA576 输入窗口的独立覆盖身份。
    pub ema_input_coverage: EmaInputCoverage,
    /// 已澄清但不在本批次执行的退出合同。
    pub clarified_exit_contract: ClarifiedExitContract,
    /// 无标签覆盖汇总。
    pub summary: RecentFastEmaLeadSummary,
    /// 目标样本定义复核。
    pub target_sample_audit: Vec<TargetSampleAudit>,
    /// 预注册停止条件结果。
    pub decision: RecentFastEmaLeadDecision,
    /// 全部基础触轨 setup 的信号时账本。
    pub candidates: Vec<RecentFastEmaLeadCandidate>,
}

/// 对向 EMA12 领先状态的方向。
#[derive(Debug, Clone, Copy)]
enum FastLeadDirection {
    /// `EMA12 > EMA144 && EMA12 > EMA576`。
    Bullish,
    /// `EMA12 < EMA144 && EMA12 < EMA576`。
    Bearish,
}

/// 读取冻结行情并写出不含任何结果标签的 L1 机器报告。
pub async fn run_recent_fast_ema_lead_l1_scan(output: &Path) -> Result<RecentFastEmaLeadReport> {
    run_recent_fast_ema_lead_variant_l1_scan(output, AGE_192_VARIANT).await
}

/// 读取相同冻结行情并写出 96 根独立版本的无标签 L1 机器报告。
pub async fn run_recent_fast_ema_lead_96_l1_scan(output: &Path) -> Result<RecentFastEmaLeadReport> {
    run_recent_fast_ema_lead_variant_l1_scan(output, AGE_96_VARIANT).await
}

/// 两个固定版本共享行情加载与账本生成，避免阈值变体发生实现漂移。
async fn run_recent_fast_ema_lead_variant_l1_scan(
    output: &Path,
    variant: RecentFastEmaLeadVariant,
) -> Result<RecentFastEmaLeadReport> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for recent fast EMA lead L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_recent_fast_ema_lead_report(&data, &config.args, variant)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建近期 EMA L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入近期 EMA L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 在相同基础 cohort 上附加趋势年龄，不改变来源候选的方向或其他字段。
fn build_recent_fast_ema_lead_report(
    data: &BacktestDataSet,
    args: &MarketVelocityEventBacktestArgs,
    variant: RecentFastEmaLeadVariant,
) -> Result<RecentFastEmaLeadReport> {
    let base_report = build_l1_report(data, args)?;
    let ema_input_coverage = ema_input_coverage(data, &base_report.coverage)?;
    let candidates = build_candidates(data, &base_report.candidates, variant.max_age_bars)?;
    let summary = summarize_candidates(&candidates);
    let target_sample_audit = audit_target_samples(&candidates);
    let decision = decide_l1(&summary, &target_sample_audit);

    Ok(RecentFastEmaLeadReport {
        schema_version: variant.schema_version,
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: RecentFastEmaLeadIdentity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: RESEARCH_CANDIDATE_KEY,
            rule_version: variant.rule_version,
            source_rule_version: RESEARCH_RULE_VERSION,
            source_strategy_key: MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_STRATEGY_KEY,
            source_entry_rule_version:
                MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION,
            filter_pipeline: "existing V2 directional-wick long/short candidate -> corresponding Bollinger(20,2.5) outer-band touch -> recent opposite EMA12-leading-both-slow-lines veto",
            only_variable: variant.only_variable,
            fast_lead_definition: "short veto state is EMA12>EMA144 && EMA12>EMA576; long veto is the mirror; EMA144 versus EMA576 ordering is irrelevant",
            age_definition: "age is 1 on the first completed candle where the state is true, increments on each adjacent true candle, and resets to 0 when false; no three-candle confirmation is required",
            ema_calculation_policy: "EMA12/144 use existing SMA-seeded computed series; EMA576 uses the first 576 closes SMA seed and alpha=2/(576+1) recursion; no EMA slope is read",
            label_boundary: "candidate construction reads no fill, future candle, MFE, MAE, exit, PnL, R, win, or loss fields",
        },
        base_coverage: base_report.coverage,
        ema_input_coverage,
        clarified_exit_contract: ClarifiedExitContract {
            middle_band_partial_close_fraction: 0.5,
            remaining_stop_after_partial: "actual_entry_price",
            final_exit_plan:
                "close remaining quantity at opposite outer-band proximity or +5 initial R, whichever occurs first",
            evaluated_in_this_batch: false,
        },
        summary,
        target_sample_audit,
        decision,
        candidates,
    })
}

/// 为每个币种只递推一次 EMA576 和双向年龄，再映射到基础触轨候选。
fn build_candidates(
    data: &BacktestDataSet,
    source_candidates: &[L1Candidate],
    max_age_bars: usize,
) -> Result<Vec<RecentFastEmaLeadCandidate>> {
    let mut sources_by_symbol: BTreeMap<&str, Vec<&L1Candidate>> = BTreeMap::new();
    for source in source_candidates
        .iter()
        .filter(|candidate| candidate.touches_directional_outer_band)
    {
        sources_by_symbol
            .entry(source.symbol.as_str())
            .or_default()
            .push(source);
    }

    let mut candidates = Vec::new();
    for (symbol, sources) in sources_by_symbol {
        let candles = data
            .candles_15m_computed
            .get(symbol)
            .with_context(|| format!("missing computed candles for {symbol}"))?;
        let ema576_series = ema576_close_series(candles);
        let bullish_ages =
            fast_lead_age_series(candles, &ema576_series, FastLeadDirection::Bullish);
        let bearish_ages =
            fast_lead_age_series(candles, &ema576_series, FastLeadDirection::Bearish);
        for source in sources {
            let signal_idx = candles
                .binary_search_by_key(&source.signal_ts_ms, |candle| candle.candle.ts)
                .map_err(|_| anyhow::anyhow!("missing signal candle for {}", source.symbol))?;
            let latest = &candles[signal_idx];
            let direction = opposite_fast_lead(source.direction)
                .with_context(|| format!("invalid trade direction: {}", source.direction))?;
            let age = match direction {
                FastLeadDirection::Bullish => bullish_ages[signal_idx],
                FastLeadDirection::Bearish => bearish_ages[signal_idx],
            };
            candidates.push(RecentFastEmaLeadCandidate {
                symbol: source.symbol.clone(),
                signal_ts_ms: source.signal_ts_ms,
                signal_month_utc: source.signal_month_utc.clone(),
                direction: source.direction,
                opposite_fast_lead: fast_lead_label(direction),
                ema12: latest.ema12,
                ema144: latest.ema144,
                ema576: ema576_series[signal_idx],
                opposite_fast_lead_age_bars: age,
                rejected_by_recent_fast_ema_lead: age
                    .is_some_and(|age| is_recent_age(age, max_age_bars)),
            });
        }
    }
    candidates.sort_by(|left, right| {
        (left.signal_ts_ms, left.direction, left.symbol.as_str()).cmp(&(
            right.signal_ts_ms,
            right.direction,
            right.symbol.as_str(),
        ))
    });
    Ok(candidates)
}

/// 单次前向遍历生成连续年龄，避免为每个候选反复向历史回扫。
fn fast_lead_age_series(
    candles: &[ComputedCandle],
    ema576_series: &[Option<f64>],
    direction: FastLeadDirection,
) -> Vec<Option<usize>> {
    let mut ages = Vec::with_capacity(candles.len());
    let mut consecutive_age = 0usize;
    for (idx, candle) in candles.iter().enumerate() {
        let state = fast_lead_state(candle, ema576_series.get(idx).copied().flatten(), direction);
        match state {
            Some(true) => {
                consecutive_age = consecutive_age.saturating_add(1);
                ages.push(Some(consecutive_age));
            }
            Some(false) => {
                consecutive_age = 0;
                ages.push(Some(0));
            }
            None => {
                consecutive_age = 0;
                ages.push(None);
            }
        }
    }
    ages
}

/// 只比较 EMA12 是否在两条慢线同一侧；EMA144/576 互相交叉不改变早期趋势状态。
fn fast_lead_state(
    candle: &ComputedCandle,
    ema576: Option<f64>,
    direction: FastLeadDirection,
) -> Option<bool> {
    let ema12 = candle.ema12.filter(|value| positive(*value))?;
    let ema144 = candle.ema144.filter(|value| positive(*value))?;
    let ema576 = ema576.filter(|value| positive(*value))?;
    Some(match direction {
        FastLeadDirection::Bullish => ema12 > ema144 && ema12 > ema576,
        FastLeadDirection::Bearish => ema12 < ema144 && ema12 < ema576,
    })
}

/// 做空检查对向多头状态，做多检查对向空头状态。
fn opposite_fast_lead(direction: &str) -> Option<FastLeadDirection> {
    match direction {
        "long" => Some(FastLeadDirection::Bearish),
        "short" => Some(FastLeadDirection::Bullish),
        _ => None,
    }
}

/// 为机器账本提供稳定的趋势方向标签。
fn fast_lead_label(direction: FastLeadDirection) -> &'static str {
    match direction {
        FastLeadDirection::Bullish => "bullish",
        FastLeadDirection::Bearish => "bearish",
    }
}

/// 近期窗口包含首次成立的信号 K，超过当前版本上限后不再定义为“刚形成不久”。
fn is_recent_age(age: usize, max_age_bars: usize) -> bool {
    (1..=max_age_bars).contains(&age)
}

/// 汇总只依赖信号时字段的覆盖、年龄与分散性。
fn summarize_candidates(candidates: &[RecentFastEmaLeadCandidate]) -> RecentFastEmaLeadSummary {
    let mut rejected_by_direction = BTreeMap::new();
    let mut symbols = BTreeSet::new();
    let mut months = BTreeSet::new();
    let mut age_distribution = BTreeMap::from([
        ("not_ready", 0),
        ("age_0_not_leading", 0),
        ("age_1_4", 0),
        ("age_5_12", 0),
        ("age_13_48", 0),
        ("age_49_96", 0),
        ("age_97_192", 0),
        ("age_gt_192", 0),
    ]);
    for candidate in candidates {
        let bucket = match candidate.opposite_fast_lead_age_bars {
            None => "not_ready",
            Some(0) => "age_0_not_leading",
            Some(1..=4) => "age_1_4",
            Some(5..=12) => "age_5_12",
            Some(13..=48) => "age_13_48",
            Some(49..=96) => "age_49_96",
            Some(97..=RECENT_FAST_EMA_LEAD_MAX_AGE_BARS) => "age_97_192",
            Some(_) => "age_gt_192",
        };
        *age_distribution.entry(bucket).or_default() += 1;
        if !candidate.rejected_by_recent_fast_ema_lead {
            continue;
        }
        *rejected_by_direction
            .entry(candidate.direction)
            .or_default() += 1;
        symbols.insert(candidate.symbol.as_str());
        months.insert(candidate.signal_month_utc.as_str());
    }
    let ema_not_ready_setups = candidates
        .iter()
        .filter(|candidate| candidate.opposite_fast_lead_age_bars.is_none())
        .count();
    let current_opposite_fast_lead_setups = candidates
        .iter()
        .filter(|candidate| {
            candidate
                .opposite_fast_lead_age_bars
                .is_some_and(|age| age > 0)
        })
        .count();
    let rejected_setups = candidates
        .iter()
        .filter(|candidate| candidate.rejected_by_recent_fast_ema_lead)
        .count();
    RecentFastEmaLeadSummary {
        base_touch_setups: candidates.len(),
        ema_ready_setups: candidates.len().saturating_sub(ema_not_ready_setups),
        ema_not_ready_setups,
        current_opposite_fast_lead_setups,
        rejected_setups,
        impact_pct: percentage(rejected_setups, candidates.len()),
        rejected_by_direction,
        rejected_symbol_count: symbols.len(),
        rejected_month_count: months.len(),
        rejected_effective_market_events: effective_market_event_count(candidates),
        age_distribution,
    }
}

/// 将 5 个用户样本与机器复算年龄逐笔对齐，不接触其交易结果。
fn audit_target_samples(candidates: &[RecentFastEmaLeadCandidate]) -> Vec<TargetSampleAudit> {
    TARGET_SAMPLES
        .iter()
        .map(|(symbol, signal_ts_ms, expected_age_bars)| {
            let candidate = candidates.iter().find(|candidate| {
                candidate.symbol == *symbol && candidate.signal_ts_ms == *signal_ts_ms
            });
            let actual_age_bars =
                candidate.and_then(|candidate| candidate.opposite_fast_lead_age_bars);
            TargetSampleAudit {
                symbol,
                signal_ts_ms: *signal_ts_ms,
                expected_age_bars: *expected_age_bars,
                actual_age_bars,
                found: candidate.is_some(),
                age_matches: actual_age_bars == Some(*expected_age_bars),
                rejected: candidate
                    .is_some_and(|candidate| candidate.rejected_by_recent_fast_ema_lead),
            }
        })
        .collect()
}

/// 应用扫描前冻结的覆盖与目标样本门槛；任一失败均不进入结果回放。
fn decide_l1(
    summary: &RecentFastEmaLeadSummary,
    target_sample_audit: &[TargetSampleAudit],
) -> RecentFastEmaLeadDecision {
    let long_count = summary
        .rejected_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_count = summary
        .rejected_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    // 两个年龄版本故意共享原 5/5 目标门禁。这样 96 根放行 NMR 会明确失败，
    // 不会通过“预期放行”重写成功标准，掩盖阈值缩短带来的定义取舍。
    let targets_pass = target_sample_audit.len() == TARGET_SAMPLES.len()
        && target_sample_audit
            .iter()
            .all(|sample| sample.found && sample.age_matches && sample.rejected);
    let mut gates = BTreeMap::new();
    gates.insert(
        "ema_ready_for_all_base_setups",
        summary.ema_not_ready_setups == 0,
    );
    gates.insert("rejected_setups_at_least_20", summary.rejected_setups >= 20);
    gates.insert(
        "impact_between_5_and_45_pct",
        (5.0..=45.0).contains(&summary.impact_pct),
    );
    gates.insert(
        "both_directions_at_least_5",
        long_count >= 5 && short_count >= 5,
    );
    gates.insert("symbols_at_least_10", summary.rejected_symbol_count >= 10);
    gates.insert("months_at_least_6", summary.rejected_month_count >= 6);
    gates.insert(
        "effective_events_at_least_15",
        summary.rejected_effective_market_events >= 15,
    );
    gates.insert("all_5_target_samples_match_and_reject", targets_pass);
    let passed = gates.values().all(|value| *value);
    RecentFastEmaLeadDecision {
        status: if passed {
            "coverage_pass_ready_for_l2_prereg"
        } else {
            "stop"
        },
        gates,
        reason: if passed {
            "近期 EMA12 领先过滤通过预注册无标签覆盖与目标定义审计；可以另行预注册 L2，但本报告仍未读取结果标签。".to_owned()
        } else {
            "至少一项预注册无标签门槛未通过；按 L1 停止，不读取成交后结果标签。".to_owned()
        },
        outcome_evaluation_performed: false,
        target_sample_audit_completed: targets_pass,
    }
}

/// 按方向把 60 分钟内被拒绝的相关币种信号单链归并为市场事件。
fn effective_market_event_count(candidates: &[RecentFastEmaLeadCandidate]) -> usize {
    let mut sorted = candidates
        .iter()
        .filter(|candidate| candidate.rejected_by_recent_fast_ema_lead)
        .collect::<Vec<_>>();
    sorted.sort_by_key(|candidate| {
        (
            candidate.signal_ts_ms,
            candidate.direction,
            candidate.symbol.as_str(),
        )
    });
    let mut last_by_direction: BTreeMap<&str, i64> = BTreeMap::new();
    let mut count = 0;
    for candidate in sorted {
        let starts_new = last_by_direction
            .get(candidate.direction)
            .is_none_or(|previous| candidate.signal_ts_ms - *previous > EVENT_CLUSTER_WINDOW_MS);
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(candidate.direction, candidate.signal_ts_ms);
    }
    count
}

/// EMA 只能接受有限正数，缺失或异常值必须保持不可判定。
fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

/// 返回稳定百分比；空 cohort 按零处理以触发停止条件。
fn percentage(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{BacktestCandle, MS_15M};

    /// 构造只用于 EMA 状态测试的 K 线，价格路径和结果字段均不参与判断。
    fn candle(idx: usize, ema12: Option<f64>, ema144: Option<f64>) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: idx as i64 * MS_15M,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 10.0,
            },
            volume_ccy: Some(100.0),
            sma: Some(100.0),
            ema: Some(100.0),
            ema12,
            ema144,
            ema169: None,
            ema696: None,
            previous_volume_avg: Some(10.0),
            previous_range_avg: Some(2.0),
            rsi14: Some(50.0),
            atr14: Some(2.0),
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: None,
            macd_signal_line: None,
            macd_histogram: None,
        }
    }

    /// EMA12 在两条慢线上方即可识别早期多头，慢线交叉顺序不应造成漏判。
    #[test]
    fn bullish_fast_lead_ignores_slow_line_order() {
        let candle = candle(0, Some(8.0), Some(5.0));

        assert_eq!(
            fast_lead_state(&candle, Some(7.0), FastLeadDirection::Bullish),
            Some(true)
        );
    }

    /// 做多反向过滤完全镜像，只要求 EMA12 同时低于两条慢线。
    #[test]
    fn bearish_fast_lead_is_the_long_veto_mirror() {
        let candle = candle(0, Some(2.0), Some(5.0));

        assert_eq!(
            fast_lead_state(&candle, Some(3.0), FastLeadDirection::Bearish),
            Some(true)
        );
        assert!(matches!(
            opposite_fast_lead("long"),
            Some(FastLeadDirection::Bearish)
        ));
    }

    /// 连续年龄从 1 开始、逐根增加，并在状态中断后归零重新计数。
    #[test]
    fn age_series_counts_adjacent_true_candles_and_resets() {
        let candles = vec![
            candle(0, Some(4.0), Some(5.0)),
            candle(1, Some(8.0), Some(5.0)),
            candle(2, Some(9.0), Some(5.0)),
            candle(3, Some(4.0), Some(5.0)),
            candle(4, Some(8.0), Some(5.0)),
        ];
        let ema576 = vec![Some(6.0); candles.len()];

        assert_eq!(
            fast_lead_age_series(&candles, &ema576, FastLeadDirection::Bullish),
            vec![Some(0), Some(1), Some(2), Some(0), Some(1)]
        );
    }

    /// 缺少任一 EMA 时年龄不可判定，并切断此前连续状态。
    #[test]
    fn missing_ema_breaks_age_and_remains_not_ready() {
        let candles = vec![
            candle(0, Some(8.0), Some(5.0)),
            candle(1, None, Some(5.0)),
            candle(2, Some(8.0), Some(5.0)),
        ];
        let ema576 = vec![Some(6.0); candles.len()];

        assert_eq!(
            fast_lead_age_series(&candles, &ema576, FastLeadDirection::Bullish),
            vec![Some(1), None, Some(1)]
        );
    }

    /// 192 根仍属于冻结的 48 小时窗口，第 193 根必须放行以固定边界。
    #[test]
    fn recent_age_boundary_is_inclusive_at_192_only() {
        assert!(is_recent_age(1, RECENT_FAST_EMA_LEAD_MAX_AGE_BARS));
        assert!(is_recent_age(
            RECENT_FAST_EMA_LEAD_MAX_AGE_BARS,
            RECENT_FAST_EMA_LEAD_MAX_AGE_BARS
        ));
        assert!(!is_recent_age(0, RECENT_FAST_EMA_LEAD_MAX_AGE_BARS));
        assert!(!is_recent_age(
            RECENT_FAST_EMA_LEAD_MAX_AGE_BARS + 1,
            RECENT_FAST_EMA_LEAD_MAX_AGE_BARS
        ));
    }

    /// 96 根版本包含第 96 根并在第 97 根放行，证明没有复用 192 根常量。
    #[test]
    fn recent_age_boundary_is_inclusive_at_96_only() {
        assert!(is_recent_age(1, RECENT_FAST_EMA_LEAD_96_MAX_AGE_BARS));
        assert!(is_recent_age(
            RECENT_FAST_EMA_LEAD_96_MAX_AGE_BARS,
            RECENT_FAST_EMA_LEAD_96_MAX_AGE_BARS
        ));
        assert!(!is_recent_age(
            RECENT_FAST_EMA_LEAD_96_MAX_AGE_BARS + 1,
            RECENT_FAST_EMA_LEAD_96_MAX_AGE_BARS
        ));
    }

    /// 候选账本 schema 不允许悄悄加入结果标签。
    #[test]
    fn candidate_schema_contains_no_outcome_fields() {
        let candidate = RecentFastEmaLeadCandidate {
            symbol: "TEST-USDT-SWAP".to_owned(),
            signal_ts_ms: 0,
            signal_month_utc: "2026-01".to_owned(),
            direction: "short",
            opposite_fast_lead: "bullish",
            ema12: Some(3.0),
            ema144: Some(2.0),
            ema576: Some(1.0),
            opposite_fast_lead_age_bars: Some(1),
            rejected_by_recent_fast_ema_lead: true,
        };
        let serialized = serde_json::to_string(&candidate).expect("serialize candidate");

        for forbidden in ["pnl", "net_r", "mfe", "mae", "win", "loss", "exit"] {
            assert!(!serialized.contains(forbidden), "found {forbidden}");
        }
    }
}
