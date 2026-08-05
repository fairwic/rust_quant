use super::{CandlePatterns, Divergence};
use crate::app::tradingview_velocity_parity::model::{BlockedSignal, Candle, Direction};

/// V7 只把没有方向相反长影拒绝的实体吞没视为可交易 RSI 反转形态。
pub(super) fn accepted_bullish_engulfing(patterns: CandlePatterns) -> bool {
    patterns.bullish_engulfing && !patterns.long_upper_shadow
}

/// 看跌吞没使用完全镜像的下影拒绝门禁，避免多空规则不对称。
pub(super) fn accepted_bearish_engulfing(patterns: CandlePatterns) -> bool {
    patterns.bearish_engulfing && !patterns.long_lower_shadow
}

/// 底背离信号棒若是既有定义的长上影，说明收盘前仍遭遇明显上方拒绝。
pub(super) fn bullish_divergence_wick_allowed(patterns: CandlePatterns) -> bool {
    !patterns.long_upper_shadow
}

/// 顶背离信号棒若是既有定义的长下影，说明收盘前仍存在明显下方承接。
pub(super) fn bearish_divergence_wick_allowed(patterns: CandlePatterns) -> bool {
    !patterns.long_lower_shadow
}

/// 假突破空单覆盖十字星式锤子线：下影达到整根 60% 且长于上影即视为下方承接。
pub(super) fn false_breakout_short_has_long_lower_wick(candle: Candle) -> bool {
    if !candle.is_valid() || candle.range() <= 0.0 {
        return false;
    }
    let upper_shadow = candle.high - candle.open.max(candle.close);
    let lower_shadow = candle.open.min(candle.close) - candle.low;
    lower_shadow / candle.range() >= 0.60 && lower_shadow > upper_shadow
}

/// 原地删除带反向长影的 RSI 背离方向，并返回可审计的阻断原因。
pub(super) fn guard_divergence_opposing_wicks(
    divergence: &mut Divergence,
    patterns: CandlePatterns,
    timestamp_ms: i64,
) -> Vec<BlockedSignal> {
    let mut blocked = Vec::with_capacity(2);
    if divergence.bullish && !bullish_divergence_wick_allowed(patterns) {
        divergence.bullish = false;
        blocked.push(BlockedSignal {
            signal_time_ms: timestamp_ms,
            direction: Some(Direction::Long),
            reason: "V7_RSI_BULL_DIV_REJECTS_LONG_UPPER_SHADOW".to_owned(),
        });
    }
    if divergence.bearish && !bearish_divergence_wick_allowed(patterns) {
        divergence.bearish = false;
        blocked.push(BlockedSignal {
            signal_time_ms: timestamp_ms,
            direction: Some(Direction::Short),
            reason: "V7_RSI_BEAR_DIV_REJECTS_LONG_LOWER_SHADOW".to_owned(),
        });
    }
    blocked
}

#[cfg(test)]
mod tests {
    use super::super::candle_patterns;
    use super::*;

    fn candle(open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle {
            timestamp_ms: 0,
            open,
            high,
            low,
            close,
            volume: 1.0,
        }
    }

    #[test]
    fn atom_body_engulfing_with_72_percent_upper_shadow_is_rejected() {
        let candles = [
            candle(1.296, 1.297, 1.292, 1.295),
            candle(1.295, 1.306, 1.295, 1.298),
        ];
        let patterns = candle_patterns(&candles, 1);

        assert!(patterns.bullish_engulfing);
        assert!(patterns.long_upper_shadow);
        assert!(!accepted_bullish_engulfing(patterns));
        assert!(!bullish_divergence_wick_allowed(patterns));
    }

    #[test]
    fn opposing_wick_guard_is_symmetric_for_bearish_engulfing() {
        let candles = [
            candle(100.0, 101.0, 99.0, 101.0),
            candle(101.0, 101.0, 90.0, 99.0),
        ];
        let patterns = candle_patterns(&candles, 1);

        assert!(patterns.bearish_engulfing);
        assert!(patterns.long_lower_shadow);
        assert!(!accepted_bearish_engulfing(patterns));
        assert!(!bearish_divergence_wick_allowed(patterns));
    }

    #[test]
    fn exactly_60_percent_opposing_shadow_is_included_in_the_guard() {
        let candles = [
            candle(13.0, 14.0, 9.0, 10.0),
            candle(10.0, 20.0, 10.0, 14.0),
        ];
        let patterns = candle_patterns(&candles, 1);

        assert!(patterns.bullish_engulfing);
        assert!(patterns.long_upper_shadow);
        assert!(!accepted_bullish_engulfing(patterns));
    }

    #[test]
    fn engulfing_below_60_percent_opposing_shadow_remains_accepted() {
        let candles = [
            candle(13.0, 14.0, 9.0, 10.0),
            candle(10.0, 19.9, 10.0, 14.0),
        ];
        let patterns = candle_patterns(&candles, 1);

        assert!(patterns.bullish_engulfing);
        assert!(!patterns.long_upper_shadow);
        assert!(accepted_bullish_engulfing(patterns));
    }

    #[test]
    fn ltc_false_breakout_doji_with_65_percent_lower_wick_is_rejected() {
        let signal = candle(43.30, 43.35, 43.15, 43.28);

        assert!(false_breakout_short_has_long_lower_wick(signal));
    }

    #[test]
    fn clean_bearish_close_below_anchor_remains_eligible() {
        let signal = candle(43.30, 43.35, 43.15, 43.18);

        assert!(!false_breakout_short_has_long_lower_wick(signal));
    }
}
