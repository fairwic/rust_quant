use super::indicators::compute_indicators;
use super::model::{
    AnchorUpthrustResearchVariant, BlockedSignal, Candle, Direction, EmaShortResearchVariant,
    EmaTrendLongResearchVariant, EntryIntent, ExitPolicy, ExitReason, Metrics, Position,
    ReplayConfig, ReplayReport, SellClimaxBaseReclaimResearchVariant, StopEntryIntent,
    StrictVisualBreakoutResearchVariant, Trade,
};
use super::signals::SignalState;

const V5_NET_BREAK_EVEN_COST_BPS_PER_SIDE: f64 = 8.0;
const V5_TWO_R_ACTIVATION_MULTIPLE: f64 = 2.0;
const V5_TRAILING_DISTANCE_R: f64 = 2.0;
const V15_PARTIAL_QUANTITY: f64 = 0.33;
const V15_NET_BREAK_EVEN_COST_BPS_PER_SIDE: f64 = 8.0;

/// 严格按确认棒收盘计算、下一根开盘成交的 Pine 对照回放。
pub fn replay(candles: &[Candle], config: ReplayConfig) -> ReplayReport {
    replay_with_research_variants(
        candles,
        config,
        EmaShortResearchVariant::Baseline,
        EmaTrendLongResearchVariant::Baseline,
        SellClimaxBaseReclaimResearchVariant::Baseline,
        AnchorUpthrustResearchVariant::Baseline,
        StrictVisualBreakoutResearchVariant::Baseline,
    )
}

/// 在冻结 Pine 规则上只替换 EMA 空头单变量；该入口不注册到 Paper 或 Live。
pub fn replay_with_ema_short_variant(
    candles: &[Candle],
    config: ReplayConfig,
    ema_short_variant: EmaShortResearchVariant,
) -> ReplayReport {
    replay_with_research_variants(
        candles,
        config,
        ema_short_variant,
        EmaTrendLongResearchVariant::Baseline,
        SellClimaxBaseReclaimResearchVariant::Baseline,
        AnchorUpthrustResearchVariant::Baseline,
        StrictVisualBreakoutResearchVariant::Baseline,
    )
}

/// 在冻结 V19 上只推进 EMA 趋势多的逐层门槛；其他家族保持同源基线。
pub fn replay_with_ema_trend_long_variant(
    candles: &[Candle],
    config: ReplayConfig,
    ema_trend_long_variant: EmaTrendLongResearchVariant,
) -> ReplayReport {
    replay_with_research_variants(
        candles,
        config,
        EmaShortResearchVariant::Baseline,
        ema_trend_long_variant,
        SellClimaxBaseReclaimResearchVariant::Baseline,
        AnchorUpthrustResearchVariant::Baseline,
        StrictVisualBreakoutResearchVariant::Baseline,
    )
}

/// 在 V19 保守趋势多基线上只追加冻结的卖压衰竭收回形态。
pub fn replay_with_sell_climax_base_reclaim_variant(
    candles: &[Candle],
    config: ReplayConfig,
    variant: SellClimaxBaseReclaimResearchVariant,
) -> ReplayReport {
    replay_with_research_variants(
        candles,
        config,
        EmaShortResearchVariant::Baseline,
        EmaTrendLongResearchVariant::ConservativeTargetGap,
        variant,
        AnchorUpthrustResearchVariant::Baseline,
        StrictVisualBreakoutResearchVariant::Baseline,
    )
}

/// 在冻结 V20 上只替换扫高失败家族的锚区或确认时序；不注册到 Paper 或 Live。
pub fn replay_with_anchor_upthrust_variant(
    candles: &[Candle],
    config: ReplayConfig,
    variant: AnchorUpthrustResearchVariant,
) -> ReplayReport {
    replay_with_research_variants(
        candles,
        config,
        EmaShortResearchVariant::Baseline,
        EmaTrendLongResearchVariant::Baseline,
        SellClimaxBaseReclaimResearchVariant::Baseline,
        variant,
        StrictVisualBreakoutResearchVariant::Baseline,
    )
}

/// 在冻结 Candidate V20 上只新增严格视觉横盘上破做多家族。
pub fn replay_with_strict_visual_breakout_variant(
    candles: &[Candle],
    config: ReplayConfig,
    variant: StrictVisualBreakoutResearchVariant,
) -> ReplayReport {
    replay_with_research_variants(
        candles,
        config,
        EmaShortResearchVariant::Baseline,
        EmaTrendLongResearchVariant::Baseline,
        SellClimaxBaseReclaimResearchVariant::Baseline,
        AnchorUpthrustResearchVariant::Baseline,
        variant,
    )
}

/// 共用回放主链路，同时禁止调用方在一次实验里混合两个研究变量。
fn replay_with_research_variants(
    candles: &[Candle],
    config: ReplayConfig,
    ema_short_variant: EmaShortResearchVariant,
    ema_trend_long_variant: EmaTrendLongResearchVariant,
    sell_climax_base_reclaim_variant: SellClimaxBaseReclaimResearchVariant,
    anchor_upthrust_variant: AnchorUpthrustResearchVariant,
    strict_visual_breakout_variant: StrictVisualBreakoutResearchVariant,
) -> ReplayReport {
    debug_assert!(if sell_climax_base_reclaim_variant.is_enabled() {
        ema_short_variant == EmaShortResearchVariant::Baseline
            && ema_trend_long_variant == EmaTrendLongResearchVariant::ConservativeTargetGap
            && !anchor_upthrust_variant.is_enabled()
    } else if strict_visual_breakout_variant.is_enabled() {
        ema_short_variant == EmaShortResearchVariant::Baseline
            && ema_trend_long_variant == EmaTrendLongResearchVariant::Baseline
            && !sell_climax_base_reclaim_variant.is_enabled()
            && !anchor_upthrust_variant.is_enabled()
    } else if anchor_upthrust_variant.is_enabled() {
        ema_short_variant == EmaShortResearchVariant::Baseline
            && ema_trend_long_variant == EmaTrendLongResearchVariant::Baseline
    } else {
        ema_short_variant == EmaShortResearchVariant::Baseline
            || ema_trend_long_variant == EmaTrendLongResearchVariant::Baseline
    });
    let indicators = compute_indicators(candles, config.rule_version);
    let mut state = SignalState::new_with_research_variants(
        ema_short_variant,
        ema_trend_long_variant,
        sell_climax_base_reclaim_variant,
        anchor_upthrust_variant,
        strict_visual_breakout_variant,
    );
    let mut broker = Broker::new(config.clone());
    let mut entry_candidates = Vec::new();

    for (index, candle) in candles.iter().copied().enumerate() {
        if candle.timestamp_ms > config.evaluation_end_ms {
            break;
        }
        broker.fill_open_orders(candle, index);
        if !broker.process_stop_entry(candle, index) {
            broker.process_protective_orders(candle, index);
        }
        broker.update_dynamic_protection_at_close(
            candle,
            indicators.get(index).and_then(|point| point.atr14),
        );

        let entries_enabled = candle.timestamp_ms >= config.evaluation_start_ms
            && candle.timestamp_ms <= config.evaluation_end_ms;
        let evaluation = state.evaluate(
            candles,
            &indicators,
            index,
            config.tick_size,
            broker.position.as_ref().map(|position| position.direction),
            entries_enabled,
            config.rule_version,
        );
        broker.blocked.extend(evaluation.blocked);
        if let Some(intent) = evaluation.intent {
            entry_candidates.push(intent.clone());
            broker.pending_entry = Some(intent);
        }
        if let Some(stop_entry) = evaluation.stop_entry {
            entry_candidates.push(stop_entry.intent.clone());
            broker.pending_stop_entry = Some(stop_entry);
        }
    }

    let mut metrics = summarize(&broker.trades);
    metrics.closed_equity_max_drawdown = metrics.max_drawdown;
    metrics.max_drawdown = broker.max_intrabar_drawdown;
    let open_position_at_end = broker.position.is_some();
    let pending_entry_at_end =
        broker.pending_entry.is_some() || broker.pending_stop_entry.is_some();
    ReplayReport {
        strategy_version: if strict_visual_breakout_variant.is_enabled() {
            strict_visual_breakout_variant.strategy_version(config.rule_version)
        } else if anchor_upthrust_variant.is_enabled() {
            anchor_upthrust_variant.strategy_version(config.rule_version)
        } else if sell_climax_base_reclaim_variant.is_enabled() {
            sell_climax_base_reclaim_variant.strategy_version(config.rule_version)
        } else if ema_trend_long_variant == EmaTrendLongResearchVariant::Baseline {
            ema_short_variant.strategy_version(config.rule_version)
        } else {
            ema_trend_long_variant.strategy_version(config.rule_version)
        },
        pine_source_fnv1a32: config.rule_version.pine_source_fnv1a32(),
        symbol: config.symbol,
        tick_size: config.tick_size,
        evaluation_start_ms: config.evaluation_start_ms,
        evaluation_end_ms: config.evaluation_end_ms,
        fee_bps_per_side: config.fee_bps_per_side,
        slippage_bps_per_side: config.slippage_bps_per_side,
        metrics,
        entry_candidates,
        trades: broker.trades,
        blocked_signals: broker.blocked,
        open_position_at_end,
        pending_entry_at_end,
    }
}

/// Pine broker emulator 的最小状态机；仅模拟固定 1 单位，不连接任何下单或持仓事实源。
#[derive(Debug)]
struct Broker {
    config: ReplayConfig,
    position: Option<Position>,
    pending_entry: Option<EntryIntent>,
    pending_stop_entry: Option<StopEntryIntent>,
    pending_close_reason: Option<ExitReason>,
    trades: Vec<Trade>,
    blocked: Vec<super::model::BlockedSignal>,
    realized_equity: f64,
    closed_equity_peak: f64,
    max_intrabar_drawdown: f64,
}

impl Broker {
    /// 以零权益和空仓启动独立回放，所有成交状态只存在于本次进程内。
    fn new(config: ReplayConfig) -> Self {
        Self {
            config,
            position: None,
            pending_entry: None,
            pending_stop_entry: None,
            pending_close_reason: None,
            trades: Vec::new(),
            blocked: Vec::new(),
            realized_equity: 0.0,
            closed_equity_peak: 0.0,
            max_intrabar_drawdown: 0.0,
        }
    }

    /// 市价平仓和入场都在确认棒之后的下一根开盘执行。
    fn fill_open_orders(&mut self, candle: Candle, _index: usize) {
        if let Some(reason) = self.pending_close_reason.take() {
            self.close_position(candle.open, candle.timestamp_ms, reason);
        }
        let Some(intent) = self.pending_entry.take() else {
            return;
        };
        if candle.timestamp_ms > self.config.evaluation_end_ms {
            return;
        }
        if !strict_visual_breakout_candle_stop_is_valid(&intent, candle.open) {
            // 确认后跳空越过冻结结构止损时不能把保护位反转到持仓另一侧，也不能先平旧仓再失败。
            self.blocked.push(BlockedSignal {
                signal_time_ms: intent.signal_time_ms,
                direction: Some(intent.direction),
                reason: "STRICT_VISUAL_BREAKOUT_CANDLE_STOP_INVALID_AT_ENTRY".to_owned(),
            });
            return;
        }
        if self.position.is_some() {
            self.close_position(
                candle.open,
                candle.timestamp_ms,
                ExitReason::ReverseAtNextOpen,
            );
        }
        self.position = Some(position_from_intent(
            intent,
            candle.open,
            candle.timestamp_ms,
            self.config.tick_size,
        ));
        self.record_intrabar_drawdown(candle.open);
    }

    /// V16 stop entry 在接受棒后的有限窗口内按 TradingView OHLC 路径触发。
    fn process_stop_entry(&mut self, candle: Candle, index: usize) -> bool {
        let Some(order) = self.pending_stop_entry.take() else {
            return false;
        };
        if candle.timestamp_ms > self.config.evaluation_end_ms {
            return false;
        }
        if self.position.is_some() {
            self.blocked.push(stop_entry_blocked(
                candle.timestamp_ms,
                order.intent.direction,
                "V16_TRIGGER_CANCELLED_BY_EXISTING_POSITION",
            ));
            return false;
        }
        if index > order.expires_at_index {
            self.blocked.push(stop_entry_blocked(
                candle.timestamp_ms,
                order.intent.direction,
                "V16_TRIGGER_WINDOW_EXPIRED",
            ));
            return false;
        }

        let path = broker_path(candle);
        if stop_entry_marketable_at_open(order.intent.direction, candle.open, order.trigger_price) {
            self.position = Some(position_from_intent(
                order.intent,
                candle.open,
                candle.timestamp_ms,
                self.config.tick_size,
            ));
            self.process_protective_orders(candle, index);
            return true;
        }

        for (segment_index, segment) in path.windows(2).enumerate() {
            if !between(order.trigger_price, segment[0], segment[1]) {
                continue;
            }
            self.position = Some(position_from_intent(
                order.intent,
                order.trigger_price,
                candle.timestamp_ms,
                self.config.tick_size,
            ));
            self.record_intrabar_drawdown(order.trigger_price);
            let mut remaining_path = Vec::with_capacity(path.len() - segment_index);
            remaining_path.push(order.trigger_price);
            if segment[1] != order.trigger_price {
                remaining_path.push(segment[1]);
            }
            remaining_path.extend_from_slice(&path[segment_index + 2..]);
            self.process_position_path(candle.timestamp_ms, &remaining_path, false);
            return true;
        }

        let stop = order
            .intent
            .stop_price
            .expect("V16 stop entry always freezes an absolute structural stop");
        let boundary = order
            .intent
            .breakout_line
            .expect("V16 stop entry always freezes a breakout boundary");
        let structure_invalidated = match order.intent.direction {
            Direction::Long => candle.low <= stop,
            Direction::Short => candle.high >= stop,
        };
        let close_reentered = match order.intent.direction {
            Direction::Long => candle.close <= boundary,
            Direction::Short => candle.close >= boundary,
        };
        let reason = if structure_invalidated {
            Some("V16_TRIGGER_STRUCTURE_INVALIDATED_BEFORE_FILL")
        } else if close_reentered {
            Some("V16_TRIGGER_CLOSE_REENTERED_FROZEN_BOX")
        } else if index >= order.expires_at_index {
            Some("V16_TRIGGER_WINDOW_EXPIRED")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.blocked.push(stop_entry_blocked(
                candle.timestamp_ms,
                order.intent.direction,
                reason,
            ));
        } else {
            self.pending_stop_entry = Some(order);
        }
        false
    }

    /// 使用 TradingView 默认 broker emulator 的 OHLC 路径处理保护单。
    fn process_protective_orders(&mut self, candle: Candle, _index: usize) {
        if self.position.is_none() {
            return;
        }
        // Pine 在信号棒预挂结构止损/最终目标；实际 1R 只能在下一开盘成交后计算，
        // 因而成交当根只执行完整结构保护，分批从后续 K 线开始。
        let allow_range_partial = self
            .position
            .as_ref()
            .is_some_and(|position| candle.timestamp_ms > position.entry_time_ms);
        self.record_intrabar_drawdown(candle.open);
        let marketable = self
            .position
            .as_ref()
            .and_then(|position| marketable_at_open(position, candle.open));
        if let Some((price, reason)) = marketable {
            self.close_position(price, candle.timestamp_ms, reason);
            return;
        }
        if self.position.as_ref().is_some_and(|position| {
            allow_range_partial && range_partial_marketable_at_open(position, candle.open)
        }) {
            self.take_range_partial(candle.open);
        }

        let path = broker_path(candle);
        self.process_position_path(candle.timestamp_ms, &path, allow_range_partial);
    }

    /// 从持仓已经存在的路径位置继续处理保护单，避免 V16 把入场前价格误算为 MAE。
    fn process_position_path(
        &mut self,
        timestamp_ms: i64,
        path: &[f64],
        allow_range_partial: bool,
    ) {
        'segments: for segment in path.windows(2) {
            if self.position.is_none() {
                break;
            }
            let mut cursor = segment[0];
            loop {
                let triggered = self.position.as_ref().and_then(|position| {
                    first_action_on_segment(position, cursor, segment[1], allow_range_partial)
                });
                match triggered {
                    Some(ProtectiveAction::Close(price, reason)) => {
                        self.record_intrabar_drawdown(price);
                        self.close_position(price, timestamp_ms, reason);
                        break 'segments;
                    }
                    Some(ProtectiveAction::RangePartial(price)) => {
                        self.record_intrabar_drawdown(price);
                        self.take_range_partial(price);
                        cursor = price;
                    }
                    None => {
                        self.record_intrabar_drawdown(segment[1]);
                        break;
                    }
                }
            }
        }
    }

    /// 动态保本只在整根 15m K 线完成后生效，避免用未知的棒内先后次序回填成交。
    fn update_dynamic_protection_at_close(&mut self, candle: Candle, atr: Option<f64>) {
        let Some(position) = self.position.as_mut() else {
            return;
        };
        if position.exit_policy == ExitPolicy::RangeSqueezeStaged {
            if let Some(reason) =
                update_range_squeeze_v15_protection(position, candle, atr, self.config.tick_size)
            {
                self.pending_close_reason = Some(reason);
            }
            return;
        }
        if position.exit_policy == ExitPolicy::RsiCounterTrendAgeV5 {
            if let Some(reason) =
                update_rsi_counter_trend_v5_protection(position, candle, self.config.tick_size)
            {
                self.pending_close_reason = Some(reason);
            }
            return;
        }
        if position.exit_policy == ExitPolicy::CounterTrendStructureV4 {
            if update_counter_trend_v4_trailing_stop(position, candle, self.config.tick_size) {
                self.pending_close_reason = Some(ExitReason::CounterTrendTrailingStop);
            }
            return;
        }
        let Some(activation_price) = position.activation_price else {
            return;
        };
        if !position.activated {
            position.activated = match position.direction {
                Direction::Long => candle.high >= activation_price,
                Direction::Short => candle.low <= activation_price,
            };
        }
        if !position.activated {
            return;
        }

        let lock_stop = match position.direction {
            Direction::Long => round_down(
                position.entry_price + self.config.tick_size,
                self.config.tick_size,
            ),
            Direction::Short => round_up(
                position.entry_price - self.config.tick_size,
                self.config.tick_size,
            ),
        };
        let close_has_crossed_lock = match position.direction {
            Direction::Long => candle.close <= lock_stop,
            Direction::Short => candle.close >= lock_stop,
        };
        if close_has_crossed_lock {
            self.pending_close_reason = Some(break_even_reason(position.exit_policy));
        } else {
            position.stop = lock_stop;
            position.target = position.final_target;
        }
    }

    /// V15 首次触及实际 1R 时只结算 33%；其余仓位继续使用冻结结构保护。
    fn take_range_partial(&mut self, raw_exit_price: f64) {
        let Some(position) = self.position.as_mut() else {
            return;
        };
        if position.exit_policy != ExitPolicy::RangeSqueezeStaged
            || position.range_partial_one_r_taken
        {
            return;
        }
        let quantity = V15_PARTIAL_QUANTITY.min(position.remaining_quantity);
        let cost_bps = self.config.fee_bps_per_side + self.config.slippage_bps_per_side;
        let gross_pnl = position
            .direction
            .gross_pnl(position.entry_price, raw_exit_price)
            * quantity;
        let costs = (position.entry_price + raw_exit_price) * quantity * cost_bps / 10_000.0;
        let net_pnl = gross_pnl - costs;
        position.remaining_quantity -= quantity;
        position.realized_quantity += quantity;
        position.realized_gross_pnl += gross_pnl;
        position.realized_exit_notional += raw_exit_price * quantity;
        position.realized_exit_cost += raw_exit_price * quantity * cost_bps / 10_000.0;
        position.realized_net_pnl += net_pnl;
        position.range_partial_one_r_taken = true;
        self.realized_equity += net_pnl;
        self.closed_equity_peak = self.closed_equity_peak.max(self.realized_equity);
    }

    /// 聚合已分批和最终退出，仍以一次入场输出一笔交易，避免虚增独立样本数。
    fn close_position(&mut self, raw_exit_price: f64, exit_time_ms: i64, reason: ExitReason) {
        self.record_intrabar_drawdown(raw_exit_price);
        let Some(position) = self.position.take() else {
            return;
        };
        let final_gross_pnl = position
            .direction
            .gross_pnl(position.entry_price, raw_exit_price)
            * position.remaining_quantity;
        let cost_bps = self.config.fee_bps_per_side + self.config.slippage_bps_per_side;
        let final_costs =
            (position.entry_price + raw_exit_price) * position.remaining_quantity * cost_bps
                / 10_000.0;
        let final_net_pnl = final_gross_pnl - final_costs;
        let gross_pnl = position.realized_gross_pnl + final_gross_pnl;
        let net_pnl = position.realized_net_pnl + final_net_pnl;
        self.realized_equity += final_net_pnl;
        self.max_intrabar_drawdown = self
            .max_intrabar_drawdown
            .max(self.closed_equity_peak - self.realized_equity);
        self.closed_equity_peak = self.closed_equity_peak.max(self.realized_equity);
        let exit_price = (position.realized_exit_notional
            + raw_exit_price * position.remaining_quantity)
            / (position.realized_quantity + position.remaining_quantity);
        let initial_risk = (position.entry_price - position.initial_stop).abs();
        let net_r = if initial_risk > 0.0 {
            net_pnl / initial_risk
        } else {
            0.0
        };
        self.trades.push(Trade {
            direction: position.direction,
            families: position.families,
            exit_policy: position.exit_policy,
            signal_counter_trend_ema_age_bars_capped_600: position
                .signal_counter_trend_ema_age_bars_capped_600,
            counter_trend_structure_breakout_line: position.counter_trend_structure_breakout_line,
            counter_trend_structure_confirmed: position.counter_trend_structure_confirmed,
            counter_trend_two_r_trailing_activated: position.counter_trend_two_r_trailing_activated,
            range_partial_one_r_taken: position.range_partial_one_r_taken,
            range_two_r_trailing_activated: position.range_two_r_trailing_activated,
            signal_time_ms: position.signal_time_ms,
            entry_time_ms: position.entry_time_ms,
            exit_time_ms,
            entry_price: position.entry_price,
            exit_price,
            initial_stop: position.initial_stop,
            exit_reason: reason,
            gross_pnl,
            net_pnl,
            initial_risk,
            net_r,
            anchor_upthrust_target_consumption_ratio: position
                .anchor_upthrust_target_consumption_ratio,
            volume_ratio: position.volume_ratio,
            rsi: position.rsi,
        });
    }

    /// 复刻 TradingView：峰值只取入场前已平仓权益，持仓棒再叠加可达的最差价格偏移。
    fn record_intrabar_drawdown(&mut self, price: f64) {
        let Some(position) = self.position.as_ref() else {
            return;
        };
        let adverse_excursion = match position.direction {
            Direction::Long => (position.entry_price - price).max(0.0),
            Direction::Short => (price - position.entry_price).max(0.0),
        } * position.remaining_quantity;
        let drawdown = self.closed_equity_peak - self.realized_equity + adverse_excursion;
        self.max_intrabar_drawdown = self.max_intrabar_drawdown.max(drawdown);
    }
}

/// V11/V12 只允许在下一开盘仍位于冻结结构保护位有利一侧时建立仓位。
fn strict_visual_breakout_candle_stop_is_valid(intent: &EntryIntent, entry_price: f64) -> bool {
    if !intent.strict_visual_breakout_candle_extreme_stop {
        return true;
    }
    intent
        .stop_price
        .is_some_and(|stop| match intent.direction {
            Direction::Long => stop < entry_price,
            Direction::Short => stop > entry_price,
        })
}

/// 在下一根实际开盘价已知后，把冻结结构价和最小风险 tick 合成为最终保护单。
fn position_from_intent(
    intent: EntryIntent,
    entry_price: f64,
    entry_time_ms: i64,
    tick_size: f64,
) -> Position {
    let stop = match intent.stop_price {
        Some(structural_stop) if intent.strict_visual_breakout_candle_extreme_stop => {
            // V12 只能把 V11 结构止损向外扩展；结构本身更远时必须原样保留。
            intent.stop_ticks.map_or(structural_stop, |ticks| {
                let minimum_risk_stop = match intent.direction {
                    Direction::Long => entry_price - ticks as f64 * tick_size,
                    Direction::Short => entry_price + ticks as f64 * tick_size,
                };
                match intent.direction {
                    Direction::Long => structural_stop.min(minimum_risk_stop),
                    Direction::Short => structural_stop.max(minimum_risk_stop),
                }
            })
        }
        Some(structural_stop) => structural_stop,
        None => {
            let ticks = intent
                .stop_ticks
                .expect("every non-pattern intent freezes stop ticks");
            match intent.direction {
                Direction::Long => entry_price - ticks as f64 * tick_size,
                Direction::Short => entry_price + ticks as f64 * tick_size,
            }
        }
    };
    let target = intent.target_price.or_else(|| {
        intent.target_ticks.map(|ticks| match intent.direction {
            Direction::Long => entry_price + ticks as f64 * tick_size,
            Direction::Short => entry_price - ticks as f64 * tick_size,
        })
    });
    let activation_price = if intent.exit_policy == ExitPolicy::CounterTrendStructureV4 {
        let initial_risk = (entry_price - stop).abs();
        (initial_risk > 0.0).then(|| match intent.direction {
            Direction::Long => entry_price + initial_risk,
            Direction::Short => entry_price - initial_risk,
        })
    } else {
        intent.activation_ticks.map(|ticks| match intent.direction {
            Direction::Long => entry_price + ticks as f64 * tick_size,
            Direction::Short => entry_price - ticks as f64 * tick_size,
        })
    };
    let initial_risk = (entry_price - stop).abs();
    let (range_one_r_price, range_two_r_price) =
        if intent.exit_policy == ExitPolicy::RangeSqueezeStaged && initial_risk > 0.0 {
            (
                Some(match intent.direction {
                    Direction::Long => entry_price + initial_risk,
                    Direction::Short => entry_price - initial_risk,
                }),
                Some(match intent.direction {
                    Direction::Long => entry_price + 2.0 * initial_risk,
                    Direction::Short => entry_price - 2.0 * initial_risk,
                }),
            )
        } else {
            (None, None)
        };

    Position {
        direction: intent.direction,
        entry_time_ms,
        entry_price,
        signal_time_ms: intent.signal_time_ms,
        families: intent.families,
        initial_stop: stop,
        stop,
        target,
        final_target: target,
        activation_price,
        exit_policy: intent.exit_policy,
        activated: false,
        signal_counter_trend_ema_age_bars_capped_600: intent
            .signal_counter_trend_ema_age_bars_capped_600,
        counter_trend_structure_breakout_line: intent.counter_trend_structure_breakout_line,
        counter_trend_structure_confirmed: false,
        counter_trend_two_r_trailing_activated: false,
        highest_high_since_entry: entry_price,
        lowest_low_since_entry: entry_price,
        range_boundary: (intent.exit_policy == ExitPolicy::RangeSqueezeStaged)
            .then_some(intent.breakout_line)
            .flatten(),
        remaining_quantity: 1.0,
        realized_quantity: 0.0,
        realized_gross_pnl: 0.0,
        realized_exit_notional: 0.0,
        realized_exit_cost: 0.0,
        realized_net_pnl: 0.0,
        range_one_r_price,
        range_two_r_price,
        range_partial_one_r_taken: false,
        range_one_r_close_confirmed: false,
        range_two_r_trailing_activated: false,
        anchor_upthrust_target_consumption_ratio: intent.anchor_upthrust_target_consumption_ratio,
        volume_ratio: intent.volume_ratio,
        rsi: intent.rsi,
    }
}

/// V15 只在完成棒收盘确认 1R/2R；盘中触及 1R 只执行部分止盈，不提前抬止损。
fn update_range_squeeze_v15_protection(
    position: &mut Position,
    candle: Candle,
    atr: Option<f64>,
    tick_size: f64,
) -> Option<ExitReason> {
    position.highest_high_since_entry = position.highest_high_since_entry.max(candle.high);
    position.lowest_low_since_entry = position.lowest_low_since_entry.min(candle.low);
    let Some(boundary) = position.range_boundary else {
        return None;
    };
    let reentered = match position.direction {
        Direction::Long => candle.close <= boundary,
        Direction::Short => candle.close >= boundary,
    };
    if reentered {
        return Some(ExitReason::RangeSqueezeBoxReentry);
    }

    if !position.range_one_r_close_confirmed {
        position.range_one_r_close_confirmed =
            position
                .range_one_r_price
                .is_some_and(|one_r| match position.direction {
                    Direction::Long => candle.close >= one_r,
                    Direction::Short => candle.close <= one_r,
                });
    }
    if position.range_one_r_close_confirmed {
        let net_break_even =
            v15_net_break_even_price(position.direction, position.entry_price, tick_size);
        position.stop = match position.direction {
            Direction::Long => position.stop.max(net_break_even),
            Direction::Short => position.stop.min(net_break_even),
        };
    }

    if !position.range_two_r_trailing_activated {
        position.range_two_r_trailing_activated =
            position
                .range_two_r_price
                .is_some_and(|two_r| match position.direction {
                    Direction::Long => candle.close >= two_r,
                    Direction::Short => candle.close <= two_r,
                });
    }
    if position.range_two_r_trailing_activated {
        if let Some(atr) = atr.filter(|value| *value > 0.0) {
            position.stop = match position.direction {
                Direction::Long => position.stop.max(round_down(
                    position.highest_high_since_entry - atr,
                    tick_size,
                )),
                Direction::Short => position
                    .stop
                    .min(round_up(position.lowest_low_since_entry + atr, tick_size)),
            };
        }
    }

    let close_crossed_stop = match position.direction {
        Direction::Long => candle.close <= position.stop,
        Direction::Short => candle.close >= position.stop,
    };
    close_crossed_stop.then_some(range_dynamic_stop_reason(position))
}

/// 固定 8bps/边与 Pine 默认输入一致；压力报告仍另外按配置结算真实回放成本。
fn v15_net_break_even_price(direction: Direction, entry_price: f64, tick_size: f64) -> f64 {
    let cost_ratio = V15_NET_BREAK_EVEN_COST_BPS_PER_SIDE / 10_000.0;
    match direction {
        Direction::Long => round_up(
            entry_price * (1.0 + cost_ratio) / (1.0 - cost_ratio),
            tick_size,
        ),
        Direction::Short => round_down(
            entry_price * (1.0 - cost_ratio) / (1.0 + cost_ratio),
            tick_size,
        ),
    }
}

fn range_dynamic_stop_reason(position: &Position) -> ExitReason {
    if position.range_two_r_trailing_activated {
        ExitReason::RangeSqueezeAtrTrailingStop
    } else {
        ExitReason::RangeSqueezeNetBreakEven
    }
}

/// V5 先等待完成棒收回冻结近边，再锁定固定成本净保本；只有结构确认后才允许 2R 宽追踪。
fn update_rsi_counter_trend_v5_protection(
    position: &mut Position,
    candle: Candle,
    tick_size: f64,
) -> Option<ExitReason> {
    position.highest_high_since_entry = position.highest_high_since_entry.max(candle.high);
    position.lowest_low_since_entry = position.lowest_low_since_entry.min(candle.low);
    let initial_risk = (position.entry_price - position.initial_stop).abs();
    if initial_risk <= 0.0 {
        return None;
    }

    let Some(structure_line) = position.counter_trend_structure_breakout_line else {
        return None;
    };
    if !position.counter_trend_structure_confirmed {
        position.counter_trend_structure_confirmed = match position.direction {
            Direction::Long => candle.close > structure_line,
            Direction::Short => candle.close < structure_line,
        };
    }
    if !position.counter_trend_structure_confirmed {
        return None;
    }

    position.activated = true;
    let net_break_even =
        v5_net_break_even_price(position.direction, position.entry_price, tick_size);
    let mfe = match position.direction {
        Direction::Long => position.highest_high_since_entry - position.entry_price,
        Direction::Short => position.entry_price - position.lowest_low_since_entry,
    };
    if mfe >= V5_TWO_R_ACTIVATION_MULTIPLE * initial_risk {
        position.counter_trend_two_r_trailing_activated = true;
    }

    position.stop = match position.direction {
        Direction::Long => {
            let trailing = round_down(
                position.highest_high_since_entry - V5_TRAILING_DISTANCE_R * initial_risk,
                tick_size,
            );
            if position.counter_trend_two_r_trailing_activated {
                position.stop.max(net_break_even).max(trailing)
            } else {
                position.stop.max(net_break_even)
            }
        }
        Direction::Short => {
            let trailing = round_up(
                position.lowest_low_since_entry + V5_TRAILING_DISTANCE_R * initial_risk,
                tick_size,
            );
            if position.counter_trend_two_r_trailing_activated {
                position.stop.min(net_break_even).min(trailing)
            } else {
                position.stop.min(net_break_even)
            }
        }
    };

    let close_crossed_stop = match position.direction {
        Direction::Long => candle.close <= position.stop,
        Direction::Short => candle.close >= position.stop,
    };
    close_crossed_stop.then_some(v5_dynamic_stop_reason(position))
}

/// 用固定 8bps/边计算价格层面的净保本，避免零成本与压力回放改变保护触发价。
fn v5_net_break_even_price(direction: Direction, entry_price: f64, tick_size: f64) -> f64 {
    let cost_ratio = V5_NET_BREAK_EVEN_COST_BPS_PER_SIDE / 10_000.0;
    match direction {
        Direction::Long => round_up(
            entry_price * (1.0 + cost_ratio) / (1.0 - cost_ratio),
            tick_size,
        ),
        Direction::Short => round_down(
            entry_price * (1.0 - cost_ratio) / (1.0 + cost_ratio),
            tick_size,
        ),
    }
}

/// V5 动态止损原因随已完成棒状态冻结，逐笔报告可区分净保本和 2R 宽追踪。
fn v5_dynamic_stop_reason(position: &Position) -> ExitReason {
    if position.counter_trend_two_r_trailing_activated {
        ExitReason::RsiCounterTrendTwoRTrailingStop
    } else {
        ExitReason::RsiCounterTrendNetBreakEven
    }
}

/// V4 只用已完成棒更新 MFE 与保护位；返回 `true` 表示收盘已越过新保护，须下一开盘退出。
fn update_counter_trend_v4_trailing_stop(
    position: &mut Position,
    candle: Candle,
    tick_size: f64,
) -> bool {
    position.highest_high_since_entry = position.highest_high_since_entry.max(candle.high);
    position.lowest_low_since_entry = position.lowest_low_since_entry.min(candle.low);
    let initial_risk = (position.entry_price - position.initial_stop).abs();
    if initial_risk <= 0.0 {
        return false;
    }

    if !position.activated {
        position.activated = match position.direction {
            Direction::Long => {
                position.highest_high_since_entry - position.entry_price >= initial_risk
            }
            Direction::Short => {
                position.entry_price - position.lowest_low_since_entry >= initial_risk
            }
        };
    }
    if !position.activated {
        return false;
    }

    let next_stop = match position.direction {
        Direction::Long => {
            let break_even = round_down(position.entry_price + tick_size, tick_size);
            let trailing = round_down(position.highest_high_since_entry - initial_risk, tick_size);
            position.stop.max(break_even).max(trailing)
        }
        Direction::Short => {
            let break_even = round_up(position.entry_price - tick_size, tick_size);
            let trailing = round_up(position.lowest_low_since_entry + initial_risk, tick_size);
            position.stop.min(break_even).min(trailing)
        }
    };
    position.stop = next_stop;
    match position.direction {
        Direction::Long => candle.close <= next_stop,
        Direction::Short => candle.close >= next_stop,
    }
}

/// 处理跳空越过保护价的情况；TradingView 此时以该根开盘价成交，而非回填挂单价。
fn marketable_at_open(position: &Position, open: f64) -> Option<(f64, ExitReason)> {
    match position.direction {
        Direction::Long if open <= position.stop => Some((open, stop_reason(position))),
        Direction::Long if position.target.is_some_and(|target| open >= target) => Some((
            open,
            profit_reason(position.exit_policy, position.activated),
        )),
        Direction::Short if open >= position.stop => Some((open, stop_reason(position))),
        Direction::Short if position.target.is_some_and(|target| open <= target) => Some((
            open,
            profit_reason(position.exit_policy, position.activated),
        )),
        _ => None,
    }
}

/// 单调价格路径上的保护动作；部分止盈不会结束本次持仓。
#[derive(Debug, Clone, Copy, PartialEq)]
enum ProtectiveAction {
    Close(f64, ExitReason),
    RangePartial(f64),
}

/// 后续 K 线若跳空越过 1R，则部分仓位按该根实际开盘成交。
fn range_partial_marketable_at_open(position: &Position, open: f64) -> bool {
    position.exit_policy == ExitPolicy::RangeSqueezeStaged
        && !position.range_partial_one_r_taken
        && position
            .range_one_r_price
            .is_some_and(|one_r| match position.direction {
                Direction::Long => open >= one_r,
                Direction::Short => open <= one_r,
            })
}

/// 在一段单调价格路径上选择最先遇到的止损、1R 部分目标或最终目标。
fn first_action_on_segment(
    position: &Position,
    start: f64,
    end: f64,
    allow_range_partial: bool,
) -> Option<ProtectiveAction> {
    let mut candidates = Vec::with_capacity(3);
    if between(position.stop, start, end) {
        candidates.push(ProtectiveAction::Close(
            position.stop,
            stop_reason(position),
        ));
    }
    if position.exit_policy == ExitPolicy::RangeSqueezeStaged
        && !position.range_partial_one_r_taken
        && allow_range_partial
    {
        if let Some(one_r) = position
            .range_one_r_price
            .filter(|one_r| between(*one_r, start, end))
        {
            candidates.push(ProtectiveAction::RangePartial(one_r));
        }
    }
    if let Some(target) = position
        .target
        .filter(|target| between(*target, start, end))
    {
        candidates.push(ProtectiveAction::Close(
            target,
            profit_reason(position.exit_policy, position.activated),
        ));
    }
    if end >= start {
        candidates
            .into_iter()
            .min_by(|left, right| action_price(*left).total_cmp(&action_price(*right)))
    } else {
        candidates
            .into_iter()
            .max_by(|left, right| action_price(*left).total_cmp(&action_price(*right)))
    }
}

/// 抽取保护动作价格，供单调路径按实际旅行顺序选择首个触发。
fn action_price(action: ProtectiveAction) -> f64 {
    match action {
        ProtectiveAction::Close(price, _) | ProtectiveAction::RangePartial(price) => price,
    }
}

/// 旧测试和非 V15 路径仍可直接检查最近的完整退出保护价。
#[cfg(test)]
fn first_order_on_segment(position: &Position, start: f64, end: f64) -> Option<(f64, ExitReason)> {
    match first_action_on_segment(position, start, end, true) {
        Some(ProtectiveAction::Close(price, reason)) => Some((price, reason)),
        Some(ProtectiveAction::RangePartial(_)) | None => None,
    }
}

fn between(level: f64, start: f64, end: f64) -> bool {
    if end >= start {
        level > start && level <= end
    } else {
        level < start && level >= end
    }
}

/// TradingView 默认 broker emulator 先走离开盘价更近的一侧。
fn broker_path(candle: Candle) -> [f64; 4] {
    if (candle.open - candle.high).abs() < (candle.open - candle.low).abs() {
        [candle.open, candle.high, candle.low, candle.close]
    } else {
        [candle.open, candle.low, candle.high, candle.close]
    }
}

/// 跳空越过 stop entry 时按该根实际开盘成交。
fn stop_entry_marketable_at_open(direction: Direction, open: f64, trigger: f64) -> bool {
    match direction {
        Direction::Long => open >= trigger,
        Direction::Short => open <= trigger,
    }
}

/// 统一生成 V16 挂单撤销证据，避免把未成交候选计入交易。
fn stop_entry_blocked(timestamp_ms: i64, direction: Direction, reason: &str) -> BlockedSignal {
    BlockedSignal {
        signal_time_ms: timestamp_ms,
        direction: Some(direction),
        reason: reason.to_owned(),
    }
}

/// 激活保护后触及 stop 需保留具体保本原因，便于逐笔对照 Pine 注释。
fn stop_reason(position: &Position) -> ExitReason {
    if position.exit_policy == ExitPolicy::RangeSqueezeStaged
        && position.range_one_r_close_confirmed
    {
        return range_dynamic_stop_reason(position);
    }
    if position.exit_policy == ExitPolicy::RsiCounterTrendAgeV5
        && position.counter_trend_structure_confirmed
    {
        return v5_dynamic_stop_reason(position);
    }
    if position.activated {
        return break_even_reason(position.exit_policy);
    }
    ExitReason::StopLoss
}

/// 把共享目标触发映射回各策略家族的审计原因。
fn profit_reason(policy: ExitPolicy, _activated: bool) -> ExitReason {
    match policy {
        ExitPolicy::CounterTrendStructure | ExitPolicy::CounterTrendStructureV4 => {
            ExitReason::StructureTakeProfit
        }
        ExitPolicy::RsiCounterTrendAgeV5 => ExitReason::RsiCounterTrendStructureTakeProfit,
        ExitPolicy::DivergenceRegime => ExitReason::DivergenceTakeProfit,
        ExitPolicy::ThreeBarEngulfing => ExitReason::EngulfingTakeProfit,
        ExitPolicy::ShortTrendExtension => ExitReason::TrendExtensionTakeProfit,
        ExitPolicy::EffortNoResult => ExitReason::EffortNoResultTakeProfit,
        ExitPolicy::BollingerLowerReclaim => ExitReason::BollingerLowerReclaimTakeProfit,
        ExitPolicy::Ema596ReclaimDeparture => ExitReason::Ema596ReclaimDepartureTakeProfit,
        ExitPolicy::RangeSqueezeStaged => ExitReason::RangeSqueezeTakeProfit,
        ExitPolicy::Fixed => ExitReason::TakeProfit,
    }
}

/// 只有定义了动态保护的家族具有专用保本原因，其余策略仍归类为普通止损。
fn break_even_reason(policy: ExitPolicy) -> ExitReason {
    match policy {
        ExitPolicy::DivergenceRegime => ExitReason::DivergenceBreakEven,
        ExitPolicy::ThreeBarEngulfing => ExitReason::EngulfingBreakEven,
        ExitPolicy::ShortTrendExtension => ExitReason::TrendExtensionBreakEven,
        ExitPolicy::EffortNoResult => ExitReason::EffortNoResultBreakEven,
        ExitPolicy::CounterTrendStructureV4 => ExitReason::CounterTrendTrailingStop,
        ExitPolicy::RsiCounterTrendAgeV5 => ExitReason::RsiCounterTrendNetBreakEven,
        ExitPolicy::RangeSqueezeStaged => ExitReason::RangeSqueezeNetBreakEven,
        ExitPolicy::Fixed
        | ExitPolicy::CounterTrendStructure
        | ExitPolicy::BollingerLowerReclaim
        | ExitPolicy::Ema596ReclaimDeparture => ExitReason::StopLoss,
    }
}

/// 汇总已平仓交易；未结仓位不进入 TradingView closed-trade 指标。
fn summarize(trades: &[Trade]) -> Metrics {
    let mut metrics = Metrics::default();
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    for trade in trades {
        metrics.trades += 1;
        metrics.net_pnl += trade.net_pnl;
        if trade.net_pnl > 0.0 {
            metrics.wins += 1;
            metrics.gross_profit += trade.net_pnl;
        } else if trade.net_pnl < 0.0 {
            metrics.losses += 1;
            metrics.gross_loss += -trade.net_pnl;
        }
        metrics.average_net_r += trade.net_r;
        equity += trade.net_pnl;
        peak = peak.max(equity);
        metrics.max_drawdown = metrics.max_drawdown.max(peak - equity);
    }
    if metrics.trades > 0 {
        metrics.win_rate_percent = metrics.wins as f64 / metrics.trades as f64 * 100.0;
        metrics.average_net_r /= metrics.trades as f64;
    }
    if metrics.gross_loss > 0.0 {
        metrics.profit_factor = Some(metrics.gross_profit / metrics.gross_loss);
    } else if metrics.gross_profit > 0.0 {
        metrics.profit_factor = Some(f64::INFINITY);
    }
    metrics
}

fn round_down(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).floor() * tick_size
}

fn round_up(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).ceil() * tick_size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tradingview_velocity_parity::model::SignalFamily;

    fn intent(direction: Direction) -> EntryIntent {
        EntryIntent {
            signal_index: 0,
            signal_time_ms: 0,
            direction,
            families: vec![SignalFamily::EmaTrendLong],
            signal_close: 100.0,
            signal_atr: 2.0,
            stop_price: None,
            stop_ticks: Some(30),
            target_price: None,
            target_ticks: Some(60),
            activation_ticks: None,
            exit_policy: ExitPolicy::Fixed,
            counter_trend: false,
            signal_counter_trend_ema_age_bars_capped_600: None,
            counter_trend_structure_breakout_line: None,
            anchor_upthrust_target_consumption_ratio: None,
            active_parent_horizontal_anchor: None,
            strict_visual_range_length_bars: None,
            strict_visual_range_height: None,
            strict_visual_short_range_one_r_target: None,
            strict_visual_breakout_candle_extreme_stop: false,
            volume_ratio: Some(3.0),
            rsi: Some(50.0),
            breakout_line: None,
        }
    }

    fn v5_intent(direction: Direction, structure_line: f64) -> EntryIntent {
        let mut intent = intent(direction);
        intent.exit_policy = ExitPolicy::RsiCounterTrendAgeV5;
        intent.stop_ticks = Some(20);
        intent.target_price = Some(match direction {
            Direction::Long => 110.0,
            Direction::Short => 90.0,
        });
        intent.target_ticks = None;
        intent.counter_trend = true;
        intent.signal_counter_trend_ema_age_bars_capped_600 = Some(600);
        intent.counter_trend_structure_breakout_line = Some(structure_line);
        intent
    }

    fn v15_intent(direction: Direction) -> EntryIntent {
        let mut intent = intent(direction);
        intent.families = vec![match direction {
            Direction::Long => SignalFamily::RangeSqueezeBreakAcceptanceLong,
            Direction::Short => SignalFamily::RangeSqueezeBreakAcceptanceShort,
        }];
        intent.exit_policy = ExitPolicy::RangeSqueezeStaged;
        intent.stop_ticks = None;
        intent.target_ticks = None;
        intent.stop_price = Some(match direction {
            Direction::Long => 98.0,
            Direction::Short => 102.0,
        });
        intent.target_price = Some(match direction {
            Direction::Long => 104.0,
            Direction::Short => 96.0,
        });
        intent.breakout_line = Some(match direction {
            Direction::Long => 99.0,
            Direction::Short => 101.0,
        });
        intent
    }

    fn v16_stop_entry(direction: Direction, trigger_price: f64) -> StopEntryIntent {
        let mut intent = v15_intent(direction);
        intent.families = vec![match direction {
            Direction::Long => SignalFamily::RangeSqueezeRightSideTriggerLong,
            Direction::Short => SignalFamily::RangeSqueezeRightSideTriggerShort,
        }];
        StopEntryIntent {
            intent,
            trigger_price,
            expires_at_index: 4,
        }
    }

    #[test]
    fn entry_intent_fills_at_next_bar_open() {
        let position = position_from_intent(intent(Direction::Long), 105.0, 900_000, 0.1);
        assert_eq!(position.entry_price, 105.0);
        assert_eq!(position.stop, 102.0);
        assert_eq!(position.target, Some(111.0));
    }

    #[test]
    fn v11_blocks_a_gap_beyond_the_frozen_stop_without_reversing_an_existing_position() {
        let config = ReplayConfig::tradingview_baseline("TEST", 0.1, 0, i64::MAX);
        let mut broker = Broker::new(config);
        broker.position = Some(position_from_intent(
            intent(Direction::Short),
            100.0,
            0,
            0.1,
        ));
        let mut pending = intent(Direction::Long);
        pending.stop_price = Some(101.0);
        pending.stop_ticks = Some(10);
        pending.strict_visual_breakout_candle_extreme_stop = true;
        broker.pending_entry = Some(pending);

        broker.fill_open_orders(
            Candle {
                timestamp_ms: 900_000,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1.0,
            },
            1,
        );

        assert_eq!(broker.position.unwrap().direction, Direction::Short);
        assert!(broker.trades.is_empty());
        assert_eq!(broker.blocked.len(), 1);
        assert_eq!(
            broker.blocked[0].reason,
            "STRICT_VISUAL_BREAKOUT_CANDLE_STOP_INVALID_AT_ENTRY"
        );
    }

    #[test]
    fn v11_entry_stop_validity_is_a_true_directional_mirror() {
        let mut long = intent(Direction::Long);
        long.strict_visual_breakout_candle_extreme_stop = true;
        long.stop_price = Some(99.0);
        let mut short = intent(Direction::Short);
        short.strict_visual_breakout_candle_extreme_stop = true;
        short.stop_price = Some(101.0);

        assert!(strict_visual_breakout_candle_stop_is_valid(&long, 100.0));
        assert!(strict_visual_breakout_candle_stop_is_valid(&short, 100.0));
        long.stop_price = Some(100.0);
        short.stop_price = Some(100.0);
        assert!(!strict_visual_breakout_candle_stop_is_valid(&long, 100.0));
        assert!(!strict_visual_breakout_candle_stop_is_valid(&short, 100.0));
    }

    #[test]
    fn v12_keeps_the_farther_of_structure_and_one_atr_risk_for_both_directions() {
        let mut long_floor = intent(Direction::Long);
        long_floor.strict_visual_breakout_candle_extreme_stop = true;
        long_floor.stop_price = Some(99.5);
        long_floor.stop_ticks = Some(10);
        let long_floor = position_from_intent(long_floor, 100.0, 900_000, 0.1);
        assert!((long_floor.initial_stop - 99.0).abs() < 1e-9);

        let mut long_structure = intent(Direction::Long);
        long_structure.strict_visual_breakout_candle_extreme_stop = true;
        long_structure.stop_price = Some(98.5);
        long_structure.stop_ticks = Some(10);
        let long_structure = position_from_intent(long_structure, 100.0, 900_000, 0.1);
        assert!((long_structure.initial_stop - 98.5).abs() < 1e-9);

        let mut short_floor = intent(Direction::Short);
        short_floor.strict_visual_breakout_candle_extreme_stop = true;
        short_floor.stop_price = Some(100.5);
        short_floor.stop_ticks = Some(10);
        let short_floor = position_from_intent(short_floor, 100.0, 900_000, 0.1);
        assert!((short_floor.initial_stop - 101.0).abs() < 1e-9);

        let mut short_structure = intent(Direction::Short);
        short_structure.strict_visual_breakout_candle_extreme_stop = true;
        short_structure.stop_price = Some(101.5);
        short_structure.stop_ticks = Some(10);
        let short_structure = position_from_intent(short_structure, 100.0, 900_000, 0.1);
        assert!((short_structure.initial_stop - 101.5).abs() < 1e-9);
    }

    #[test]
    fn default_path_hits_the_nearest_level_in_travel_order() {
        let mut position = position_from_intent(intent(Direction::Long), 100.0, 900_000, 0.1);
        position.stop = 98.0;
        position.target = Some(104.0);

        assert_eq!(
            first_order_on_segment(&position, 100.0, 105.0),
            Some((104.0, ExitReason::TakeProfit))
        );
        assert_eq!(
            first_order_on_segment(&position, 100.0, 97.0),
            Some((98.0, ExitReason::StopLoss))
        );
    }

    #[test]
    fn intrabar_drawdown_uses_pre_trade_closed_equity_peak() {
        let config = ReplayConfig::tradingview_baseline("TEST", 0.1, 0, i64::MAX);
        let mut broker = Broker::new(config);
        let mut position = position_from_intent(intent(Direction::Short), 100.0, 900_000, 0.1);
        position.stop = 110.0;
        position.initial_stop = 110.0;
        position.target = None;
        broker.position = Some(position);

        broker.process_protective_orders(
            Candle {
                timestamp_ms: 900_000,
                open: 100.0,
                high: 110.0,
                low: 90.0,
                close: 110.0,
                volume: 1.0,
            },
            1,
        );

        assert_eq!(broker.trades[0].net_pnl, -10.0);
        assert_eq!(broker.max_intrabar_drawdown, 10.0);
    }

    #[test]
    fn v16_waits_for_micro_break_and_only_processes_the_post_fill_path() {
        let config = ReplayConfig::current_pine_v16("TEST", 0.1, 0, i64::MAX);
        let mut broker = Broker::new(config);
        broker.pending_stop_entry = Some(v16_stop_entry(Direction::Long, 101.0));

        let consumed = broker.process_stop_entry(
            Candle {
                timestamp_ms: 1_800_000,
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 101.5,
                volume: 1.0,
            },
            2,
        );

        assert!(consumed);
        assert!(broker.trades.is_empty());
        let position = broker.position.expect("triggered V16 position");
        assert_eq!(position.entry_price, 101.0);
        assert_eq!(position.initial_stop, 98.0);
        assert_eq!(
            position.families,
            vec![SignalFamily::RangeSqueezeRightSideTriggerLong]
        );
    }

    #[test]
    fn v16_unfilled_structure_invalidation_cancels_the_pending_order() {
        let config = ReplayConfig::current_pine_v16("TEST", 0.1, 0, i64::MAX);
        let mut broker = Broker::new(config);
        broker.pending_stop_entry = Some(v16_stop_entry(Direction::Long, 103.0));

        let consumed = broker.process_stop_entry(
            Candle {
                timestamp_ms: 1_800_000,
                open: 100.0,
                high: 102.0,
                low: 97.5,
                close: 100.0,
                volume: 1.0,
            },
            2,
        );

        assert!(!consumed);
        assert!(broker.position.is_none());
        assert!(broker.pending_stop_entry.is_none());
        assert!(broker
            .blocked
            .iter()
            .any(|blocked| { blocked.reason == "V16_TRIGGER_STRUCTURE_INVALIDATED_BEFORE_FILL" }));
    }

    #[test]
    fn v4_counter_trend_activation_uses_actual_entry_risk_and_trails_one_way() {
        let mut v4_intent = intent(Direction::Long);
        v4_intent.exit_policy = ExitPolicy::CounterTrendStructureV4;
        v4_intent.stop_ticks = Some(20);
        v4_intent.target_price = Some(110.0);
        v4_intent.target_ticks = None;
        let mut position = position_from_intent(v4_intent, 100.0, 900_000, 0.1);

        assert_eq!(position.initial_stop, 98.0);
        assert_eq!(position.activation_price, Some(102.0));
        assert!(!update_counter_trend_v4_trailing_stop(
            &mut position,
            Candle {
                timestamp_ms: 900_000,
                open: 100.0,
                high: 101.9,
                low: 99.0,
                close: 101.0,
                volume: 1.0,
            },
            0.1,
        ));
        assert!(!position.activated);
        assert_eq!(position.stop, 98.0);

        assert!(!update_counter_trend_v4_trailing_stop(
            &mut position,
            Candle {
                timestamp_ms: 1_800_000,
                open: 101.0,
                high: 102.5,
                low: 100.5,
                close: 102.0,
                volume: 1.0,
            },
            0.1,
        ));
        assert!(position.activated);
        assert!((position.stop - 100.5).abs() < 1e-9);

        assert!(!update_counter_trend_v4_trailing_stop(
            &mut position,
            Candle {
                timestamp_ms: 2_700_000,
                open: 102.0,
                high: 102.2,
                low: 101.5,
                close: 102.0,
                volume: 1.0,
            },
            0.1,
        ));
        assert!((position.stop - 100.5).abs() < 1e-9);
        assert_eq!(position.target, Some(110.0));
    }

    #[test]
    fn v4_trailing_cross_at_close_exits_only_at_next_open() {
        let config = ReplayConfig::current_pine_v4("TEST", 0.1, 0, i64::MAX);
        let mut broker = Broker::new(config);
        let mut v4_intent = intent(Direction::Long);
        v4_intent.exit_policy = ExitPolicy::CounterTrendStructureV4;
        v4_intent.stop_ticks = Some(20);
        v4_intent.target_price = Some(110.0);
        v4_intent.target_ticks = None;
        broker.position = Some(position_from_intent(v4_intent, 100.0, 900_000, 0.1));

        broker.update_dynamic_protection_at_close(
            Candle {
                timestamp_ms: 1_800_000,
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 100.0,
                volume: 1.0,
            },
            None,
        );
        assert!(broker.position.is_some());
        assert_eq!(
            broker.pending_close_reason,
            Some(ExitReason::CounterTrendTrailingStop)
        );

        broker.fill_open_orders(
            Candle {
                timestamp_ms: 2_700_000,
                open: 100.2,
                high: 101.0,
                low: 100.0,
                close: 100.5,
                volume: 1.0,
            },
            3,
        );
        assert!(broker.position.is_none());
        assert_eq!(
            broker.trades[0].exit_reason,
            ExitReason::CounterTrendTrailingStop
        );
        assert_eq!(
            broker.trades[0].exit_policy,
            ExitPolicy::CounterTrendStructureV4
        );
        assert_eq!(broker.trades[0].exit_price, 100.2);
    }

    #[test]
    fn v5_fixed_cost_net_break_even_rounds_outward_for_both_directions() {
        assert!((v5_net_break_even_price(Direction::Long, 100.0, 0.1) - 100.2).abs() < 1e-9);
        assert!((v5_net_break_even_price(Direction::Short, 100.0, 0.1) - 99.8).abs() < 1e-9);
    }

    #[test]
    fn v5_one_and_two_r_mfe_do_not_protect_before_structure_confirmation() {
        let mut position =
            position_from_intent(v5_intent(Direction::Long, 105.0), 100.0, 900_000, 0.1);

        assert_eq!(
            update_rsi_counter_trend_v5_protection(
                &mut position,
                Candle {
                    timestamp_ms: 1_800_000,
                    open: 100.0,
                    high: 102.0,
                    low: 99.0,
                    close: 101.9,
                    volume: 1.0,
                },
                0.1,
            ),
            None
        );
        assert_eq!(position.stop, 98.0);
        assert!(!position.counter_trend_structure_confirmed);
        assert!(!position.counter_trend_two_r_trailing_activated);

        assert_eq!(
            update_rsi_counter_trend_v5_protection(
                &mut position,
                Candle {
                    timestamp_ms: 2_700_000,
                    open: 101.9,
                    high: 105.5,
                    low: 101.0,
                    close: 105.0,
                    volume: 1.0,
                },
                0.1,
            ),
            None
        );
        assert_eq!(position.stop, 98.0);
        assert!(!position.counter_trend_structure_confirmed);
        assert!(!position.counter_trend_two_r_trailing_activated);
    }

    #[test]
    fn v5_long_and_short_confirm_structure_then_trail_two_r_one_way() {
        let mut long = position_from_intent(v5_intent(Direction::Long, 103.0), 100.0, 900_000, 0.1);
        assert_eq!(
            update_rsi_counter_trend_v5_protection(
                &mut long,
                Candle {
                    timestamp_ms: 1_800_000,
                    open: 100.0,
                    high: 103.5,
                    low: 99.0,
                    close: 103.1,
                    volume: 1.0,
                },
                0.1,
            ),
            None
        );
        assert!(long.counter_trend_structure_confirmed);
        assert!(!long.counter_trend_two_r_trailing_activated);
        assert!((long.stop - 100.2).abs() < 1e-9);

        assert_eq!(
            update_rsi_counter_trend_v5_protection(
                &mut long,
                Candle {
                    timestamp_ms: 2_700_000,
                    open: 103.1,
                    high: 105.5,
                    low: 103.0,
                    close: 105.0,
                    volume: 1.0,
                },
                0.1,
            ),
            None
        );
        assert!(long.counter_trend_two_r_trailing_activated);
        assert!((long.stop - 101.5).abs() < 1e-9);
        assert_eq!(
            update_rsi_counter_trend_v5_protection(
                &mut long,
                Candle {
                    timestamp_ms: 3_600_000,
                    open: 105.0,
                    high: 105.4,
                    low: 104.0,
                    close: 104.5,
                    volume: 1.0,
                },
                0.1,
            ),
            None
        );
        assert!((long.stop - 101.5).abs() < 1e-9);

        let mut short =
            position_from_intent(v5_intent(Direction::Short, 97.0), 100.0, 900_000, 0.1);
        assert_eq!(
            update_rsi_counter_trend_v5_protection(
                &mut short,
                Candle {
                    timestamp_ms: 1_800_000,
                    open: 100.0,
                    high: 101.0,
                    low: 96.5,
                    close: 96.9,
                    volume: 1.0,
                },
                0.1,
            ),
            None
        );
        assert!(short.counter_trend_structure_confirmed);
        assert!(!short.counter_trend_two_r_trailing_activated);
        assert!((short.stop - 99.8).abs() < 1e-9);

        assert_eq!(
            update_rsi_counter_trend_v5_protection(
                &mut short,
                Candle {
                    timestamp_ms: 2_700_000,
                    open: 96.9,
                    high: 97.0,
                    low: 94.5,
                    close: 95.0,
                    volume: 1.0,
                },
                0.1,
            ),
            None
        );
        assert!(short.counter_trend_two_r_trailing_activated);
        assert!((short.stop - 98.5).abs() < 1e-9);
    }

    #[test]
    fn v5_new_completed_bar_trailing_cross_exits_only_at_next_open() {
        let config = ReplayConfig::current_pine_v5("TEST", 0.1, 0, i64::MAX);
        let mut broker = Broker::new(config);
        broker.position = Some(position_from_intent(
            v5_intent(Direction::Long, 103.0),
            100.0,
            900_000,
            0.1,
        ));

        broker.update_dynamic_protection_at_close(
            Candle {
                timestamp_ms: 1_800_000,
                open: 100.0,
                high: 103.5,
                low: 99.0,
                close: 103.1,
                volume: 1.0,
            },
            None,
        );
        assert_eq!(
            broker.position.as_ref().map(|position| position.stop),
            Some(100.2)
        );
        assert_eq!(broker.pending_close_reason, None);

        let pullback = Candle {
            timestamp_ms: 2_700_000,
            open: 103.1,
            high: 105.0,
            low: 100.5,
            close: 100.8,
            volume: 1.0,
        };
        broker.process_protective_orders(pullback, 3);
        assert!(broker.position.is_some());
        broker.update_dynamic_protection_at_close(pullback, None);
        assert_eq!(
            broker.pending_close_reason,
            Some(ExitReason::RsiCounterTrendTwoRTrailingStop)
        );

        broker.fill_open_orders(
            Candle {
                timestamp_ms: 3_600_000,
                open: 100.7,
                high: 101.0,
                low: 100.5,
                close: 100.8,
                volume: 1.0,
            },
            4,
        );
        assert!(broker.position.is_none());
        assert_eq!(
            broker.trades[0].exit_reason,
            ExitReason::RsiCounterTrendTwoRTrailingStop
        );
        assert_eq!(
            broker.trades[0].signal_counter_trend_ema_age_bars_capped_600,
            Some(600)
        );
        assert!(broker.trades[0].counter_trend_structure_confirmed);
        assert!(broker.trades[0].counter_trend_two_r_trailing_activated);
        assert_eq!(broker.trades[0].exit_time_ms, 3_600_000);
        assert_eq!(broker.trades[0].exit_price, 100.7);
    }

    #[test]
    fn v15_entry_bar_keeps_full_structural_protection_before_actual_r_is_known() {
        let config = ReplayConfig::current_pine_v15("TEST", 0.1, 0, i64::MAX);
        let mut broker = Broker::new(config);
        broker.position = Some(position_from_intent(
            v15_intent(Direction::Long),
            100.0,
            900_000,
            0.1,
        ));

        broker.process_protective_orders(
            Candle {
                timestamp_ms: 900_000,
                open: 100.0,
                high: 102.5,
                low: 99.5,
                close: 101.5,
                volume: 1.0,
            },
            1,
        );

        let position = broker.position.as_ref().expect("position remains open");
        assert!(!position.range_partial_one_r_taken);
        assert_eq!(position.remaining_quantity, 1.0);
    }

    #[test]
    fn v15_partial_is_one_trade_and_break_even_waits_for_completed_close() {
        let config = ReplayConfig::current_pine_v15("TEST", 0.1, 0, i64::MAX);
        let mut broker = Broker::new(config);
        broker.position = Some(position_from_intent(
            v15_intent(Direction::Long),
            100.0,
            900_000,
            0.1,
        ));

        let intrabar_one_r = Candle {
            timestamp_ms: 1_800_000,
            open: 100.0,
            high: 102.5,
            low: 99.5,
            close: 101.5,
            volume: 1.0,
        };
        broker.process_protective_orders(intrabar_one_r, 2);
        broker.update_dynamic_protection_at_close(intrabar_one_r, Some(1.0));
        let position = broker.position.as_ref().expect("remaining position");
        assert!(position.range_partial_one_r_taken);
        assert!((position.remaining_quantity - 0.67).abs() < 1e-9);
        assert!(!position.range_one_r_close_confirmed);
        assert_eq!(position.stop, 98.0);

        let close_confirmed = Candle {
            timestamp_ms: 2_700_000,
            open: 101.5,
            high: 102.4,
            low: 101.0,
            close: 102.1,
            volume: 1.0,
        };
        broker.process_protective_orders(close_confirmed, 3);
        broker.update_dynamic_protection_at_close(close_confirmed, Some(1.0));
        let position = broker.position.as_ref().expect("protected remainder");
        assert!(position.range_one_r_close_confirmed);
        assert_eq!(position.stop, 100.2);

        broker.process_protective_orders(
            Candle {
                timestamp_ms: 3_600_000,
                open: 102.1,
                high: 104.2,
                low: 101.8,
                close: 104.0,
                volume: 1.0,
            },
            4,
        );
        assert!(broker.position.is_none());
        assert_eq!(broker.trades.len(), 1);
        let trade = &broker.trades[0];
        assert!(trade.range_partial_one_r_taken);
        assert!((trade.gross_pnl - 3.34).abs() < 1e-9);
        assert!((trade.exit_price - 103.34).abs() < 1e-9);
        assert!((trade.net_r - 1.67).abs() < 1e-9);
        assert_eq!(trade.exit_reason, ExitReason::RangeSqueezeTakeProfit);
    }

    #[test]
    fn v15_completed_close_back_inside_box_exits_at_next_open() {
        let config = ReplayConfig::current_pine_v15("TEST", 0.1, 0, i64::MAX);
        let mut broker = Broker::new(config);
        broker.position = Some(position_from_intent(
            v15_intent(Direction::Long),
            100.0,
            900_000,
            0.1,
        ));

        broker.update_dynamic_protection_at_close(
            Candle {
                timestamp_ms: 1_800_000,
                open: 100.0,
                high: 101.0,
                low: 98.5,
                close: 98.9,
                volume: 1.0,
            },
            Some(1.0),
        );
        assert_eq!(
            broker.pending_close_reason,
            Some(ExitReason::RangeSqueezeBoxReentry)
        );

        broker.fill_open_orders(
            Candle {
                timestamp_ms: 2_700_000,
                open: 99.1,
                high: 99.5,
                low: 99.0,
                close: 99.2,
                volume: 1.0,
            },
            3,
        );
        assert_eq!(
            broker.trades[0].exit_reason,
            ExitReason::RangeSqueezeBoxReentry
        );
        assert_eq!(broker.trades[0].exit_price, 99.1);
    }
}
