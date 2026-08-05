use super::*;

impl MarketVelocityReplayStrategy {
    /// 在预注册持仓窗口结束时按该根已完成 K 线收盘价退出。
    fn maybe_build_max_holding_exit_signal(&mut self, candle: &CandleItem) -> Option<SignalResult> {
        let max_holding_ms = self.max_holding_ms?;
        let active = self.active_position.as_ref()?;
        if candle.ts < active.entry_ts.saturating_add(max_holding_ms) {
            return None;
        }
        let event_id = active.event_id;
        let trigger = active.trigger.clone();
        let direction = active.direction;
        self.active_position = None;
        Some(SignalResult {
            should_buy: direction == MarketVelocityTradeDirection::Short,
            should_sell: direction == MarketVelocityTradeDirection::Long,
            open_price: candle.c,
            ts: candle.ts,
            single_value: Some(
                json!({
                    "source": "market_velocity_framework_replay",
                    "rank_event_id": event_id,
                    "entry_trigger": trigger,
                    "exit_reason": "max_holding_timeout",
                    "max_holding_ms": max_holding_ms,
                })
                .to_string(),
            ),
            single_result: Some("market_velocity_framework_replay".to_string()),
            // 框架使用反向信号平掉当前仓位；同时阻断反向开仓，避免超时退出变成新仓位。
            filter_reasons: vec![match direction {
                MarketVelocityTradeDirection::Long => "FIB_STRICT_MAJOR_BULL_BLOCK_SHORT",
                MarketVelocityTradeDirection::Short => "FIB_STRICT_MAJOR_BEAR_BLOCK_LONG",
                MarketVelocityTradeDirection::Both => "MARKET_VELOCITY_MAX_HOLDING_TIMEOUT",
            }
            .to_string()],
            direction: match direction {
                MarketVelocityTradeDirection::Long => SignalDirection::Short,
                MarketVelocityTradeDirection::Short => SignalDirection::Long,
                MarketVelocityTradeDirection::Both => SignalDirection::None,
            },
            ..SignalResult::default()
        })
    }

    /// 构建框架回放的方向信号，并保留止损、止盈与事件审计字段。
    pub(super) fn build_entry_direction_signal(
        &self,
        candle_ts: i64,
        entry_price: f64,
        stop_loss_price: f64,
        stop_loss_pct: f64,
        stop_loss_source: &str,
        event_id: i64,
        trigger: &str,
        direction: MarketVelocityTradeDirection,
        target_r: f64,
        profit_protected: bool,
    ) -> SignalResult {
        let immediate_exit = stop_loss_source == FILTERED_VOLUME_V3_INVALID_AT_FILL_STOP_SOURCE;
        SignalResult {
            should_buy: direction == MarketVelocityTradeDirection::Long,
            should_sell: direction == MarketVelocityTradeDirection::Short,
            open_price: entry_price,
            signal_kline_stop_loss_price: Some(stop_loss_price),
            stop_loss_source: Some(stop_loss_source.to_string()),
            long_signal_take_profit_price: (direction == MarketVelocityTradeDirection::Long)
                .then_some(target_price_for(
                    entry_price,
                    stop_loss_pct,
                    target_r,
                    direction,
                )),
            short_signal_take_profit_price: (direction == MarketVelocityTradeDirection::Short)
                .then_some(target_price_for(
                    entry_price,
                    stop_loss_pct,
                    target_r,
                    direction,
                )),
            ts: candle_ts,
            single_value: Some(
                json!({
                    "source": "market_velocity_framework_replay",
                    "rank_event_id": event_id,
                    "entry_trigger": trigger,
                    "trade_direction": direction.label(),
                    "target_r": target_r,
                    "stop_loss_pct": stop_loss_pct,
                    "profit_protected": profit_protected,
                })
                .to_string(),
            ),
            single_result: Some("market_velocity_framework_replay".to_string()),
            dynamic_adjustments: immediate_exit
                .then(|| IMMEDIATE_EXIT_AT_ENTRY_ADJUSTMENT.to_string())
                .into_iter()
                .collect(),
            direction: match direction {
                MarketVelocityTradeDirection::Long => SignalDirection::Long,
                MarketVelocityTradeDirection::Short => SignalDirection::Short,
                MarketVelocityTradeDirection::Both => SignalDirection::None,
            },
            ..SignalResult::default()
        }
    }
}

impl IndicatorStrategyBacktest for MarketVelocityReplayStrategy {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        // 持仓放量保护需要当前 K、前 10 根基线 K，以及用于逐根标记这 10 根异常量的
        // 再前 10 根。框架只把这里声明的滑动窗口传给 `generate_signal`，少于 21 根会
        // 让 V12 的因果过滤量比始终处于 not-ready；旧版本仍只读取窗口最后一根。
        21
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {}

    fn build_indicator_values(
        _: &mut Self::IndicatorCombine,
        _: &CandleItem,
    ) -> Self::IndicatorValues {
    }

    /// 严格按已完成 K 线生成框架回放信号，不读取当前时点之后的数据。
    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        _: &mut Self::IndicatorValues,
        _: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        let Some(candle) = candles.last() else {
            return SignalResult::default();
        };
        let entry = self.entries_by_ts.get(&candle.ts).cloned();
        let ignore_entry_for_current_candle = entry
            .as_ref()
            .is_some_and(|entry| self.should_ignore_entry_update(entry));
        self.clear_active_position_if_exit_hit(candle);
        if let Some(entry) = entry {
            if !ignore_entry_for_current_candle && !self.should_ignore_entry_update(&entry) {
                return self.build_entry_signal(candle, &entry);
            }
        }
        if let Some(signal) = self.maybe_build_max_holding_exit_signal(candle) {
            return signal;
        }
        if let Some(signal) = self.maybe_build_early_exit_signal(candle) {
            return signal;
        }
        if let Some(signal) = self.maybe_build_profit_observation_signal(candle) {
            return signal;
        }
        if let Some(signal) = self.maybe_build_volume_atr_trailing_signal(candles) {
            return signal;
        }
        if let Some(signal) = self.maybe_build_profit_protection_signal(candle) {
            return signal;
        }
        SignalResult {
            ts: candle.ts,
            open_price: candle.c,
            ..SignalResult::default()
        }
    }
}
