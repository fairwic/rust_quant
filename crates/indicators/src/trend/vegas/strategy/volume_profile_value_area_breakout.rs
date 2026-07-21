/// 固定历史成交量价值区即时突破生成的方向与边界保护位。
#[derive(Debug, Clone, Copy)]
struct VolumeProfileValueAreaBreakoutDecision {
    direction: SignalDirect,
    protective_stop: f64,
}

/// Wilder DMI/ADX 在当前完成 K 线时点的方向强度快照。
#[derive(Debug, Clone, Copy)]
struct DirectionalMovementSnapshot {
    adx: f64,
    plus_di: f64,
    minus_di: f64,
}

impl VegasStrategy {
    /// 识别“冻结 48 根成交量价值区 -> 当前 2x 放量从区内收盘突破边界”。
    ///
    /// 当前突破棒不参与价值区或历史均量计算，避免同棒信息污染触发边界。
    fn volume_profile_value_area_breakout_decision(
        &self,
        data_items: &[CandleItem],
    ) -> Option<VolumeProfileValueAreaBreakoutDecision> {
        const PROFILE_LOOKBACK: usize = 48;
        const PRICE_BINS: usize = 24;
        const VALUE_AREA_RATIO: f64 = 0.70;
        const BREAKOUT_VOLUME_RATIO: f64 = 2.0;

        let config = self.volume_profile_value_area_breakout;
        if !config.is_open || data_items.len() < PROFILE_LOOKBACK + 1 {
            return None;
        }
        let breakout_index = data_items.len() - 1;
        let profile_start = breakout_index - PROFILE_LOOKBACK;
        let profile_candles = &data_items[profile_start..breakout_index];
        let breakout = data_items.last()?;
        if breakout.confirm != 1 || profile_candles.iter().any(|candle| candle.confirm != 1) {
            return None;
        }

        let average_volume = profile_candles
            .iter()
            .map(|candle| candle.v)
            .sum::<f64>()
            / PROFILE_LOOKBACK as f64;
        if !average_volume.is_finite()
            || average_volume <= 0.0
            || !breakout.v.is_finite()
            || breakout.v < average_volume * BREAKOUT_VOLUME_RATIO
        {
            return None;
        }

        let mut indicator = crate::volume::VolumeProfileIndicator::new(
            PROFILE_LOOKBACK,
            PRICE_BINS,
            VALUE_AREA_RATIO,
        );
        let mut profile = None;
        for candle in profile_candles {
            profile = Some(indicator.next(candle));
        }
        let profile = profile?;
        let value_area_high = profile.value_area_high;
        let value_area_low = profile.value_area_low;
        if !value_area_high.is_finite()
            || !value_area_low.is_finite()
            || value_area_high <= value_area_low
        {
            return None;
        }

        let buffer = config
            .stop_loss_buffer_ratio
            .is_finite()
            .then_some(config.stop_loss_buffer_ratio.max(0.0))?;
        let opened_inside_value_area = breakout.o >= value_area_low && breakout.o <= value_area_high;
        let long_confirmed = config.enable_long
            && opened_inside_value_area
            && breakout.c > value_area_high
            && breakout.c > breakout.o;
        if long_confirmed {
            let protective_stop = value_area_high * (1.0 - buffer);
            if protective_stop.is_finite()
                && protective_stop > 0.0
                && protective_stop < breakout.c
            {
                return Some(VolumeProfileValueAreaBreakoutDecision {
                    direction: SignalDirect::IsLong,
                    protective_stop,
                });
            }
        }

        let short_structure = config.enable_short
            && opened_inside_value_area
            && breakout.c < value_area_low
            && breakout.c < breakout.o;
        if short_structure {
            if config.require_short_adx_25 {
                let dmi = Self::directional_movement_snapshot(data_items, 14, 14)?;
                if dmi.adx < 25.0 || dmi.minus_di <= dmi.plus_di {
                    return None;
                }
            }
            let protective_stop = value_area_low * (1.0 + buffer);
            if protective_stop.is_finite() && protective_stop > breakout.c {
                return Some(VolumeProfileValueAreaBreakoutDecision {
                    direction: SignalDirect::IsShort,
                    protective_stop,
                });
            }
        }

        None
    }

    /// 按 Wilder 平滑计算 DMI 与 ADX；末值包含当前已完成 K 线，不读取未来数据。
    fn directional_movement_snapshot(
        candles: &[CandleItem],
        trend_length: usize,
        smoothing: usize,
    ) -> Option<DirectionalMovementSnapshot> {
        if trend_length == 0 || smoothing == 0 || candles.len() < trend_length + smoothing + 1 {
            return None;
        }
        let mut tr_values = Vec::with_capacity(candles.len().saturating_sub(1));
        let mut plus_dm_values = Vec::with_capacity(candles.len().saturating_sub(1));
        let mut minus_dm_values = Vec::with_capacity(candles.len().saturating_sub(1));
        for index in 1..candles.len() {
            let current = &candles[index];
            let previous = &candles[index - 1];
            let up_move = current.h - previous.h;
            let down_move = previous.l - current.l;
            let true_range = (current.h - current.l)
                .max((current.h - previous.c).abs())
                .max((current.l - previous.c).abs());
            if !true_range.is_finite() || true_range < 0.0 {
                return None;
            }
            tr_values.push(true_range);
            plus_dm_values.push(if up_move > down_move && up_move > 0.0 {
                up_move
            } else {
                0.0
            });
            minus_dm_values.push(if down_move > up_move && down_move > 0.0 {
                down_move
            } else {
                0.0
            });
        }

        let mut smooth_tr = tr_values[..trend_length].iter().sum::<f64>();
        let mut smooth_plus = plus_dm_values[..trend_length].iter().sum::<f64>();
        let mut smooth_minus = minus_dm_values[..trend_length].iter().sum::<f64>();
        let mut dx_values = Vec::with_capacity(tr_values.len() - trend_length + 1);
        dx_values.push(Self::dx_from_directional_movement(
            smooth_tr,
            smooth_plus,
            smooth_minus,
        ));
        for index in trend_length..tr_values.len() {
            smooth_tr = smooth_tr - smooth_tr / trend_length as f64 + tr_values[index];
            smooth_plus =
                smooth_plus - smooth_plus / trend_length as f64 + plus_dm_values[index];
            smooth_minus =
                smooth_minus - smooth_minus / trend_length as f64 + minus_dm_values[index];
            dx_values.push(Self::dx_from_directional_movement(
                smooth_tr,
                smooth_plus,
                smooth_minus,
            ));
        }
        if dx_values.len() < smoothing || smooth_tr <= 0.0 {
            return None;
        }
        let mut adx = dx_values[..smoothing].iter().sum::<f64>() / smoothing as f64;
        for &dx in &dx_values[smoothing..] {
            adx = (adx * (smoothing as f64 - 1.0) + dx) / smoothing as f64;
        }
        Some(DirectionalMovementSnapshot {
            adx,
            plus_di: 100.0 * smooth_plus / smooth_tr,
            minus_di: 100.0 * smooth_minus / smooth_tr,
        })
    }

    /// 把平滑后的正负方向移动量转换成 DX。
    fn dx_from_directional_movement(tr: f64, plus_dm: f64, minus_dm: f64) -> f64 {
        if tr <= 0.0 {
            return 0.0;
        }
        let plus_di = 100.0 * plus_dm / tr;
        let minus_di = 100.0 * minus_dm / tr;
        let total = plus_di + minus_di;
        if total <= 0.0 {
            0.0
        } else {
            100.0 * (plus_di - minus_di).abs() / total
        }
    }
}

#[cfg(test)]
mod volume_profile_value_area_breakout_tests {
    use super::*;

    /// 构造已确认测试 K 线。
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

    /// 构造价格位于 99—101 的冻结 48 根成交量分布。
    fn profile_history() -> Vec<CandleItem> {
        (0..48)
            .map(|index| candle(100.0, 101.0, 99.0, 100.0, 10.0, index))
            .collect()
    }

    /// 构造只开启 V72 的研究策略。
    fn strategy() -> VegasStrategy {
        VegasStrategy {
            volume_profile_value_area_breakout: VolumeProfileValueAreaBreakoutConfig {
                is_open: true,
                ..VolumeProfileValueAreaBreakoutConfig::default()
            },
            ..VegasStrategy::default()
        }
    }

    #[test]
    fn bullish_value_area_breakout_enters_on_breakout_close() {
        let mut candles = profile_history();
        candles.push(candle(100.0, 103.5, 99.8, 103.0, 20.0, 48));

        let decision = strategy()
            .volume_profile_value_area_breakout_decision(&candles)
            .expect("fresh VAH breakout should enter at the completed breakout close");
        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!(decision.protective_stop < candles[48].c);
    }

    #[test]
    fn bearish_value_area_breakout_is_symmetric() {
        let mut candles = profile_history();
        candles.push(candle(100.0, 100.2, 97.5, 98.0, 20.0, 48));

        let decision = strategy()
            .volume_profile_value_area_breakout_decision(&candles)
            .expect("fresh VAL breakdown should enter at the completed breakout close");
        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!(decision.protective_stop > candles[48].c);
    }

    #[test]
    fn breakout_requires_full_frozen_profile_window() {
        let mut candles = profile_history();
        candles.remove(0);
        candles.push(candle(100.0, 103.5, 99.8, 103.0, 20.0, 48));

        assert!(strategy()
            .volume_profile_value_area_breakout_decision(&candles)
            .is_none());
    }

    #[test]
    fn breakout_requires_two_times_historical_volume() {
        let mut candles = profile_history();
        candles.push(candle(100.0, 103.5, 99.8, 103.0, 19.9, 48));

        assert!(strategy()
            .volume_profile_value_area_breakout_decision(&candles)
            .is_none());
    }

    #[test]
    fn breakout_must_open_inside_value_area() {
        let mut candles = profile_history();
        candles.push(candle(102.0, 103.5, 101.5, 103.0, 20.0, 48));

        assert!(strategy()
            .volume_profile_value_area_breakout_decision(&candles)
            .is_none());
    }

    #[test]
    fn unconfirmed_breakout_is_rejected() {
        let mut candles = profile_history();
        let mut breakout = candle(100.0, 103.5, 99.8, 103.0, 20.0, 48);
        breakout.confirm = 0;
        candles.push(breakout);

        assert!(strategy()
            .volume_profile_value_area_breakout_decision(&candles)
            .is_none());
    }

    #[test]
    fn breakout_volume_does_not_change_frozen_boundary() {
        let mut normal = profile_history();
        normal.push(candle(100.0, 103.5, 99.8, 103.0, 20.0, 48));
        let mut extreme_volume = normal.clone();
        extreme_volume[48].v = 1_000_000.0;

        let normal_stop = strategy()
            .volume_profile_value_area_breakout_decision(&normal)
            .expect("normal breakout")
            .protective_stop;
        let extreme_stop = strategy()
            .volume_profile_value_area_breakout_decision(&extreme_volume)
            .expect("extreme volume breakout")
            .protective_stop;
        assert_eq!(normal_stop, extreme_stop);
    }

    #[test]
    fn directional_movement_detects_strong_completed_downtrend() {
        let candles = (0..60)
            .map(|index| {
                let close = 160.0 - index as f64;
                candle(close + 0.4, close + 0.8, close - 0.8, close, 10.0, index)
            })
            .collect::<Vec<_>>();
        let snapshot = VegasStrategy::directional_movement_snapshot(&candles, 14, 14)
            .expect("completed downtrend should produce DMI/ADX");

        assert!(snapshot.adx >= 25.0);
        assert!(snapshot.minus_di > snapshot.plus_di);
    }

    #[test]
    fn adx_gate_rejects_flat_profile_with_single_down_break() {
        let mut candles = profile_history();
        candles.push(candle(100.0, 100.2, 97.5, 98.0, 20.0, 48));
        let gated = VegasStrategy {
            volume_profile_value_area_breakout: VolumeProfileValueAreaBreakoutConfig {
                is_open: true,
                enable_long: false,
                require_short_adx_25: true,
                ..VolumeProfileValueAreaBreakoutConfig::default()
            },
            ..VegasStrategy::default()
        };

        assert!(gated
            .volume_profile_value_area_breakout_decision(&candles)
            .is_none());
    }
}
