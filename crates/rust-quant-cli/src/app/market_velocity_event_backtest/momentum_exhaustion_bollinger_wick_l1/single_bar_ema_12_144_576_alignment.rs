//! Bollinger 长影基础形态之上的“单根 EMA12/144/576 对向排列刚形成”L1 无标签诊断。
//!
//! 只比较信号 K 与上一根已完成 K 的 EMA12/144/576 严格排列，不读取斜率或成交后行情。

use super::{
    build_l1_report, complete_window_bounds, dataset_fingerprint, frozen_l1_args, L1Candidate,
    L1Coverage, EVALUATION_END_MS, EVALUATION_START_MS, RESEARCH_CANDIDATE_KEY,
    RESEARCH_RULE_VERSION,
};
use crate::app::market_velocity_event_backtest::{
    config_from_env_and_args, load_backtest_data, BacktestDataSet, ComputedCandle,
    MarketVelocityEventBacktestArgs, MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_STRATEGY_KEY, MS_15M,
};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// 本轮只拒绝在信号 K 收盘时首次出现的对向单根 EMA 严格排列。
pub const SINGLE_BAR_EMA_ALIGNMENT_RULE_VERSION: &str =
    "l1_bb20x2p5_wick_touch_reject_new_opposite_ema12_144_576_alignment_v1";
/// 沿用数据加载器的 699 根预热，保证 EMA576 与上一根状态都由完整递推输入产生。
const EMA_INPUT_WARMUP_CANDLES: usize = 699;
/// 用户最终指定的慢速趋势 EMA 周期，禁止用现有 EMA696 字段替代。
const EMA_SLOW_PERIOD: usize = 576;
/// 同方向候选在 60 分钟内单链归并为一个市场事件。
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;

/// 机器报告身份明确排除连续确认、斜率和来源 96 根净移动年龄。
#[derive(Debug, Clone, Serialize)]
pub struct SingleBarEmaAlignmentIdentity {
    /// 当前研究等级；L1 禁止读取结果标签。
    pub level: &'static str,
    /// 用户确认后的 Bollinger 长影候选键。
    pub candidate_key: &'static str,
    /// 本轮独立规则版本。
    pub rule_version: &'static str,
    /// 提供基础触轨 cohort 的 H1 规则版本。
    pub source_rule_version: &'static str,
    /// 真正产生多空方向的既有 15m 动量策略；EMA 不产生方向。
    pub source_strategy_key: &'static str,
    /// 既有 15m 动量策略的精确入场规则版本。
    pub source_entry_rule_version: &'static str,
    /// 既有 V2 做空候选定义，布林和 EMA 均在其后执行。
    pub source_short_definition: &'static str,
    /// 既有 V2 做多候选定义，保持与做空镜像。
    pub source_long_definition: &'static str,
    /// 从既有方向信号到新增过滤的固定执行顺序。
    pub filter_pipeline: &'static str,
    /// 本轮唯一变化字段。
    pub only_variable: &'static str,
    /// 当前单根 K 的严格 EMA 排列合同。
    pub ema_alignment_definition: &'static str,
    /// EMA576 的种子和递推口径。
    pub ema_calculation_policy: &'static str,
    /// “刚形成”的因果定义。
    pub newly_formed_definition: &'static str,
    /// 明确禁止读取的结果标签。
    pub label_boundary: &'static str,
}

/// EMA 研究需要覆盖慢线初始化输入，不能复用只覆盖 672 根成交额窗口的指纹。
#[derive(Debug, Clone, Serialize)]
pub struct EmaInputCoverage {
    /// H1 完整成员中同时具备 699 根 EMA576 输入预热的成员数。
    pub eligible_symbol_count: usize,
    /// EMA 指标计算所需的信号前 K 线数。
    pub warmup_candles: usize,
    /// 从 EMA 输入起点到评价终点的行情指纹。
    pub dataset_fingerprint_sha256: String,
}

/// 用户已澄清但尚未进入退出回放的分批平仓合同。
#[derive(Debug, Clone, Serialize)]
pub struct ClarifiedExitContract {
    /// 首次回归因果中轨时平掉原始仓位的比例。
    pub middle_band_partial_close_fraction: f64,
    /// 部分平仓后余仓止损移动到裸开仓价。
    pub remaining_stop_after_partial: &'static str,
    /// 余仓最终目标仍为对侧外轨附近或初始风险 5R 先到者。
    pub final_exit_plan: &'static str,
    /// L1 只登记语义，不执行退出回放。
    pub evaluated_in_this_batch: bool,
}

/// 一条只包含信号 K 与上一根已完成 K 的 EMA 状态的基础触轨候选。
#[derive(Debug, Clone, Serialize)]
pub struct SingleBarEmaAlignmentCandidate {
    /// OKX 永续合约标识。
    pub symbol: String,
    /// 信号 K 开始时间，Unix 毫秒。
    pub signal_ts_ms: i64,
    /// UTC 月份用于无标签分散性检查。
    pub signal_month_utc: String,
    /// `long` 或 `short`。
    pub direction: &'static str,
    /// 做多检查空头排列，做空检查多头排列。
    pub opposite_ema_alignment: &'static str,
    /// 信号 K 收盘后 EMA12；`None` 表示慢线预热或行情值不完整。
    pub ema12: Option<f64>,
    /// 信号 K 收盘后 EMA144。
    pub ema144: Option<f64>,
    /// 信号 K 收盘后独立计算的 EMA576。
    pub ema576: Option<f64>,
    /// 上一根已完成 K 的 EMA12；用于状态切换比较，不构成连续确认。
    pub previous_ema12: Option<f64>,
    /// 上一根已完成 K 的 EMA144。
    pub previous_ema144: Option<f64>,
    /// 上一根已完成 K 的 EMA576。
    pub previous_ema576: Option<f64>,
    /// 当前 K 是否严格对向排列；`Some(true)` 为已排列，`Some(false)` 为未排列，`None` 为任一 EMA 不可用。
    pub current_opposite_alignment: Option<bool>,
    /// 上一根 K 是否严格同方向排列；三态语义与当前 K 一致。
    pub previous_opposite_alignment: Option<bool>,
    /// true 只表示同方向严格排列从上一根 `false` 切换为当前 `true`；其他状态均为 false。
    pub newly_formed_opposite_alignment: bool,
}

/// 新形成对向单根 EMA 排列过滤的无标签覆盖汇总。
#[derive(Debug, Clone, Serialize)]
pub struct SingleBarEmaAlignmentSummary {
    /// 已确认 Bollinger 长影基础 setup 数。
    pub base_touch_setups: usize,
    /// 当前 K 与上一根 K 的三条 EMA 均完整、可以确定状态切换的 setup 数。
    pub ema_ready_setups: usize,
    /// 当前 K 或上一根 K 存在 EMA 缺值、必须失败关闭的 setup 数。
    pub ema_not_ready_setups: usize,
    /// 当前 K 处于对向单根严格排列的 setup 数，包含上一根已排列者。
    pub current_opposite_aligned_setups: usize,
    /// 当前 K 恰好首次出现对向严格排列的 setup 数。
    pub newly_formed_opposite_setups: usize,
    /// 新形成过滤相对基础 cohort 的影响比例。
    pub impact_pct: f64,
    /// 新形成 setup 的多空分布。
    pub newly_formed_by_direction: BTreeMap<&'static str, usize>,
    /// 新形成 setup 覆盖币种数。
    pub newly_formed_symbol_count: usize,
    /// 新形成 setup 覆盖 UTC 月份数。
    pub newly_formed_month_count: usize,
    /// 新形成 setup 按方向与 60 分钟归并后的有效事件数。
    pub newly_formed_effective_market_events: usize,
    /// 全部基础 setup 的上一根到当前 K 排列状态切换分布。
    pub alignment_transition_distribution: BTreeMap<&'static str, usize>,
}

/// 预注册门槛与最终 L1 停止边界。
#[derive(Debug, Clone, Serialize)]
pub struct SingleBarEmaAlignmentDecision {
    /// `stop` 或 `coverage_pass_target_audit_pending`。
    pub status: &'static str,
    /// 查看结果前冻结的逐项门槛。
    pub gates: BTreeMap<&'static str, bool>,
    /// 当前停止或继续原因。
    pub reason: String,
    /// 本轮没有读取成交后结果。
    pub outcome_evaluation_performed: bool,
    /// 用户尚未提供目标图表。
    pub target_chart_audit_completed: bool,
}

/// 单根 EMA 排列刚形成 L1 的完整机器产物。
#[derive(Debug, Clone, Serialize)]
pub struct SingleBarEmaAlignmentReport {
    /// 报告 schema；字段含义变化必须升级。
    pub schema_version: &'static str,
    /// 生成时间不参与数据指纹。
    pub generated_at_utc: String,
    /// 冻结研究身份。
    pub identity: SingleBarEmaAlignmentIdentity,
    /// H1 基础 cohort 的覆盖与局限。
    pub base_coverage: L1Coverage,
    /// 额外覆盖 EMA576 初始化输入的数据身份。
    pub ema_input_coverage: EmaInputCoverage,
    /// 本轮只登记、不执行的退出合同。
    pub clarified_exit_contract: ClarifiedExitContract,
    /// 无标签覆盖汇总。
    pub summary: SingleBarEmaAlignmentSummary,
    /// 预注册停止条件结果。
    pub decision: SingleBarEmaAlignmentDecision,
    /// 全部基础触轨 setup 的信号时 EMA 账本。
    pub candidates: Vec<SingleBarEmaAlignmentCandidate>,
}

/// 反向过滤要检查的严格 EMA 排列方向。
#[derive(Debug, Clone, Copy)]
enum EmaTrendDirection {
    /// `EMA12 > EMA144 > EMA576`。
    Bullish,
    /// `EMA12 < EMA144 < EMA576`。
    Bearish,
}

/// 读取冻结数据并写出单根 EMA12/144/576 排列刚形成 L1 机器报告。
pub async fn run_single_bar_ema_12_144_576_alignment_l1_scan(
    output: &Path,
) -> Result<SingleBarEmaAlignmentReport> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for EMA12/144/576 alignment L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_single_bar_ema_12_144_576_alignment_report(&data, &config.args)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 EMA12/144/576 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 EMA12/144/576 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 在 H1 基础触轨账本上附加当前与上一根的 EMA12/144/576 排列，不改变基础信号。
fn build_single_bar_ema_12_144_576_alignment_report(
    data: &BacktestDataSet,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<SingleBarEmaAlignmentReport> {
    let base_report = build_l1_report(data, args)?;
    let ema_input_coverage = ema_input_coverage(data, &base_report.coverage)?;
    let candidates = build_candidates(data, &base_report.candidates)?;
    let summary = summarize_candidates(&candidates);
    let decision = decide_l1(&summary);

    Ok(SingleBarEmaAlignmentReport {
        schema_version: "momentum_bollinger_ema12_144_576_alignment_l1_v2",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: SingleBarEmaAlignmentIdentity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: RESEARCH_CANDIDATE_KEY,
            rule_version: SINGLE_BAR_EMA_ALIGNMENT_RULE_VERSION,
            source_rule_version: RESEARCH_RULE_VERSION,
            source_strategy_key: MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_STRATEGY_KEY,
            source_entry_rule_version:
                MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION,
            source_short_definition: "existing V2 short: filtered-volume ratio >=2.5; current vol_ccy >= prior 672-candle nearest-rank P90; prior 96 completed candles net move >=+8%; signal body/range >0.10; upper-wick/range >=0.60 and upper wick > lower wick; use the signal high as a limit price valid for 12 candles",
            source_long_definition: "existing V2 long mirror: filtered-volume ratio >=2.5; current vol_ccy >= prior 672-candle nearest-rank P90; prior 96 completed candles net move <=-8%; signal body/range >0.10; lower-wick/range >=0.60 and lower wick > upper wick; use the signal low as a limit price valid for 12 candles",
            filter_pipeline: "existing V2 directional-wick long/short candidate -> corresponding Bollinger(20,2.5) outer-band touch -> newly formed opposite EMA12/144/576 alignment veto",
            only_variable: "reject a reversal setup only when the opposite strict EMA12/144/576 single-bar alignment changes from false on the previous candle to true on the signal candle",
            ema_alignment_definition: "one completed signal candle strictly orders EMA12/144/576; EMA169 and EMA696 are excluded, with no multi-bar confirmation or slope condition",
            ema_calculation_policy: "EMA12/144 use the existing SMA-seeded computed series; EMA576 uses the same first-576-close SMA seed and alpha=2/(576+1) recursion",
            newly_formed_definition: "the strict alignment is true at the signal close and false at the previous completed close",
            label_boundary: "candidate construction reads no fill, future candle, MFE, MAE, exit, PnL, R, win, or loss fields",
        },
        base_coverage: base_report.coverage,
        ema_input_coverage,
        clarified_exit_contract: ClarifiedExitContract {
            middle_band_partial_close_fraction: 0.5,
            remaining_stop_after_partial: "actual_entry_price",
            final_exit_plan: "close remaining quantity at opposite outer-band proximity or +5 initial R, whichever occurs first",
            evaluated_in_this_batch: false,
        },
        summary,
        decision,
        candidates,
    })
}

/// 为 EMA 递推输入生成独立指纹，防止只校验 672 根成交额窗口而遗漏慢线种子变化。
pub(super) fn ema_input_coverage(
    data: &BacktestDataSet,
    base: &L1Coverage,
) -> Result<EmaInputCoverage> {
    let start_ms = EVALUATION_START_MS
        .checked_sub(EMA_INPUT_WARMUP_CANDLES as i64 * MS_15M)
        .context("EMA input warmup start overflow")?;
    let excluded = base
        .excluded_symbols
        .iter()
        .map(|item| item.symbol.as_str())
        .collect::<BTreeSet<_>>();
    let mut eligible = Vec::new();
    for pair in &data.pairs {
        if excluded.contains(pair.symbol.as_str()) {
            continue;
        }
        let candles = data
            .candles_15m_computed
            .get(&pair.symbol)
            .with_context(|| format!("missing computed candles for {}", pair.symbol))?;
        let (start_idx, end_idx) = complete_window_bounds(candles, start_ms, EVALUATION_END_MS)
            .with_context(|| format!("incomplete EMA warmup window for {}", pair.symbol))?;
        eligible.push((pair.symbol.as_str(), candles.as_slice(), start_idx, end_idx));
    }
    eligible.sort_by(|left, right| left.0.cmp(right.0));
    Ok(EmaInputCoverage {
        eligible_symbol_count: eligible.len(),
        warmup_candles: EMA_INPUT_WARMUP_CANDLES,
        dataset_fingerprint_sha256: dataset_fingerprint(&eligible),
    })
}

/// 仅保留已触碰对应外轨的基础成员，并读取当前与上一根的对向 EMA 排列。
fn build_candidates(
    data: &BacktestDataSet,
    source_candidates: &[L1Candidate],
) -> Result<Vec<SingleBarEmaAlignmentCandidate>> {
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
        // EMA576 不存在于通用 ComputedCandle。按币种只递推一次，既避免把 EMA696 冒充
        // EMA576，也避免为同币种的每个候选重复扫描整段行情。
        let ema576_series = ema576_close_series(candles);
        for source in sources {
            let signal_idx = candles
                .binary_search_by_key(&source.signal_ts_ms, |candle| candle.candle.ts)
                .map_err(|_| anyhow::anyhow!("missing signal candle for {}", source.symbol))?;
            let latest = &candles[signal_idx];
            let previous_idx = signal_idx.checked_sub(1);
            let previous = previous_idx.and_then(|idx| candles.get(idx));
            let current_ema576 = ema576_series.get(signal_idx).copied().flatten();
            let previous_ema576 = previous_idx
                .and_then(|idx| ema576_series.get(idx))
                .copied()
                .flatten();
            let opposite = opposite_trend(source.direction)
                .with_context(|| format!("invalid trade direction: {}", source.direction))?;
            let (previous_opposite_alignment, current_opposite_alignment) =
                alignment_states(candles, &ema576_series, signal_idx, opposite);
            let newly_formed_opposite_alignment =
                is_newly_formed(previous_opposite_alignment, current_opposite_alignment);
            candidates.push(SingleBarEmaAlignmentCandidate {
                symbol: source.symbol.clone(),
                signal_ts_ms: source.signal_ts_ms,
                signal_month_utc: source.signal_month_utc.clone(),
                direction: source.direction,
                opposite_ema_alignment: trend_label(opposite),
                ema12: latest.ema12,
                ema144: latest.ema144,
                ema576: current_ema576,
                previous_ema12: previous.and_then(|candle| candle.ema12),
                previous_ema144: previous.and_then(|candle| candle.ema144),
                previous_ema576,
                current_opposite_alignment,
                previous_opposite_alignment,
                newly_formed_opposite_alignment,
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

/// 以首个完整 576 根收盘价的 SMA 为种子递推 EMA576，与当前 ComputedCandle EMA 语义一致。
pub(super) fn ema576_close_series(candles: &[ComputedCandle]) -> Vec<Option<f64>> {
    let mut values = vec![None; candles.len()];
    if candles.len() < EMA_SLOW_PERIOD {
        return values;
    }
    let mut seed_sum = 0.0;
    for candle in &candles[..EMA_SLOW_PERIOD] {
        let close = candle.candle.close;
        if !positive(close) {
            return values;
        }
        seed_sum += close;
    }
    let seed_idx = EMA_SLOW_PERIOD - 1;
    let mut previous = seed_sum / EMA_SLOW_PERIOD as f64;
    values[seed_idx] = Some(previous);
    let multiplier = 2.0 / (EMA_SLOW_PERIOD as f64 + 1.0);
    for idx in EMA_SLOW_PERIOD..candles.len() {
        let close = candles[idx].candle.close;
        if !positive(close) {
            break;
        }
        previous = (close - previous) * multiplier + previous;
        values[idx] = Some(previous);
    }
    values
}

/// 返回上一根与当前 K 的单根排列状态；缺少任何必要 EMA 时保留 `None` 供门禁失败关闭。
fn alignment_states(
    candles: &[ComputedCandle],
    ema576_series: &[Option<f64>],
    signal_idx: usize,
    direction: EmaTrendDirection,
) -> (Option<bool>, Option<bool>) {
    let previous = signal_idx
        .checked_sub(1)
        .and_then(|idx| alignment_at(candles, ema576_series, idx, direction));
    let current = alignment_at(candles, ema576_series, signal_idx, direction);
    (previous, current)
}

/// 在一个已完成 K 线时点组合通用 EMA12/144 与独立 EMA576，不读取其他 EMA 字段。
fn alignment_at(
    candles: &[ComputedCandle],
    ema576_series: &[Option<f64>],
    idx: usize,
    direction: EmaTrendDirection,
) -> Option<bool> {
    let candle = candles.get(idx)?;
    let ema576 = ema576_series.get(idx).copied().flatten();
    ordered_for_direction(candle, ema576, direction)
}

/// “刚形成”只认 `false -> true`，避免把上一根已排列的成熟趋势重复拒绝。
fn is_newly_formed(previous: Option<bool>, current: Option<bool>) -> bool {
    matches!((previous, current), (Some(false), Some(true)))
}

/// 只在 EMA12/144/576 均已预热且为正数时判断严格排列。
fn ordered_for_direction(
    candle: &ComputedCandle,
    ema576: Option<f64>,
    direction: EmaTrendDirection,
) -> Option<bool> {
    let (ema12, ema144, ema576) = (
        candle.ema12.filter(|value| positive(*value))?,
        candle.ema144.filter(|value| positive(*value))?,
        ema576.filter(|value| positive(*value))?,
    );
    Some(match direction {
        EmaTrendDirection::Bullish => ema12 > ema144 && ema144 > ema576,
        EmaTrendDirection::Bearish => ema12 < ema144 && ema144 < ema576,
    })
}

/// 做多是对空头趋势的反向信号，做空是对多头趋势的反向信号。
fn opposite_trend(direction: &str) -> Option<EmaTrendDirection> {
    match direction {
        "long" => Some(EmaTrendDirection::Bearish),
        "short" => Some(EmaTrendDirection::Bullish),
        _ => None,
    }
}

/// 为机器账本提供稳定趋势标签。
fn trend_label(direction: EmaTrendDirection) -> &'static str {
    match direction {
        EmaTrendDirection::Bullish => "bullish",
        EmaTrendDirection::Bearish => "bearish",
    }
}

/// 汇总精确 `false -> true` 过滤覆盖，不把已持续排列混入被拒绝 cohort。
fn summarize_candidates(
    candidates: &[SingleBarEmaAlignmentCandidate],
) -> SingleBarEmaAlignmentSummary {
    let mut newly_formed_by_direction = BTreeMap::new();
    let mut symbols = BTreeSet::new();
    let mut months = BTreeSet::new();
    let mut alignment_transition_distribution = BTreeMap::from([
        ("not_ready", 0),
        ("not_aligned_to_not_aligned", 0),
        ("not_aligned_to_aligned_newly_formed", 0),
        ("aligned_to_aligned", 0),
        ("aligned_to_not_aligned", 0),
    ]);
    for candidate in candidates {
        let bucket = match (
            candidate.previous_opposite_alignment,
            candidate.current_opposite_alignment,
        ) {
            (Some(false), Some(false)) => "not_aligned_to_not_aligned",
            (Some(false), Some(true)) => "not_aligned_to_aligned_newly_formed",
            (Some(true), Some(true)) => "aligned_to_aligned",
            (Some(true), Some(false)) => "aligned_to_not_aligned",
            _ => "not_ready",
        };
        *alignment_transition_distribution.entry(bucket).or_default() += 1;
        if !candidate.newly_formed_opposite_alignment {
            continue;
        }
        *newly_formed_by_direction
            .entry(candidate.direction)
            .or_default() += 1;
        symbols.insert(candidate.symbol.as_str());
        months.insert(candidate.signal_month_utc.as_str());
    }
    let ema_not_ready_setups = candidates
        .iter()
        .filter(|candidate| {
            candidate.previous_opposite_alignment.is_none()
                || candidate.current_opposite_alignment.is_none()
        })
        .count();
    let current_opposite_aligned_setups = candidates
        .iter()
        .filter(|candidate| {
            candidate.previous_opposite_alignment.is_some()
                && candidate.current_opposite_alignment == Some(true)
        })
        .count();
    let newly_formed_opposite_setups = candidates
        .iter()
        .filter(|candidate| candidate.newly_formed_opposite_alignment)
        .count();
    SingleBarEmaAlignmentSummary {
        base_touch_setups: candidates.len(),
        ema_ready_setups: candidates.len().saturating_sub(ema_not_ready_setups),
        ema_not_ready_setups,
        current_opposite_aligned_setups,
        newly_formed_opposite_setups,
        impact_pct: percentage(newly_formed_opposite_setups, candidates.len()),
        newly_formed_by_direction,
        newly_formed_symbol_count: symbols.len(),
        newly_formed_month_count: months.len(),
        newly_formed_effective_market_events: effective_market_event_count(candidates),
        alignment_transition_distribution,
    }
}

/// 应用预注册门槛；即使数值通过也必须等待目标图表审计。
fn decide_l1(summary: &SingleBarEmaAlignmentSummary) -> SingleBarEmaAlignmentDecision {
    let long_count = summary
        .newly_formed_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_count = summary
        .newly_formed_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let mut gates = BTreeMap::new();
    gates.insert(
        "ema_ready_for_all_base_setups",
        summary.ema_not_ready_setups == 0,
    );
    gates.insert(
        "newly_formed_setups_at_least_10",
        summary.newly_formed_opposite_setups >= 10,
    );
    gates.insert(
        "impact_between_2_and_25_pct",
        (2.0..=25.0).contains(&summary.impact_pct),
    );
    gates.insert(
        "effective_events_at_least_5",
        summary.newly_formed_effective_market_events >= 5,
    );
    gates.insert("symbols_at_least_4", summary.newly_formed_symbol_count >= 4);
    gates.insert("months_at_least_3", summary.newly_formed_month_count >= 3);
    gates.insert(
        "both_directions_at_least_2",
        long_count >= 2 && short_count >= 2,
    );
    let passed = gates.values().all(|value| *value);
    SingleBarEmaAlignmentDecision {
        status: if passed {
            "coverage_pass_target_audit_pending"
        } else {
            "stop"
        },
        gates,
        reason: if passed {
            "新形成对向单根 EMA 排列过滤通过无标签覆盖门槛；目标图表尚未审计，禁止读取结果标签。"
                .to_owned()
        } else {
            "至少一项预注册无标签门槛未通过；按 L1 停止，不读取结果标签。".to_owned()
        },
        outcome_evaluation_performed: false,
        target_chart_audit_completed: false,
    }
}

/// 按方向把 60 分钟内的新形成对向趋势信号单链归并为有效事件。
fn effective_market_event_count(candidates: &[SingleBarEmaAlignmentCandidate]) -> usize {
    let mut sorted = candidates
        .iter()
        .filter(|candidate| candidate.newly_formed_opposite_alignment)
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

/// EMA 只能接受有限正数，缺值或异常值必须让排列状态失败关闭。
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
    use crate::app::market_velocity_event_backtest::BacktestCandle;

    /// 构造连续 K 线；测试只修改 EMA，不让价格或结果字段参与排列判断。
    fn candle(idx: usize) -> ComputedCandle {
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
            ema12: Some(2.0),
            ema144: Some(2.0),
            ema169: Some(2.0),
            ema696: Some(5.0),
            previous_volume_avg: Some(10.0),
            previous_range_avg: Some(2.0),
            rsi14: Some(50.0),
            atr14: Some(2.0),
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: Some(0.0),
            macd_signal_line: Some(0.0),
            macd_histogram: Some(0.0),
        }
    }

    /// 在指定 K 线写入严格 EMA12/144/576 排列，不触碰被排除的 EMA169/696。
    fn set_ordered(
        candles: &mut [ComputedCandle],
        ema576_series: &mut [Option<f64>],
        idx: usize,
        direction: EmaTrendDirection,
        ema576: f64,
    ) {
        ema576_series[idx] = Some(ema576);
        match direction {
            EmaTrendDirection::Bullish => {
                candles[idx].ema12 = Some(8.0);
                candles[idx].ema144 = Some(7.0);
            }
            EmaTrendDirection::Bearish => {
                candles[idx].ema12 = Some(1.0);
                candles[idx].ema144 = Some(2.0);
            }
        }
    }

    /// 前两根无需排列；信号 K 单根首次空头排列即可拒绝做多反向信号。
    #[test]
    fn bearish_alignment_needs_only_the_signal_candle() {
        let mut candles = (0..6).map(candle).collect::<Vec<_>>();
        let mut ema576_series = vec![Some(5.0); candles.len()];
        set_ordered(
            &mut candles,
            &mut ema576_series,
            5,
            EmaTrendDirection::Bearish,
            5.0,
        );

        let (previous, current) =
            alignment_states(&candles, &ema576_series, 5, EmaTrendDirection::Bearish);

        assert_eq!((previous, current), (Some(false), Some(true)));
        assert!(is_newly_formed(previous, current));
    }

    /// 多头排列是做空反向信号的镜像，并且同样只看信号 K 单根排列。
    #[test]
    fn bullish_alignment_is_the_opposite_state_for_short() {
        let mut candles = (0..6).map(candle).collect::<Vec<_>>();
        let mut ema576_series = vec![Some(5.0); candles.len()];
        set_ordered(
            &mut candles,
            &mut ema576_series,
            5,
            EmaTrendDirection::Bullish,
            5.0,
        );
        let direction = opposite_trend("short").expect("short opposite trend");
        let (previous, current) = alignment_states(&candles, &ema576_series, 5, direction);

        assert_eq!(trend_label(direction), "bullish");
        assert!(is_newly_formed(previous, current));
    }

    /// 上一根已经严格排列时属于成熟趋势，当前 K 不应再次被“刚形成”过滤拒绝。
    #[test]
    fn existing_alignment_is_not_newly_formed() {
        let mut candles = (0..6).map(candle).collect::<Vec<_>>();
        let mut ema576_series = vec![Some(5.0); candles.len()];
        set_ordered(
            &mut candles,
            &mut ema576_series,
            4,
            EmaTrendDirection::Bearish,
            4.0,
        );
        set_ordered(
            &mut candles,
            &mut ema576_series,
            5,
            EmaTrendDirection::Bearish,
            5.0,
        );
        let (previous, current) =
            alignment_states(&candles, &ema576_series, 5, EmaTrendDirection::Bearish);

        assert_eq!((previous, current), (Some(true), Some(true)));
        assert!(!is_newly_formed(previous, current));
    }

    /// EMA576 即使与上一根反向移动，只要当前三线严格排列也必须成立，证明没有斜率门禁。
    #[test]
    fn ema576_slope_is_not_part_of_the_alignment() {
        let mut candles = (0..6).map(candle).collect::<Vec<_>>();
        let mut ema576_series = vec![Some(5.0); candles.len()];
        ema576_series[4] = Some(10.0);
        set_ordered(
            &mut candles,
            &mut ema576_series,
            5,
            EmaTrendDirection::Bullish,
            5.0,
        );
        let (previous, current) =
            alignment_states(&candles, &ema576_series, 5, EmaTrendDirection::Bullish);

        assert_eq!((previous, current), (Some(false), Some(true)));
        assert!(is_newly_formed(previous, current));
    }

    /// 缺少当前或上一根任一 EMA 时状态不可判定，不能悄悄按非趋势放行。
    #[test]
    fn missing_ema_fails_closed_as_not_ready() {
        let mut candles = (0..6).map(candle).collect::<Vec<_>>();
        let mut ema576_series = vec![Some(5.0); candles.len()];
        ema576_series[4] = None;
        set_ordered(
            &mut candles,
            &mut ema576_series,
            5,
            EmaTrendDirection::Bearish,
            5.0,
        );
        let (previous, current) =
            alignment_states(&candles, &ema576_series, 5, EmaTrendDirection::Bearish);

        assert_eq!((previous, current), (None, Some(true)));
        assert!(!is_newly_formed(previous, current));
    }

    /// 三线相等不满足严格排列，防止把过渡态误判为趋势形成。
    #[test]
    fn equal_ema_values_are_not_strictly_aligned() {
        let candle = candle(0);

        assert_eq!(
            ordered_for_direction(&candle, Some(2.0), EmaTrendDirection::Bullish),
            Some(false)
        );
        assert_eq!(
            ordered_for_direction(&candle, Some(2.0), EmaTrendDirection::Bearish),
            Some(false)
        );
    }

    /// EMA169 与 EMA696 即使缺失也不能影响用户指定的三均线排列。
    #[test]
    fn ema169_and_ema696_are_not_part_of_the_alignment() {
        let mut candle = candle(0);
        candle.ema12 = Some(8.0);
        candle.ema144 = Some(7.0);
        candle.ema169 = None;
        candle.ema696 = None;

        assert_eq!(
            ordered_for_direction(&candle, Some(5.0), EmaTrendDirection::Bullish),
            Some(true)
        );
    }

    /// EMA576 首值使用 576 根 SMA，后一根按固定 alpha 递推，避免与 EMA696 或 Pine 首值种子混淆。
    #[test]
    fn ema576_uses_full_window_sma_seed_and_recursive_update() {
        let mut candles = (0..=EMA_SLOW_PERIOD).map(candle).collect::<Vec<_>>();
        candles[EMA_SLOW_PERIOD].candle.close = 110.0;

        let values = ema576_close_series(&candles);
        let expected_next = 100.0 + (110.0 - 100.0) * 2.0 / (EMA_SLOW_PERIOD as f64 + 1.0);

        assert_eq!(values[EMA_SLOW_PERIOD - 2], None);
        assert_eq!(values[EMA_SLOW_PERIOD - 1], Some(100.0));
        assert_eq!(values[EMA_SLOW_PERIOD], Some(expected_next));
    }
}
