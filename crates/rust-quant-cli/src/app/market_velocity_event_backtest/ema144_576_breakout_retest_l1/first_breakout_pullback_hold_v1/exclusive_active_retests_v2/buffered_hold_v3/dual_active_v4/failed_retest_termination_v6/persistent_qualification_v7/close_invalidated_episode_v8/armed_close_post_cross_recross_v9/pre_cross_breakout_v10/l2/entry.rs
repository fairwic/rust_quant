//! 已完成信号到下一根连续 15m 开盘的因果入场与风险冻结。

use super::*;

/// 一份已经用下一根连续 15m 开盘价冻结风险的入场计划。
#[derive(Debug, Clone)]
pub(in super::super) struct EntryPlan {
    pub(in super::super) candidate_id: String,
    pub(in super::super) symbol: String,
    pub(in super::super) direction: L2Direction,
    pub(in super::super) setup_ts_ms: i64,
    pub(in super::super) breakout_ts_ms: i64,
    pub(in super::super) rearmed_ts_ms: i64,
    pub(in super::super) signal_ts_ms: i64,
    pub(in super::super) cross_phase: &'static str,
    pub(in super::super) signal_ema144: f64,
    pub(in super::super) signal_ema576: f64,
    pub(in super::super) signal_atr14: f64,
    pub(in super::super) retest_extreme_to_ema144_atr: f64,
    pub(in super::super) close_to_ema144_directional_atr: f64,
    pub(in super::super) entry_idx: usize,
    pub(in super::super) entry_ts_ms: i64,
    pub(in super::super) entry_price: f64,
    pub(in super::super) initial_risk: f64,
    pub(in super::super) stop_price: f64,
    pub(in super::super) target_price: f64,
}

/// V10 信号收盘后只允许紧接的一根 15m 开盘成交，禁止跨缺口补造机会。
pub(super) fn resolve_entry(
    data: &BacktestDataSet,
    candidate: V2Candidate,
    initial_risk_policy: InitialRiskPolicy,
    target_risk_policy: TargetRiskPolicy,
    entry_risk_gate_policy: EntryRiskGatePolicy,
) -> Result<EntryPlan, &'static str> {
    let plan = inspect_entry_risk(data, &candidate, initial_risk_policy, target_risk_policy)?;
    validate_entry_risk_gate(
        plan.entry_price,
        plan.stop_price,
        plan.initial_risk,
        entry_risk_gate_policy,
    )?;
    Ok(plan)
}

/// 只读取信号收盘与下一根连续开盘，冻结入场、结构风险和目标的因果证据。
pub(in super::super) fn inspect_entry_risk(
    data: &BacktestDataSet,
    candidate: &V2Candidate,
    initial_risk_policy: InitialRiskPolicy,
    target_risk_policy: TargetRiskPolicy,
) -> Result<EntryPlan, &'static str> {
    let direction = parse_direction(candidate.direction)?;
    let candles = data
        .candles_15m_computed
        .get(&candidate.symbol)
        .ok_or("symbol_candles_missing")?;
    let signal_idx = candles
        .binary_search_by_key(&candidate.signal_ts_ms, |candle| candle.candle.ts)
        .map_err(|_| "signal_candle_missing")?;
    let entry_idx = signal_idx
        .checked_add(1)
        .ok_or("next_entry_index_overflow")?;
    let entry = candles.get(entry_idx).ok_or("next_entry_candle_missing")?;
    let expected_entry_ts = candidate
        .signal_ts_ms
        .checked_add(MS_15M)
        .ok_or("next_entry_timestamp_overflow")?;
    if entry.candle.ts != expected_entry_ts {
        return Err("next_entry_candle_not_contiguous");
    }
    let entry_price = entry.candle.open;
    let (stop_price, target_price) = risk_prices_for_candidate(
        entry_price,
        direction,
        candidate.ema144,
        candidate.atr14,
        initial_risk_policy,
        target_risk_policy,
    )?;
    let initial_risk =
        initial_risk_amount(entry_price, stop_price, direction, initial_risk_policy)?;
    Ok(EntryPlan {
        candidate_id: format!(
            "{}:{}:{}",
            candidate.symbol,
            candidate.signal_ts_ms,
            direction.label()
        ),
        symbol: candidate.symbol.clone(),
        direction,
        setup_ts_ms: candidate.setup_ts_ms,
        breakout_ts_ms: candidate.breakout_ts_ms,
        rearmed_ts_ms: candidate.rearmed_ts_ms,
        signal_ts_ms: candidate.signal_ts_ms,
        cross_phase: candidate.cross_phase,
        signal_ema144: candidate.ema144,
        signal_ema576: candidate.ema576,
        signal_atr14: candidate.atr14,
        retest_extreme_to_ema144_atr: candidate.retest_extreme_to_ema144_atr,
        close_to_ema144_directional_atr: candidate.close_to_ema144_directional_atr,
        entry_idx,
        entry_ts_ms: entry.candle.ts,
        entry_price,
        initial_risk,
        stop_price,
        target_price,
    })
}
