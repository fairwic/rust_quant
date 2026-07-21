/// EMA 隧道回踩确认生成的冻结方向与结构保护位。
#[derive(Debug, Clone, Copy)]
struct EmaTunnelRetestConfirmationDecision {
    direction: SignalDirect,
    protective_stop: f64,
}

/// 单个已完成时点的 EMA12/144/169/576 快照。
#[derive(Debug, Clone, Copy)]
struct EmaTunnelSnapshot {
    ema1: f64,
    ema2: f64,
    ema3: f64,
    ema4: f64,
}

impl EmaTunnelSnapshot {
    /// 判断短中长期 EMA 是否形成严格多头排列。
    fn is_bullish(self) -> bool {
        self.ema1 > self.ema2 && self.ema2 > self.ema3 && self.ema3 > self.ema4
    }

    /// 判断短中长期 EMA 是否形成严格空头排列。
    fn is_bearish(self) -> bool {
        self.ema1 < self.ema2 && self.ema2 < self.ema3 && self.ema3 < self.ema4
    }
}

impl VegasStrategy {
    /// 用当前 EMA 和当前收盘反推上一根已完成 K 线的 EMA。
    ///
    /// `EMA_t = alpha * close_t + (1-alpha) * EMA_(t-1)`，因此该反推只使用信号时点
    /// 已知值，不需要重新回放或读取未来 K 线。
    fn rollback_ema(current: f64, close: f64, period: usize) -> Option<f64> {
        if period <= 1 || !current.is_finite() || current <= 0.0 || !close.is_finite() {
            return None;
        }
        let alpha = 2.0 / (period as f64 + 1.0);
        let previous = (current - alpha * close) / (1.0 - alpha);
        previous.is_finite().then_some(previous)
    }

    /// 从当前快照依次回退一个已完成 K 线时点。
    fn rollback_ema_tunnel_snapshot(
        snapshot: EmaTunnelSnapshot,
        close: f64,
        ema: EmaSignalConfig,
    ) -> Option<EmaTunnelSnapshot> {
        Some(EmaTunnelSnapshot {
            ema1: Self::rollback_ema(snapshot.ema1, close, ema.ema1_length)?,
            ema2: Self::rollback_ema(snapshot.ema2, close, ema.ema2_length)?,
            ema3: Self::rollback_ema(snapshot.ema3, close, ema.ema3_length)?,
            ema4: Self::rollback_ema(snapshot.ema4, close, ema.ema4_length)?,
        })
    }

    /// 识别“趋势侧干净离开 -> 回踩 EMA144/169 隧道并收复 -> 下一棒突破”的固定三棒序列。
    fn ema_tunnel_retest_confirmation_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<EmaTunnelRetestConfirmationDecision> {
        let config = self.ema_tunnel_retest_confirmation;
        if !config.is_open || data_items.len() < 3 {
            return None;
        }
        let ema = self.ema_signal?;
        let [clean, touch, confirmation] = data_items.get(data_items.len() - 3..)? else {
            return None;
        };
        if clean.confirm != 1 || touch.confirm != 1 || confirmation.confirm != 1 {
            return None;
        }

        let current = EmaTunnelSnapshot {
            ema1: values.ema_values.ema1_value,
            ema2: values.ema_values.ema2_value,
            ema3: values.ema_values.ema3_value,
            ema4: values.ema_values.ema4_value,
        };
        let touch_ema = Self::rollback_ema_tunnel_snapshot(current, confirmation.c, ema)?;
        let clean_ema = Self::rollback_ema_tunnel_snapshot(touch_ema, touch.c, ema)?;
        let buffer = config
            .stop_loss_buffer_ratio
            .is_finite()
            .then_some(config.stop_loss_buffer_ratio.max(0.0))?;

        let long_confirmed = config.enable_long
            && current.is_bullish()
            && touch_ema.is_bullish()
            && clean.l > clean_ema.ema2
            && touch.l <= touch_ema.ema2
            && touch.l >= touch_ema.ema3
            && touch.c > touch_ema.ema2
            && confirmation.c > confirmation.o
            && confirmation.c > touch.h
            && confirmation.l > touch.l;
        if long_confirmed {
            let protective_stop = touch.l.min(confirmation.l) * (1.0 - buffer);
            if protective_stop.is_finite()
                && protective_stop > 0.0
                && protective_stop < confirmation.c
            {
                return Some(EmaTunnelRetestConfirmationDecision {
                    direction: SignalDirect::IsLong,
                    protective_stop,
                });
            }
        }

        let short_confirmed = config.enable_short
            && current.is_bearish()
            && touch_ema.is_bearish()
            && clean.h < clean_ema.ema2
            && touch.h >= touch_ema.ema2
            && touch.h <= touch_ema.ema3
            && touch.c < touch_ema.ema2
            && confirmation.c < confirmation.o
            && confirmation.c < touch.l
            && confirmation.h < touch.h;
        if short_confirmed {
            let protective_stop = touch.h.max(confirmation.h) * (1.0 + buffer);
            if protective_stop.is_finite() && protective_stop > confirmation.c {
                return Some(EmaTunnelRetestConfirmationDecision {
                    direction: SignalDirect::IsShort,
                    protective_stop,
                });
            }
        }

        None
    }
}

#[cfg(test)]
mod ema_tunnel_retest_confirmation_tests {
    use super::*;

    /// 构造已确认测试 K 线。
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

    /// 按 EMA 递推把回踩棒时点快照推进到确认棒时点。
    fn advance(previous: f64, close: f64, period: usize) -> f64 {
        let alpha = 2.0 / (period as f64 + 1.0);
        alpha * close + (1.0 - alpha) * previous
    }

    /// 构造只开启 V70 的研究策略。
    fn strategy() -> VegasStrategy {
        VegasStrategy {
            ema_signal: Some(EmaSignalConfig::default()),
            ema_tunnel_retest_confirmation: EmaTunnelRetestConfirmationConfig {
                is_open: true,
                ..EmaTunnelRetestConfirmationConfig::default()
            },
            ..VegasStrategy::default()
        }
    }

    /// 生成能被反推为指定回踩棒 EMA 的确认棒快照。
    fn current_values(
        touch_snapshot: EmaTunnelSnapshot,
        confirmation_close: f64,
    ) -> VegasIndicatorSignalValue {
        let ema = EmaSignalConfig::default();
        let mut values = VegasIndicatorSignalValue::default();
        values.ema_values = EmaSignalValue {
            ema1_value: advance(touch_snapshot.ema1, confirmation_close, ema.ema1_length),
            ema2_value: advance(touch_snapshot.ema2, confirmation_close, ema.ema2_length),
            ema3_value: advance(touch_snapshot.ema3, confirmation_close, ema.ema3_length),
            ema4_value: advance(touch_snapshot.ema4, confirmation_close, ema.ema4_length),
            ..EmaSignalValue::default()
        };
        values
    }

    #[test]
    fn bullish_tunnel_retest_requires_next_bar_breakout() {
        let candles = vec![
            candle(102.0, 103.0, 101.0, 102.0, 1),
            candle(102.0, 103.0, 99.5, 102.0, 2),
            candle(102.0, 104.0, 100.5, 103.5, 3),
        ];
        let touch_ema = EmaTunnelSnapshot {
            ema1: 104.0,
            ema2: 100.0,
            ema3: 99.0,
            ema4: 95.0,
        };
        let decision = strategy()
            .ema_tunnel_retest_confirmation_decision(&candles, &current_values(touch_ema, 103.5))
            .expect("clean bullish retest should confirm");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert!(decision.protective_stop < candles[2].c);
    }

    #[test]
    fn bearish_tunnel_retest_is_symmetric() {
        let candles = vec![
            candle(98.0, 99.0, 97.0, 98.0, 1),
            candle(98.0, 100.5, 97.5, 98.0, 2),
            candle(98.0, 98.8, 96.0, 97.0, 3),
        ];
        let touch_ema = EmaTunnelSnapshot {
            ema1: 96.0,
            ema2: 100.0,
            ema3: 101.0,
            ema4: 105.0,
        };
        let decision = strategy()
            .ema_tunnel_retest_confirmation_decision(&candles, &current_values(touch_ema, 97.0))
            .expect("clean bearish retest should confirm");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert!(decision.protective_stop > candles[2].c);
    }

    #[test]
    fn retest_is_rejected_when_prior_bar_was_not_cleanly_above_tunnel() {
        let candles = vec![
            candle(100.0, 102.0, 99.0, 101.0, 1),
            candle(102.0, 103.0, 99.5, 102.0, 2),
            candle(102.0, 104.0, 100.5, 103.5, 3),
        ];
        let touch_ema = EmaTunnelSnapshot {
            ema1: 104.0,
            ema2: 100.0,
            ema3: 99.0,
            ema4: 95.0,
        };

        assert!(strategy()
            .ema_tunnel_retest_confirmation_decision(&candles, &current_values(touch_ema, 103.5))
            .is_none());
    }

    #[test]
    fn retest_is_rejected_when_touch_closes_inside_tunnel() {
        let candles = vec![
            candle(102.0, 103.0, 101.0, 102.0, 1),
            candle(102.0, 103.0, 99.5, 99.8, 2),
            candle(100.0, 104.0, 100.5, 103.5, 3),
        ];
        let touch_ema = EmaTunnelSnapshot {
            ema1: 104.0,
            ema2: 100.0,
            ema3: 99.0,
            ema4: 95.0,
        };

        assert!(strategy()
            .ema_tunnel_retest_confirmation_decision(&candles, &current_values(touch_ema, 103.5))
            .is_none());
    }

    #[test]
    fn retest_is_rejected_without_confirmation_breakout() {
        let candles = vec![
            candle(102.0, 103.0, 101.0, 102.0, 1),
            candle(102.0, 103.0, 99.5, 102.0, 2),
            candle(102.0, 102.9, 100.5, 102.8, 3),
        ];
        let touch_ema = EmaTunnelSnapshot {
            ema1: 104.0,
            ema2: 100.0,
            ema3: 99.0,
            ema4: 95.0,
        };

        assert!(strategy()
            .ema_tunnel_retest_confirmation_decision(&candles, &current_values(touch_ema, 102.8))
            .is_none());
    }

    #[test]
    fn retest_is_rejected_when_ema_alignment_is_mixed() {
        let candles = vec![
            candle(102.0, 103.0, 101.0, 102.0, 1),
            candle(102.0, 103.0, 99.5, 102.0, 2),
            candle(102.0, 104.0, 100.5, 103.5, 3),
        ];
        let touch_ema = EmaTunnelSnapshot {
            ema1: 98.0,
            ema2: 100.0,
            ema3: 99.0,
            ema4: 95.0,
        };

        assert!(strategy()
            .ema_tunnel_retest_confirmation_decision(&candles, &current_values(touch_ema, 103.5))
            .is_none());
    }
}
