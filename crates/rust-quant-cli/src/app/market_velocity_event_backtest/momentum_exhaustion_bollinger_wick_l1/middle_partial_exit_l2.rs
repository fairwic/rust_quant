//! Momentum V2 + Bollinger 外轨触碰基础形态的首次因果中轨减半 L2 本地诊断。
//!
//! 基线与变体共享入场、初始止损、原目标和最终退出路径；唯一差异是变体可以多一个
//! 50% 中轨退出 leg。该入口只读本地行情，不注册任何运行态策略。

use super::{build_l1_report, frozen_l1_args, L1Candidate, RESEARCH_CANDIDATE_KEY};
use crate::app::market_velocity_event_backtest::{
    config_from_env_and_args, load_backtest_data, BacktestDataSet, ComputedCandle,
    MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// 首次因果中轨减半的独立 L2 规则版本，不覆盖来源 V2 或任何运行入口。
pub const MIDDLE_PARTIAL_EXIT_L2_RULE_VERSION: &str =
    "l2_first_causal_middle_touch_close_50pct_keep_v2_stop_target_v1";
/// 方向影线极值限价从信号后的 12 根完整 15m K 线内等待成交。
const LIMIT_VALID_CANDLES: usize = 12;
/// 既有 V2 初始风险固定为信号 ATR14 的 1.5 倍。
const INITIAL_STOP_ATR_MULTIPLIER: f64 = 1.5;
/// 用户确认的中轨首次减仓比例，按原始数量计算且只执行一次。
const PARTIAL_CLOSE_FRACTION: f64 = 0.5;
/// 手续费 5 bps 与滑点 3 bps 合并后的单边名义成本率。
const PER_SIDE_COST_RATE: f64 = 0.0008;
/// V2 冻结的最长持仓时间为 48 小时。
const MAX_HOLDING_MS: i64 = 48 * 60 * 60 * 1_000;
/// 同方向实际入场相邻不超过 60 分钟时归入同一市场事件。
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
/// L2 结果引用的 L1 基础账本身份，防止换数据后仍复用当前结论。
const SOURCE_LEDGER_SHA256: &str =
    "d11dfe783dce14f441007b4575c919c563891ca6dac8e5258dd5069716df2788";

/// L2 机器报告的冻结研究身份与唯一变量边界。
#[derive(Debug, Clone, Serialize)]
pub struct MiddlePartialExitL2Identity {
    /// 当前研究等级；只允许称为本地多币种诊断。
    pub level: &'static str,
    /// 独立候选策略键，不覆盖既有 Momentum V2。
    pub candidate_key: &'static str,
    /// 本批退出规则版本。
    pub rule_version: &'static str,
    /// 本批唯一变化的退出行为。
    pub only_variable: &'static str,
    /// 中轨退出占原始仓位的比例。
    pub partial_close_fraction_of_original_quantity: f64,
    /// 余仓止损保持来源 V2 初始止损，未移动到开仓价。
    pub remaining_stop_policy: &'static str,
    /// 余仓目标保持来源 V2 量比分档 ATR 目标。
    pub remaining_target_policy: &'static str,
    /// 单边手续费与滑点合计费率。
    pub per_side_cost_rate: f64,
    /// 是否读取实际成交和退出结果；L2 必须显式为 true。
    pub outcome_evaluation_performed: bool,
}

/// 从基础 setup 到真实回放交易的数量与阻塞证据。
#[derive(Debug, Clone, Serialize)]
pub struct MiddlePartialExitEntrySummary {
    /// 通过 Momentum V2 + Bollinger 外轨触碰的信号 setup 数。
    pub base_touch_setups: usize,
    /// 在替换和 12 根有效期后实际触及限价的候选数，尚未应用持仓冲突。
    pub resolved_limit_fills: usize,
    /// 同币种已有仓位时被忽略后，进入回放的交易数。
    pub executed_trades: usize,
    /// 具备完整 48 小时或更早退出证据的交易数。
    pub completed_trades: usize,
    /// forward K 线不足导致不能进入绩效指标的交易数。
    pub incomplete_trades: usize,
    /// 完整交易中实际执行中轨减半的交易数。
    pub partial_triggered_trades: usize,
    /// 中轨减半交易的多空分布。
    pub partial_triggered_by_direction: BTreeMap<&'static str, usize>,
    /// 中轨减半交易覆盖的币种数。
    pub partial_triggered_symbol_count: usize,
    /// 中轨减半交易覆盖的 UTC 月份数。
    pub partial_triggered_month_count: usize,
    /// 中轨减半交易按实际入场方向和 60 分钟单链归并后的事件数。
    pub partial_triggered_effective_market_events: usize,
    /// setup 替换、过期、数据不足和持仓冲突的逐项计数。
    pub blockers: BTreeMap<String, usize>,
}

/// 一组成本后逐笔 R 的交易级统计，不冒充统一资金曲线。
#[derive(Debug, Clone, Serialize)]
pub struct MiddlePartialExitPerformance {
    /// 纳入统计的完整交易数。
    pub trades: usize,
    /// 所有正向净 R 之和。
    pub positive_net_r: f64,
    /// 所有负向净 R 绝对值之和。
    pub negative_net_r_abs: f64,
    /// 全部交易成本后净 R 合计。
    pub net_sum_r: f64,
    /// 成本后净每笔期望。
    pub net_expectancy_r: f64,
    /// 成本后 Profit Factor；没有负 R 交易时为 `None`。
    pub net_profit_factor: Option<f64>,
    /// 净 R 严格大于零的交易比例。
    pub win_rate_pct: f64,
    /// 逐笔净 R 的均值除以样本标准差并乘以根号交易数。
    pub trade_sharpe: Option<f64>,
    /// 按实际入场时间排序的累计净 R 最大回撤。
    pub max_drawdown_r: f64,
}

/// 净增量集中度证据，用于拒绝由少数交易或币种支撑的结果。
#[derive(Debug, Clone, Serialize)]
pub struct MiddlePartialExitConcentration {
    /// 变体相对基线的全部净 R 增量。
    pub total_delta_net_r: f64,
    /// 移除净增量最高两笔后的剩余增量。
    pub delta_net_r_after_removing_top_two_trades: f64,
    /// 单一币种对全部正向净增量的最大贡献比例；没有正向增量时为空。
    pub max_symbol_positive_delta_share_pct: Option<f64>,
    /// 每个币种的净 R 增量，负值保持可见。
    pub delta_net_r_by_symbol: BTreeMap<String, f64>,
    /// 做多、做空各自的净 R 增量。
    pub delta_net_r_by_direction: BTreeMap<&'static str, f64>,
}

/// 单笔基线与中轨减半变体共享路径的审计账本。
#[derive(Debug, Clone, Serialize)]
pub struct MiddlePartialExitTradeRecord {
    /// `symbol:signal_ts` 组成的稳定研究记录标识。
    pub candidate_id: String,
    /// OKX 永续合约标识。
    pub symbol: String,
    /// 原始 setup K 开始时间，Unix 毫秒。
    pub signal_ts_ms: i64,
    /// 实际限价成交 K 开始时间，Unix 毫秒。
    pub entry_ts_ms: i64,
    /// 实际成交时间所在 UTC 月份。
    pub entry_month_utc: String,
    /// `long` 或 `short`。
    pub direction: &'static str,
    /// 来源 V2 方向长影触发标签。
    pub source_trigger: String,
    /// 信号时过滤量比，用于冻结原目标档位。
    pub filtered_volume_ratio: f64,
    /// 实际按信号影线极值成交的价格。
    pub entry_price: f64,
    /// 入场时冻结的初始止损价格。
    pub initial_stop_price: f64,
    /// 实际入场价到初始止损的价格风险，即本笔 `1R` 分母。
    pub initial_risk_price: f64,
    /// 来源 V2 的原目标价格。
    pub original_target_price: f64,
    /// 来源 V2 的原毛目标 R。
    pub original_target_r: f64,
    /// 是否有足够 forward K 线得到完整退出。
    pub complete: bool,
    /// 基线与变体共同的最终退出时间，Unix 毫秒。
    pub baseline_final_exit_ts_ms: i64,
    /// 变体余仓最终退出时间；必须与基线完全一致。
    pub variant_final_exit_ts_ms: i64,
    /// 基线整仓最终退出价格。
    pub baseline_final_exit_price: f64,
    /// 变体余仓最终退出价格；必须与基线完全一致。
    pub variant_final_exit_price: f64,
    /// 最终退出原因：止损、原目标、48 小时或 forward 不完整。
    pub final_exit_reason: &'static str,
    /// 是否执行过一次中轨减半。
    pub partial_triggered: bool,
    /// 中轨减半时间；`None` 表示直到最终退出都未满足合同。
    pub partial_exit_ts_ms: Option<i64>,
    /// 中轨减半价格；`None` 表示没有部分退出。
    pub partial_exit_price: Option<f64>,
    /// 中轨退出价格相对初始风险的毛 R；未触发时为空。
    pub partial_exit_gross_r: Option<f64>,
    /// 基线整仓按共同成本口径得到的净 R。
    pub baseline_net_r: f64,
    /// 中轨减半变体各退出 leg 合并后的净 R。
    pub variant_net_r: f64,
    /// `variant_net_r - baseline_net_r`，是本批唯一归因量。
    pub delta_net_r: f64,
}

/// 查看结果前冻结的 L2 门禁及最终停止或继续决定。
#[derive(Debug, Clone, Serialize)]
pub struct MiddlePartialExitL2Decision {
    /// `stop` 或 `promote_next_exit_hypothesis`。
    pub status: &'static str,
    /// 预注册门槛的逐项布尔结果。
    pub gates: BTreeMap<&'static str, bool>,
    /// 停止或继续的主要证据。
    pub reason: String,
}

/// 首次因果中轨减半 L2 的完整机器报告。
#[derive(Debug, Clone, Serialize)]
pub struct MiddlePartialExitL2Report {
    /// 报告 schema；字段语义变化时必须升级。
    pub schema_version: &'static str,
    /// 生成时间不参与数据或策略身份。
    pub generated_at_utc: String,
    /// 冻结研究身份。
    pub identity: MiddlePartialExitL2Identity,
    /// 来源账本与行情身份。
    pub source_candidate_ledger_sha256: &'static str,
    /// 基础行情指纹。
    pub dataset_fingerprint_sha256: String,
    /// current-live Top60 返回成员数。
    pub returned_symbol_count: usize,
    /// 具备完整基础窗口的成员数。
    pub eligible_symbol_count: usize,
    /// 因基础窗口缺口跳过的成员数。
    pub excluded_symbol_count: usize,
    /// 入场、冲突和部分退出覆盖。
    pub entry_summary: MiddlePartialExitEntrySummary,
    /// 基线成本后交易级指标。
    pub baseline: MiddlePartialExitPerformance,
    /// 中轨减半变体成本后交易级指标。
    pub variant: MiddlePartialExitPerformance,
    /// 基线按方向指标。
    pub baseline_by_direction: BTreeMap<&'static str, MiddlePartialExitPerformance>,
    /// 变体按方向指标。
    pub variant_by_direction: BTreeMap<&'static str, MiddlePartialExitPerformance>,
    /// 净增量集中度。
    pub concentration: MiddlePartialExitConcentration,
    /// 成交、止损、原目标与最终退出路径一致性。
    pub shared_path_identity_verified: bool,
    /// 预注册门禁结果。
    pub decision: MiddlePartialExitL2Decision,
    /// 全部进入回放的逐笔审计账本，包含不完整项但指标只读取完整项。
    pub trades: Vec<MiddlePartialExitTradeRecord>,
}

#[derive(Debug, Clone)]
struct ResolvedEntry {
    candidate_id: String,
    symbol: String,
    signal_ts_ms: i64,
    entry_ts_ms: i64,
    entry_idx: usize,
    direction: MarketVelocityTradeDirection,
    source_trigger: String,
    filtered_volume_ratio: f64,
    entry_price: f64,
    stop_price: f64,
    target_price: f64,
    target_r: f64,
}

#[derive(Debug, Clone, Copy)]
struct PartialExitLeg {
    ts_ms: i64,
    price: f64,
}

#[derive(Debug, Clone, Copy)]
struct SharedExitPath {
    complete: bool,
    final_exit_ts_ms: i64,
    final_exit_price: f64,
    final_exit_reason: &'static str,
    partial: Option<PartialExitLeg>,
}

/// 读取冻结本地数据并写出首次因果中轨减半 L2 机器报告。
pub async fn run_middle_partial_exit_l2(output: &Path) -> Result<MiddlePartialExitL2Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for middle partial exit L2")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_middle_partial_exit_l2_report(&data, &config.args)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建中轨减半 L2 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入中轨减半 L2 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 复用同一基础 setup 账本，先解析真实限价成交，再执行共享最终路径的双变体回放。
fn build_middle_partial_exit_l2_report(
    data: &BacktestDataSet,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<MiddlePartialExitL2Report> {
    let base_report = build_l1_report(data, args)?;
    let base_candidates = base_report
        .candidates
        .iter()
        .filter(|candidate| candidate.touches_directional_outer_band)
        .collect::<Vec<_>>();
    let (resolved_entries, mut blockers) =
        resolve_limit_entries(data, &base_candidates).context("resolve Bollinger V2 limits")?;
    let resolved_limit_fills = resolved_entries.len();
    let mut trades = simulate_resolved_entries(data, resolved_entries, &mut blockers);
    trades.sort_by(|left, right| {
        (
            left.entry_ts_ms,
            left.symbol.as_str(),
            left.candidate_id.as_str(),
        )
            .cmp(&(
                right.entry_ts_ms,
                right.symbol.as_str(),
                right.candidate_id.as_str(),
            ))
    });

    let complete = trades
        .iter()
        .filter(|trade| trade.complete)
        .collect::<Vec<_>>();
    let partial = complete
        .iter()
        .copied()
        .filter(|trade| trade.partial_triggered)
        .collect::<Vec<_>>();
    let baseline = performance(complete.iter().map(|trade| trade.baseline_net_r));
    let variant = performance(complete.iter().map(|trade| trade.variant_net_r));
    let baseline_by_direction = performance_by_direction(&complete, false);
    let variant_by_direction = performance_by_direction(&complete, true);
    let concentration = concentration(&complete);
    let shared_path_identity_verified = trades.iter().all(|trade| {
        trade.baseline_final_exit_ts_ms == trade.variant_final_exit_ts_ms
            && trade.baseline_final_exit_price == trade.variant_final_exit_price
            && trade.initial_risk_price.is_finite()
            && trade.initial_risk_price > 0.0
    });

    let partial_long = partial
        .iter()
        .filter(|trade| trade.direction == "long")
        .count();
    let partial_short = partial
        .iter()
        .filter(|trade| trade.direction == "short")
        .count();
    let partial_by_direction = BTreeMap::from([("long", partial_long), ("short", partial_short)]);
    let partial_symbol_count = partial
        .iter()
        .map(|trade| trade.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let partial_month_count = partial
        .iter()
        .map(|trade| trade.entry_month_utc.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let partial_events = effective_market_event_count(&partial);
    let incomplete_trades = trades.len().saturating_sub(complete.len());
    let entry_summary = MiddlePartialExitEntrySummary {
        base_touch_setups: base_candidates.len(),
        resolved_limit_fills,
        executed_trades: trades.len(),
        completed_trades: complete.len(),
        incomplete_trades,
        partial_triggered_trades: partial.len(),
        partial_triggered_by_direction: partial_by_direction,
        partial_triggered_symbol_count: partial_symbol_count,
        partial_triggered_month_count: partial_month_count,
        partial_triggered_effective_market_events: partial_events,
        blockers,
    };
    let decision = decide_l2(
        &entry_summary,
        &baseline,
        &variant,
        &concentration,
        shared_path_identity_verified,
    );

    Ok(MiddlePartialExitL2Report {
        schema_version: "momentum_bollinger_middle_partial_exit_l2_v1",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: MiddlePartialExitL2Identity {
            level: "L2_local_multi_symbol_diagnostic",
            candidate_key: RESEARCH_CANDIDATE_KEY,
            rule_version: MIDDLE_PARTIAL_EXIT_L2_RULE_VERSION,
            only_variable: "first causal Bollinger middle touch after fill closes 50% of original quantity; remainder keeps the original V2 stop and target",
            partial_close_fraction_of_original_quantity: PARTIAL_CLOSE_FRACTION,
            remaining_stop_policy: "unchanged_v2_initial_1p5_atr_stop",
            remaining_target_policy: "unchanged_v2_volume_tier_2p7_3p6_4p5_atr_target",
            per_side_cost_rate: PER_SIDE_COST_RATE,
            outcome_evaluation_performed: true,
        },
        source_candidate_ledger_sha256: SOURCE_LEDGER_SHA256,
        dataset_fingerprint_sha256: base_report.coverage.dataset_fingerprint_sha256,
        returned_symbol_count: base_report.coverage.returned_symbol_count,
        eligible_symbol_count: base_report.coverage.eligible_symbol_count,
        excluded_symbol_count: base_report.coverage.excluded_symbols.len(),
        entry_summary,
        baseline,
        variant,
        baseline_by_direction,
        variant_by_direction,
        concentration,
        shared_path_identity_verified,
        decision,
        trades,
    })
}

/// 只允许新的 Bollinger 触轨 setup 替换旧挂单；非触轨 V2 信号不属于新策略身份。
fn resolve_limit_entries(
    data: &BacktestDataSet,
    candidates: &[&L1Candidate],
) -> Result<(Vec<ResolvedEntry>, BTreeMap<String, usize>)> {
    let mut by_symbol: BTreeMap<&str, Vec<&L1Candidate>> = BTreeMap::new();
    for candidate in candidates {
        by_symbol
            .entry(candidate.symbol.as_str())
            .or_default()
            .push(*candidate);
    }
    let mut resolved = Vec::new();
    let mut blockers = BTreeMap::new();
    for (symbol, mut symbol_candidates) in by_symbol {
        symbol_candidates.sort_by_key(|candidate| candidate.signal_ts_ms);
        let replacement_signal_times = symbol_candidates
            .iter()
            .map(|candidate| candidate.signal_ts_ms)
            .collect::<BTreeSet<_>>();
        let candles = data
            .candles_15m_computed
            .get(symbol)
            .with_context(|| format!("missing computed candles for {symbol}"))?;
        for candidate in symbol_candidates {
            match resolve_limit_entry(candles, candidate, &replacement_signal_times) {
                Ok(entry) => resolved.push(entry),
                Err(reason) => *blockers.entry(reason.to_string()).or_default() += 1,
            }
        }
    }
    resolved.sort_by(|left, right| {
        (left.entry_ts_ms, left.signal_ts_ms, left.symbol.as_str()).cmp(&(
            right.entry_ts_ms,
            right.signal_ts_ms,
            right.symbol.as_str(),
        ))
    });
    Ok((resolved, blockers))
}

/// 依次执行“旧限价先盘中成交、新 setup 后在收盘可见”的 12 根替换合同。
fn resolve_limit_entry(
    candles: &[ComputedCandle],
    candidate: &L1Candidate,
    replacement_signal_times: &BTreeSet<i64>,
) -> Result<ResolvedEntry, &'static str> {
    let signal_idx = candles
        .binary_search_by_key(&candidate.signal_ts_ms, |item| item.candle.ts)
        .map_err(|_| "signal_candle_missing")?;
    let signal_candle = candles.get(signal_idx).ok_or("signal_candle_missing")?;
    let direction = parse_direction(candidate.direction)?;
    let entry_price = match direction {
        MarketVelocityTradeDirection::Long => signal_candle.candle.low,
        MarketVelocityTradeDirection::Short => signal_candle.candle.high,
        MarketVelocityTradeDirection::Both => return Err("candidate_direction_invalid"),
    };
    let atr14 = signal_candle
        .atr14
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or("signal_atr14_not_ready")?;
    let target_atr = target_atr_multiplier(candidate.filtered_volume_ratio)
        .ok_or("source_target_atr_not_ready")?;
    let stop_distance = atr14 * INITIAL_STOP_ATR_MULTIPLIER;
    let target_distance = atr14 * target_atr;
    let (stop_price, target_price) = match direction {
        MarketVelocityTradeDirection::Long => {
            (entry_price - stop_distance, entry_price + target_distance)
        }
        MarketVelocityTradeDirection::Short => {
            (entry_price + stop_distance, entry_price - target_distance)
        }
        MarketVelocityTradeDirection::Both => return Err("candidate_direction_invalid"),
    };
    if !entry_price.is_finite()
        || entry_price <= 0.0
        || !stop_price.is_finite()
        || stop_price <= 0.0
        || !target_price.is_finite()
        || target_price <= 0.0
    {
        return Err("source_risk_price_invalid");
    }
    for offset in 1..=LIMIT_VALID_CANDLES {
        let entry_idx = signal_idx
            .checked_add(offset)
            .ok_or("limit_entry_index_overflow")?;
        let candle = candles
            .get(entry_idx)
            .ok_or("limit_forward_data_incomplete")?;
        if limit_touched(candle, entry_price, direction) {
            return Ok(ResolvedEntry {
                candidate_id: format!("{}:{}", candidate.symbol, candidate.signal_ts_ms),
                symbol: candidate.symbol.clone(),
                signal_ts_ms: candidate.signal_ts_ms,
                entry_ts_ms: candle.candle.ts,
                entry_idx,
                direction,
                source_trigger: candidate.source_trigger.clone(),
                filtered_volume_ratio: candidate.filtered_volume_ratio,
                entry_price,
                stop_price,
                target_price,
                target_r: target_atr / INITIAL_STOP_ATR_MULTIPLIER,
            });
        }
        // 新 setup 只有在当根盘中旧限价未成交后，才能于收盘替换旧挂单。
        if replacement_signal_times.contains(&candle.candle.ts) {
            return Err("pending_replaced_by_new_bollinger_touch_setup");
        }
    }
    Err("limit_not_touched_within_12_candles")
}

/// 按币种串行持仓；部分减仓不改变最终退出时间，所以基线与变体共享同一冲突集合。
fn simulate_resolved_entries(
    data: &BacktestDataSet,
    entries: Vec<ResolvedEntry>,
    blockers: &mut BTreeMap<String, usize>,
) -> Vec<MiddlePartialExitTradeRecord> {
    let mut by_symbol: BTreeMap<String, Vec<ResolvedEntry>> = BTreeMap::new();
    for entry in entries {
        by_symbol
            .entry(entry.symbol.clone())
            .or_default()
            .push(entry);
    }
    let mut records = Vec::new();
    for (symbol, mut symbol_entries) in by_symbol {
        symbol_entries.sort_by_key(|entry| (entry.entry_ts_ms, entry.signal_ts_ms));
        let Some(candles) = data.candles_15m_computed.get(&symbol) else {
            *blockers
                .entry("computed_candles_missing_during_replay".to_string())
                .or_default() += symbol_entries.len();
            continue;
        };
        let mut locked_until = i64::MIN;
        for entry in symbol_entries {
            if entry.entry_ts_ms <= locked_until {
                *blockers
                    .entry("entry_ignored_while_same_symbol_position_open".to_string())
                    .or_default() += 1;
                continue;
            }
            let Some(path) = simulate_shared_exit_path(candles, &entry) else {
                *blockers
                    .entry("entry_candle_missing_during_replay".to_string())
                    .or_default() += 1;
                continue;
            };
            locked_until = path.final_exit_ts_ms;
            records.push(build_trade_record(&entry, path));
        }
    }
    records
}

/// 先执行共同止损，再执行合法中轨减半，最后执行原目标和 48 小时退出。
fn simulate_shared_exit_path(
    candles: &[ComputedCandle],
    entry: &ResolvedEntry,
) -> Option<SharedExitPath> {
    let horizon_end = entry.entry_ts_ms.saturating_add(MAX_HOLDING_MS);
    let mut partial = None;
    let mut last_seen = None;
    for idx in entry.entry_idx..candles.len() {
        let candle = &candles[idx];
        if candle.candle.ts > horizon_end {
            break;
        }
        last_seen = Some(candle);
        let stop_hit = loss_level_touched(candle, entry.stop_price, entry.direction);
        let target_hit = profit_level_touched(candle, entry.target_price, entry.direction);
        // 缺少 tick 路径时坚持止损优先，禁止同棒中轨减半美化本应整仓止损的结果。
        if stop_hit {
            return Some(SharedExitPath {
                complete: true,
                final_exit_ts_ms: candle.candle.ts,
                final_exit_price: entry.stop_price,
                final_exit_reason: if target_hit {
                    "both_hit_stop_first"
                } else {
                    "stop_hit"
                },
                partial,
            });
        }
        if idx > entry.entry_idx && partial.is_none() {
            let causal_middle = candles
                .get(idx - 1)
                .and_then(|previous| previous.bollinger_middle)
                .filter(|middle| legal_middle_price(*middle, entry));
            if let Some(middle) = causal_middle
                .filter(|middle| profit_level_touched(candle, *middle, entry.direction))
            {
                partial = Some(PartialExitLeg {
                    ts_ms: candle.candle.ts,
                    price: middle,
                });
            }
        }
        if target_hit {
            return Some(SharedExitPath {
                complete: true,
                final_exit_ts_ms: candle.candle.ts,
                final_exit_price: entry.target_price,
                final_exit_reason: "original_target_hit",
                partial,
            });
        }
        if candle.candle.ts >= horizon_end {
            return Some(SharedExitPath {
                complete: true,
                final_exit_ts_ms: candle.candle.ts,
                final_exit_price: candle.candle.close,
                final_exit_reason: "max_holding_timeout",
                partial,
            });
        }
    }
    let last = last_seen?;
    Some(SharedExitPath {
        complete: false,
        final_exit_ts_ms: last.candle.ts,
        final_exit_price: last.candle.close,
        final_exit_reason: "forward_data_incomplete",
        partial,
    })
}

/// 把共同最终路径拆成基线整仓与变体两条退出 leg，并按同一初始风险扣除成本。
fn build_trade_record(entry: &ResolvedEntry, path: SharedExitPath) -> MiddlePartialExitTradeRecord {
    let risk = (entry.entry_price - entry.stop_price).abs();
    let baseline_net_r = net_leg_r(
        entry.entry_price,
        path.final_exit_price,
        risk,
        entry.direction,
        1.0,
    );
    let (variant_net_r, partial_exit_ts_ms, partial_exit_price, partial_exit_gross_r) =
        match path.partial {
            Some(partial) => {
                let partial_net = net_leg_r(
                    entry.entry_price,
                    partial.price,
                    risk,
                    entry.direction,
                    PARTIAL_CLOSE_FRACTION,
                );
                let remaining_net = net_leg_r(
                    entry.entry_price,
                    path.final_exit_price,
                    risk,
                    entry.direction,
                    1.0 - PARTIAL_CLOSE_FRACTION,
                );
                (
                    partial_net + remaining_net,
                    Some(partial.ts_ms),
                    Some(partial.price),
                    Some(gross_r(
                        entry.entry_price,
                        partial.price,
                        risk,
                        entry.direction,
                    )),
                )
            }
            None => (baseline_net_r, None, None, None),
        };
    MiddlePartialExitTradeRecord {
        candidate_id: entry.candidate_id.clone(),
        symbol: entry.symbol.clone(),
        signal_ts_ms: entry.signal_ts_ms,
        entry_ts_ms: entry.entry_ts_ms,
        entry_month_utc: Utc
            .timestamp_millis_opt(entry.entry_ts_ms)
            .single()
            .map(|value| value.format("%Y-%m").to_string())
            .unwrap_or_else(|| "invalid".to_string()),
        direction: direction_label(entry.direction),
        source_trigger: entry.source_trigger.clone(),
        filtered_volume_ratio: entry.filtered_volume_ratio,
        entry_price: entry.entry_price,
        initial_stop_price: entry.stop_price,
        initial_risk_price: risk,
        original_target_price: entry.target_price,
        original_target_r: entry.target_r,
        complete: path.complete,
        baseline_final_exit_ts_ms: path.final_exit_ts_ms,
        variant_final_exit_ts_ms: path.final_exit_ts_ms,
        baseline_final_exit_price: path.final_exit_price,
        variant_final_exit_price: path.final_exit_price,
        final_exit_reason: path.final_exit_reason,
        partial_triggered: path.partial.is_some(),
        partial_exit_ts_ms,
        partial_exit_price,
        partial_exit_gross_r,
        baseline_net_r,
        variant_net_r,
        delta_net_r: variant_net_r - baseline_net_r,
    }
}

/// 计算成本后交易级指标；输入顺序就是实际入场时间顺序，用于累计 R 回撤。
fn performance(values: impl Iterator<Item = f64>) -> MiddlePartialExitPerformance {
    let values = values.collect::<Vec<_>>();
    let trades = values.len();
    let positive_net_r = values.iter().copied().filter(|value| *value > 0.0).sum();
    let negative_net_r_abs = -values
        .iter()
        .copied()
        .filter(|value| *value < 0.0)
        .sum::<f64>();
    let net_sum_r = values.iter().sum::<f64>();
    let net_expectancy_r = if trades == 0 {
        0.0
    } else {
        net_sum_r / trades as f64
    };
    let net_profit_factor =
        (negative_net_r_abs > 0.0).then_some(positive_net_r / negative_net_r_abs);
    let win_rate_pct = if trades == 0 {
        0.0
    } else {
        values.iter().filter(|value| **value > 0.0).count() as f64 / trades as f64 * 100.0
    };
    let trade_sharpe = trade_sharpe(&values);
    MiddlePartialExitPerformance {
        trades,
        positive_net_r,
        negative_net_r_abs,
        net_sum_r,
        net_expectancy_r,
        net_profit_factor,
        win_rate_pct,
        trade_sharpe,
        max_drawdown_r: max_drawdown_r(&values),
    }
}

/// 按方向生成与总体完全相同的指标，防止镜像规则被单边结果掩盖。
fn performance_by_direction(
    trades: &[&MiddlePartialExitTradeRecord],
    use_variant: bool,
) -> BTreeMap<&'static str, MiddlePartialExitPerformance> {
    ["long", "short"]
        .into_iter()
        .map(|direction| {
            let metrics = performance(
                trades
                    .iter()
                    .filter(|trade| trade.direction == direction)
                    .map(|trade| {
                        if use_variant {
                            trade.variant_net_r
                        } else {
                            trade.baseline_net_r
                        }
                    }),
            );
            (direction, metrics)
        })
        .collect()
}

/// 汇总净增量的交易与币种集中度，门禁只读取查看结果前冻结的聚合方式。
fn concentration(trades: &[&MiddlePartialExitTradeRecord]) -> MiddlePartialExitConcentration {
    let mut deltas = trades
        .iter()
        .map(|trade| trade.delta_net_r)
        .collect::<Vec<_>>();
    deltas.sort_by(|left, right| right.total_cmp(left));
    let total_delta_net_r = deltas.iter().sum::<f64>();
    let top_two = deltas.iter().take(2).sum::<f64>();
    let mut delta_net_r_by_symbol = BTreeMap::new();
    let mut delta_net_r_by_direction = BTreeMap::from([("long", 0.0), ("short", 0.0)]);
    let mut positive_by_symbol = BTreeMap::new();
    let mut total_positive_delta = 0.0;
    for trade in trades {
        *delta_net_r_by_symbol
            .entry(trade.symbol.clone())
            .or_default() += trade.delta_net_r;
        *delta_net_r_by_direction.entry(trade.direction).or_default() += trade.delta_net_r;
        if trade.delta_net_r > 0.0 {
            *positive_by_symbol.entry(trade.symbol.clone()).or_default() += trade.delta_net_r;
            total_positive_delta += trade.delta_net_r;
        }
    }
    let max_symbol_positive_delta_share_pct = (total_positive_delta > 0.0).then(|| {
        positive_by_symbol.values().copied().fold(0.0_f64, f64::max) / total_positive_delta * 100.0
    });
    MiddlePartialExitConcentration {
        total_delta_net_r,
        delta_net_r_after_removing_top_two_trades: total_delta_net_r - top_two,
        max_symbol_positive_delta_share_pct,
        delta_net_r_by_symbol,
        delta_net_r_by_direction,
    }
}

/// 应用 L2 预注册联合门禁；任一失败都不能继续叠加保本或最终目标变量。
fn decide_l2(
    summary: &MiddlePartialExitEntrySummary,
    baseline: &MiddlePartialExitPerformance,
    variant: &MiddlePartialExitPerformance,
    concentration: &MiddlePartialExitConcentration,
    shared_path_identity_verified: bool,
) -> MiddlePartialExitL2Decision {
    let long_delta = concentration
        .delta_net_r_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_delta = concentration
        .delta_net_r_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let partial_long = summary
        .partial_triggered_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let partial_short = summary
        .partial_triggered_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let mut gates = BTreeMap::new();
    gates.insert(
        "completed_trades_at_least_30",
        summary.completed_trades >= 30,
    );
    gates.insert(
        "partial_triggers_at_least_30",
        summary.partial_triggered_trades >= 30,
    );
    gates.insert(
        "partial_effective_events_at_least_15",
        summary.partial_triggered_effective_market_events >= 15,
    );
    gates.insert(
        "partial_both_directions_at_least_5",
        partial_long >= 5 && partial_short >= 5,
    );
    gates.insert(
        "delta_net_expectancy_positive",
        variant.net_expectancy_r > baseline.net_expectancy_r,
    );
    gates.insert(
        "variant_profit_factor_not_below_baseline",
        profit_factor_not_worse(baseline, variant),
    );
    gates.insert(
        "both_direction_delta_non_negative",
        long_delta >= 0.0 && short_delta >= 0.0,
    );
    gates.insert(
        "delta_positive_after_removing_top_two",
        concentration.delta_net_r_after_removing_top_two_trades > 0.0,
    );
    gates.insert(
        "max_symbol_positive_delta_share_at_most_35_pct",
        concentration
            .max_symbol_positive_delta_share_pct
            .is_some_and(|share| share <= 35.0),
    );
    gates.insert(
        "shared_path_identity_verified",
        shared_path_identity_verified,
    );
    let passed = gates.values().all(|value| *value);
    MiddlePartialExitL2Decision {
        status: if passed {
            "promote_next_exit_hypothesis"
        } else {
            "stop"
        },
        gates,
        reason: if passed {
            "中轨减半在相同成交与最终退出路径上产生分散的成本后正边际，可单独研究余仓保本。"
                .to_string()
        } else {
            "至少一项预注册 L2 门禁失败；不得继续叠加余仓保本或外轨/5R。".to_string()
        },
    }
}

fn target_atr_multiplier(filtered_volume_ratio: f64) -> Option<f64> {
    if !filtered_volume_ratio.is_finite() || filtered_volume_ratio < 2.5 {
        return None;
    }
    if filtered_volume_ratio < 4.0 {
        Some(2.7)
    } else if filtered_volume_ratio < 6.0 {
        Some(3.6)
    } else {
        Some(4.5)
    }
}

fn parse_direction(value: &str) -> Result<MarketVelocityTradeDirection, &'static str> {
    match value {
        "long" => Ok(MarketVelocityTradeDirection::Long),
        "short" => Ok(MarketVelocityTradeDirection::Short),
        _ => Err("candidate_direction_invalid"),
    }
}

fn direction_label(direction: MarketVelocityTradeDirection) -> &'static str {
    match direction {
        MarketVelocityTradeDirection::Long => "long",
        MarketVelocityTradeDirection::Short => "short",
        MarketVelocityTradeDirection::Both => "both",
    }
}

fn limit_touched(
    candle: &ComputedCandle,
    price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.candle.low <= price,
        MarketVelocityTradeDirection::Short => candle.candle.high >= price,
        MarketVelocityTradeDirection::Both => false,
    }
}

fn loss_level_touched(
    candle: &ComputedCandle,
    price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.candle.low <= price,
        MarketVelocityTradeDirection::Short => candle.candle.high >= price,
        MarketVelocityTradeDirection::Both => true,
    }
}

fn profit_level_touched(
    candle: &ComputedCandle,
    price: f64,
    direction: MarketVelocityTradeDirection,
) -> bool {
    match direction {
        MarketVelocityTradeDirection::Long => candle.candle.high >= price,
        MarketVelocityTradeDirection::Short => candle.candle.low <= price,
        MarketVelocityTradeDirection::Both => false,
    }
}

fn legal_middle_price(middle: f64, entry: &ResolvedEntry) -> bool {
    middle.is_finite()
        && middle > 0.0
        && match entry.direction {
            MarketVelocityTradeDirection::Long => {
                middle > entry.entry_price && middle < entry.target_price
            }
            MarketVelocityTradeDirection::Short => {
                middle < entry.entry_price && middle > entry.target_price
            }
            MarketVelocityTradeDirection::Both => false,
        }
}

fn gross_r(
    entry_price: f64,
    exit_price: f64,
    risk: f64,
    direction: MarketVelocityTradeDirection,
) -> f64 {
    match direction {
        MarketVelocityTradeDirection::Long => (exit_price - entry_price) / risk,
        MarketVelocityTradeDirection::Short => (entry_price - exit_price) / risk,
        MarketVelocityTradeDirection::Both => 0.0,
    }
}

fn net_leg_r(
    entry_price: f64,
    exit_price: f64,
    risk: f64,
    direction: MarketVelocityTradeDirection,
    fraction: f64,
) -> f64 {
    let gross = gross_r(entry_price, exit_price, risk, direction);
    let cost_r = (entry_price + exit_price) * PER_SIDE_COST_RATE / risk;
    fraction * (gross - cost_r)
}

fn trade_sharpe(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (values.len() - 1) as f64;
    (variance > 0.0).then_some(mean / variance.sqrt() * (values.len() as f64).sqrt())
}

fn max_drawdown_r(values: &[f64]) -> f64 {
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for value in values {
        equity += value;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    max_drawdown
}

fn profit_factor_not_worse(
    baseline: &MiddlePartialExitPerformance,
    variant: &MiddlePartialExitPerformance,
) -> bool {
    match (baseline.net_profit_factor, variant.net_profit_factor) {
        (_, None) if variant.negative_net_r_abs == 0.0 => true,
        (None, Some(_)) if baseline.negative_net_r_abs == 0.0 => false,
        (Some(base), Some(candidate)) => candidate >= base,
        _ => false,
    }
}

fn effective_market_event_count(trades: &[&MiddlePartialExitTradeRecord]) -> usize {
    let mut ordered = trades.to_vec();
    ordered.sort_by_key(|trade| (trade.entry_ts_ms, trade.direction, trade.symbol.as_str()));
    let mut last_by_direction: BTreeMap<&str, i64> = BTreeMap::new();
    let mut count = 0;
    for trade in ordered {
        let starts_new = last_by_direction
            .get(trade.direction)
            .is_none_or(|previous| trade.entry_ts_ms - *previous > EVENT_CLUSTER_WINDOW_MS);
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(trade.direction, trade.entry_ts_ms);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{BacktestCandle, MS_15M};

    fn candle(idx: usize, low: f64, high: f64, close: f64, middle: Option<f64>) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: idx as i64 * MS_15M,
                open: close,
                high,
                low,
                close,
                volume: 10.0,
            },
            volume_ccy: Some(100.0),
            sma: Some(100.0),
            ema: Some(100.0),
            ema12: Some(100.0),
            ema144: Some(100.0),
            ema169: Some(100.0),
            ema696: Some(100.0),
            previous_volume_avg: Some(10.0),
            previous_range_avg: Some(2.0),
            rsi14: Some(50.0),
            atr14: Some(2.0),
            bollinger_middle: middle,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: Some(0.0),
            macd_signal_line: Some(0.0),
            macd_histogram: Some(0.0),
        }
    }

    fn long_entry() -> ResolvedEntry {
        ResolvedEntry {
            candidate_id: "TEST:0".to_string(),
            symbol: "TEST-USDT-SWAP".to_string(),
            signal_ts_ms: 0,
            entry_ts_ms: 0,
            entry_idx: 0,
            direction: MarketVelocityTradeDirection::Long,
            source_trigger: "test".to_string(),
            filtered_volume_ratio: 3.0,
            entry_price: 100.0,
            stop_price: 90.0,
            target_price: 120.0,
            target_r: 2.0,
        }
    }

    /// 同棒止损必须压过中轨减半和原目标，避免 OHLC 回放乐观选择盘中顺序。
    #[test]
    fn stop_has_priority_over_middle_and_target() {
        let candles = vec![
            candle(0, 99.0, 101.0, 100.0, Some(105.0)),
            candle(1, 89.0, 121.0, 100.0, Some(999.0)),
        ];

        let path = simulate_shared_exit_path(&candles, &long_entry()).expect("path");

        assert_eq!(path.final_exit_reason, "both_hit_stop_first");
        assert_eq!(path.final_exit_price, 90.0);
        assert!(path.partial.is_none());
    }

    /// 当前 K 未完成后的中轨不能反向参与本根触发，必须使用上一根已完成 K 的 105。
    #[test]
    fn middle_touch_uses_previous_completed_candle_value() {
        let candles = vec![
            candle(0, 99.0, 101.0, 100.0, Some(105.0)),
            candle(1, 100.0, 106.0, 104.0, Some(119.0)),
            candle(2, 100.0, 121.0, 120.0, Some(119.0)),
        ];

        let path = simulate_shared_exit_path(&candles, &long_entry()).expect("path");

        assert_eq!(path.partial.expect("partial").price, 105.0);
        assert_eq!(path.final_exit_reason, "original_target_hit");
    }

    /// 中轨和原目标同棒时必须先记录 50% 中轨 leg，再让余仓按原目标退出。
    #[test]
    fn middle_and_target_same_candle_create_two_weighted_legs() {
        let candles = vec![
            candle(0, 99.0, 101.0, 100.0, Some(105.0)),
            candle(1, 100.0, 121.0, 120.0, Some(105.0)),
        ];
        let entry = long_entry();

        let path = simulate_shared_exit_path(&candles, &entry).expect("path");
        let record = build_trade_record(&entry, path);

        assert!(record.partial_triggered);
        assert_eq!(record.partial_exit_price, Some(105.0));
        assert!(record.variant_net_r < record.baseline_net_r);
    }

    /// 已触发中轨减半后不得随动态中轨变化重复减仓。
    #[test]
    fn middle_partial_exit_can_trigger_only_once() {
        let candles = vec![
            candle(0, 99.0, 101.0, 100.0, Some(105.0)),
            candle(1, 100.0, 106.0, 104.0, Some(107.0)),
            candle(2, 100.0, 108.0, 106.0, Some(109.0)),
            candle(3, 100.0, 121.0, 120.0, Some(109.0)),
        ];

        let path = simulate_shared_exit_path(&candles, &long_entry()).expect("path");

        let partial = path.partial.expect("single partial");
        assert_eq!(partial.ts_ms, MS_15M);
        assert_eq!(partial.price, 105.0);
    }

    /// 新 setup 只在收盘可见；若旧限价在同一根盘中先成交，不能事后取消成交。
    #[test]
    fn old_limit_fill_precedes_same_candle_replacement() {
        let mut candles = vec![
            candle(0, 99.0, 100.0, 100.0, Some(100.0)),
            candle(1, 99.0, 104.0, 100.0, Some(100.0)),
            candle(2, 99.0, 106.0, 100.0, Some(100.0)),
        ];
        candles[0].candle.high = 105.0;
        candles[0].atr14 = Some(2.0);
        let candidate = L1Candidate {
            symbol: "TEST-USDT-SWAP".to_string(),
            signal_ts_ms: 0,
            signal_month_utc: "1970-01".to_string(),
            direction: "short",
            source_trigger: "momentum_exhaustion_upper_wick_limit12_short_v2".to_string(),
            filtered_volume_ratio: 3.0,
            filtered_volume_retained_candles: 5,
            current_volume_ccy: 200.0,
            weekly_volume_ccy_p90: 100.0,
            prior_96_net_move_pct: 9.0,
            body_range_ratio: 0.2,
            directional_wick_range_ratio: 0.7,
            opposite_wick_range_ratio: 0.1,
            bollinger_middle: 100.0,
            bollinger_upper: 104.0,
            bollinger_lower: 96.0,
            touches_directional_outer_band: true,
            outer_excursion_half_band_ratio: 0.25,
            close_middle_distance_half_band_ratio: 0.0,
            source_initial_stop_atr: 1.5,
            source_limit_valid_candles: 12,
        };
        let replacements = BTreeSet::from([2 * MS_15M]);

        let resolved = resolve_limit_entry(&candles, &candidate, &replacements).expect("fill");

        assert_eq!(resolved.entry_ts_ms, 2 * MS_15M);
        assert_eq!(resolved.entry_price, 105.0);
    }
}
