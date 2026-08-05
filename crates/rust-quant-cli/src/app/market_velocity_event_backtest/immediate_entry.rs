use super::filtered_volume_rsi_ema_macd::{
    FILTERED_VOLUME_STOP_ATR_MULTIPLIER, FILTERED_VOLUME_V3_ATR_STOP_SOURCE,
    FILTERED_VOLUME_V3_INVALID_AT_FILL_STOP_SOURCE,
};
use super::{
    completed_candle_entry_signal, is_filtered_volume_weekly_base_version,
    uses_anchor_wick_or_next_touch_entry, uses_momentum_exhaustion_limit_entry,
    CompletedCandleEntrySignalEvidence, ComputedCandle, ConfirmedEvent,
    MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection, RadarEvent, MS_15M,
};

#[derive(Debug, Clone, Copy, PartialEq)]
struct ResolvedAnchorEntry {
    price: f64,
    candle_idx: usize,
    candle_ts_ms: i64,
}

/// 构造即时入场；v3 使用信号确认后的下一根开盘，旧版本保持原收盘价口径。
pub(super) fn immediate_entry_from_signal(
    event: &RadarEvent,
    candles: &[ComputedCandle],
    direction: MarketVelocityTradeDirection,
    trigger: String,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<ConfirmedEvent, String> {
    if !event.current_price.is_finite() || event.current_price <= 0.0 {
        return Err("entry_current_price_invalid".to_string());
    }
    let first_entry_idx = outcome_start_candle_idx(candles, event.ts)
        .ok_or_else(|| "no_entry_outcome_candle".to_string())?;
    let is_filtered_volume_weekly_base =
        is_filtered_volume_weekly_base_version(&args.paper_outcome_entry_rule_version);
    let mut strategy_signal =
        if args.entry_filtered_volume_rsi_ema_macd || args.entry_rsi_volume_regime {
            let completed_count = completed_candle_count(candles, event.ts);
            let signal = completed_candle_entry_signal(candles, completed_count, args)
                .map_err(str::to_string)?;
            if signal.direction != direction || signal.trigger != trigger {
                return Err("completed_candle_strategy_signal_changed".to_string());
            }
            Some(signal)
        } else {
            None
        };
    let mut resolved_entry_idx = first_entry_idx;
    let mut resolved_entry_ts = event.ts;
    let entry_price =
        if uses_anchor_wick_or_next_touch_entry(&args.paper_outcome_entry_rule_version) {
            let evidence = strategy_signal
                .as_mut()
                .and_then(|signal| signal.evidence.as_mut())
                .ok_or_else(|| "filtered_volume_v11_anchor_entry_evidence_missing".to_string())?;
            let resolved =
                if uses_momentum_exhaustion_limit_entry(&args.paper_outcome_entry_rule_version) {
                    momentum_exhaustion_v2_anchor_entry(
                        candles,
                        first_entry_idx,
                        direction,
                        evidence,
                        args,
                    )?
                } else {
                    let entry_candle = candles
                        .get(first_entry_idx)
                        .ok_or_else(|| "no_entry_outcome_candle".to_string())?;
                    ResolvedAnchorEntry {
                        price: anchor_wick_or_next_touch_entry_price(
                            entry_candle,
                            direction,
                            evidence,
                        )?,
                        candle_idx: first_entry_idx,
                        candle_ts_ms: entry_candle.candle.ts,
                    }
                };
            resolved_entry_idx = resolved.candle_idx;
            resolved_entry_ts = resolved.candle_ts_ms;
            resolved.price
        } else if is_filtered_volume_weekly_base {
            candles
                .get(first_entry_idx)
                .map(|candle| candle.candle.open)
                .filter(|price| price.is_finite() && *price > 0.0)
                .ok_or_else(|| "filtered_volume_v3_next_open_invalid".to_string())?
        } else {
            event.current_price
        };
    let mut structure_stop_loss_price = strategy_signal.as_ref().map(|signal| {
        if is_filtered_volume_weekly_base
            && signal.structure_stop_loss_source == FILTERED_VOLUME_V3_ATR_STOP_SOURCE
        {
            let atr14 = signal
                .evidence
                .as_ref()
                .map(|evidence| evidence.atr14)
                .unwrap_or(0.0);
            return match signal.direction {
                MarketVelocityTradeDirection::Long => {
                    entry_price - atr14 * FILTERED_VOLUME_STOP_ATR_MULTIPLIER
                }
                MarketVelocityTradeDirection::Short => {
                    entry_price + atr14 * FILTERED_VOLUME_STOP_ATR_MULTIPLIER
                }
                MarketVelocityTradeDirection::Both => entry_price,
            };
        }
        signal.structure_stop_loss_price
    });
    let mut structure_stop_loss_source = strategy_signal
        .as_ref()
        .map(|signal| signal.structure_stop_loss_source.clone());
    if is_filtered_volume_weekly_base
        && !structure_stop_loss_price.is_some_and(|stop| {
            stop.is_finite()
                && stop > 0.0
                && match direction {
                    MarketVelocityTradeDirection::Long => stop < entry_price,
                    MarketVelocityTradeDirection::Short => stop > entry_price,
                    MarketVelocityTradeDirection::Both => false,
                }
        })
    {
        let is_pattern_stop =
            structure_stop_loss_source.as_deref() != Some(FILTERED_VOLUME_V3_ATR_STOP_SOURCE);
        if !is_pattern_stop {
            return Err("filtered_volume_v3_atr_stop_invalid_at_fill".to_string());
        }
        // 文档要求形态止损被实际成交价越过时仍记录成交，并立即按成交价退出。
        // 用零风险占位价交给回放层，逐笔结果只计双边成本且不生成虚假的 R。
        structure_stop_loss_price = Some(entry_price);
        structure_stop_loss_source =
            Some(FILTERED_VOLUME_V3_INVALID_AT_FILL_STOP_SOURCE.to_string());
    }
    Ok(ConfirmedEvent {
        event: event.clone(),
        direction,
        entry_ts: resolved_entry_ts,
        entry_price,
        entry_idx: resolved_entry_idx,
        trigger,
        structure_stop_loss_price,
        structure_stop_loss_source,
        entry_signal_evidence: strategy_signal.and_then(|signal| signal.evidence),
    })
}

/// 方向性影线在 p 极值挂限价 12 根；任何更新的有效 p 在收盘后替换旧未成交 setup。
///
/// 每根 K 先判断旧限价是否盘中触及，再在该 K 完成后判断新 setup，避免用收盘后信息
/// 取消同一根 K 内本可成交的旧订单。跳空也固定按 p 极值成交，保持保守且可复现。
fn momentum_exhaustion_v2_anchor_entry(
    candles: &[ComputedCandle],
    first_entry_idx: usize,
    direction: MarketVelocityTradeDirection,
    evidence: &mut CompletedCandleEntrySignalEvidence,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<ResolvedAnchorEntry, String> {
    let anchor = evidence
        .anchor_entry
        .as_ref()
        .ok_or_else(|| "momentum_exhaustion_v2_anchor_entry_evidence_missing".to_string())?;
    if anchor.activation_mode != "directional_wick_limit_12_candles" {
        let entry_candle = candles
            .get(first_entry_idx)
            .ok_or_else(|| "no_entry_outcome_candle".to_string())?;
        return Ok(ResolvedAnchorEntry {
            price: anchor_wick_or_next_touch_entry_price(entry_candle, direction, evidence)?,
            candle_idx: first_entry_idx,
            candle_ts_ms: entry_candle.candle.ts,
        });
    }
    let activation_price = anchor.activation_price;
    if !activation_price.is_finite() || activation_price <= 0.0 {
        return Err("momentum_exhaustion_v2_limit_price_invalid".to_string());
    }
    for offset in 0
        ..super::filtered_volume_rsi_ema_macd::momentum_exhaustion_reversal_v2::MOMENTUM_EXHAUSTION_LIMIT_VALID_CANDLES
    {
        let candle_idx = first_entry_idx + offset;
        let entry_candle = candles
            .get(candle_idx)
            .ok_or_else(|| "momentum_exhaustion_v2_limit_expired_at_data_end".to_string())?;
        let high = entry_candle.candle.high;
        let low = entry_candle.candle.low;
        if !high.is_finite() || high <= 0.0 || !low.is_finite() || low <= 0.0 {
            return Err("momentum_exhaustion_v2_activation_candle_invalid".to_string());
        }
        let touched = match direction {
            MarketVelocityTradeDirection::Long => low <= activation_price,
            MarketVelocityTradeDirection::Short => high >= activation_price,
            MarketVelocityTradeDirection::Both => {
                return Err("momentum_exhaustion_v2_direction_invalid".to_string());
            }
        };
        if touched {
            let anchor = evidence
                .anchor_entry
                .as_mut()
                .ok_or_else(|| "momentum_exhaustion_v2_anchor_entry_evidence_missing".to_string())?;
            anchor.activation_candle_ts_ms = Some(entry_candle.candle.ts);
            anchor.fill_price = Some(activation_price);
            anchor.fill_price_source = Some("directional_wick_limit_at_p_extreme");
            return Ok(ResolvedAnchorEntry {
                price: activation_price,
                candle_idx,
                candle_ts_ms: entry_candle.candle.ts,
            });
        }

        // 新 p 只能在当前 K 完成后可见；若成立，主事件流会单独处理新 setup。
        if completed_candle_entry_signal(candles, candle_idx + 1, args).is_ok() {
            return Err("momentum_exhaustion_v2_pending_replaced_by_new_setup".to_string());
        }
    }
    Err("momentum_exhaustion_v2_limit_not_touched_within_12_candles".to_string())
}

/// 使用紧邻下一根 15m K 线完成 v11 成交；未越过触发线时该 setup 立即过期。
fn anchor_wick_or_next_touch_entry_price(
    entry_candle: &ComputedCandle,
    direction: MarketVelocityTradeDirection,
    evidence: &mut CompletedCandleEntrySignalEvidence,
) -> Result<f64, String> {
    let anchor = evidence
        .anchor_entry
        .as_mut()
        .ok_or_else(|| "filtered_volume_v11_anchor_entry_evidence_missing".to_string())?;
    let open = entry_candle.candle.open;
    let high = entry_candle.candle.high;
    let low = entry_candle.candle.low;
    if !open.is_finite()
        || open <= 0.0
        || !high.is_finite()
        || high <= 0.0
        || !low.is_finite()
        || low <= 0.0
    {
        return Err("filtered_volume_v11_activation_candle_invalid".to_string());
    }
    let (fill_price, fill_source) = match anchor.activation_mode {
        "pivot_directional_wick_next_open" => (open, "next_open_after_directional_wick"),
        "next_candle_intrabar_break" => match direction {
            MarketVelocityTradeDirection::Long if open > anchor.activation_price => {
                (open, "next_open_gap_through_activation")
            }
            MarketVelocityTradeDirection::Long if high > anchor.activation_price => {
                (anchor.activation_price, "intrabar_activation_price")
            }
            MarketVelocityTradeDirection::Short if open < anchor.activation_price => {
                (open, "next_open_gap_through_activation")
            }
            MarketVelocityTradeDirection::Short if low < anchor.activation_price => {
                (anchor.activation_price, "intrabar_activation_price")
            }
            MarketVelocityTradeDirection::Long | MarketVelocityTradeDirection::Short => {
                return Err("filtered_volume_v11_next_candle_activation_not_touched".to_string());
            }
            MarketVelocityTradeDirection::Both => {
                return Err("filtered_volume_v11_direction_invalid".to_string());
            }
        },
        _ => return Err("filtered_volume_v11_activation_mode_invalid".to_string()),
    };
    anchor.activation_candle_ts_ms = Some(entry_candle.candle.ts);
    anchor.fill_price = Some(fill_price);
    anchor.fill_price_source = Some(fill_source);
    Ok(fill_price)
}

/// 找到入场后用于收益/止损模拟的第一根 K 线；该索引只服务 outcome。
fn outcome_start_candle_idx(candles: &[ComputedCandle], entry_ts: i64) -> Option<usize> {
    candles
        .binary_search_by_key(&entry_ts, |item| item.candle.ts)
        .map_or_else(|idx| (idx < candles.len()).then_some(idx), Some)
}

/// 返回信号时点已完成的 15m K 线数量，不读取信号后的 K 线。
fn completed_candle_count(candles: &[ComputedCandle], event_ts: i64) -> usize {
    candles.partition_point(|item| item.candle.ts + MS_15M <= event_ts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::args::{
        market_momentum_exhaustion_reversal_v2_research_args,
        market_momentum_exhaustion_reversal_v3_research_args,
    };
    use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::CompletedCandleEntrySignal;
    use crate::app::market_velocity_event_backtest::{
        effective_target_r_for_confirmed_signal,
        market_filtered_volume_rsi_ema_macd_v3_research_args,
        select_stop_loss_for_confirmed_signal, BacktestCandle,
    };

    /// 构造具备周 `vol_ccy`、EMA696 与 ATR 预热证据的连续回放 K 线。
    fn candle(idx: usize) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: idx as i64 * MS_15M,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
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
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: Some(0.0),
            macd_signal_line: Some(0.0),
            macd_histogram: Some(0.0),
        }
    }

    #[test]
    fn v3_fills_at_next_open_and_reanchors_non_pattern_atr_risk() {
        let mut candles = (0..=700).map(candle).collect::<Vec<_>>();
        let signal_idx = 699;
        let signal_candle = &mut candles[signal_idx];
        signal_candle.candle.open = 100.0;
        signal_candle.candle.high = 102.2;
        signal_candle.candle.low = 99.8;
        signal_candle.candle.close = 102.0;
        signal_candle.candle.volume = 30.0;
        signal_candle.ema12 = Some(100.0);
        signal_candle.ema144 = Some(99.0);
        signal_candle.ema696 = Some(98.0);
        let entry_candle = &mut candles[700];
        entry_candle.candle.open = 104.0;
        entry_candle.candle.high = 105.0;
        entry_candle.candle.low = 103.0;
        entry_candle.candle.close = 104.5;
        let event_ts = 700_i64 * MS_15M;
        let args = market_filtered_volume_rsi_ema_macd_v3_research_args().unwrap();
        let strategy_signal = completed_candle_entry_signal(&candles, 700, &args).unwrap();
        let event = RadarEvent {
            id: 7,
            exchange: "okx".to_string(),
            symbol: "TEST-USDT-SWAP".to_string(),
            ts: event_ts,
            detected_at: "2026-07-22T00:00:00Z".to_string(),
            new_rank: 0,
            delta_rank: 0,
            current_price: 102.0,
            price_change_pct: 2.0,
        };

        let confirmed = immediate_entry_from_signal(
            &event,
            &candles,
            strategy_signal.direction,
            strategy_signal.trigger,
            &args,
        )
        .unwrap();
        let stop = select_stop_loss_for_confirmed_signal(&confirmed, &args);
        let target_r = effective_target_r_for_confirmed_signal(
            &confirmed,
            &candles
                .iter()
                .map(|item| item.candle.clone())
                .collect::<Vec<_>>(),
            stop.stop_loss_pct,
            1.0,
            &args,
        )
        .unwrap();

        assert_eq!(confirmed.entry_price, 104.0);
        assert_eq!(confirmed.entry_idx, 700);
        assert!((stop.price - 101.0).abs() < 1e-12);
        assert_eq!(stop.source, FILTERED_VOLUME_V3_ATR_STOP_SOURCE);
        assert!((target_r - 1.8).abs() < 1e-12);
    }

    #[test]
    fn v3_keeps_an_invalid_pattern_fill_as_an_immediate_cost_only_exit() {
        let mut candles = (0..=700).map(candle).collect::<Vec<_>>();
        let previous = &mut candles[698];
        previous.candle.open = 100.0;
        previous.candle.high = 100.5;
        previous.candle.low = 99.0;
        previous.candle.close = 99.5;
        let signal_candle = &mut candles[699];
        signal_candle.candle.open = 99.4;
        signal_candle.candle.high = 100.4;
        signal_candle.candle.low = 98.8;
        signal_candle.candle.close = 100.2;
        signal_candle.candle.volume = 30.0;
        signal_candle.rsi14 = Some(30.0);
        let entry_candle = &mut candles[700];
        entry_candle.candle.open = 98.0;
        entry_candle.candle.high = 99.0;
        entry_candle.candle.low = 97.5;
        entry_candle.candle.close = 98.5;
        let args = market_filtered_volume_rsi_ema_macd_v3_research_args().unwrap();
        let strategy_signal = completed_candle_entry_signal(&candles, 700, &args).unwrap();
        let event = RadarEvent {
            id: 8,
            exchange: "okx".to_string(),
            symbol: "TEST-USDT-SWAP".to_string(),
            ts: 700_i64 * MS_15M,
            detected_at: "2026-07-22T00:15:00Z".to_string(),
            new_rank: 0,
            delta_rank: 0,
            current_price: 100.2,
            price_change_pct: 0.8,
        };

        let confirmed = immediate_entry_from_signal(
            &event,
            &candles,
            strategy_signal.direction,
            strategy_signal.trigger,
            &args,
        )
        .unwrap();
        let selected = select_stop_loss_for_confirmed_signal(&confirmed, &args);

        assert_eq!(confirmed.entry_price, 98.0);
        assert_eq!(
            confirmed.structure_stop_loss_source.as_deref(),
            Some(FILTERED_VOLUME_V3_INVALID_AT_FILL_STOP_SOURCE)
        );
        assert_eq!(selected.price, confirmed.entry_price);
        assert_eq!(selected.stop_loss_pct, 0.0);
        assert_eq!(
            selected.source,
            FILTERED_VOLUME_V3_INVALID_AT_FILL_STOP_SOURCE
        );
        assert_eq!(
            effective_target_r_for_confirmed_signal(&confirmed, &[], 0.0, 1.0, &args),
            Some(1.0)
        );
    }

    fn momentum_v2_candles(future_candles: usize) -> Vec<ComputedCandle> {
        let completed_count = 750;
        let mut candles = (0..completed_count + future_candles)
            .map(candle)
            .collect::<Vec<_>>();
        let pivot_idx = completed_count - 1;
        let history_start = pivot_idx - 96;
        candles[history_start].candle.open = 100.0;
        candles[pivot_idx - 1].candle.close = 91.0;
        candles[pivot_idx].candle = BacktestCandle {
            ts: pivot_idx as i64 * MS_15M,
            open: 91.0,
            high: 92.0,
            low: 87.5,
            close: 91.8,
            volume: 25.0,
        };
        candles[pivot_idx].volume_ccy = Some(200.0);
        candles
    }

    fn momentum_v2_event_and_signal(
        candles: &[ComputedCandle],
    ) -> (
        MarketVelocityEventBacktestArgs,
        RadarEvent,
        CompletedCandleEntrySignal,
    ) {
        let args = market_momentum_exhaustion_reversal_v2_research_args().unwrap();
        let completed_count = 750;
        let signal = completed_candle_entry_signal(candles, completed_count, &args).unwrap();
        let event = RadarEvent {
            id: 9,
            exchange: "okx".to_string(),
            symbol: "TEST-USDT-SWAP".to_string(),
            ts: completed_count as i64 * MS_15M,
            detected_at: "2026-07-23T00:00:00Z".to_string(),
            new_rank: 0,
            delta_rank: 0,
            current_price: 91.8,
            price_change_pct: 0.0,
        };
        (args, event, signal)
    }

    #[test]
    fn momentum_v2_limit_can_fill_on_twelfth_candle_at_p_extreme() {
        let mut candles = momentum_v2_candles(12);
        let fill_idx = candles.len() - 1;
        candles[fill_idx].candle.open = 86.0;
        candles[fill_idx].candle.high = 90.0;
        candles[fill_idx].candle.low = 85.0;
        candles[fill_idx].candle.close = 88.0;
        let (args, event, signal) = momentum_v2_event_and_signal(&candles);

        let confirmed =
            immediate_entry_from_signal(&event, &candles, signal.direction, signal.trigger, &args)
                .unwrap();
        let selected = select_stop_loss_for_confirmed_signal(&confirmed, &args);
        let target_r = effective_target_r_for_confirmed_signal(
            &confirmed,
            &[],
            selected.stop_loss_pct,
            1.0,
            &args,
        )
        .unwrap();

        assert_eq!(confirmed.entry_idx, fill_idx);
        assert_eq!(confirmed.entry_ts, fill_idx as i64 * MS_15M);
        assert_eq!(confirmed.entry_price, 87.5);
        assert!((selected.price - 84.5).abs() < 1e-12);
        assert!((target_r - 1.8).abs() < 1e-12);
        assert_eq!(
            confirmed
                .entry_signal_evidence
                .as_ref()
                .and_then(|evidence| evidence.anchor_entry.as_ref())
                .and_then(|anchor| anchor.fill_price_source),
            Some("directional_wick_limit_at_p_extreme")
        );
    }

    #[test]
    fn momentum_v2_limit_expires_before_thirteenth_candle_touch() {
        let mut candles = momentum_v2_candles(13);
        let thirteenth_idx = candles.len() - 1;
        candles[thirteenth_idx].candle.low = 87.0;
        let (args, event, signal) = momentum_v2_event_and_signal(&candles);

        assert_eq!(
            immediate_entry_from_signal(&event, &candles, signal.direction, signal.trigger, &args,),
            Err("momentum_exhaustion_v2_limit_not_touched_within_12_candles".to_string())
        );
    }

    #[test]
    fn momentum_v2_new_valid_p_replaces_older_unfilled_limit() {
        let mut candles = momentum_v2_candles(6);
        let replacement_idx = 754;
        candles[replacement_idx - 1].candle.close = 91.0;
        candles[replacement_idx].candle = BacktestCandle {
            ts: replacement_idx as i64 * MS_15M,
            open: 91.0,
            high: 92.0,
            low: 90.0,
            close: 91.5,
            volume: 25.0,
        };
        candles[replacement_idx].volume_ccy = Some(200.0);
        let (args, event, signal) = momentum_v2_event_and_signal(&candles);

        assert_eq!(
            immediate_entry_from_signal(&event, &candles, signal.direction, signal.trigger, &args,),
            Err("momentum_exhaustion_v2_pending_replaced_by_new_setup".to_string())
        );
    }

    #[test]
    fn momentum_v3_virtual_wick55_waits_six_candles_for_p_high() {
        let completed_count = 750;
        let mut candles = (0..completed_count + 6).map(candle).collect::<Vec<_>>();
        let pivot_idx = completed_count - 1;
        candles[pivot_idx - 96].candle.open = 0.53;
        candles[pivot_idx - 1].candle.close = 0.637;
        candles[pivot_idx].candle = BacktestCandle {
            ts: pivot_idx as i64 * MS_15M,
            open: 0.6337,
            high: 0.6452,
            low: 0.6309,
            close: 0.6371,
            volume: 25.0,
        };
        candles[pivot_idx].volume_ccy = Some(200.0);
        candles[pivot_idx].atr14 = Some(0.005);
        for entry_candle in &mut candles[completed_count..completed_count + 5] {
            entry_candle.candle.open = 0.637;
            entry_candle.candle.high = 0.644;
            entry_candle.candle.low = 0.625;
            entry_candle.candle.close = 0.635;
        }
        let fill_idx = completed_count + 5;
        candles[fill_idx].candle.open = 0.64;
        candles[fill_idx].candle.high = 0.6477;
        candles[fill_idx].candle.low = 0.62;
        candles[fill_idx].candle.close = 0.646;

        let args = market_momentum_exhaustion_reversal_v3_research_args().unwrap();
        let signal = completed_candle_entry_signal(&candles, completed_count, &args).unwrap();
        let event = RadarEvent {
            id: 10,
            exchange: "okx".to_string(),
            symbol: "VIRTUAL-USDT-SWAP".to_string(),
            ts: completed_count as i64 * MS_15M,
            detected_at: "2026-07-11T13:15:00Z".to_string(),
            new_rank: 0,
            delta_rank: 0,
            current_price: 0.6371,
            price_change_pct: 0.0,
        };

        let confirmed =
            immediate_entry_from_signal(&event, &candles, signal.direction, signal.trigger, &args)
                .unwrap();

        assert_eq!(
            confirmed.trigger,
            "momentum_exhaustion_upper_wick_limit12_short_v3"
        );
        assert_eq!(confirmed.entry_idx, fill_idx);
        assert_eq!(confirmed.entry_ts, fill_idx as i64 * MS_15M);
        assert_eq!(confirmed.entry_price, 0.6452);
    }
}
