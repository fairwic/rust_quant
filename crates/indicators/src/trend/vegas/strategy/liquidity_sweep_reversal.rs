/// 扫单反转候选给主信号流程返回的方向与结构保护止损。
#[derive(Debug, Clone, Copy, PartialEq)]
struct LiquiditySweepReversalDecision {
    /// 确认棒收盘后允许的交易方向。
    direction: SignalDirect,
    /// 两根形态 K 线极值外的保护止损。
    protective_stop: f64,
    /// 是否来自收盘突破失败后立即收回结构内的独立空头形态。
    failed_breakout_close_reentry: bool,
    /// 是否来自收盘跌破失败后立即收回结构内的独立多头形态。
    failed_breakdown_close_reentry: bool,
    /// 是否来自失败跌破收回、更高低点回踩和后续突破的四棒多头形态。
    failed_breakdown_higher_low_breakout: bool,
    /// 是否来自扫高收回后跌破确认棒低点的三棒空头形态。
    upper_sweep_confirmation_low_break: bool,
    /// 是否来自扫低收回后突破确认棒高点的三棒多头形态。
    lower_sweep_confirmation_high_break: bool,
    /// 是否来自确认收回后紧邻的第一次中点回测。
    first_retest_confirmation: bool,
    /// 首次回测发生在确认棒后的第几根；非首次回测信号固定为 0。
    first_retest_wait_bars: usize,
}

impl VegasStrategy {
    /// 使用当前及此前已完成 K 线识别两棒确认的流动性扫单反转。
    ///
    /// 冲击棒必须先突破更早的结构高低点并显著放量；当前棒再次扫过极值后
    /// 以长影线反向收盘，信号才在当前棒收盘生成，避免在冲击棒上直接猜顶或猜底。
    fn liquidity_sweep_reversal_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<LiquiditySweepReversalDecision> {
        let config = self.liquidity_sweep_reversal;
        let lookback = config.lookback_bars.max(1);
        if !config.is_open {
            return None;
        }
        if data_items.len() < lookback.saturating_add(2) {
            return None;
        }

        let current_index = data_items.len() - 1;
        let shock_index = current_index - 1;
        let current = &data_items[current_index];
        let shock = &data_items[shock_index];
        let prior = &data_items[shock_index - lookback..shock_index];
        let prior_high = prior
            .iter()
            .map(CandleItem::h)
            .fold(f64::NEG_INFINITY, f64::max);
        let prior_low = prior
            .iter()
            .map(CandleItem::l)
            .fold(f64::INFINITY, f64::min);
        let volume_baseline_bars = self
            .volume_signal
            .map(|value| value.volume_bar_num)
            .unwrap_or(4)
            .max(1);
        let shock_volume_ratio =
            relative_volume_ratio_at(data_items, shock_index, volume_baseline_bars)?;
        let is_expansion_shock = shock.body_ratio() >= config.shock_min_body_ratio
            && shock_volume_ratio >= config.shock_min_volume_ratio;
        if !is_expansion_shock {
            return self
                .liquidity_sweep_first_retest_decision(data_items, values)
                .or_else(|| {
                    self.failed_breakdown_higher_low_breakout_long_decision(data_items, values)
                })
                .or_else(|| {
                    self.upper_sweep_confirmation_low_break_short_decision(data_items, values)
                })
                .or_else(|| {
                    self.lower_sweep_confirmation_high_break_long_decision(data_items, values)
                });
        }

        let fib_ratio = values.fib_retracement_value.retracement_ratio;
        let stop_buffer = config.stop_loss_buffer_ratio.max(0.0);
        let short_volatility_allowed = !config.require_short_below_choppy_atr_min
            || self.short_sweep_is_below_choppy_atr_min(values);
        let short_confirmed = config.enable_short
            && short_volatility_allowed
            && shock.c > shock.o
            && shock.h > prior_high
            && current.h >= shock.h
            && current.c < current.o
            && current.c < shock.c
            && current.up_shadow_ratio() >= config.confirmation_min_shadow_ratio
            && !values.ema_values.is_long_trend
            && fib_ratio >= config.fib_midline_ratio;
        if short_confirmed {
            let pattern_high = shock.h.max(current.h);
            return Some(LiquiditySweepReversalDecision {
                direction: SignalDirect::IsShort,
                protective_stop: pattern_high * (1.0 + stop_buffer),
                failed_breakout_close_reentry: false,
                failed_breakdown_close_reentry: false,
                failed_breakdown_higher_low_breakout: false,
                upper_sweep_confirmation_low_break: false,
                lower_sweep_confirmation_high_break: false,
                first_retest_confirmation: false,
                first_retest_wait_bars: 0,
            });
        }

        let shock_midpoint = (shock.o + shock.c) / 2.0;
        let failed_breakout_close_reentry = config.enable_short
            && config.enable_failed_breakout_close_reentry_short
            && shock.c > shock.o
            && shock.c > prior_high
            && current.c < current.o
            && current.c < prior_high
            && current.c < shock_midpoint
            && !values.ema_values.is_long_trend
            && fib_ratio >= config.fib_midline_ratio;
        if failed_breakout_close_reentry {
            // 收盘重新跌回结构内比低 ATR 或同高复扫更强，因此不复用仅属于原扫高形态的波动门禁。
            let pattern_high = shock.h.max(current.h);
            return Some(LiquiditySweepReversalDecision {
                direction: SignalDirect::IsShort,
                protective_stop: pattern_high * (1.0 + stop_buffer),
                failed_breakout_close_reentry: true,
                failed_breakdown_close_reentry: false,
                failed_breakdown_higher_low_breakout: false,
                upper_sweep_confirmation_low_break: false,
                lower_sweep_confirmation_high_break: false,
                first_retest_confirmation: false,
                first_retest_wait_bars: 0,
            });
        }

        let failed_breakdown_close_reentry = config.enable_failed_breakdown_close_reentry_long
            && shock.c < shock.o
            && shock.c < prior_low
            && current.c > current.o
            && current.c > prior_low
            && current.c > shock_midpoint
            && !values.ema_values.is_short_trend
            && fib_ratio <= config.fib_midline_ratio;
        if failed_breakdown_close_reentry {
            let pattern_low = shock.l.min(current.l);
            return Some(LiquiditySweepReversalDecision {
                direction: SignalDirect::IsLong,
                protective_stop: pattern_low * (1.0 - stop_buffer).max(0.0),
                failed_breakout_close_reentry: false,
                failed_breakdown_close_reentry: true,
                failed_breakdown_higher_low_breakout: false,
                upper_sweep_confirmation_low_break: false,
                lower_sweep_confirmation_high_break: false,
                first_retest_confirmation: false,
                first_retest_wait_bars: 0,
            });
        }

        let long_confirmed = config.enable_long
            && shock.c < shock.o
            && shock.l < prior_low
            && current.l <= shock.l
            && current.c > current.o
            && current.c > shock.c
            && current.down_shadow_ratio() >= config.confirmation_min_shadow_ratio
            && !values.ema_values.is_short_trend
            && fib_ratio <= config.fib_midline_ratio;
        if long_confirmed {
            let pattern_low = shock.l.min(current.l);
            return Some(LiquiditySweepReversalDecision {
                direction: SignalDirect::IsLong,
                protective_stop: pattern_low * (1.0 - stop_buffer).max(0.0),
                failed_breakout_close_reentry: false,
                failed_breakdown_close_reentry: false,
                failed_breakdown_higher_low_breakout: false,
                upper_sweep_confirmation_low_break: false,
                lower_sweep_confirmation_high_break: false,
                first_retest_confirmation: false,
                first_retest_wait_bars: 0,
            });
        }

        self.liquidity_sweep_first_retest_decision(data_items, values)
            .or_else(|| self.failed_breakdown_higher_low_breakout_long_decision(data_items, values))
            .or_else(|| self.upper_sweep_confirmation_low_break_short_decision(data_items, values))
            .or_else(|| self.lower_sweep_confirmation_high_break_long_decision(data_items, values))
    }

    /// 识别“扫流动性、确认收回、首次回测守住”的严格三棒序列。
    ///
    /// 三个角色固定为当前棒及其前两根，任何更晚确认都会自然过期；这条边界防止回测用后续走势
    /// 为历史扫单补造更好看的入场。量能和形态阈值完全复用两棒规则，只验证新的时序机理。
    fn liquidity_sweep_first_retest_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<LiquiditySweepReversalDecision> {
        let config = self.liquidity_sweep_reversal;
        let lookback = config.lookback_bars.max(1);
        if data_items.len() < lookback.saturating_add(3) {
            return None;
        }

        let retest_index = data_items.len() - 1;
        let confirmation_index = retest_index - 1;
        let shock_index = retest_index - 2;
        let retest = &data_items[retest_index];
        let confirmation = &data_items[confirmation_index];
        let shock = &data_items[shock_index];
        let prior = &data_items[shock_index - lookback..shock_index];
        let prior_high = prior
            .iter()
            .map(CandleItem::h)
            .fold(f64::NEG_INFINITY, f64::max);
        let prior_low = prior
            .iter()
            .map(CandleItem::l)
            .fold(f64::INFINITY, f64::min);
        let volume_baseline_bars = self
            .volume_signal
            .map(|value| value.volume_bar_num)
            .unwrap_or(4)
            .max(1);
        let shock_volume_ratio =
            relative_volume_ratio_at(data_items, shock_index, volume_baseline_bars)?;
        let first_retest_min_volume_ratio = config
            .first_retest_min_volume_ratio
            .unwrap_or(config.shock_min_volume_ratio);
        if shock.body_ratio() < config.shock_min_body_ratio
            || shock_volume_ratio < first_retest_min_volume_ratio
        {
            // 当前三棒窗口的“冲击”其实可能是四棒序列中的确认棒；它不满足冲击量时，
            // 仍需在显式两棒等待配置下检查再往前一根的真实冲击棒。
            return if config.first_retest_max_wait_bars >= 2 {
                self.liquidity_sweep_second_bar_first_retest_decision(data_items, values)
            } else {
                None
            };
        }

        let midpoint = (shock.o + shock.c) / 2.0;
        let stop_buffer = config.stop_loss_buffer_ratio.max(0.0);
        let short_confirmed = config.enable_first_retest_short
            && shock.c > shock.o
            && shock.h > prior_high
            && confirmation.c < confirmation.o
            && confirmation.c < midpoint
            && retest.h >= midpoint
            && retest.h <= shock.h
            && retest.c < retest.o
            && retest.c < midpoint
            && !values.ema_values.is_long_trend;
        if short_confirmed {
            let pattern_high = shock.h.max(confirmation.h).max(retest.h);
            return Some(LiquiditySweepReversalDecision {
                direction: SignalDirect::IsShort,
                protective_stop: pattern_high * (1.0 + stop_buffer),
                failed_breakout_close_reentry: false,
                failed_breakdown_close_reentry: false,
                failed_breakdown_higher_low_breakout: false,
                upper_sweep_confirmation_low_break: false,
                lower_sweep_confirmation_high_break: false,
                first_retest_confirmation: true,
                first_retest_wait_bars: 1,
            });
        }

        let long_confirmed = config.enable_first_retest_long
            && shock.c < shock.o
            && shock.l < prior_low
            && confirmation.c > confirmation.o
            && confirmation.c > midpoint
            && retest.l <= midpoint
            && retest.l >= shock.l
            && retest.c > retest.o
            && retest.c > midpoint
            && !values.ema_values.is_short_trend;
        if long_confirmed {
            let pattern_low = shock.l.min(confirmation.l).min(retest.l);
            return Some(LiquiditySweepReversalDecision {
                direction: SignalDirect::IsLong,
                protective_stop: pattern_low * (1.0 - stop_buffer).max(0.0),
                failed_breakout_close_reentry: false,
                failed_breakdown_close_reentry: false,
                failed_breakdown_higher_low_breakout: false,
                upper_sweep_confirmation_low_break: false,
                lower_sweep_confirmation_high_break: false,
                first_retest_confirmation: true,
                first_retest_wait_bars: 1,
            });
        }

        if config.first_retest_max_wait_bars >= 2 {
            self.liquidity_sweep_second_bar_first_retest_decision(data_items, values)
        } else {
            None
        }
    }

    /// 识别确认后的第一根未触及中点、第二根才完成首次回测的四棒序列。
    ///
    /// 等待棒必须留在确认方向一侧且不能刷新冲击极值；这样第二根触及中点才是真正的
    /// 第一次回测。窗口固定为两根，当前棒之后的数据不会参与信号判断。
    fn liquidity_sweep_second_bar_first_retest_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<LiquiditySweepReversalDecision> {
        let config = self.liquidity_sweep_reversal;
        let lookback = config.lookback_bars.max(1);
        if data_items.len() < lookback.saturating_add(4) {
            return None;
        }

        let retest_index = data_items.len() - 1;
        let waiting_index = retest_index - 1;
        let confirmation_index = retest_index - 2;
        let shock_index = retest_index - 3;
        let retest = &data_items[retest_index];
        let waiting = &data_items[waiting_index];
        let confirmation = &data_items[confirmation_index];
        let shock = &data_items[shock_index];
        let prior = &data_items[shock_index - lookback..shock_index];
        let prior_high = prior
            .iter()
            .map(CandleItem::h)
            .fold(f64::NEG_INFINITY, f64::max);
        let prior_low = prior
            .iter()
            .map(CandleItem::l)
            .fold(f64::INFINITY, f64::min);
        let volume_baseline_bars = self
            .volume_signal
            .map(|value| value.volume_bar_num)
            .unwrap_or(4)
            .max(1);
        let shock_volume_ratio =
            relative_volume_ratio_at(data_items, shock_index, volume_baseline_bars)?;
        let first_retest_min_volume_ratio = config
            .first_retest_min_volume_ratio
            .unwrap_or(config.shock_min_volume_ratio);
        if shock.body_ratio() < config.shock_min_body_ratio
            || shock_volume_ratio < first_retest_min_volume_ratio
        {
            return None;
        }

        let midpoint = (shock.o + shock.c) / 2.0;
        let stop_buffer = config.stop_loss_buffer_ratio.max(0.0);
        let short_confirmed = config.enable_first_retest_short
            && shock.c > shock.o
            && shock.h > prior_high
            && confirmation.c < confirmation.o
            && confirmation.c < midpoint
            // 严格小于中点证明等待棒尚未消耗第一次回测机会。
            && waiting.h < midpoint
            && waiting.c < midpoint
            && waiting.h <= shock.h
            && retest.h >= midpoint
            && retest.h <= shock.h
            && retest.c < retest.o
            && retest.c < midpoint
            && !values.ema_values.is_long_trend;
        if short_confirmed {
            let pattern_high = shock.h.max(confirmation.h).max(waiting.h).max(retest.h);
            return Some(LiquiditySweepReversalDecision {
                direction: SignalDirect::IsShort,
                protective_stop: pattern_high * (1.0 + stop_buffer),
                failed_breakout_close_reentry: false,
                failed_breakdown_close_reentry: false,
                failed_breakdown_higher_low_breakout: false,
                upper_sweep_confirmation_low_break: false,
                lower_sweep_confirmation_high_break: false,
                first_retest_confirmation: true,
                first_retest_wait_bars: 2,
            });
        }

        let long_confirmed = config.enable_first_retest_long
            && shock.c < shock.o
            && shock.l < prior_low
            && confirmation.c > confirmation.o
            && confirmation.c > midpoint
            // 严格大于中点与空头规则镜像，避免把第二次触及误标为首次回测。
            && waiting.l > midpoint
            && waiting.c > midpoint
            && waiting.l >= shock.l
            && retest.l <= midpoint
            && retest.l >= shock.l
            && retest.c > retest.o
            && retest.c > midpoint
            && !values.ema_values.is_short_trend;
        if long_confirmed {
            let pattern_low = shock.l.min(confirmation.l).min(waiting.l).min(retest.l);
            return Some(LiquiditySweepReversalDecision {
                direction: SignalDirect::IsLong,
                protective_stop: pattern_low * (1.0 - stop_buffer).max(0.0),
                failed_breakout_close_reentry: false,
                failed_breakdown_close_reentry: false,
                failed_breakdown_higher_low_breakout: false,
                upper_sweep_confirmation_low_break: false,
                lower_sweep_confirmation_high_break: false,
                first_retest_confirmation: true,
                first_retest_wait_bars: 2,
            });
        }

        None
    }

    /// 等待失败跌破收回后形成更高低点，并在下一棒突破时确认多头。
    ///
    /// 四个角色必须严格相邻；超过这一窗口即过期，避免用更晚上涨为历史跌破补造入场。
    /// 保护位锚定回踩低点，因为该点失守会直接否定“更高低点已经形成”的市场机理。
    fn failed_breakdown_higher_low_breakout_long_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<LiquiditySweepReversalDecision> {
        let config = self.liquidity_sweep_reversal;
        let lookback = config.lookback_bars.max(1);
        if !config.enable_failed_breakdown_higher_low_breakout_long
            || data_items.len() < lookback.saturating_add(4)
        {
            return None;
        }

        let breakout_index = data_items.len() - 1;
        let pullback_index = breakout_index - 1;
        let reclaim_index = breakout_index - 2;
        let shock_index = breakout_index - 3;
        let breakout = &data_items[breakout_index];
        let pullback = &data_items[pullback_index];
        let reclaim = &data_items[reclaim_index];
        let shock = &data_items[shock_index];
        let prior_low = data_items[shock_index - lookback..shock_index]
            .iter()
            .map(CandleItem::l)
            .fold(f64::INFINITY, f64::min);
        let volume_baseline_bars = self
            .volume_signal
            .map(|value| value.volume_bar_num)
            .unwrap_or(4)
            .max(1);
        let shock_volume_ratio =
            relative_volume_ratio_at(data_items, shock_index, volume_baseline_bars)?;
        let shock_midpoint = (shock.o + shock.c) / 2.0;
        let reclaim_midpoint = (reclaim.o + reclaim.c) / 2.0;
        let fib_ratio = values.fib_retracement_value.retracement_ratio;

        let confirmed = shock.c < shock.o
            && shock.c < prior_low
            && shock.body_ratio() >= config.shock_min_body_ratio
            && shock_volume_ratio >= config.shock_min_volume_ratio
            && reclaim.c > reclaim.o
            && reclaim.c > prior_low
            && reclaim.c > shock_midpoint
            && pullback.l > shock.l
            && pullback.l <= reclaim_midpoint
            && pullback.c > prior_low
            && pullback.c <= reclaim.c
            && breakout.c > breakout.o
            && breakout.c > reclaim.h
            && !values.ema_values.is_short_trend
            && fib_ratio <= config.fib_midline_ratio;
        if !confirmed {
            return None;
        }

        let stop_buffer = config.stop_loss_buffer_ratio.max(0.0);
        Some(LiquiditySweepReversalDecision {
            direction: SignalDirect::IsLong,
            protective_stop: pullback.l * (1.0 - stop_buffer).max(0.0),
            failed_breakout_close_reentry: false,
            failed_breakdown_close_reentry: false,
            failed_breakdown_higher_low_breakout: true,
            upper_sweep_confirmation_low_break: false,
            lower_sweep_confirmation_high_break: false,
            first_retest_confirmation: false,
            first_retest_wait_bars: 0,
        })
    }

    /// 在扫高收回后等待下一根跌破确认棒低点，确认不给回测的空头加速。
    ///
    /// 三根角色严格相邻，且只在既有两棒和首次回测规则均未入场时评估，
    /// 从而保持冻结基线优先，并防止把更晚下跌追溯为历史扫高信号。
    fn upper_sweep_confirmation_low_break_short_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<LiquiditySweepReversalDecision> {
        let config = self.liquidity_sweep_reversal;
        let lookback = config.lookback_bars.max(1);
        if !config.enable_upper_sweep_confirmation_low_break_short
            || data_items.len() < lookback.saturating_add(3)
        {
            return None;
        }
        if config.require_upper_sweep_confirmation_macd_above_zero && !values.macd_value.above_zero
        {
            return None;
        }

        let break_index = data_items.len() - 1;
        let confirmation_index = break_index - 1;
        let shock_index = break_index - 2;
        let break_bar = &data_items[break_index];
        let confirmation = &data_items[confirmation_index];
        let shock = &data_items[shock_index];
        let prior_high = data_items[shock_index - lookback..shock_index]
            .iter()
            .map(CandleItem::h)
            .fold(f64::NEG_INFINITY, f64::max);
        let volume_baseline_bars = self
            .volume_signal
            .map(|value| value.volume_bar_num)
            .unwrap_or(4)
            .max(1);
        let shock_volume_ratio =
            relative_volume_ratio_at(data_items, shock_index, volume_baseline_bars)?;
        let shock_midpoint = (shock.o + shock.c) / 2.0;
        let confirmed = shock.c > shock.o
            && shock.h > prior_high
            && shock.c > prior_high
            && shock.body_ratio() >= config.shock_min_body_ratio
            && shock_volume_ratio >= config.shock_min_volume_ratio
            && confirmation.c < confirmation.o
            && confirmation.c < shock_midpoint
            && confirmation.h <= shock.h
            && break_bar.c < break_bar.o
            && break_bar.c < confirmation.l
            && break_bar.h <= shock.h
            && !values.ema_values.is_long_trend;
        if !confirmed {
            return None;
        }

        let stop_buffer = config.stop_loss_buffer_ratio.max(0.0);
        Some(LiquiditySweepReversalDecision {
            direction: SignalDirect::IsShort,
            protective_stop: confirmation.h * (1.0 + stop_buffer),
            failed_breakout_close_reentry: false,
            failed_breakdown_close_reentry: false,
            failed_breakdown_higher_low_breakout: false,
            upper_sweep_confirmation_low_break: true,
            lower_sweep_confirmation_high_break: false,
            first_retest_confirmation: false,
            first_retest_wait_bars: 0,
        })
    }

    /// 在扫低收回后等待下一根突破确认棒高点，以局部 bullish BOS 确认多头反转。
    ///
    /// 三根角色严格相邻，并复用上方扫单空头的 20 根结构、实体、量能和止损缓冲；
    /// 当前棒收盘前不生成信号，避免用更晚反弹替历史扫低补造入场。
    fn lower_sweep_confirmation_high_break_long_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<LiquiditySweepReversalDecision> {
        let config = self.liquidity_sweep_reversal;
        let lookback = config.lookback_bars.max(1);
        if !config.enable_lower_sweep_confirmation_high_break_long
            || data_items.len() < lookback.saturating_add(3)
        {
            return None;
        }
        if config.require_lower_sweep_confirmation_macd_below_zero
            && !(values.macd_value.macd_line < 0.0)
        {
            return None;
        }

        let break_index = data_items.len() - 1;
        let confirmation_index = break_index - 1;
        let shock_index = break_index - 2;
        let break_bar = &data_items[break_index];
        let confirmation = &data_items[confirmation_index];
        let shock = &data_items[shock_index];
        let prior_low = data_items[shock_index - lookback..shock_index]
            .iter()
            .map(CandleItem::l)
            .fold(f64::INFINITY, f64::min);
        let volume_baseline_bars = self
            .volume_signal
            .map(|value| value.volume_bar_num)
            .unwrap_or(4)
            .max(1);
        let shock_volume_ratio =
            relative_volume_ratio_at(data_items, shock_index, volume_baseline_bars)?;
        let shock_midpoint = (shock.o + shock.c) / 2.0;
        let confirmed = shock.c < shock.o
            && shock.l < prior_low
            && shock.c < prior_low
            && shock.body_ratio() >= config.shock_min_body_ratio
            && shock_volume_ratio >= config.shock_min_volume_ratio
            && confirmation.c > confirmation.o
            && confirmation.c > shock_midpoint
            && confirmation.l >= shock.l
            && break_bar.c > break_bar.o
            && break_bar.c > confirmation.h
            && break_bar.l >= shock.l
            && !values.ema_values.is_short_trend;
        if !confirmed {
            return None;
        }

        let stop_buffer = config.stop_loss_buffer_ratio.max(0.0);
        Some(LiquiditySweepReversalDecision {
            direction: SignalDirect::IsLong,
            protective_stop: confirmation.l * (1.0 - stop_buffer).max(0.0),
            failed_breakout_close_reentry: false,
            failed_breakdown_close_reentry: false,
            failed_breakdown_higher_low_breakout: false,
            upper_sweep_confirmation_low_break: false,
            lower_sweep_confirmation_high_break: true,
            first_retest_confirmation: false,
            first_retest_wait_bars: 0,
        })
    }

    /// 判断当前信号 ATR 是否严格低于已经配置的震荡波动带下界。
    ///
    /// 候选显式要求该条件时，缺失或非法的自适应波动配置按不通过处理，避免静默放宽研究规则。
    fn short_sweep_is_below_choppy_atr_min(&self, values: &VegasIndicatorSignalValue) -> bool {
        let adaptive = self.cross_asset_adaptive_threshold;
        let band = adaptive.choppy_volatility_filter;
        let atr_ratio = values.cross_asset_adaptive_value.atr_ratio;
        adaptive.is_open
            && band.is_open
            && atr_ratio.is_finite()
            && band.min_atr_ratio.is_finite()
            && band.min_atr_ratio > 0.0
            && atr_ratio < band.min_atr_ratio
    }

    /// 判断空头是否同时遭遇长下影、布林多头支撑与 MACD 回升三重冲突。
    fn is_bullish_rejection_momentum_recovery_short(values: &VegasIndicatorSignalValue) -> bool {
        let hammer = &values.kline_hammer_value;
        let bollinger = &values.bollinger_value;
        let macd = &values.macd_value;
        hammer.is_hammer
            && hammer.is_long_signal
            && hammer.down_shadow_ratio >= 0.60
            && hammer.body_ratio <= 0.40
            && bollinger.is_long_signal
            && !bollinger.is_short_signal
            && macd.above_zero
            && macd.histogram < 0.0
            && macd.histogram_increasing
    }

    /// 对已生成的空头应用窄口径方向冲突拦截，并保留可审计原因。
    fn apply_bullish_rejection_momentum_recovery_short_block(
        &self,
        values: &VegasIndicatorSignalValue,
        signal_result: &mut SignalResult,
    ) {
        if self
            .entry_block_config
            .block_bullish_rejection_momentum_recovery_short
            && signal_result.should_sell.unwrap_or(false)
            && Self::is_bullish_rejection_momentum_recovery_short(values)
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("BULLISH_REJECTION_MOMENTUM_RECOVERY_SHORT_BLOCK".to_string());
        }
    }

    /// 判断空头信号棒是否形成下探被承接的长下影拒绝形态。
    fn is_short_lower_rejection(
        values: &VegasIndicatorSignalValue,
        min_lower_shadow_ratio: f64,
        max_upper_shadow_ratio: f64,
    ) -> bool {
        let hammer = &values.kline_hammer_value;
        hammer.down_shadow_ratio >= min_lower_shadow_ratio
            && hammer.up_shadow_ratio <= max_upper_shadow_ratio
    }

    /// 使用信号时点已完成 K 线阻断方向冲突空头，不等待后续 K 线补偿入场。
    fn apply_short_lower_rejection_block(
        &self,
        values: &VegasIndicatorSignalValue,
        signal_result: &mut SignalResult,
    ) {
        let config = self.entry_block_config;
        if config.block_short_lower_rejection_entry
            && signal_result.should_sell.unwrap_or(false)
            && Self::is_short_lower_rejection(
                values,
                config.short_rejection_min_lower_shadow_ratio,
                config.short_rejection_max_upper_shadow_ratio,
            )
        {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("SHORT_LOWER_REJECTION_ENTRY_BLOCK".to_string());
        }
    }
}

#[cfg(test)]
mod liquidity_sweep_reversal_tests {
    use super::*;

    /// 构造测试 K 线，保持价格、成交量与确认状态显式可读。
    fn candle(o: f64, h: f64, l: f64, c: f64, v: f64, ts: i64) -> CandleItem {
        CandleItem {
            o,
            h,
            l,
            c,
            v,
            ts,
            confirm: 1,
        }
    }

    /// 构造只开启扫单反转的研究策略，避免其他配置影响单元判断。
    fn strategy() -> VegasStrategy {
        VegasStrategy {
            volume_signal: Some(VolumeSignalConfig {
                volume_bar_num: 4,
                ..VolumeSignalConfig::default()
            }),
            liquidity_sweep_reversal: LiquiditySweepReversalConfig {
                is_open: true,
                ..LiquiditySweepReversalConfig::default()
            },
            ..VegasStrategy::default()
        }
    }

    #[test]
    fn upper_liquidity_sweep_waits_for_bearish_confirmation_bar() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 110.0, 99.0, 108.0, 30.0, 20));
        let mut values = VegasIndicatorSignalValue::default();
        values.fib_retracement_value.retracement_ratio = 0.60;

        assert!(strategy()
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());

        candles.push(candle(108.0, 111.0, 105.0, 106.0, 12.0, 21));
        let decision = strategy()
            .liquidity_sweep_reversal_decision(&candles, &values)
            .expect("bearish confirmation should produce a short");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!((decision.protective_stop - 111.666).abs() < 1e-9);
        assert!(!decision.failed_breakout_close_reentry);
    }

    #[test]
    fn lower_liquidity_sweep_has_symmetric_long_confirmation() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 101.0, 90.0, 92.0, 30.0, 20));
        candles.push(candle(92.0, 95.0, 89.0, 94.0, 12.0, 21));
        let mut values = VegasIndicatorSignalValue::default();
        values.fib_retracement_value.retracement_ratio = 0.40;

        let decision = strategy()
            .liquidity_sweep_reversal_decision(&candles, &values)
            .expect("bullish confirmation should produce a long");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!((decision.protective_stop - 88.466).abs() < 1e-9);
        assert!(!decision.failed_breakout_close_reentry);
        assert!(!decision.failed_breakdown_close_reentry);
    }

    #[test]
    fn failed_breakdown_long_waits_for_close_back_inside_prior_support() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 101.0, 90.0, 92.0, 30.0, 20));
        candles.push(candle(92.0, 102.0, 91.0, 101.0, 12.0, 21));
        let mut candidate = strategy();
        candidate
            .liquidity_sweep_reversal
            .enable_failed_breakdown_close_reentry_long = true;
        candidate.liquidity_sweep_reversal.enable_long = false;
        let mut values = VegasIndicatorSignalValue::default();
        values.fib_retracement_value.retracement_ratio = 0.40;

        let decision = candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .expect("close back above prior support should confirm the long");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!((decision.protective_stop - 89.46).abs() < 1e-9);
        assert!(decision.failed_breakdown_close_reentry);
    }

    #[test]
    fn higher_low_breakout_long_requires_four_adjacent_completed_bars() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 101.0, 90.0, 92.0, 30.0, 20));
        candles.push(candle(92.0, 102.0, 91.0, 101.0, 12.0, 21));
        candles.push(candle(101.0, 101.0, 96.0, 99.5, 11.0, 22));
        let mut candidate = strategy();
        candidate.liquidity_sweep_reversal.enable_long = false;
        candidate.liquidity_sweep_reversal.enable_short = false;
        candidate
            .liquidity_sweep_reversal
            .enable_failed_breakdown_higher_low_breakout_long = true;
        let mut values = VegasIndicatorSignalValue::default();
        values.fib_retracement_value.retracement_ratio = 0.40;

        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());

        candles.push(candle(99.5, 103.0, 99.0, 102.5, 10.0, 23));
        let decision = candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .expect("breakout above the reclaim high should confirm the higher-low long");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!((decision.protective_stop - 95.424).abs() < 1e-9);
        assert!(decision.failed_breakdown_higher_low_breakout);

        candles.push(candle(102.5, 104.0, 101.0, 103.0, 10.0, 24));
        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());
    }

    #[test]
    fn upper_sweep_low_break_short_requires_three_adjacent_completed_bars() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 110.0, 99.0, 108.0, 30.0, 20));
        candles.push(candle(108.0, 109.0, 102.0, 103.0, 12.0, 21));
        let mut candidate = strategy();
        candidate.liquidity_sweep_reversal.enable_long = false;
        candidate.liquidity_sweep_reversal.enable_short = false;
        candidate
            .liquidity_sweep_reversal
            .enable_upper_sweep_confirmation_low_break_short = true;
        let mut values = VegasIndicatorSignalValue::default();

        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());

        candles.push(candle(103.0, 104.0, 100.0, 101.0, 10.0, 22));
        let decision = candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .expect("close below the confirmation low should confirm the sweep short");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!((decision.protective_stop - 109.654).abs() < 1e-9);
        assert!(decision.upper_sweep_confirmation_low_break);

        candidate
            .liquidity_sweep_reversal
            .require_upper_sweep_confirmation_macd_above_zero = true;
        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());
        values.macd_value.above_zero = true;
        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_some());

        candles.push(candle(101.0, 102.0, 99.0, 100.0, 10.0, 23));
        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());
    }

    #[test]
    fn lower_sweep_high_break_long_is_symmetric_and_requires_macd_below_zero() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 101.0, 90.0, 92.0, 30.0, 20));
        candles.push(candle(92.0, 98.0, 91.0, 97.0, 12.0, 21));
        let mut candidate = strategy();
        candidate.liquidity_sweep_reversal.enable_long = false;
        candidate.liquidity_sweep_reversal.enable_short = false;
        candidate
            .liquidity_sweep_reversal
            .enable_lower_sweep_confirmation_high_break_long = true;
        let mut values = VegasIndicatorSignalValue::default();

        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());

        candles.push(candle(97.0, 100.0, 96.0, 99.0, 10.0, 22));
        let decision = candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .expect("close above the confirmation high should confirm the sweep long");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!((decision.protective_stop - 90.454).abs() < 1e-9);
        assert!(decision.lower_sweep_confirmation_high_break);

        candidate
            .liquidity_sweep_reversal
            .require_lower_sweep_confirmation_macd_below_zero = true;
        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());
        values.macd_value.macd_line = -0.1;
        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_some());

        candles.push(candle(99.0, 101.0, 98.0, 100.0, 10.0, 23));
        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());
    }

    #[test]
    fn first_retest_short_requires_three_adjacent_completed_bars() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 110.0, 99.0, 108.0, 30.0, 20));
        candles.push(candle(108.0, 109.0, 102.0, 103.0, 12.0, 21));
        let mut candidate = strategy();
        candidate.liquidity_sweep_reversal.enable_first_retest_short = true;
        let values = VegasIndicatorSignalValue::default();

        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());

        candles.push(candle(104.0, 105.0, 102.0, 103.0, 11.0, 22));
        let decision = candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .expect("the adjacent first retest should confirm the short");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!((decision.protective_stop - 110.66).abs() < 1e-9);
        assert!(decision.first_retest_confirmation);
        assert_eq!(decision.first_retest_wait_bars, 1);

        candles.push(candle(103.0, 104.0, 101.0, 102.0, 10.0, 23));
        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());
    }

    #[test]
    fn first_retest_can_use_a_dedicated_standard_volume_floor() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 110.0, 99.0, 108.0, 22.0, 20));
        candles.push(candle(108.0, 109.0, 102.0, 103.0, 12.0, 21));
        candles.push(candle(104.0, 105.0, 102.0, 103.0, 11.0, 22));
        let mut candidate = strategy();
        candidate.liquidity_sweep_reversal.enable_first_retest_short = true;
        candidate.liquidity_sweep_reversal.enable_long = false;
        candidate.liquidity_sweep_reversal.enable_short = false;

        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &VegasIndicatorSignalValue::default())
            .is_none());

        candidate
            .liquidity_sweep_reversal
            .first_retest_min_volume_ratio = Some(2.0);
        let decision = candidate
            .liquidity_sweep_reversal_decision(&candles, &VegasIndicatorSignalValue::default())
            .expect("2.0x dedicated first-retest volume should allow the same structure");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!(decision.first_retest_confirmation);
    }

    #[test]
    fn second_bar_first_retest_requires_an_explicit_two_bar_window() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 110.0, 99.0, 108.0, 30.0, 20));
        candles.push(candle(108.0, 109.0, 102.0, 103.0, 12.0, 21));
        candles.push(candle(103.0, 103.8, 101.0, 102.5, 11.0, 22));
        candles.push(candle(103.0, 105.0, 101.5, 102.0, 11.0, 23));
        let mut candidate = strategy();
        candidate.liquidity_sweep_reversal.enable_long = false;
        candidate.liquidity_sweep_reversal.enable_short = false;
        candidate.liquidity_sweep_reversal.enable_first_retest_short = true;
        let values = VegasIndicatorSignalValue::default();

        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());

        candidate
            .liquidity_sweep_reversal
            .first_retest_max_wait_bars = 2;
        let decision = candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .expect("第一根未触中点、第二根首次回测时应在第二根收盘确认");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!(decision.first_retest_confirmation);
        assert_eq!(decision.first_retest_wait_bars, 2);
        assert!((decision.protective_stop - 110.66).abs() < 1e-9);
    }

    #[test]
    fn second_bar_retest_is_rejected_if_the_waiting_bar_already_touched_midpoint() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 110.0, 99.0, 108.0, 30.0, 20));
        candles.push(candle(108.0, 109.0, 102.0, 103.0, 12.0, 21));
        candles.push(candle(103.0, 104.5, 101.0, 102.5, 11.0, 22));
        candles.push(candle(103.0, 105.0, 101.5, 102.0, 11.0, 23));
        let mut candidate = strategy();
        candidate.liquidity_sweep_reversal.enable_long = false;
        candidate.liquidity_sweep_reversal.enable_short = false;
        candidate.liquidity_sweep_reversal.enable_first_retest_short = true;
        candidate
            .liquidity_sweep_reversal
            .first_retest_max_wait_bars = 2;

        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &VegasIndicatorSignalValue::default())
            .is_none());
    }

    #[test]
    fn second_bar_first_retest_long_is_symmetric() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 101.0, 90.0, 92.0, 30.0, 20));
        candles.push(candle(92.0, 98.0, 91.0, 97.0, 12.0, 21));
        candles.push(candle(97.0, 99.0, 96.2, 98.0, 11.0, 22));
        candles.push(candle(97.0, 98.0, 95.0, 97.5, 11.0, 23));
        let mut candidate = strategy();
        candidate.liquidity_sweep_reversal.enable_long = false;
        candidate.liquidity_sweep_reversal.enable_short = false;
        candidate.liquidity_sweep_reversal.enable_first_retest_long = true;
        candidate
            .liquidity_sweep_reversal
            .first_retest_max_wait_bars = 2;

        let decision = candidate
            .liquidity_sweep_reversal_decision(&candles, &VegasIndicatorSignalValue::default())
            .expect("多头第二根首次回测应与空头规则对称");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert_eq!(decision.first_retest_wait_bars, 2);
        assert!((decision.protective_stop - 89.46).abs() < 1e-9);
    }

    #[test]
    fn first_retest_gate_does_not_replace_an_existing_two_bar_short() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 110.0, 99.0, 108.0, 30.0, 20));
        candles.push(candle(108.0, 111.0, 105.0, 106.0, 12.0, 21));
        let mut candidate = strategy();
        candidate.liquidity_sweep_reversal.enable_first_retest_short = true;
        let mut values = VegasIndicatorSignalValue::default();
        values.fib_retracement_value.retracement_ratio = 0.60;

        let decision = candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .expect("the existing two-bar short must retain priority");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!(!decision.first_retest_confirmation);
    }

    #[test]
    fn first_retest_rejects_a_new_sweep_extreme() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 110.0, 99.0, 108.0, 30.0, 20));
        candles.push(candle(108.0, 109.0, 102.0, 103.0, 12.0, 21));
        candles.push(candle(104.0, 111.0, 102.0, 103.0, 11.0, 22));
        let mut candidate = strategy();
        candidate.liquidity_sweep_reversal.enable_first_retest_short = true;

        assert!(candidate
            .liquidity_sweep_reversal_decision(&candles, &VegasIndicatorSignalValue::default())
            .is_none());
    }

    #[test]
    fn first_retest_long_is_symmetric() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 101.0, 90.0, 92.0, 30.0, 20));
        candles.push(candle(92.0, 98.0, 91.0, 97.0, 12.0, 21));
        candles.push(candle(96.0, 98.0, 95.0, 97.0, 11.0, 22));
        let mut candidate = strategy();
        candidate.liquidity_sweep_reversal.enable_first_retest_long = true;

        let decision = candidate
            .liquidity_sweep_reversal_decision(&candles, &VegasIndicatorSignalValue::default())
            .expect("the adjacent first retest should confirm the long");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!((decision.protective_stop - 89.46).abs() < 1e-9);
        assert!(decision.first_retest_confirmation);
        assert_eq!(decision.first_retest_wait_bars, 1);
    }

    #[test]
    fn failed_close_breakout_reentry_does_not_require_retouch_or_long_upper_shadow() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 104.0, 99.8, 103.0, 30.0, 20));
        candles.push(candle(103.0, 103.2, 99.0, 100.5, 12.0, 21));
        let mut values = VegasIndicatorSignalValue::default();
        values.fib_retracement_value.retracement_ratio = 0.60;
        let candidate = VegasStrategy {
            liquidity_sweep_reversal: LiquiditySweepReversalConfig {
                is_open: true,
                enable_long: false,
                enable_failed_breakout_close_reentry_short: true,
                ..LiquiditySweepReversalConfig::default()
            },
            ..strategy()
        };

        let decision = candidate
            .liquidity_sweep_reversal_decision(&candles, &values)
            .expect("close reentry should confirm the failed breakout short");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!((decision.protective_stop - 104.624).abs() < 1e-9);
        assert!(decision.failed_breakout_close_reentry);
    }

    #[test]
    fn directional_gate_can_keep_short_sweeps_without_adding_countertrend_longs() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 101.0, 90.0, 92.0, 30.0, 20));
        candles.push(candle(92.0, 95.0, 89.0, 94.0, 12.0, 21));
        let mut values = VegasIndicatorSignalValue::default();
        values.fib_retracement_value.retracement_ratio = 0.40;
        let short_only = VegasStrategy {
            liquidity_sweep_reversal: LiquiditySweepReversalConfig {
                is_open: true,
                enable_long: false,
                enable_short: true,
                ..LiquiditySweepReversalConfig::default()
            },
            ..strategy()
        };

        assert!(short_only
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());
    }

    #[test]
    fn short_sweep_can_reuse_existing_choppy_atr_lower_boundary() {
        let mut candles = (0..20)
            .map(|ts| candle(100.0, 101.0, 99.0, 100.0, 10.0, ts))
            .collect::<Vec<_>>();
        candles.push(candle(100.0, 110.0, 99.0, 108.0, 30.0, 20));
        candles.push(candle(108.0, 111.0, 105.0, 106.0, 12.0, 21));
        let mut strategy = strategy();
        strategy.liquidity_sweep_reversal.enable_long = false;
        strategy
            .liquidity_sweep_reversal
            .require_short_below_choppy_atr_min = true;
        strategy.cross_asset_adaptive_threshold.is_open = true;
        strategy
            .cross_asset_adaptive_threshold
            .choppy_volatility_filter = ChoppyVolatilityFilterConfig {
            is_open: true,
            min_atr_ratio: 0.018,
            max_atr_ratio: 0.032,
            ..ChoppyVolatilityFilterConfig::default()
        };
        let mut values = VegasIndicatorSignalValue::default();
        values.fib_retracement_value.retracement_ratio = 0.60;
        values.cross_asset_adaptive_value.atr_ratio = 0.0179;

        assert!(strategy
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_some());

        values.cross_asset_adaptive_value.atr_ratio = 0.018;
        assert!(strategy
            .liquidity_sweep_reversal_decision(&candles, &values)
            .is_none());
    }

    #[test]
    fn bullish_rejection_block_requires_all_three_confirmation_families() {
        let mut values = VegasIndicatorSignalValue::default();
        values.kline_hammer_value = KlineHammerSignalValue {
            is_hammer: true,
            is_long_signal: true,
            down_shadow_ratio: 0.70,
            body_ratio: 0.28,
            ..KlineHammerSignalValue::default()
        };
        values.bollinger_value = BollingerSignalValue {
            is_long_signal: true,
            ..BollingerSignalValue::default()
        };
        values.macd_value = MacdSignalValue {
            above_zero: true,
            histogram: -0.01,
            histogram_increasing: true,
            ..MacdSignalValue::default()
        };

        assert!(VegasStrategy::is_bullish_rejection_momentum_recovery_short(
            &values
        ));
        values.macd_value.histogram_increasing = false;
        assert!(!VegasStrategy::is_bullish_rejection_momentum_recovery_short(&values));
    }

    #[test]
    fn short_lower_rejection_block_uses_only_current_signal_candle_shape() {
        let strategy = VegasStrategy {
            entry_block_config: EntryBlockConfig {
                block_short_lower_rejection_entry: true,
                short_rejection_min_lower_shadow_ratio: 0.55,
                short_rejection_max_upper_shadow_ratio: 0.15,
                ..EntryBlockConfig::default()
            },
            ..VegasStrategy::default()
        };
        let values = VegasIndicatorSignalValue {
            kline_hammer_value: KlineHammerSignalValue {
                down_shadow_ratio: 0.70,
                up_shadow_ratio: 0.05,
                ..KlineHammerSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        let mut signal = SignalResult {
            should_sell: Some(true),
            ..SignalResult::empty()
        };

        strategy.apply_short_lower_rejection_block(&values, &mut signal);

        assert_eq!(signal.should_sell, Some(false));
        assert_eq!(
            signal.filter_reasons,
            vec!["SHORT_LOWER_REJECTION_ENTRY_BLOCK".to_string()]
        );
    }
}
