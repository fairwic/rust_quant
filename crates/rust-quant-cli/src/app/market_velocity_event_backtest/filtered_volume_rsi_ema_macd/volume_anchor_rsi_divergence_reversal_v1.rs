use super::super::{
    ComputedCandle, IsolatedStrategyFamilySignalEvidence, MarketVelocityEventBacktestArgs,
    MarketVelocityTradeDirection,
};
use super::isolated_family_common::{
    anchor_entry_evidence, fixed_one_r_signal, ISOLATED_FILTERED_VOLUME_MIN_RATIO,
};
use super::weekly_base_volume_v3::{filtered_volume_evidence, WeeklyBaseVolumeEvidence};
use super::weekly_p90_anchor_rsi_divergence_v9::rsi_divergence_candidates;
use super::FilteredVolumeRsiEmaMacdSignal;

/// 独立 RSI 家族落库使用的比较模式，明确不包含同根 EMA/MACD 候选。
pub(crate) const ISOLATED_RSI_COMPARISON_MODE: &str =
    "isolated_weekly_p90_filtered_volume_anchors_wick_or_next_touch";
/// 独立 RSI 家族的冻结最小假设标识。
pub(crate) const VOLUME_ANCHOR_RSI_HYPOTHESIS: &str =
    "nearest_volume_anchor_price_extreme_with_non_worsening_rsi";

const LONG_WICK_TRIGGER: &str = "rsi_anchor_divergence_lower_wick_next_open_long";
const LONG_TOUCH_TRIGGER: &str = "rsi_anchor_divergence_next_high_touch_long";
const SHORT_WICK_TRIGGER: &str = "rsi_anchor_divergence_upper_wick_next_open_short";
const SHORT_TOUCH_TRIGGER: &str = "rsi_anchor_divergence_next_low_touch_short";

/// 只保留最近 q/p 放量锚点 RSI 背离，不合并 EMA、MACD 或历史净移动候选。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    if (args.entry_min_volume_ratio - ISOLATED_FILTERED_VOLUME_MIN_RATIO).abs() > f64::EPSILON {
        return Err("volume_anchor_rsi_ratio_policy_mismatch");
    }
    let latest_idx = completed_count
        .checked_sub(1)
        .ok_or("volume_anchor_rsi_not_ready")?;
    let latest = candles
        .get(latest_idx)
        .ok_or("volume_anchor_rsi_not_ready")?;
    let current_rsi = latest
        .rsi14
        .filter(|value| value.is_finite())
        .ok_or("volume_anchor_rsi_not_ready")?;
    let (candidates, mut divergences) = rsi_divergence_candidates(
        candles,
        latest_idx,
        current_rsi,
        ISOLATED_FILTERED_VOLUME_MIN_RATIO,
    );
    let candidate = candidates
        .first()
        .copied()
        .filter(|_| candidates.len() == 1 && divergences.len() == 1)
        .ok_or("volume_anchor_rsi_divergence_not_confirmed")?;
    let divergence = divergences
        .first_mut()
        .ok_or("volume_anchor_rsi_divergence_evidence_missing")?;
    divergence.comparison_mode = ISOLATED_RSI_COMPARISON_MODE;
    let weekly_volume = WeeklyBaseVolumeEvidence {
        current: divergence
            .pivot_volume_ccy
            .ok_or("volume_anchor_rsi_current_volume_ccy_missing")?,
        p90: divergence
            .pivot_weekly_volume_ccy_p90
            .ok_or("volume_anchor_rsi_weekly_p90_missing")?,
    };
    let volume = filtered_volume_evidence(candles, latest_idx, ISOLATED_FILTERED_VOLUME_MIN_RATIO)?;
    let (anchor_entry, direct_wick_entry) = anchor_entry_evidence(latest, candidate.direction)?;
    let trigger = match (candidate.direction, direct_wick_entry) {
        (MarketVelocityTradeDirection::Long, true) => LONG_WICK_TRIGGER,
        (MarketVelocityTradeDirection::Long, false) => LONG_TOUCH_TRIGGER,
        (MarketVelocityTradeDirection::Short, true) => SHORT_WICK_TRIGGER,
        (MarketVelocityTradeDirection::Short, false) => SHORT_TOUCH_TRIGGER,
        (MarketVelocityTradeDirection::Both, _) => unreachable!(),
    };

    fixed_one_r_signal(
        latest,
        candidate.direction,
        trigger,
        volume,
        weekly_volume,
        divergences,
        Some(anchor_entry),
        IsolatedStrategyFamilySignalEvidence {
            family: "volume_anchor_rsi_divergence",
            hypothesis: VOLUME_ANCHOR_RSI_HYPOTHESIS,
            prior_96_net_move_pct: None,
            platform_breakdown: None,
            long_term_ema_confirmed: false,
            ema696_recent: Vec::new(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::args::market_volume_anchor_rsi_divergence_reversal_v1_research_args;
    use crate::app::market_velocity_event_backtest::{BacktestCandle, MS_15M};

    fn candle(idx: usize) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: idx as i64 * MS_15M,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
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

    fn long_setup() -> Vec<ComputedCandle> {
        let mut candles = (0..750).map(candle).collect::<Vec<_>>();
        let latest_idx = candles.len() - 1;
        let reference_idx = latest_idx - 24;
        candles[reference_idx].candle.low = 97.0;
        candles[reference_idx].candle.volume = 25.0;
        candles[reference_idx].volume_ccy = Some(200.0);
        candles[reference_idx].rsi14 = Some(25.0);
        candles[latest_idx].candle = BacktestCandle {
            ts: latest_idx as i64 * MS_15M,
            open: 100.0,
            high: 101.5,
            low: 96.0,
            close: 100.8,
            volume: 25.0,
        };
        candles[latest_idx].volume_ccy = Some(200.0);
        candles[latest_idx].rsi14 = Some(27.0);
        candles
    }

    #[test]
    fn ignores_ema_and_macd_even_when_their_values_are_removed() {
        let mut candles = long_setup();
        let args = market_volume_anchor_rsi_divergence_reversal_v1_research_args().unwrap();
        let before = signal(&candles, candles.len(), &args).unwrap();
        let latest_idx = candles.len() - 1;
        candles[latest_idx].macd_line = None;
        candles[latest_idx].ema12 = None;
        candles[latest_idx].ema144 = None;
        candles[latest_idx].ema169 = None;
        candles[latest_idx].ema696 = None;
        let after = signal(&candles, candles.len(), &args).unwrap();

        assert_eq!(before.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(before.trigger, after.trigger);
        assert_eq!(before.direction, after.direction);
        assert_eq!(before.evidence.rsi_divergences.len(), 1);
        assert_eq!(
            before.evidence.rsi_divergences[0].comparison_mode,
            ISOLATED_RSI_COMPARISON_MODE
        );
        assert_eq!(
            before
                .evidence
                .isolated_family
                .as_ref()
                .map(|family| family.family),
            Some("volume_anchor_rsi_divergence")
        );
    }

    #[test]
    fn closest_qualified_anchor_is_used_without_future_data() {
        let mut candles = long_setup();
        let completed_count = candles.len();
        let latest_idx = completed_count - 1;
        let nearest_reference_idx = latest_idx - 24;
        let args = market_volume_anchor_rsi_divergence_reversal_v1_research_args().unwrap();
        let before = signal(&candles, completed_count, &args).unwrap();
        candles.push(candle(completed_count));
        candles.last_mut().unwrap().rsi14 = Some(99.0);
        candles.last_mut().unwrap().candle.low = 1.0;
        let after = signal(&candles, completed_count, &args).unwrap();

        assert_eq!(before, after);
        assert_eq!(
            before.evidence.rsi_divergences[0].reference_pivot_ts_ms,
            candles[nearest_reference_idx].candle.ts
        );
    }
}
