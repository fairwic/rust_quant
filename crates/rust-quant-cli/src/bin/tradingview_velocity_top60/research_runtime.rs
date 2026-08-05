use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    Direction, EntryIntent, ExitPolicy, ExitReason, FrozenUniverseData, HorizontalAnchorEvidence,
    ReplayReport, SignalFamily, Trade,
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{family_name, CANDLE_INTERVAL_MS, EFFECTIVE_EVENT_CLUSTER_MS};

pub(super) const BASELINE_CACHE_SCHEMA_VERSION: &str =
    "tradingview_velocity_top60_baseline_cache_v1";
pub(super) const CANDIDATE_LEDGER_SCHEMA_VERSION: &str = "tradingview_velocity_candidate_ledger_v7";

/// 同一进程内可比较的阶段耗时；毫秒不足 1 时保留为 0，不伪造精度。
#[derive(Debug, Clone, Default, Serialize)]
pub(super) struct PhaseTimings {
    pub(super) pine_source_verification_ms: u64,
    pub(super) data_load_ms: u64,
    pub(super) dataset_fingerprint_ms: u64,
    pub(super) eligibility_ms: u64,
    pub(super) replay_ms: u64,
    pub(super) analysis_ms: u64,
    pub(super) candidate_ledger_ms: u64,
    pub(super) serialization_ms: u64,
}

/// 研究报告的运行证据；缓存命中只允许省略回放，不能改变数据或源码身份。
#[derive(Debug, Clone, Serialize)]
pub(super) struct ResearchRuntimeDiagnostics {
    pub(super) cache_schema_version: &'static str,
    pub(super) dataset_fingerprint_sha256: String,
    pub(super) executable_fingerprint_sha256: String,
    pub(super) baseline_cache_key_sha256: Option<String>,
    pub(super) baseline_cache_hit: bool,
    pub(super) baseline_cache_reused_at_ms: Option<i64>,
    pub(super) phase_timings: PhaseTimings,
}

/// 账本只把信号时可见特征和后验结果并排保存；筛阈值时必须只读前者。
#[derive(Debug, Serialize)]
pub(super) struct CandidateLedger {
    pub(super) schema_version: &'static str,
    pub(super) label_boundary: &'static str,
    pub(super) setup_time_semantics: &'static str,
    pub(super) candidates: Vec<CandidateLedgerEntry>,
    pub(super) blocked_events: Vec<BlockedCandidateEvent>,
}

/// 一个在信号收盘时已经冻结的入场意图及其独立后验标签。
#[derive(Debug, Serialize)]
pub(super) struct CandidateLedgerEntry {
    pub(super) symbol: String,
    pub(super) setup_time_ms: Option<i64>,
    pub(super) signal_time_ms: i64,
    pub(super) direction: Direction,
    pub(super) families: Vec<&'static str>,
    pub(super) event_cluster_id: String,
    pub(super) time_visible_features: CandidateVisibleFeatures,
    pub(super) frozen_risk: CandidateFrozenRisk,
    pub(super) outcome: CandidateOutcome,
}

/// 只含信号棒收盘时已经存在的值，禁止被退出路径反向补写。
#[derive(Debug, Serialize)]
pub(super) struct CandidateVisibleFeatures {
    pub(super) signal_close: f64,
    pub(super) signal_atr: f64,
    pub(super) volume_ratio: Option<f64>,
    pub(super) rsi: Option<f64>,
    pub(super) counter_trend: bool,
    pub(super) counter_trend_ema_age_bars_capped_600: Option<usize>,
    pub(super) breakout_line: Option<f64>,
    pub(super) anchor_upthrust_target_consumption_ratio: Option<f64>,
    /// V26～V29 父横盘、实体否定与突破超幅的信号时证据；`None` 表示其他策略家族。
    pub(super) active_parent_horizontal_anchor: Option<HorizontalAnchorEvidence>,
    /// 严格视觉横盘突破源棒已经冻结的区间长度；确认等待期间不会增长。
    pub(super) strict_visual_range_length_bars: Option<usize>,
    /// 严格视觉横盘冻结上下沿的绝对价差；`None` 表示其他策略家族。
    pub(super) strict_visual_range_height: Option<f64>,
    /// V4 是否对该冻结来源启用短区间 1R Fixed 目标。
    pub(super) strict_visual_short_range_one_r_target: Option<bool>,
    /// `true` 表示该候选冻结了突破棒整根极值止损，而不是成交后的 ATR tick 距离。
    pub(super) strict_visual_breakout_candle_extreme_stop: bool,
}

/// 绝对价与相对 tick 分开保留，避免把下一根实际开盘误写成信号时已知价格。
#[derive(Debug, Serialize)]
pub(super) struct CandidateFrozenRisk {
    pub(super) stop_price: Option<f64>,
    pub(super) stop_ticks: Option<i64>,
    pub(super) target_price: Option<f64>,
    pub(super) target_ticks: Option<i64>,
    pub(super) activation_ticks: Option<i64>,
    pub(super) exit_policy: ExitPolicy,
}

/// 候选的成交与退出标签；它们可以评估结果，但不能参与 L1 无标签筛选。
#[derive(Debug, Serialize)]
pub(super) struct CandidateOutcome {
    pub(super) status: &'static str,
    pub(super) blocked_reason: Option<String>,
    pub(super) entry_time_ms: Option<i64>,
    pub(super) exit_time_ms: Option<i64>,
    pub(super) entry_price: Option<f64>,
    pub(super) exit_price: Option<f64>,
    pub(super) actual_initial_stop: Option<f64>,
    pub(super) actual_target_from_frozen_intent: Option<f64>,
    pub(super) exit_reason: Option<ExitReason>,
    pub(super) zero_cost_net_r: Option<f64>,
    pub(super) cost_adjusted_net_r: Option<f64>,
}

/// 没有形成可执行意图的冲突或撤单证据；与候选分栏以避免伪造其冻结指标。
#[derive(Debug, Serialize)]
pub(super) struct BlockedCandidateEvent {
    pub(super) symbol: String,
    pub(super) observed_time_ms: i64,
    pub(super) direction: Option<Direction>,
    pub(super) reason: String,
}

/// 把 `Instant` 安全收敛为报告中的毫秒整数。
pub(super) fn elapsed_ms(started_at: Instant) -> u64 {
    duration_ms(started_at.elapsed())
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// 对冻结成员、覆盖证据与每根 OHLCV 做顺序敏感指纹，缓存不能跨数据快照复用。
pub(super) fn dataset_fingerprint(dataset: &FrozenUniverseData) -> String {
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, &dataset.universe_version);
    hash_text(&mut hasher, &dataset.manifest_sha256);
    hash_i64(&mut hasher, dataset.window_start_ms);
    hash_i64(&mut hasher, dataset.window_end_ms);
    hash_usize(&mut hasher, dataset.coverage.expected_symbol_count);
    hash_usize(&mut hasher, dataset.coverage.returned_symbol_count);
    for symbol in &dataset.symbols {
        hash_text(&mut hasher, &symbol.symbol);
        hash_u64(&mut hasher, symbol.tick_size.to_bits());
        hash_usize(&mut hasher, symbol.warmup_expected_candle_count);
        hash_usize(&mut hasher, symbol.warmup_loaded_candle_count);
        hasher.update([u8::from(symbol.warmup_is_complete)]);
        hash_usize(&mut hasher, symbol.coverage.expected_candle_count);
        hash_usize(&mut hasher, symbol.coverage.loaded_candle_count);
        hash_usize(&mut hasher, symbol.coverage.missing_candle_count);
        for candle in &symbol.candles {
            hash_i64(&mut hasher, candle.timestamp_ms);
            hash_u64(&mut hasher, candle.open.to_bits());
            hash_u64(&mut hasher, candle.high.to_bits());
            hash_u64(&mut hasher, candle.low.to_bits());
            hash_u64(&mut hasher, candle.close.to_bits());
            hash_u64(&mut hasher, candle.volume.to_bits());
        }
    }
    hex::encode(hasher.finalize())
}

/// 哈希当前实际运行的二进制，Rust 逻辑变化后即使 Pine 标签相同也必须缓存 miss。
pub(super) fn executable_fingerprint() -> Result<String> {
    let path = std::env::current_exe().context("定位当前 Top60 可执行文件失败")?;
    let mut file =
        File::open(&path).with_context(|| format!("打开当前可执行文件失败：{}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("读取当前可执行文件失败：{}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// 用完整实验身份生成缓存键；调用方必须同时把该身份写入机器结果。
pub(super) fn cache_key(identity: &str) -> String {
    hex::encode(Sha256::digest(identity.as_bytes()))
}

/// 读取一个完全同身份的 baseline 报告；损坏缓存直接报错而不是静默回退旧结果。
pub(super) fn load_baseline_cache(cache_dir: &Path, key: &str) -> Result<Option<Value>> {
    let path = baseline_cache_path(cache_dir, key);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read(&path)
        .with_context(|| format!("读取 Top60 baseline 缓存失败：{}", path.display()))?;
    let value: Value = serde_json::from_slice(&raw)
        .with_context(|| format!("解析 Top60 baseline 缓存失败：{}", path.display()))?;
    let schema = value
        .pointer("/research_runtime/cache_schema_version")
        .and_then(Value::as_str);
    if schema != Some(BASELINE_CACHE_SCHEMA_VERSION) {
        bail!("Top60 baseline 缓存 schema 不匹配：{}", path.display());
    }
    Ok(Some(value))
}

/// 原子写入 baseline 报告；只缓存可由完整身份重新计算的机器结果。
pub(super) fn store_baseline_cache(cache_dir: &Path, key: &str, value: &Value) -> Result<()> {
    std::fs::create_dir_all(cache_dir)
        .with_context(|| format!("创建 Top60 baseline 缓存目录失败：{}", cache_dir.display()))?;
    let path = baseline_cache_path(cache_dir, key);
    let temporary = cache_dir.join(format!(".{key}.{}.tmp", std::process::id()));
    let mut bytes = serde_json::to_vec_pretty(value).context("序列化 Top60 baseline 缓存失败")?;
    bytes.push(b'\n');
    std::fs::write(&temporary, bytes)
        .with_context(|| format!("写入 Top60 baseline 临时缓存失败：{}", temporary.display()))?;
    std::fs::rename(&temporary, &path).with_context(|| {
        format!(
            "提交 Top60 baseline 缓存失败：{} -> {}",
            temporary.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn baseline_cache_path(cache_dir: &Path, key: &str) -> PathBuf {
    cache_dir.join(format!("{key}.json"))
}

/// 合并零成本和成本压力路径，生成一次即可反复筛选的候选账本。
pub(super) fn build_candidate_ledger(
    zero_cost_reports: &[ReplayReport],
    cost_adjusted_reports: &[ReplayReport],
) -> Result<CandidateLedger> {
    if zero_cost_reports.len() != cost_adjusted_reports.len() {
        bail!("候选账本的零成本与成本后成员数量不一致");
    }
    let mut candidates = Vec::new();
    let mut blocked_events = Vec::new();
    for (zero, cost) in zero_cost_reports.iter().zip(cost_adjusted_reports) {
        if zero.symbol != cost.symbol {
            bail!("候选账本成员顺序不一致：{} != {}", zero.symbol, cost.symbol);
        }
        validate_candidate_identity(zero, cost)?;
        for intent in &zero.entry_candidates {
            let zero_trade = matching_trade(&zero.trades, intent);
            let cost_trade = matching_trade(&cost.trades, intent);
            if zero_trade.is_some() != cost_trade.is_some() {
                bail!(
                    "候选账本成本路径成交身份漂移：{} @ {}",
                    zero.symbol,
                    intent.signal_time_ms
                );
            }
            let blocked_reason = zero
                .blocked_signals
                .iter()
                .find(|blocked| {
                    blocked.signal_time_ms == intent.signal_time_ms
                        && blocked
                            .direction
                            .is_none_or(|direction| direction == intent.direction)
                })
                .map(|blocked| blocked.reason.clone());
            candidates.push(candidate_entry(
                &zero.symbol,
                zero.tick_size,
                intent,
                zero_trade,
                cost_trade,
                blocked_reason,
                zero.open_position_at_end,
                zero.pending_entry_at_end,
            ));
        }
        blocked_events.extend(
            zero.blocked_signals
                .iter()
                .map(|blocked| BlockedCandidateEvent {
                    symbol: zero.symbol.clone(),
                    observed_time_ms: blocked.signal_time_ms,
                    direction: blocked.direction,
                    reason: blocked.reason.clone(),
                }),
        );
    }
    assign_event_clusters(&mut candidates);
    blocked_events.sort_by(|left, right| {
        left.observed_time_ms
            .cmp(&right.observed_time_ms)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    Ok(CandidateLedger {
        schema_version: CANDIDATE_LEDGER_SCHEMA_VERSION,
        label_boundary: "L1 threshold scans may read only time_visible_features and frozen_risk; outcome and blocked_events are labels for later evaluation",
        setup_time_semantics: "setup_time_ms is only inferred for the registered one-bar anchor-upthrust right-side confirmation; other stateful families remain null until their engine exposes an explicit setup timestamp",
        candidates,
        blocked_events,
    })
}

fn validate_candidate_identity(zero: &ReplayReport, cost: &ReplayReport) -> Result<()> {
    if zero.entry_candidates.len() != cost.entry_candidates.len() {
        bail!("{} 的零成本与成本后候选数量发生漂移", zero.symbol);
    }
    for (left, right) in zero.entry_candidates.iter().zip(&cost.entry_candidates) {
        if left.signal_time_ms != right.signal_time_ms
            || left.direction != right.direction
            || left.families != right.families
            || left.strict_visual_range_length_bars != right.strict_visual_range_length_bars
            || left.strict_visual_range_height != right.strict_visual_range_height
            || left.strict_visual_short_range_one_r_target
                != right.strict_visual_short_range_one_r_target
            || left.strict_visual_breakout_candle_extreme_stop
                != right.strict_visual_breakout_candle_extreme_stop
        {
            bail!("{} 的零成本与成本后候选身份发生漂移", zero.symbol);
        }
    }
    Ok(())
}

fn candidate_entry(
    symbol: &str,
    tick_size: f64,
    intent: &EntryIntent,
    zero_trade: Option<&Trade>,
    cost_trade: Option<&Trade>,
    blocked_reason: Option<String>,
    open_position_at_end: bool,
    pending_entry_at_end: bool,
) -> CandidateLedgerEntry {
    let status = if zero_trade.is_some() {
        "closed_trade"
    } else if blocked_reason.is_some() {
        "blocked"
    } else if pending_entry_at_end {
        "pending_at_end"
    } else if open_position_at_end {
        "possibly_open_at_end"
    } else {
        "unfilled_or_replaced"
    };
    CandidateLedgerEntry {
        symbol: symbol.to_owned(),
        setup_time_ms: inferred_setup_time(intent),
        signal_time_ms: intent.signal_time_ms,
        direction: intent.direction,
        families: intent.families.iter().copied().map(family_name).collect(),
        event_cluster_id: String::new(),
        time_visible_features: CandidateVisibleFeatures {
            signal_close: intent.signal_close,
            signal_atr: intent.signal_atr,
            volume_ratio: intent.volume_ratio,
            rsi: intent.rsi,
            counter_trend: intent.counter_trend,
            counter_trend_ema_age_bars_capped_600: intent
                .signal_counter_trend_ema_age_bars_capped_600,
            breakout_line: intent.breakout_line,
            anchor_upthrust_target_consumption_ratio: intent
                .anchor_upthrust_target_consumption_ratio,
            active_parent_horizontal_anchor: intent.active_parent_horizontal_anchor,
            strict_visual_range_length_bars: intent.strict_visual_range_length_bars,
            strict_visual_range_height: intent.strict_visual_range_height,
            strict_visual_short_range_one_r_target: intent.strict_visual_short_range_one_r_target,
            strict_visual_breakout_candle_extreme_stop: intent
                .strict_visual_breakout_candle_extreme_stop,
        },
        frozen_risk: CandidateFrozenRisk {
            stop_price: intent.stop_price,
            stop_ticks: intent.stop_ticks,
            target_price: intent.target_price,
            target_ticks: intent.target_ticks,
            activation_ticks: intent.activation_ticks,
            exit_policy: intent.exit_policy,
        },
        outcome: CandidateOutcome {
            status,
            blocked_reason,
            entry_time_ms: zero_trade.map(|trade| trade.entry_time_ms),
            exit_time_ms: zero_trade.map(|trade| trade.exit_time_ms),
            entry_price: zero_trade.map(|trade| trade.entry_price),
            exit_price: zero_trade.map(|trade| trade.exit_price),
            actual_initial_stop: zero_trade.map(|trade| trade.initial_stop),
            actual_target_from_frozen_intent: zero_trade
                .and_then(|trade| actual_target_from_intent(intent, trade.entry_price, tick_size)),
            exit_reason: zero_trade.map(|trade| trade.exit_reason),
            zero_cost_net_r: zero_trade.map(|trade| trade.net_r),
            cost_adjusted_net_r: cost_trade.map(|trade| trade.net_r),
        },
    }
}

fn matching_trade<'a>(trades: &'a [Trade], intent: &EntryIntent) -> Option<&'a Trade> {
    trades.iter().find(|trade| {
        trade.signal_time_ms == intent.signal_time_ms && trade.direction == intent.direction
    })
}

fn inferred_setup_time(intent: &EntryIntent) -> Option<i64> {
    intent
        .families
        .contains(&SignalFamily::AnchorUpthrustFailedAcceptanceRightSideShort)
        .then(|| intent.signal_time_ms.checked_sub(CANDLE_INTERVAL_MS))
        .flatten()
}

fn actual_target_from_intent(
    intent: &EntryIntent,
    entry_price: f64,
    tick_size: f64,
) -> Option<f64> {
    intent.target_price.or_else(|| {
        intent.target_ticks.map(|ticks| match intent.direction {
            Direction::Long => entry_price + ticks as f64 * tick_size,
            Direction::Short => entry_price - ticks as f64 * tick_size,
        })
    })
}

fn assign_event_clusters(candidates: &mut [CandidateLedgerEntry]) {
    candidates.sort_by(|left, right| {
        left.signal_time_ms
            .cmp(&right.signal_time_ms)
            .then_with(|| direction_key(left.direction).cmp(&direction_key(right.direction)))
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    let mut active = BTreeMap::<u8, (i64, i64)>::new();
    for candidate in candidates {
        let key = direction_key(candidate.direction);
        let cluster_start = match active.get(&key).copied() {
            Some((last_time, cluster_start))
                if candidate.signal_time_ms - last_time <= EFFECTIVE_EVENT_CLUSTER_MS =>
            {
                cluster_start
            }
            _ => candidate.signal_time_ms,
        };
        active.insert(key, (candidate.signal_time_ms, cluster_start));
        candidate.event_cluster_id = format!(
            "{}-{cluster_start}",
            match candidate.direction {
                Direction::Long => "long",
                Direction::Short => "short",
            }
        );
    }
}

fn direction_key(direction: Direction) -> u8 {
    match direction {
        Direction::Long => 0,
        Direction::Short => 1,
    }
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hash_usize(hasher, value.len());
    hasher.update(value.as_bytes());
}

fn hash_i64(hasher: &mut Sha256, value: i64) {
    hasher.update(value.to_le_bytes());
}

fn hash_u64(hasher: &mut Sha256, value: u64) {
    hasher.update(value.to_le_bytes());
}

fn hash_usize(hasher: &mut Sha256, value: usize) {
    hash_u64(hasher, u64::try_from(value).unwrap_or(u64::MAX));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_cli::app::tradingview_velocity_parity::{ExitPolicy, Metrics, SignalFamily};

    fn intent(signal_time_ms: i64) -> EntryIntent {
        EntryIntent {
            signal_index: 1,
            signal_time_ms,
            direction: Direction::Short,
            families: vec![SignalFamily::AnchorUpthrustFailedAcceptanceRightSideShort],
            signal_close: 100.0,
            signal_atr: 2.0,
            stop_price: Some(103.0),
            stop_ticks: None,
            target_price: Some(94.0),
            target_ticks: None,
            activation_ticks: None,
            exit_policy: ExitPolicy::Fixed,
            counter_trend: false,
            signal_counter_trend_ema_age_bars_capped_600: None,
            counter_trend_structure_breakout_line: None,
            anchor_upthrust_target_consumption_ratio: Some(0.1),
            active_parent_horizontal_anchor: None,
            strict_visual_range_length_bars: Some(32),
            strict_visual_range_height: Some(6.0),
            strict_visual_short_range_one_r_target: Some(true),
            strict_visual_breakout_candle_extreme_stop: false,
            volume_ratio: Some(5.0),
            rsi: Some(60.0),
            breakout_line: None,
        }
    }

    fn trade(signal_time_ms: i64, net_r: f64) -> Trade {
        Trade {
            direction: Direction::Short,
            families: vec![SignalFamily::AnchorUpthrustFailedAcceptanceRightSideShort],
            exit_policy: ExitPolicy::Fixed,
            signal_counter_trend_ema_age_bars_capped_600: None,
            counter_trend_structure_breakout_line: None,
            counter_trend_structure_confirmed: false,
            counter_trend_two_r_trailing_activated: false,
            range_partial_one_r_taken: false,
            range_two_r_trailing_activated: false,
            signal_time_ms,
            entry_time_ms: signal_time_ms + CANDLE_INTERVAL_MS,
            exit_time_ms: signal_time_ms + 2 * CANDLE_INTERVAL_MS,
            entry_price: 99.0,
            exit_price: 96.0,
            initial_stop: 103.0,
            exit_reason: ExitReason::TakeProfit,
            gross_pnl: 3.0,
            net_pnl: 3.0,
            initial_risk: 4.0,
            net_r,
            anchor_upthrust_target_consumption_ratio: Some(0.1),
            volume_ratio: Some(5.0),
            rsi: Some(60.0),
        }
    }

    fn report(symbol: &str, signal_time_ms: i64, net_r: f64) -> ReplayReport {
        ReplayReport {
            strategy_version: "fixture",
            pine_source_fnv1a32: "fixture",
            symbol: symbol.to_owned(),
            tick_size: 0.1,
            evaluation_start_ms: 0,
            evaluation_end_ms: signal_time_ms + 10 * CANDLE_INTERVAL_MS,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            metrics: Metrics::default(),
            entry_candidates: vec![intent(signal_time_ms)],
            trades: vec![trade(signal_time_ms, net_r)],
            blocked_signals: Vec::new(),
            open_position_at_end: false,
            pending_entry_at_end: false,
        }
    }

    #[test]
    fn ledger_keeps_visible_features_separate_from_outcome() {
        let zero = report("BTC-USDT-SWAP", 10 * CANDLE_INTERVAL_MS, 0.75);
        let stress = report("BTC-USDT-SWAP", 10 * CANDLE_INTERVAL_MS, 0.55);
        let ledger = build_candidate_ledger(&[zero], &[stress]).expect("candidate ledger");

        assert_eq!(ledger.candidates.len(), 1);
        let candidate = &ledger.candidates[0];
        assert_eq!(candidate.setup_time_ms, Some(9 * CANDLE_INTERVAL_MS));
        assert_eq!(candidate.time_visible_features.volume_ratio, Some(5.0));
        assert_eq!(
            candidate
                .time_visible_features
                .strict_visual_range_length_bars,
            Some(32)
        );
        assert_eq!(
            candidate.time_visible_features.strict_visual_range_height,
            Some(6.0)
        );
        assert_eq!(
            candidate
                .time_visible_features
                .strict_visual_short_range_one_r_target,
            Some(true)
        );
        assert_eq!(candidate.outcome.zero_cost_net_r, Some(0.75));
        assert_eq!(candidate.outcome.cost_adjusted_net_r, Some(0.55));
    }

    #[test]
    fn cache_key_changes_with_any_identity_text() {
        assert_eq!(cache_key("same"), cache_key("same"));
        assert_ne!(cache_key("same"), cache_key("different"));
    }
}
