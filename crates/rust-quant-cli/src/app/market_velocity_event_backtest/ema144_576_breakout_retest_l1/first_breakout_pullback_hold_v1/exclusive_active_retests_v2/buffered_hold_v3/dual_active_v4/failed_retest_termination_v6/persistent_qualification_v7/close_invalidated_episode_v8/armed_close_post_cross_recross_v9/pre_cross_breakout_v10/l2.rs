//! V10 回踩确认后下一根开盘成交的 L2 多币种成本诊断。
//!
//! 本模块只消费冻结 L1 账本，并保持 Research-only；它不注册 Paper、ReadOnly、Live
//! 或生产策略，也不接受命令行风险参数。

mod entry;
mod report;
pub use report::*;
pub(super) mod replay;
mod risk_policy;

#[cfg(test)]
mod tests;

use super::*;
use crate::app::market_velocity_event_backtest::{ComputedCandle, MS_15M};
use anyhow::bail;
pub(super) use entry::inspect_entry_risk;
use entry::{resolve_entry, EntryPlan};
#[cfg(test)]
use risk_policy::risk_prices;
use risk_policy::{initial_risk_amount, risk_prices_for_candidate, validate_entry_risk_gate};
pub(super) use risk_policy::{
    stop_cost_r_for_prices, target_price_for_policy, EntryRiskGatePolicy, InitialRiskPolicy,
    TargetRiskPolicy,
};
use serde_json::Value;

/// V10 L2 的成交和风险身份；任何参数变化都必须创建新研究版本。
pub const V10_L2_RULE_VERSION: &str = "l2_v10_next_open_sl04_r052_hold24h_cost8bps_symbol_lock_v1";

const EXPECTED_L1_REPORT_SHA256: &str =
    "1d780c35eef54b71490073323b722ce09ad1b38416e1062130125b1401cc05be";
const EXPECTED_DATASET_FINGERPRINT_SHA256: &str =
    "67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997";
const EXPECTED_L1_CANDIDATES: usize = 26_656;
const EXPECTED_RETURNED_SYMBOLS: usize = 60;
const EXPECTED_ELIGIBLE_SYMBOLS: usize = 44;
const EXPECTED_EXCLUDED_SYMBOLS: usize = 16;
const STOP_LOSS_PCT: f64 = 0.04;
const TARGET_R: f64 = 0.52;
const PER_SIDE_COST_RATE: f64 = 0.0008;
const MAX_HOLDING_MS: i64 = 24 * 60 * 60 * 1_000;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;

/// 已通过 SHA、研究身份、覆盖和无 outcome 边界校验的冻结 L1 文件。
struct ValidatedL1 {
    json: Value,
    report_sha256: String,
}

/// L2 风险、保护价和排序使用的多空镜像方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum L2Direction {
    Long,
    Short,
}

/// L2 是否允许同一长期资格 setup 在首笔真实成交后再次开仓。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SetupEntryPolicy {
    /// V10/V11 冻结行为：持仓退出后，同一 setup 的后续信号仍可再次成交。
    AllowRepeated,
    /// 首笔真实成交消费该方向 setup；被持仓锁阻塞的信号不算成交。
    FirstFilledPerSetup,
}

impl L2Direction {
    fn label(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }

    fn sort_rank(self) -> u8 {
        match self {
            Self::Long => 0,
            Self::Short => 1,
        }
    }
}

/// 一笔入场计划沿冻结 OHLC 路径得到的退出证据。
#[derive(Debug, Clone, Copy)]
struct ExitPath {
    complete: bool,
    exit_ts_ms: i64,
    exit_price: f64,
    exit_reason: &'static str,
}

/// 校验冻结 L1，重载同一行情并执行唯一的 V10 L2 成本回放。
pub async fn run_v10_l2_replay(l1_source: &Path, output: &Path) -> Result<V10L2Report> {
    let source = load_and_validate_l1(l1_source)?;
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for V10 L2 replay")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_l2_report(&data, source)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V10 L2 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V10 L2 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 完整文件 SHA 与无 outcome 门禁共同阻止 L2 漂移到另一份候选账本。
fn load_and_validate_l1(source: &Path) -> Result<ValidatedL1> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取 V10 L1 报告失败：{}", source.display()))?;
    let report_sha256 = sha256_hex(&bytes);
    if report_sha256 != EXPECTED_L1_REPORT_SHA256 {
        bail!("V10 L1 report SHA mismatch");
    }
    let json: Value = serde_json::from_slice(&bytes).context("解析 V10 L1 报告失败")?;
    if json_string(&json, "/schema_version")?
        != "market_momentum_ema576_pre_cross_breakout_episode_l1_v10"
        || json_string(&json, "/identity/candidate_key")? != V10_CANDIDATE_KEY
        || json_string(&json, "/identity/rule_version")? != V10_RULE_VERSION
    {
        bail!("V10 L1 strategy identity mismatch");
    }
    if json_string(&json, "/coverage/dataset_fingerprint_sha256")?
        != EXPECTED_DATASET_FINGERPRINT_SHA256
        || json_usize(&json, "/coverage/returned_symbol_count")? != EXPECTED_RETURNED_SYMBOLS
        || json_usize(&json, "/coverage/eligible_symbol_count")? != EXPECTED_ELIGIBLE_SYMBOLS
        || json_array(&json, "/coverage/excluded_symbols")?.len() != EXPECTED_EXCLUDED_SYMBOLS
    {
        bail!("V10 L1 dataset or universe identity mismatch");
    }
    let candidates = json_array(&json, "/candidates")?;
    if json_usize(&json, "/summary/candidate_count")? != EXPECTED_L1_CANDIDATES
        || candidates.len() != EXPECTED_L1_CANDIDATES
    {
        bail!("V10 L1 candidate count mismatch");
    }
    if json_string(&json, "/decision/status")? != "coverage_pass_ready_for_l2_prereg"
        || json_bool(&json, "/decision/outcome_evaluation_performed")?
    {
        bail!("V10 L1 is not eligible for outcome replay");
    }
    if candidates.iter().any(|candidate| {
        candidate.get("execution_status").and_then(Value::as_str)
            != Some("signal_confirmed_next_bar_open_not_evaluated_l1")
    }) {
        bail!("V10 L1 candidate execution boundary mismatch");
    }
    Ok(ValidatedL1 {
        json,
        report_sha256,
    })
}

/// 从冻结 L1 JSON 读取必需字符串字段并保留字段路径错误。
fn json_string<'a>(json: &'a Value, pointer: &str) -> Result<&'a str> {
    json.pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("V10 L1 missing string field {pointer}"))
}

/// 从冻结 L1 JSON 读取可安全转换为本机 usize 的无符号字段。
fn json_usize(json: &Value, pointer: &str) -> Result<usize> {
    let value = json
        .pointer(pointer)
        .and_then(Value::as_u64)
        .with_context(|| format!("V10 L1 missing unsigned field {pointer}"))?;
    usize::try_from(value).with_context(|| format!("V10 L1 field {pointer} exceeds usize"))
}

/// 从冻结 L1 JSON 读取无 outcome 等布尔门禁字段。
fn json_bool(json: &Value, pointer: &str) -> Result<bool> {
    json.pointer(pointer)
        .and_then(Value::as_bool)
        .with_context(|| format!("V10 L1 missing boolean field {pointer}"))
}

/// 从冻结 L1 JSON 读取候选或排除成员数组。
fn json_array<'a>(json: &'a Value, pointer: &str) -> Result<&'a Vec<Value>> {
    json.pointer(pointer)
        .and_then(Value::as_array)
        .with_context(|| format!("V10 L1 missing array field {pointer}"))
}

/// 重建完整 L1 账本后才执行下一根开盘、同币种锁与固定退出合同。
fn build_l2_report(data: &BacktestDataSet, source: ValidatedL1) -> Result<V10L2Report> {
    let rebuilt = build_v10_report(data)?;
    if rebuilt.coverage.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256
        || rebuilt.summary.candidate_count != EXPECTED_L1_CANDIDATES
        || rebuilt.coverage.returned_symbol_count != EXPECTED_RETURNED_SYMBOLS
        || rebuilt.coverage.eligible_symbol_count != EXPECTED_ELIGIBLE_SYMBOLS
        || rebuilt.coverage.excluded_symbols.len() != EXPECTED_EXCLUDED_SYMBOLS
    {
        bail!("reloaded V10 L1 identity mismatch");
    }
    let rebuilt_candidates = serde_json::to_value(&rebuilt.candidates)?;
    if source.json.pointer("/candidates") != Some(&rebuilt_candidates) {
        bail!("reloaded V10 candidate ledger differs from frozen L1");
    }
    Ok(replay::replay_verified_candidate_ledger(
        data,
        replay::ReplaySource::new(
            "market_momentum_ema576_pre_cross_breakout_episode_l2_v10",
            V10L2Identity {
            level: "L2_local_multi_symbol_diagnostic",
            candidate_key: V10_CANDIDATE_KEY,
            source_l1_rule_version: V10_RULE_VERSION,
            rule_version: V10_L2_RULE_VERSION,
            only_variable: "connect the frozen V10 held-retest close signal to next-contiguous-15m-open execution under the unchanged 15m momentum risk and exit contract",
            entry_policy: "signal is known only after its close; enter exactly at the next contiguous 15m candle open, otherwise block without compensation",
            initial_stop_policy: "4 percent of actual entry price, mirrored by direction",
            target_policy: "fixed 0.52R from actual entry with no break-even, trailing, partial, runner, or reversal",
            intrabar_conflict_policy: "entry candle included; stop first when stop and target are both touched in one candle",
            symbol_position_policy: "one open trade per symbol; signals through the exit candle are ignored; equal-time long sorts before short",
            per_side_cost_rate: PER_SIDE_COST_RATE,
            max_holding_ms: MAX_HOLDING_MS,
            funding_modeled: false,
            outcome_evaluation_performed: true,
            runtime_boundary: "research-only V10 L2; not registered in paper, readonly shadow, live worker, compose, or production presets",
            },
            source.report_sha256,
            rebuilt.coverage.dataset_fingerprint_sha256,
            rebuilt.coverage.returned_symbol_count,
            rebuilt.coverage.eligible_symbol_count,
            rebuilt.coverage.excluded_symbols.len(),
            SetupEntryPolicy::AllowRepeated,
            InitialRiskPolicy::FixedFourPercent,
            TargetRiskPolicy::FixedGrossR,
            EntryRiskGatePolicy::AllowAnyPositiveRisk,
            rebuilt.candidates,
        ),
    ))
}

/// 逐币执行一个持仓锁，确保持仓期及退出 K 内的信号不会重叠成交。
fn simulate_with_symbol_lock(
    data: &BacktestDataSet,
    entries: Vec<EntryPlan>,
    blockers: &mut BTreeMap<String, usize>,
    setup_entry_policy: SetupEntryPolicy,
) -> Vec<V10L2TradeRecord> {
    let mut by_symbol: BTreeMap<String, Vec<EntryPlan>> = BTreeMap::new();
    for entry in entries {
        by_symbol
            .entry(entry.symbol.clone())
            .or_default()
            .push(entry);
    }
    let mut records = Vec::new();
    for (symbol, mut symbol_entries) in by_symbol {
        symbol_entries.sort_by(|left, right| {
            (
                left.signal_ts_ms,
                left.direction.sort_rank(),
                left.candidate_id.as_str(),
            )
                .cmp(&(
                    right.signal_ts_ms,
                    right.direction.sort_rank(),
                    right.candidate_id.as_str(),
                ))
        });
        let Some(candles) = data.candles_15m_computed.get(&symbol) else {
            *blockers
                .entry("symbol_candles_missing_during_replay".to_owned())
                .or_default() += symbol_entries.len();
            continue;
        };
        let mut locked_until = i64::MIN;
        let mut filled_setups = BTreeSet::new();
        for entry in symbol_entries {
            let setup_key = (entry.direction, entry.setup_ts_ms);
            if setup_entry_policy == SetupEntryPolicy::FirstFilledPerSetup
                && filled_setups.contains(&setup_key)
            {
                *blockers
                    .entry("setup_already_filled_once".to_owned())
                    .or_default() += 1;
                continue;
            }
            if entry.signal_ts_ms <= locked_until {
                *blockers
                    .entry("signal_ignored_while_symbol_position_open".to_owned())
                    .or_default() += 1;
                continue;
            }
            let Some(path) = simulate_exit(candles, &entry) else {
                *blockers
                    .entry("entry_candle_missing_during_replay".to_owned())
                    .or_default() += 1;
                continue;
            };
            locked_until = path.exit_ts_ms;
            if setup_entry_policy == SetupEntryPolicy::FirstFilledPerSetup {
                // 只有真实进入回放的成交才消费 setup；持仓冲突或缺 K 不补造成交。
                filled_setups.insert(setup_key);
            }
            records.push(build_trade_record(entry, path));
        }
    }
    records
}

/// 从实际入场 K 开始检查保护；forward 出现缺口时停止使用后续数据。
fn simulate_exit(candles: &[ComputedCandle], entry: &EntryPlan) -> Option<ExitPath> {
    let horizon_end = entry.entry_ts_ms.saturating_add(MAX_HOLDING_MS);
    let mut expected_ts = entry.entry_ts_ms;
    let mut last_seen: Option<&ComputedCandle> = None;
    for candle in candles.get(entry.entry_idx..)? {
        if candle.candle.ts > horizon_end {
            break;
        }
        if candle.candle.ts != expected_ts {
            let last = last_seen?;
            return Some(ExitPath {
                complete: false,
                exit_ts_ms: last.candle.ts,
                exit_price: last.candle.close,
                exit_reason: "forward_data_gap",
            });
        }
        last_seen = Some(candle);
        let (stop_hit, target_hit) = exit_hits(
            candle.candle.high,
            candle.candle.low,
            entry.stop_price,
            entry.target_price,
            entry.direction,
        );
        if stop_hit {
            return Some(ExitPath {
                complete: true,
                exit_ts_ms: candle.candle.ts,
                exit_price: entry.stop_price,
                exit_reason: if target_hit {
                    "both_hit_stop_first"
                } else {
                    "stop_hit"
                },
            });
        }
        if target_hit {
            return Some(ExitPath {
                complete: true,
                exit_ts_ms: candle.candle.ts,
                exit_price: entry.target_price,
                exit_reason: "target_hit",
            });
        }
        if candle.candle.ts == horizon_end {
            return Some(ExitPath {
                complete: true,
                exit_ts_ms: candle.candle.ts,
                exit_price: candle.candle.close,
                exit_reason: "max_holding_timeout",
            });
        }
        expected_ts = expected_ts.saturating_add(MS_15M);
    }
    let last = last_seen?;
    Some(ExitPath {
        complete: false,
        exit_ts_ms: last.candle.ts,
        exit_price: last.candle.close,
        exit_reason: "forward_data_incomplete",
    })
}

/// 将单根 OHLC 对固定保护价的触发拆出，供回放和测试共享。
fn exit_hits(high: f64, low: f64, stop: f64, target: f64, direction: L2Direction) -> (bool, bool) {
    match direction {
        L2Direction::Long => (low <= stop, high >= target),
        L2Direction::Short => (high >= stop, low <= target),
    }
}

/// 将冻结成交与退出折算成未扣成本、成本和净 R 的审计记录。
fn build_trade_record(entry: EntryPlan, path: ExitPath) -> V10L2TradeRecord {
    let risk = entry.initial_risk;
    let gross_r = directional_r(entry.entry_price, path.exit_price, risk, entry.direction);
    let cost_r = (entry.entry_price + path.exit_price) * PER_SIDE_COST_RATE / risk;
    V10L2TradeRecord {
        candidate_id: entry.candidate_id,
        asset_group: asset_group(&entry.symbol),
        symbol: entry.symbol,
        direction: entry.direction.label(),
        setup_ts_ms: entry.setup_ts_ms,
        breakout_ts_ms: entry.breakout_ts_ms,
        rearmed_ts_ms: entry.rearmed_ts_ms,
        signal_ts_ms: entry.signal_ts_ms,
        cross_phase: entry.cross_phase,
        signal_ema144: entry.signal_ema144,
        signal_ema576: entry.signal_ema576,
        signal_atr14: entry.signal_atr14,
        retest_extreme_to_ema144_atr: entry.retest_extreme_to_ema144_atr,
        close_to_ema144_directional_atr: entry.close_to_ema144_directional_atr,
        entry_ts_ms: entry.entry_ts_ms,
        entry_price: entry.entry_price,
        initial_stop_price: entry.stop_price,
        target_price: entry.target_price,
        complete: path.complete,
        exit_ts_ms: path.exit_ts_ms,
        exit_price: path.exit_price,
        exit_reason: path.exit_reason,
        gross_r,
        cost_r,
        net_r: gross_r - cost_r,
        event_cluster_id: None,
    }
}

/// 按方向把价格变化折算为入场时固定风险单位。
fn directional_r(entry: f64, exit: f64, risk: f64, direction: L2Direction) -> f64 {
    match direction {
        L2Direction::Long => (exit - entry) / risk,
        L2Direction::Short => (entry - exit) / risk,
    }
}

/// 按方向和同方向连续一小时触发链写入确定性的市场事件身份。
fn assign_event_clusters(trades: &mut [V10L2TradeRecord]) {
    let mut order = trades
        .iter()
        .enumerate()
        .filter(|(_, trade)| trade.complete)
        .map(|(idx, trade)| (idx, trade.signal_ts_ms, trade.direction))
        .collect::<Vec<_>>();
    order.sort_by_key(|(_, ts, direction)| (*ts, *direction));
    let mut chains: BTreeMap<&'static str, (i64, i64)> = BTreeMap::new();
    for (idx, ts, direction) in order {
        let chain = chains.entry(direction).or_insert((ts, ts));
        if ts.saturating_sub(chain.0) > EVENT_CLUSTER_WINDOW_MS {
            *chain = (ts, ts);
        } else {
            chain.0 = ts;
        }
        trades[idx].event_cluster_id = Some(format!("{direction}:{}", chain.1));
    }
}

#[allow(clippy::too_many_arguments)]
fn coverage(
    l1_candidates: usize,
    resolved_candidates: usize,
    trades: &[V10L2TradeRecord],
    completed: &[&V10L2TradeRecord],
    returned_symbol_count: usize,
    eligible_symbol_count: usize,
    excluded_symbol_count: usize,
    blockers: BTreeMap<String, usize>,
) -> V10L2Coverage {
    let completed_by_direction = BTreeMap::from([
        (
            "long",
            completed
                .iter()
                .filter(|trade| trade.direction == "long")
                .count(),
        ),
        (
            "short",
            completed
                .iter()
                .filter(|trade| trade.direction == "short")
                .count(),
        ),
    ]);
    let completed_symbol_count = completed
        .iter()
        .map(|trade| trade.symbol.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let months = completed
        .iter()
        .filter_map(|trade| utc_month(trade.signal_ts_ms))
        .collect::<BTreeSet<_>>();
    let completed_effective_market_events = completed
        .iter()
        .filter_map(|trade| trade.event_cluster_id.as_deref())
        .collect::<BTreeSet<_>>()
        .len();
    let mut exit_reasons = BTreeMap::new();
    for trade in completed {
        *exit_reasons.entry(trade.exit_reason).or_default() += 1;
    }
    V10L2Coverage {
        l1_candidates,
        resolved_candidates,
        executed_trades: trades.len(),
        completed_trades: completed.len(),
        incomplete_trades: trades.len().saturating_sub(completed.len()),
        completed_by_direction,
        completed_symbol_count,
        completed_month_count: months.len(),
        completed_effective_market_events,
        completed_trades_per_month: if months.is_empty() {
            0.0
        } else {
            completed.len() as f64 / months.len() as f64
        },
        returned_symbol_count,
        eligible_symbol_count,
        excluded_symbol_count,
        blockers,
        exit_reasons,
    }
}

/// 计算按时间顺序排列的一组交易级 R 指标。
fn performance(values: impl Iterator<Item = f64>) -> V10L2Performance {
    let values = values.collect::<Vec<_>>();
    let trades = values.len();
    let positive_r = values.iter().copied().filter(|value| *value > 0.0).sum();
    let negative_r_abs = -values
        .iter()
        .copied()
        .filter(|value| *value < 0.0)
        .sum::<f64>();
    let sum_r = values.iter().sum::<f64>();
    V10L2Performance {
        trades,
        positive_r,
        negative_r_abs,
        sum_r,
        expectancy_r: if trades == 0 {
            0.0
        } else {
            sum_r / trades as f64
        },
        profit_factor: (negative_r_abs > 0.0).then_some(positive_r / negative_r_abs),
        win_rate_pct: if trades == 0 {
            0.0
        } else {
            values.iter().filter(|value| **value > 0.0).count() as f64 / trades as f64 * 100.0
        },
        trade_sharpe: trade_sharpe(&values),
        max_drawdown_r: max_drawdown_r(&values),
    }
}

/// 对完整交易分别汇总多头和空头的成本后绩效。
fn performance_by_direction(
    trades: &[&V10L2TradeRecord],
) -> BTreeMap<&'static str, V10L2Performance> {
    ["long", "short"]
        .into_iter()
        .map(|direction| {
            (
                direction,
                performance(
                    trades
                        .iter()
                        .filter(|trade| trade.direction == direction)
                        .map(|trade| trade.net_r),
                ),
            )
        })
        .collect()
}

/// 对完整交易分别汇总 BTC、ETH 和其他币种的成本后绩效。
fn performance_by_asset_group(
    trades: &[&V10L2TradeRecord],
) -> BTreeMap<&'static str, V10L2Performance> {
    ["BTC", "ETH", "other"]
        .into_iter()
        .map(|group| {
            (
                group,
                performance(
                    trades
                        .iter()
                        .filter(|trade| trade.asset_group == group)
                        .map(|trade| trade.net_r),
                ),
            )
        })
        .collect()
}

/// 计算头部交易、币种、月份、方向、资产层和市场事件的成本后贡献。
fn concentration(trades: &[&V10L2TradeRecord]) -> V10L2Concentration {
    let total_net_r = trades.iter().map(|trade| trade.net_r).sum::<f64>();
    let mut ordered = trades.iter().map(|trade| trade.net_r).collect::<Vec<_>>();
    ordered.sort_by(|left, right| right.total_cmp(left));
    let mut net_r_by_symbol = BTreeMap::new();
    let mut net_r_by_month = BTreeMap::new();
    let mut net_r_by_direction = BTreeMap::from([("long", 0.0), ("short", 0.0)]);
    let mut net_r_by_asset_group = BTreeMap::from([("BTC", 0.0), ("ETH", 0.0), ("other", 0.0)]);
    let mut net_r_by_event = BTreeMap::new();
    let mut positive_by_symbol = BTreeMap::new();
    let mut positive_by_event = BTreeMap::new();
    let mut total_positive = 0.0;
    for trade in trades {
        *net_r_by_symbol.entry(trade.symbol.clone()).or_default() += trade.net_r;
        if let Some(month) = utc_month(trade.signal_ts_ms) {
            *net_r_by_month.entry(month).or_default() += trade.net_r;
        }
        *net_r_by_direction.entry(trade.direction).or_default() += trade.net_r;
        *net_r_by_asset_group.entry(trade.asset_group).or_default() += trade.net_r;
        if let Some(event) = trade.event_cluster_id.as_ref() {
            *net_r_by_event.entry(event.clone()).or_default() += trade.net_r;
            if trade.net_r > 0.0 {
                *positive_by_event.entry(event.clone()).or_default() += trade.net_r;
            }
        }
        if trade.net_r > 0.0 {
            *positive_by_symbol.entry(trade.symbol.clone()).or_default() += trade.net_r;
            total_positive += trade.net_r;
        }
    }
    let top_event = net_r_by_event.values().copied().fold(0.0_f64, f64::max);
    V10L2Concentration {
        net_r_after_removing_top_two_trades: total_net_r - ordered.iter().take(2).sum::<f64>(),
        net_r_after_removing_top_event: total_net_r - top_event,
        max_symbol_positive_r_share_pct: max_positive_share(&positive_by_symbol, total_positive),
        max_event_positive_r_share_pct: max_positive_share(&positive_by_event, total_positive),
        net_r_by_symbol,
        net_r_by_month,
        net_r_by_direction,
        net_r_by_asset_group,
    }
}

/// 返回任一键对全部正 R 的最大贡献比例。
fn max_positive_share<K: Ord>(values: &BTreeMap<K, f64>, total: f64) -> Option<f64> {
    (total > 0.0).then(|| values.values().copied().fold(0.0_f64, f64::max) / total * 100.0)
}

/// 应用查看结果前冻结的覆盖、成本、方向和集中度联合门禁。
fn decide_l2(
    coverage: &V10L2Coverage,
    gross: &V10L2Performance,
    net: &V10L2Performance,
    net_by_direction: &BTreeMap<&'static str, V10L2Performance>,
    concentration: &V10L2Concentration,
    source_candidate_ledger_verified: bool,
    contract_identity_verified: bool,
) -> V10L2Decision {
    let long = net_by_direction.get("long");
    let short = net_by_direction.get("short");
    let mut gates = BTreeMap::new();
    gates.insert(
        "l1_identity_dataset_and_candidate_ledger_verified",
        source_candidate_ledger_verified,
    );
    gates.insert(
        "completed_trades_at_least_30",
        coverage.completed_trades >= 30,
    );
    gates.insert(
        "completed_both_directions_at_least_10",
        coverage
            .completed_by_direction
            .get("long")
            .copied()
            .unwrap_or_default()
            >= 10
            && coverage
                .completed_by_direction
                .get("short")
                .copied()
                .unwrap_or_default()
                >= 10,
    );
    gates.insert(
        "completed_symbols_at_least_8",
        coverage.completed_symbol_count >= 8,
    );
    gates.insert(
        "completed_months_at_least_6",
        coverage.completed_month_count >= 6,
    );
    gates.insert(
        "completed_effective_events_at_least_15",
        coverage.completed_effective_market_events >= 15,
    );
    gates.insert(
        "gross_expectancy_and_profit_factor_positive",
        profitable(gross),
    );
    gates.insert(
        "cost_adjusted_expectancy_and_profit_factor_positive",
        profitable(net),
    );
    gates.insert(
        "both_directions_cost_adjusted_positive",
        long.is_some_and(profitable) && short.is_some_and(profitable),
    );
    gates.insert(
        "net_positive_after_removing_top_two_trades",
        concentration.net_r_after_removing_top_two_trades > 0.0,
    );
    gates.insert(
        "net_positive_after_removing_top_event",
        concentration.net_r_after_removing_top_event > 0.0,
    );
    gates.insert(
        "max_symbol_positive_r_share_at_most_35_pct",
        concentration
            .max_symbol_positive_r_share_pct
            .is_some_and(|share| share <= 35.0),
    );
    gates.insert(
        "max_event_positive_r_share_at_most_35_pct",
        concentration
            .max_event_positive_r_share_pct
            .is_some_and(|share| share <= 35.0),
    );
    gates.insert("contract_identity_verified", contract_identity_verified);
    let passed = gates.values().all(|passed| *passed);
    V10L2Decision {
        status: if passed {
            "L2_pass_L3_required"
        } else {
            "stop"
        },
        reason: if passed {
            "当前候选在冻结的下一根开盘、15m 动量风险、退出和压力成本下形成分散正边际；仍须 L3 的 point-in-time 币池、OOS/walk-forward、统一资金、资金费与压力验证。".to_owned()
        } else {
            "至少一项预注册 L2 门禁失败；当前候选停止在 Research-only，不得根据结果调参后直接接入 Paper、ReadOnly、Live 或生产。".to_owned()
        },
        gates,
    }
}

/// 同时要求正期望和大于 1 的 Profit Factor。
fn profitable(performance: &V10L2Performance) -> bool {
    performance.expectancy_r > 0.0
        && performance
            .profit_factor
            .is_some_and(|profit_factor| profit_factor > 1.0)
}

/// 逐笔回查下一根开盘、保护价、成本、事件和退出路径没有漂移。
fn contract_is_consistent(
    data: &BacktestDataSet,
    trade: &V10L2TradeRecord,
    initial_risk_policy: InitialRiskPolicy,
    target_risk_policy: TargetRiskPolicy,
    entry_risk_gate_policy: EntryRiskGatePolicy,
) -> bool {
    let direction = match parse_direction(trade.direction) {
        Ok(direction) => direction,
        Err(_) => return false,
    };
    let Some(candles) = data.candles_15m_computed.get(&trade.symbol) else {
        return false;
    };
    let Ok(entry_idx) = candles.binary_search_by_key(&trade.entry_ts_ms, |candle| candle.candle.ts)
    else {
        return false;
    };
    let Some(entry_candle) = candles.get(entry_idx) else {
        return false;
    };
    let Ok((expected_stop, expected_target)) = risk_prices_for_candidate(
        trade.entry_price,
        direction,
        trade.signal_ema144,
        trade.signal_atr14,
        initial_risk_policy,
        target_risk_policy,
    ) else {
        return false;
    };
    let Ok(risk) = initial_risk_amount(
        trade.entry_price,
        expected_stop,
        direction,
        initial_risk_policy,
    ) else {
        return false;
    };
    let expected_gross = directional_r(trade.entry_price, trade.exit_price, risk, direction);
    let expected_cost = (trade.entry_price + trade.exit_price) * PER_SIDE_COST_RATE / risk;
    if validate_entry_risk_gate(
        trade.entry_price,
        expected_stop,
        risk,
        entry_risk_gate_policy,
    )
    .is_err()
    {
        return false;
    }
    let exit_price_matches_reason = match trade.exit_reason {
        "stop_hit" | "both_hit_stop_first" => {
            approx_equal(trade.exit_price, trade.initial_stop_price)
        }
        "target_hit" => approx_equal(trade.exit_price, trade.target_price),
        "max_holding_timeout" | "forward_data_gap" | "forward_data_incomplete" => true,
        _ => false,
    };
    let completion_matches_reason = match trade.exit_reason {
        "stop_hit" | "both_hit_stop_first" | "target_hit" | "max_holding_timeout" => trade.complete,
        "forward_data_gap" | "forward_data_incomplete" => !trade.complete,
        _ => false,
    };
    trade.entry_ts_ms == trade.signal_ts_ms.saturating_add(MS_15M)
        && approx_equal(trade.entry_price, entry_candle.candle.open)
        && approx_equal(trade.initial_stop_price, expected_stop)
        && approx_equal(trade.target_price, expected_target)
        && trade.exit_ts_ms >= trade.entry_ts_ms
        && trade.exit_ts_ms <= trade.entry_ts_ms.saturating_add(MAX_HOLDING_MS)
        && exit_price_matches_reason
        && completion_matches_reason
        && approx_equal(trade.gross_r, expected_gross)
        && approx_equal(trade.cost_r, expected_cost)
        && approx_equal(trade.net_r, expected_gross - expected_cost)
        && trade.asset_group == asset_group(&trade.symbol)
        && (!trade.complete || trade.event_cluster_id.is_some())
        && (trade.complete || trade.event_cluster_id.is_none())
        && trade.candidate_id
            == format!(
                "{}:{}:{}",
                trade.symbol, trade.signal_ts_ms, trade.direction
            )
}

/// 独立复核首笔成交策略没有在同一方向 setup 下留下第二笔实际交易。
fn setup_entry_policy_is_consistent(trades: &[V10L2TradeRecord], policy: SetupEntryPolicy) -> bool {
    if policy == SetupEntryPolicy::AllowRepeated {
        return true;
    }
    let mut seen = BTreeSet::new();
    trades
        .iter()
        .all(|trade| seen.insert((trade.symbol.as_str(), trade.direction, trade.setup_ts_ms)))
}

/// 实际交易账本中同币种下一笔信号必须严格晚于上一笔退出 K。
fn symbol_lock_is_consistent(trades: &[V10L2TradeRecord]) -> bool {
    let mut last_exit_by_symbol: BTreeMap<&str, i64> = BTreeMap::new();
    for trade in trades {
        if last_exit_by_symbol
            .get(trade.symbol.as_str())
            .is_some_and(|last_exit| trade.signal_ts_ms <= *last_exit)
        {
            return false;
        }
        last_exit_by_symbol.insert(trade.symbol.as_str(), trade.exit_ts_ms);
    }
    true
}

/// 将冻结账本方向文本收敛为内部镜像枚举。
fn parse_direction(value: &str) -> Result<L2Direction, &'static str> {
    match value {
        "long" => Ok(L2Direction::Long),
        "short" => Ok(L2Direction::Short),
        _ => Err("candidate_direction_invalid"),
    }
}

/// 将币种映射到预注册的 BTC、ETH 与其他三层诊断口径。
fn asset_group(symbol: &str) -> &'static str {
    match symbol {
        "BTC-USDT-SWAP" => "BTC",
        "ETH-USDT-SWAP" => "ETH",
        _ => "other",
    }
}

/// 把 Unix 毫秒转换为用于月份覆盖与贡献统计的 UTC 月份。
fn utc_month(ts_ms: i64) -> Option<String> {
    Utc.timestamp_millis_opt(ts_ms)
        .single()
        .map(|value| value.format("%Y-%m").to_string())
}

/// 计算非年化交易级 Sharpe，避免把逐笔序列冒充组合时间收益。
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
    let standard_deviation = variance.sqrt();
    (standard_deviation > 0.0).then_some(mean / standard_deviation * (values.len() as f64).sqrt())
}

/// 计算按信号时间排序的累计 R 最大回撤。
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

/// 对价格和 R 公式使用相对容差比较，吸收浮点序列化误差。
fn approx_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9 * left.abs().max(right.abs()).max(1.0)
}

/// 生成冻结文件身份使用的小写 SHA-256 十六进制摘要。
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
