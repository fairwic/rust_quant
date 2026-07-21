/// bearish FVG 被完整收复后给主信号流程返回的结构止损与审计信息。
#[derive(Debug, Clone, Copy, PartialEq)]
struct FvgReclaimDecision {
    /// 当前版本只允许生成多头，但显式携带方向以防主流程隐式推断。
    direction: SignalDirect,
    /// FVG 下沿或收复棒低点之外的结构失效止损。
    protective_stop: f64,
    /// FVG 形成后到首次完整收复之间的已完成 K 线数量。
    fvg_age_bars: usize,
}

const FVG_RECLAIM_MAX_AGE_BARS: usize = 4;
const FVG_RECLAIM_STOP_BUFFER_RATIO: f64 = 0.006;

impl VegasStrategy {
    /// 识别 bearish FVG 在固定短窗口内被多头首次完整收复的候选。
    ///
    /// MACD 只确认收复棒动能较上一根改善；BOS 不参与门禁，因为 v48 已证明 active BOS
    /// 在验证段更接近成熟趋势与拥挤标签，而不是可迁移的继续做空确认。
    fn fvg_reclaim_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<FvgReclaimDecision> {
        let config = self.fvg_reclaim;
        if !config.is_open
            || !config.enable_long
            || !self.macd_signal.is_some_and(|macd| macd.is_open)
            || !values.macd_value.histogram_increasing
            || (config.require_internal_bullish_choch
                && !values.market_structure_value.internal_bullish_choch)
        {
            return None;
        }

        let current_index = data_items.len().checked_sub(1)?;
        if current_index < 3 {
            return None;
        }
        let current = &data_items[current_index];
        if current.c <= current.o {
            return None;
        }
        let atr = values.cross_asset_adaptive_value.atr_value;
        if !atr.is_finite() || atr <= 0.0 {
            return None;
        }

        let first_formation_index = current_index
            .saturating_sub(FVG_RECLAIM_MAX_AGE_BARS)
            .max(2);
        for formation_index in (first_formation_index..current_index).rev() {
            let Some(zone) = bearish_fvg_zone(data_items, formation_index, atr) else {
                continue;
            };

            // 首次收盘站上才是信号；已经完整收复的旧 FVG 不允许在后续棒补造入场。
            let reclaimed_before_current = data_items[zone.formation_index + 1..current_index]
                .iter()
                .any(|candle| candle.c > zone.upper);
            if reclaimed_before_current || current.c <= zone.upper {
                continue;
            }

            let invalidation_low = zone.lower.min(current.l);
            return Some(FvgReclaimDecision {
                direction: SignalDirect::IsLong,
                protective_stop: (invalidation_low * (1.0 - FVG_RECLAIM_STOP_BUFFER_RATIO))
                    .max(0.0),
                fvg_age_bars: current_index - zone.formation_index,
            });
        }
        None
    }
}

#[cfg(test)]
mod fvg_reclaim_tests {
    use super::*;

    /// 构造已确认测试 K 线，使 bearish FVG 与完整收复边界保持显式可读。
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

    /// 构造只开启 v49 setup 的策略，避免其他 Vegas 规则影响形态单测。
    fn strategy() -> VegasStrategy {
        VegasStrategy {
            macd_signal: Some(MacdSignalConfig::default()),
            fvg_reclaim: FvgReclaimConfig {
                is_open: true,
                enable_long: true,
                require_internal_bullish_choch: false,
            },
            ..VegasStrategy::default()
        }
    }

    /// 构造已完成 bearish FVG 与下一根首次完整收复棒。
    fn first_reclaim_candles() -> Vec<CandleItem> {
        vec![
            candle(103.0, 104.0, 103.0, 103.5, 0),
            candle(103.4, 103.5, 100.0, 101.0, 1),
            candle(101.0, 102.5, 100.0, 101.0, 2),
            candle(101.0, 103.4, 100.8, 103.2, 3),
        ]
    }

    /// 构造动量与 ATR 快照；两者只能来自当前及此前已完成 K 线。
    fn values() -> VegasIndicatorSignalValue {
        let mut values = VegasIndicatorSignalValue::default();
        values.macd_value.histogram_increasing = true;
        values.cross_asset_adaptive_value.atr_value = 1.0;
        values
    }

    #[test]
    fn enters_on_first_bullish_close_above_bearish_fvg_upper_with_macd_improving() {
        let decision = strategy()
            .fvg_reclaim_decision(&first_reclaim_candles(), &values())
            .expect("first full reclaim should confirm the long");

        assert_eq!(decision.direction, SignalDirect::IsLong);
        assert_eq!(decision.fvg_age_bars, 1);
        assert!((decision.protective_stop - 100.1952).abs() < 1e-9);
    }

    #[test]
    fn fvg_formation_bar_cannot_enter_before_a_later_reclaim_exists() {
        let candles = first_reclaim_candles();

        assert!(strategy()
            .fvg_reclaim_decision(&candles[..3], &values())
            .is_none());
    }

    #[test]
    fn close_inside_the_gap_is_not_a_full_reclaim() {
        let mut candles = first_reclaim_candles();
        candles[3].c = 102.8;

        assert!(strategy()
            .fvg_reclaim_decision(&candles, &values())
            .is_none());
    }

    #[test]
    fn prior_close_above_the_upper_boundary_expires_later_compensation_entries() {
        let mut candles = first_reclaim_candles();
        candles.insert(3, candle(101.0, 103.3, 100.8, 103.1, 3));
        candles[4].ts = 4;

        assert!(strategy()
            .fvg_reclaim_decision(&candles, &values())
            .is_none());
    }

    #[test]
    fn macd_histogram_must_improve_on_the_reclaim_bar() {
        let mut values = values();
        values.macd_value.histogram_increasing = false;
        values.macd_value.histogram_decreasing = true;

        assert!(strategy()
            .fvg_reclaim_decision(&first_reclaim_candles(), &values)
            .is_none());
    }

    #[test]
    fn optional_structure_gate_requires_a_fresh_internal_bullish_choch() {
        let mut strategy = strategy();
        strategy.fvg_reclaim.require_internal_bullish_choch = true;
        let mut values = values();

        assert!(strategy
            .fvg_reclaim_decision(&first_reclaim_candles(), &values)
            .is_none());

        values.market_structure_value.internal_bullish_choch = true;
        assert!(strategy
            .fvg_reclaim_decision(&first_reclaim_candles(), &values)
            .is_some());
    }
}
