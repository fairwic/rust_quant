/// 窄幅整理突破候选返回给主信号流程的确认方向。
#[derive(Debug, Clone, Copy, PartialEq)]
struct CompressedRangeBreakoutDecision {
    /// 当前已完成 K 线确认的突破方向。
    direction: SignalDirect,
    /// 可选的整理区结构失效止损；关闭结构止损时保持为空。
    protective_stop: Option<f64>,
    /// 是否由弱量触发棒后的固定一根 K 线确认生成。
    delayed_confirmation: bool,
    /// 是否由至少 `1.5ATR` 的当根价格位移替代量能门禁生成。
    price_displacement_activation: bool,
}

impl VegasStrategy {
    /// 识别 5 根窄幅整理后、由量能与 MACD 同向确认的实体突破。
    ///
    /// 当前 K 线只和此前已完成的 5 根 K 线比较；固定阈值属于
    /// `compressed_range_breakout_v1_20260720`，研究失败时整体废弃，不在本版本内调参。
    fn compressed_range_breakout_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<CompressedRangeBreakoutDecision> {
        const LOOKBACK_BARS: usize = 5;
        const MAX_RANGE_WIDTH_RATIO: f64 = 0.03;
        const MIN_CLOSE_BREAK_RATIO: f64 = 0.01;
        const MIN_BODY_RATIO: f64 = 0.60;
        const MIN_VOLUME_RATIO: f64 = 1.50;

        let config = self.compressed_range_breakout;
        if !config.is_open || data_items.len() < LOOKBACK_BARS + 1 {
            return None;
        }

        let current = data_items.last().expect("已检查至少存在一根当前 K 线");
        let prior = &data_items[data_items.len() - LOOKBACK_BARS - 1..data_items.len() - 1];
        let prior_high = prior
            .iter()
            .map(CandleItem::h)
            .fold(f64::NEG_INFINITY, f64::max);
        let prior_low = prior
            .iter()
            .map(CandleItem::l)
            .fold(f64::INFINITY, f64::min);
        let close = current.c().max(1e-9);
        let range_width_ratio = (prior_high - prior_low) / close;
        if range_width_ratio > MAX_RANGE_WIDTH_RATIO || current.body_ratio() < MIN_BODY_RATIO {
            // 当前棒不必再次成为突破棒；v44 的确认棒只负责证明上一根弱量突破仍在延续。
            return config
                .delay_low_volume_short_one_bar
                .then(|| self.delayed_low_volume_short_breakout(data_items, values))
                .flatten();
        }

        let macd = &values.macd_value;
        let market = &values.market_structure_value;
        let volume_confirmed = values.volume_value.volume_ratio >= MIN_VOLUME_RATIO;
        let candle_range_atr_multiple = values.cross_asset_adaptive_value.candle_range_atr_multiple;
        let price_displacement_confirmed = config.allow_short_price_displacement_without_volume
            && candle_range_atr_multiple.is_finite()
            && candle_range_atr_multiple >= 1.5;
        let long_break_ratio = (current.c() - prior_high).max(0.0) / close;
        let long_confirmed = config.enable_long
            && volume_confirmed
            && current.c() > current.o()
            && long_break_ratio >= MIN_CLOSE_BREAK_RATIO
            && macd.macd_line > 0.0
            && macd.signal_line > 0.0
            && macd.histogram > 0.0
            && macd.histogram_increasing
            && !values.ema_values.is_short_trend
            && !market.internal_bearish_bos
            && !market.swing_bearish_bos;
        if long_confirmed {
            return Some(CompressedRangeBreakoutDecision {
                direction: SignalDirect::IsLong,
                protective_stop: config
                    .use_prior_range_invalidation_stop
                    .then_some(prior_high),
                delayed_confirmation: false,
                price_displacement_activation: false,
            });
        }

        let short_break_ratio = (prior_low - current.c()).max(0.0) / close;
        // v42 只拦截“量能尚未达到既有冲击量标准、EMA 距离又没有形成扩张”的突破。
        // 该门禁不作用于原 Vegas 信号，避免用新增 setup 的诊断结果改写冻结基线。
        let weak_short_expansion = config.block_low_volume_normal_ema_short
            && values.cross_asset_adaptive_value.relative_volume_ratio < 2.5
            && values.ema_distance_filter.state == EmaDistanceState::Normal;
        // v44 将弱量触发棒视为“待确认”而不是立即信号；等待窗口固定为下一根，
        // 既避免在成交持续性不足时追空，也不允许用更远的未来 K 线补造入场。
        let delayed_low_volume_short = config.delay_low_volume_short_one_bar
            && values.cross_asset_adaptive_value.relative_volume_ratio < 2.5;
        // V61/V63 分别复用既有 2.5x 冲击量与 2.0x 放量标准，验证突破是否获得足够市场参与；
        // 两档门禁只约束新增压缩空头，不回写 legacy Vegas 或其他 setup。
        let required_short_relative_volume = if config.require_short_relative_volume_2_5 {
            2.5
        } else if config.require_short_relative_volume_2_0 {
            2.0
        } else {
            0.0
        };
        let short_volume_quality_passed = values.cross_asset_adaptive_value.relative_volume_ratio
            >= required_short_relative_volume;
        let short_confirmed = config.enable_short
            && !weak_short_expansion
            && !delayed_low_volume_short
            && short_volume_quality_passed
            && (volume_confirmed || price_displacement_confirmed)
            && current.c() < current.o()
            && short_break_ratio >= MIN_CLOSE_BREAK_RATIO
            && macd.macd_line < 0.0
            && macd.signal_line < 0.0
            && macd.histogram < 0.0
            && macd.histogram_decreasing
            && !values.ema_values.is_long_trend
            && !market.internal_bullish_bos
            && !market.swing_bullish_bos;
        let short_structural_stop = if config.widen_short_invalidation_stop_to_one_atr {
            let one_atr_stop = current.c() + values.cross_asset_adaptive_value.atr_value.max(0.0);
            prior_low.max(one_atr_stop)
        } else {
            prior_low
        };
        if short_confirmed {
            return Some(CompressedRangeBreakoutDecision {
                direction: SignalDirect::IsShort,
                protective_stop: config
                    .use_prior_range_invalidation_stop
                    .then_some(short_structural_stop),
                delayed_confirmation: false,
                price_displacement_activation: !volume_confirmed && price_displacement_confirmed,
            });
        }

        config
            .delay_low_volume_short_one_bar
            .then(|| self.delayed_low_volume_short_breakout(data_items, values))
            .flatten()
    }

    /// 确认上一根弱量空头突破在固定一根 4H K 线后仍有价格与 MACD 延续。
    ///
    /// 触发棒的量价结构从原始已完成 K 线重建；当前指标只用于确认时点，
    /// 避免错误地拿当前成交量或未来走势冒充触发时证据。
    fn delayed_low_volume_short_breakout(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<CompressedRangeBreakoutDecision> {
        const LOOKBACK_BARS: usize = 5;
        const MAX_RANGE_WIDTH_RATIO: f64 = 0.03;
        const MIN_CLOSE_BREAK_RATIO: f64 = 0.01;
        const MIN_BODY_RATIO: f64 = 0.60;
        const MIN_VOLUME_RATIO: f64 = 1.50;
        const IMMEDIATE_VOLUME_RATIO: f64 = 2.50;

        // 延迟路径的定义就是触发量低于 2.5x；开启 V61 质量门禁时必须整体关闭，
        // 否则同一弱量触发会绕过即时分支的质量合同。
        if self
            .compressed_range_breakout
            .require_short_relative_volume_2_5
        {
            return None;
        }

        if data_items.len() < LOOKBACK_BARS + 2 {
            return None;
        }
        let current_index = data_items.len() - 1;
        let trigger_index = current_index - 1;
        let current = &data_items[current_index];
        let trigger = &data_items[trigger_index];
        let prior = &data_items[trigger_index - LOOKBACK_BARS..trigger_index];
        let prior_high = prior
            .iter()
            .map(CandleItem::h)
            .fold(f64::NEG_INFINITY, f64::max);
        let prior_low = prior
            .iter()
            .map(CandleItem::l)
            .fold(f64::INFINITY, f64::min);
        let trigger_close = trigger.c().max(1e-9);
        let range_width_ratio = (prior_high - prior_low) / trigger_close;
        let short_break_ratio = (prior_low - trigger.c()).max(0.0) / trigger_close;
        let volume_baseline_bars = self
            .volume_signal
            .map(|value| value.volume_bar_num)
            .unwrap_or(4)
            .max(1);
        let trigger_volume_ratio =
            relative_volume_ratio_at(data_items, trigger_index, volume_baseline_bars)?;
        let trigger_is_weak_breakout = trigger.c() < trigger.o()
            && trigger.body_ratio() >= MIN_BODY_RATIO
            && range_width_ratio <= MAX_RANGE_WIDTH_RATIO
            && short_break_ratio >= MIN_CLOSE_BREAK_RATIO
            && (MIN_VOLUME_RATIO..IMMEDIATE_VOLUME_RATIO).contains(&trigger_volume_ratio)
            && (!self
                .compressed_range_breakout
                .require_short_relative_volume_2_0
                || trigger_volume_ratio >= 2.0);
        if !trigger_is_weak_breakout {
            return None;
        }

        let macd = &values.macd_value;
        let market = &values.market_structure_value;
        let confirmation_holds_breakout = current.c() <= trigger.c()
            && current.c() < prior_low
            && macd.macd_line < 0.0
            && macd.signal_line < 0.0
            && macd.histogram < 0.0
            && macd.histogram_decreasing
            && !values.ema_values.is_long_trend
            && !market.internal_bullish_bos
            && !market.swing_bullish_bos;
        confirmation_holds_breakout.then_some(CompressedRangeBreakoutDecision {
            direction: SignalDirect::IsShort,
            protective_stop: self
                .compressed_range_breakout
                .use_prior_range_invalidation_stop
                .then_some(prior_low),
            delayed_confirmation: true,
            price_displacement_activation: false,
        })
    }
}

#[cfg(test)]
mod compressed_range_breakout_tests {
    use super::*;

    /// 构造固定振幅与成交量的已确认 K 线。
    fn candle(o: f64, h: f64, l: f64, c: f64, ts: i64) -> CandleItem {
        CandleItem {
            o,
            h,
            l,
            c,
            v: 10.0,
            ts,
            confirm: 1,
        }
    }

    /// 构造只开启窄幅整理突破的研究策略。
    fn strategy() -> VegasStrategy {
        VegasStrategy {
            compressed_range_breakout: CompressedRangeBreakoutConfig {
                is_open: true,
                ..CompressedRangeBreakoutConfig::default()
            },
            ..VegasStrategy::default()
        }
    }

    #[test]
    fn confirmed_long_breakout_uses_only_prior_range() {
        let mut candles = (0..5)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 103.0, 99.8, 102.5, 5));
        let mut values = VegasIndicatorSignalValue::default();
        values.volume_value.volume_ratio = 1.5;
        values.macd_value.macd_line = 1.2;
        values.macd_value.signal_line = 1.0;
        values.macd_value.histogram = 0.2;
        values.macd_value.histogram_increasing = true;

        let decision = strategy()
            .compressed_range_breakout_decision(&candles, &values)
            .expect("同向量价与 MACD 应确认多头突破");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!(decision.protective_stop.is_none());
        assert!(!decision.delayed_confirmation);
        assert!(!decision.price_displacement_activation);
    }

    #[test]
    fn confirmed_short_breakout_is_symmetric() {
        let mut candles = (0..5)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 100.2, 97.0, 97.5, 5));
        let mut values = VegasIndicatorSignalValue::default();
        values.volume_value.volume_ratio = 1.5;
        values.macd_value.macd_line = -1.2;
        values.macd_value.signal_line = -1.0;
        values.macd_value.histogram = -0.2;
        values.macd_value.histogram_decreasing = true;

        let decision = strategy()
            .compressed_range_breakout_decision(&candles, &values)
            .expect("同向量价与 MACD 应确认空头突破");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!(decision.protective_stop.is_none());
        assert!(!decision.delayed_confirmation);
        assert!(!decision.price_displacement_activation);
    }

    #[test]
    fn short_breakout_can_freeze_prior_range_low_as_invalidation_stop() {
        let mut candles = (0..5)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 100.2, 97.0, 97.5, 5));
        let mut values = VegasIndicatorSignalValue::default();
        values.volume_value.volume_ratio = 1.5;
        values.macd_value.macd_line = -1.2;
        values.macd_value.signal_line = -1.0;
        values.macd_value.histogram = -0.2;
        values.macd_value.histogram_decreasing = true;
        let strategy = VegasStrategy {
            compressed_range_breakout: CompressedRangeBreakoutConfig {
                is_open: true,
                standalone: false,
                enable_long: false,
                enable_short: true,
                use_prior_range_invalidation_stop: true,
                block_low_volume_normal_ema_short: false,
                widen_short_invalidation_stop_to_one_atr: false,
                delay_low_volume_short_one_bar: false,
                allow_short_price_displacement_without_volume: false,
                require_short_relative_volume_2_5: false,
                require_short_relative_volume_2_0: false,
            },
            ..VegasStrategy::default()
        };

        let decision = strategy
            .compressed_range_breakout_decision(&candles, &values)
            .expect("空头突破应返回整理区失效位");

        assert_eq!(decision.protective_stop, Some(99.0));
    }

    #[test]
    fn low_volume_normal_ema_short_can_be_blocked_without_changing_thresholds() {
        let mut candles = (0..5)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 100.2, 97.0, 97.5, 5));
        let mut values = VegasIndicatorSignalValue::default();
        values.volume_value.volume_ratio = 1.5;
        values.cross_asset_adaptive_value.relative_volume_ratio = 2.49;
        values.ema_distance_filter.state = EmaDistanceState::Normal;
        values.macd_value.macd_line = -1.2;
        values.macd_value.signal_line = -1.0;
        values.macd_value.histogram = -0.2;
        values.macd_value.histogram_decreasing = true;
        let strategy = VegasStrategy {
            compressed_range_breakout: CompressedRangeBreakoutConfig {
                is_open: true,
                standalone: false,
                enable_long: false,
                enable_short: true,
                use_prior_range_invalidation_stop: true,
                block_low_volume_normal_ema_short: true,
                widen_short_invalidation_stop_to_one_atr: false,
                delay_low_volume_short_one_bar: false,
                allow_short_price_displacement_without_volume: false,
                require_short_relative_volume_2_5: false,
                require_short_relative_volume_2_0: false,
            },
            ..VegasStrategy::default()
        };

        assert!(strategy
            .compressed_range_breakout_decision(&candles, &values)
            .is_none());
    }

    #[test]
    fn high_volume_short_gate_uses_the_existing_two_point_five_boundary() {
        let mut candles = (0..5)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 100.2, 97.0, 97.5, 5));
        let mut values = VegasIndicatorSignalValue::default();
        values.volume_value.volume_ratio = 1.5;
        values.cross_asset_adaptive_value.relative_volume_ratio = 2.49;
        values.macd_value.macd_line = -1.2;
        values.macd_value.signal_line = -1.0;
        values.macd_value.histogram = -0.2;
        values.macd_value.histogram_decreasing = true;
        let strategy = VegasStrategy {
            compressed_range_breakout: CompressedRangeBreakoutConfig {
                is_open: true,
                standalone: false,
                enable_long: false,
                enable_short: true,
                require_short_relative_volume_2_5: true,
                ..CompressedRangeBreakoutConfig::default()
            },
            ..VegasStrategy::default()
        };

        assert!(strategy
            .compressed_range_breakout_decision(&candles, &values)
            .is_none());

        values.cross_asset_adaptive_value.relative_volume_ratio = 2.5;
        let decision = strategy
            .compressed_range_breakout_decision(&candles, &values)
            .expect("达到既有 2.5x 冲击量边界时应保留压缩突破空头");
        assert_eq!(decision.direction, SignalDirect::IsShort);
    }

    #[test]
    fn volume_increase_short_gate_uses_the_existing_two_point_zero_boundary() {
        let mut candles = (0..5)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 100.2, 97.0, 97.5, 5));
        let mut values = VegasIndicatorSignalValue::default();
        values.volume_value.volume_ratio = 1.5;
        values.cross_asset_adaptive_value.relative_volume_ratio = 1.99;
        values.macd_value.macd_line = -1.2;
        values.macd_value.signal_line = -1.0;
        values.macd_value.histogram = -0.2;
        values.macd_value.histogram_decreasing = true;
        let strategy = VegasStrategy {
            compressed_range_breakout: CompressedRangeBreakoutConfig {
                is_open: true,
                enable_long: false,
                enable_short: true,
                require_short_relative_volume_2_0: true,
                ..CompressedRangeBreakoutConfig::default()
            },
            ..VegasStrategy::default()
        };

        assert!(strategy
            .compressed_range_breakout_decision(&candles, &values)
            .is_none());

        values.cross_asset_adaptive_value.relative_volume_ratio = 2.0;
        let decision = strategy
            .compressed_range_breakout_decision(&candles, &values)
            .expect("达到既有 2.0x 放量边界时应保留压缩突破空头");
        assert_eq!(decision.direction, SignalDirect::IsShort);
    }

    #[test]
    fn short_structural_stop_can_stay_outside_one_atr_noise() {
        let mut candles = (0..5)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 100.2, 97.0, 97.5, 5));
        let mut values = VegasIndicatorSignalValue::default();
        values.volume_value.volume_ratio = 1.5;
        values.cross_asset_adaptive_value.atr_value = 3.0;
        values.macd_value.macd_line = -1.2;
        values.macd_value.signal_line = -1.0;
        values.macd_value.histogram = -0.2;
        values.macd_value.histogram_decreasing = true;
        let strategy = VegasStrategy {
            compressed_range_breakout: CompressedRangeBreakoutConfig {
                is_open: true,
                standalone: false,
                enable_long: false,
                enable_short: true,
                use_prior_range_invalidation_stop: true,
                block_low_volume_normal_ema_short: false,
                widen_short_invalidation_stop_to_one_atr: true,
                delay_low_volume_short_one_bar: false,
                allow_short_price_displacement_without_volume: false,
                require_short_relative_volume_2_5: false,
                require_short_relative_volume_2_0: false,
            },
            ..VegasStrategy::default()
        };

        let decision = strategy
            .compressed_range_breakout_decision(&candles, &values)
            .expect("空头突破应保留，但止损必须在一倍 ATR 噪声外");

        assert_eq!(decision.protective_stop, Some(100.5));
    }

    #[test]
    fn weak_volume_short_waits_for_exactly_one_confirmed_bar() {
        let mut candles = (0..5)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, ts))
            .collect::<Vec<_>>();
        let mut trigger = candle(100.0, 100.2, 97.0, 97.5, 5);
        trigger.v = 20.0;
        candles.push(trigger);
        candles.push(candle(97.4, 97.8, 96.5, 97.0, 6));
        let mut values = VegasIndicatorSignalValue::default();
        values.volume_value.volume_ratio = 0.8;
        values.cross_asset_adaptive_value.relative_volume_ratio = 0.8;
        values.macd_value.macd_line = -1.2;
        values.macd_value.signal_line = -1.0;
        values.macd_value.histogram = -0.2;
        values.macd_value.histogram_decreasing = true;
        let strategy = VegasStrategy {
            compressed_range_breakout: CompressedRangeBreakoutConfig {
                is_open: true,
                enable_long: false,
                enable_short: true,
                use_prior_range_invalidation_stop: true,
                delay_low_volume_short_one_bar: true,
                ..CompressedRangeBreakoutConfig::default()
            },
            ..VegasStrategy::default()
        };

        let decision = strategy
            .compressed_range_breakout_decision(&candles, &values)
            .expect("弱量触发后下一根继续收低，应在确认棒收盘生成空头");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert_eq!(decision.protective_stop, Some(99.0));
        assert!(decision.delayed_confirmation);
    }

    #[test]
    fn one_point_five_atr_short_displacement_can_replace_missing_volume() {
        let mut candles = (0..5)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 100.2, 97.0, 97.5, 5));
        let mut values = VegasIndicatorSignalValue::default();
        values.volume_value.volume_ratio = 1.49;
        values.cross_asset_adaptive_value.relative_volume_ratio = 1.49;
        values.cross_asset_adaptive_value.candle_range_atr_multiple = 1.5;
        values.macd_value.macd_line = -1.2;
        values.macd_value.signal_line = -1.0;
        values.macd_value.histogram = -0.2;
        values.macd_value.histogram_decreasing = true;
        let strategy = VegasStrategy {
            compressed_range_breakout: CompressedRangeBreakoutConfig {
                is_open: true,
                enable_long: false,
                enable_short: true,
                use_prior_range_invalidation_stop: true,
                allow_short_price_displacement_without_volume: true,
                ..CompressedRangeBreakoutConfig::default()
            },
            ..VegasStrategy::default()
        };

        let decision = strategy
            .compressed_range_breakout_decision(&candles, &values)
            .expect("至少 1.5ATR 的价格位移应能替代缺失量能");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert_eq!(decision.protective_stop, Some(99.0));
        assert!(decision.price_displacement_activation);
        assert!(!decision.delayed_confirmation);
    }

    #[test]
    fn weak_volume_short_is_not_entered_on_trigger_bar() {
        let mut candles = (0..5)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 100.2, 97.0, 97.5, 5));
        let mut values = VegasIndicatorSignalValue::default();
        values.volume_value.volume_ratio = 1.5;
        values.cross_asset_adaptive_value.relative_volume_ratio = 2.0;
        values.macd_value.macd_line = -1.2;
        values.macd_value.signal_line = -1.0;
        values.macd_value.histogram = -0.2;
        values.macd_value.histogram_decreasing = true;
        let strategy = VegasStrategy {
            compressed_range_breakout: CompressedRangeBreakoutConfig {
                is_open: true,
                enable_long: false,
                enable_short: true,
                use_prior_range_invalidation_stop: true,
                delay_low_volume_short_one_bar: true,
                ..CompressedRangeBreakoutConfig::default()
            },
            ..VegasStrategy::default()
        };

        assert!(strategy
            .compressed_range_breakout_decision(&candles, &values)
            .is_none());
    }

    #[test]
    fn wide_prior_range_is_not_a_compressed_breakout() {
        let mut candles = vec![
            candle(100.0, 106.0, 94.0, 100.0, 0),
            candle(100.0, 101.0, 99.0, 100.0, 1),
            candle(100.0, 101.0, 99.0, 100.0, 2),
            candle(100.0, 101.0, 99.0, 100.0, 3),
            candle(100.0, 101.0, 99.0, 100.0, 4),
        ];
        candles.push(candle(100.0, 108.0, 99.8, 107.0, 5));
        let mut values = VegasIndicatorSignalValue::default();
        values.volume_value.volume_ratio = 3.0;
        values.macd_value.macd_line = 1.2;
        values.macd_value.signal_line = 1.0;
        values.macd_value.histogram = 0.2;
        values.macd_value.histogram_increasing = true;

        assert!(strategy()
            .compressed_range_breakout_decision(&candles, &values)
            .is_none());
    }
}
