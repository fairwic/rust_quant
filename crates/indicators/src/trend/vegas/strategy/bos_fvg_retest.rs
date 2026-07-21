/// BOS + FVG 回补失败候选给主信号流程返回的结构止损与审计信息。
#[derive(Debug, Clone, Copy, PartialEq)]
struct BosFvgRetestDecision {
    /// 当前版本只允许生成空头，但仍显式保留方向以避免主流程隐式推断。
    direction: SignalDirect,
    /// FVG 上沿或回补棒高点之外的结构失效止损。
    protective_stop: f64,
    /// FVG 形成后到首次回补确认之间的已完成 K 线数量。
    fvg_age_bars: usize,
}

/// 已由三根完成 K 线确认的 bearish FVG 价格边界。
#[derive(Debug, Clone, Copy, PartialEq)]
struct BearishFvgZone {
    /// 第三根形成棒索引；后续 setup 只能读取该索引之后的已完成 K 线。
    formation_index: usize,
    /// 缺口下沿，即形成棒最高价。
    lower: f64,
    /// 缺口上沿，即两根前锚点最低价。
    upper: f64,
}

const BOS_FVG_MAX_RETEST_AGE_BARS: usize = 4;
const BOS_FVG_MIN_GAP_ATR: f64 = 0.10;
const BOS_FVG_MIN_DISPLACEMENT_BODY_ATR: f64 = 0.80;
const BOS_FVG_STOP_BUFFER_RATIO: f64 = 0.006;

/// 按冻结的 ATR 尺度验证指定形成棒是否构成 bearish FVG。
///
/// 该函数只读取 `formation_index` 及更早数据，供顺势回补与反向完整收复共享同一因果定义。
fn bearish_fvg_zone(
    data_items: &[CandleItem],
    formation_index: usize,
    atr: f64,
) -> Option<BearishFvgZone> {
    if formation_index < 2 || !atr.is_finite() || atr <= 0.0 {
        return None;
    }
    let formation = data_items.get(formation_index)?;
    let displacement = data_items.get(formation_index - 1)?;
    let anchor = data_items.get(formation_index - 2)?;
    if formation.h >= anchor.l || displacement.c >= displacement.o {
        return None;
    }
    let lower = formation.h;
    let upper = anchor.l;
    let gap_atr = (upper - lower) / atr;
    let displacement_body_atr = (displacement.o - displacement.c) / atr;
    if gap_atr < BOS_FVG_MIN_GAP_ATR || displacement_body_atr < BOS_FVG_MIN_DISPLACEMENT_BODY_ATR {
        return None;
    }
    Some(BearishFvgZone {
        formation_index,
        lower,
        upper,
    })
}

impl VegasStrategy {
    /// 在有效 bearish BOS 结构中识别 bearish FVG 的首次回补失败。
    ///
    /// FVG 必须先由更早的三根已完成 K 线形成；当前棒只能作为形成后的首次回补确认，
    /// 因而不会在位移棒或 FVG 形成棒追空，也不会用当前棒之后的数据补造入场。
    fn bos_fvg_retest_decision(
        &self,
        data_items: &[CandleItem],
        values: &VegasIndicatorSignalValue,
    ) -> Option<BosFvgRetestDecision> {
        let config = self.bos_fvg_retest;
        if !config.is_open
            || !config.enable_short
            || !values.market_structure_value.internal_bearish_bos_active
            || !self.macd_signal.is_some_and(|macd| macd.is_open)
            || !values.macd_value.histogram_decreasing
        {
            return None;
        }

        let current_index = data_items.len().checked_sub(1)?;
        if current_index < 3 {
            return None;
        }
        let current = &data_items[current_index];
        if current.c >= current.o {
            return None;
        }
        let atr = values.cross_asset_adaptive_value.atr_value;
        if !atr.is_finite() || atr <= 0.0 {
            return None;
        }

        let first_formation_index = current_index
            .saturating_sub(BOS_FVG_MAX_RETEST_AGE_BARS)
            .max(2);
        for formation_index in (first_formation_index..current_index).rev() {
            let Some(zone) = bearish_fvg_zone(data_items, formation_index, atr) else {
                continue;
            };

            // 只允许形成后的第一次触及，防止同一 FVG 在结果已可见后被反复当成新机会。
            let touched_before_current = data_items[zone.formation_index + 1..current_index]
                .iter()
                .any(|candle| candle.h >= zone.lower);
            if touched_before_current || current.h < zone.lower || current.c >= zone.lower {
                continue;
            }

            let invalidation_high = zone.upper.max(current.h);
            return Some(BosFvgRetestDecision {
                direction: SignalDirect::IsShort,
                protective_stop: invalidation_high * (1.0 + BOS_FVG_STOP_BUFFER_RATIO),
                fvg_age_bars: current_index - zone.formation_index,
            });
        }
        None
    }
}

#[cfg(test)]
mod bos_fvg_retest_tests {
    use super::*;

    /// 构造已确认测试 K 线，使 FVG 三棒与回补棒边界保持显式可读。
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

    /// 构造只开启 v48 setup 的策略，避免其他 Vegas 规则影响形态单测。
    fn strategy() -> VegasStrategy {
        VegasStrategy {
            macd_signal: Some(MacdSignalConfig::default()),
            bos_fvg_retest: BosFvgRetestConfig {
                is_open: true,
                enable_short: true,
            },
            ..VegasStrategy::default()
        }
    }

    /// 构造已完成 bearish FVG 与下一根首次回补失败棒。
    fn first_retest_candles() -> Vec<CandleItem> {
        vec![
            candle(103.0, 104.0, 103.0, 103.5, 0),
            candle(103.4, 103.5, 100.0, 101.0, 1),
            candle(101.0, 102.5, 100.0, 101.0, 2),
            candle(103.0, 103.2, 101.0, 102.2, 3),
        ]
    }

    /// 构造结构与动量快照；BOS 与 MACD 均只能来自当前及此前已完成 K 线。
    fn values() -> VegasIndicatorSignalValue {
        let mut values = VegasIndicatorSignalValue::default();
        values.market_structure_value.internal_bearish_bos_active = true;
        values.macd_value.histogram_decreasing = true;
        values.cross_asset_adaptive_value.atr_value = 1.0;
        values
    }

    #[test]
    fn enters_only_after_first_bearish_fvg_retest_closes_back_below_gap() {
        let decision = strategy()
            .bos_fvg_retest_decision(&first_retest_candles(), &values())
            .expect("first rejected retest should confirm the short");

        assert_eq!(decision.direction, SignalDirect::IsShort);
        assert_eq!(decision.fvg_age_bars, 1);
        assert!((decision.protective_stop - 103.8192).abs() < 1e-9);
    }

    #[test]
    fn fvg_formation_bar_cannot_enter_before_a_later_retest_exists() {
        let candles = first_retest_candles();

        assert!(strategy()
            .bos_fvg_retest_decision(&candles[..3], &values())
            .is_none());
    }

    #[test]
    fn bearish_choch_without_confirmed_bos_continuation_is_not_enough() {
        let mut values = values();
        values.market_structure_value.internal_bearish_bos_active = false;

        assert!(strategy()
            .bos_fvg_retest_decision(&first_retest_candles(), &values)
            .is_none());
    }

    #[test]
    fn prior_touch_expires_the_fvg_instead_of_creating_a_later_compensation_entry() {
        let mut candles = first_retest_candles();
        candles.insert(3, candle(102.0, 102.6, 101.0, 101.5, 3));
        candles[4].ts = 4;

        assert!(strategy()
            .bos_fvg_retest_decision(&candles, &values())
            .is_none());
    }

    #[test]
    fn macd_must_turn_weaker_on_the_retest_bar() {
        let mut values = values();
        values.macd_value.histogram_decreasing = false;
        values.macd_value.histogram_increasing = true;

        assert!(strategy()
            .bos_fvg_retest_decision(&first_retest_candles(), &values)
            .is_none());
    }
}
