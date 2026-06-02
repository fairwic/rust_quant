use rust_quant_common::CandleItem;
use rust_quant_domain::SignalResult;

use super::{EntryBlockConfig, RangeFilterConfig, VegasIndicatorSignalValue};

pub(crate) const LOW_VOLUME_INSIDE_RANGE_ENTRY_REASON: &str = "LOW_VOLUME_INSIDE_RANGE_BLOCK_ENTRY";
pub(crate) const OPPOSITE_VALUE_AREA_ENTRY_REASON: &str =
    "VOLUME_PROFILE_OPPOSITE_VALUE_AREA_BLOCK_ENTRY";
pub(crate) const LOW_VOLUME_ABOVE_VALUE_AREA_ENTRY_REASON: &str =
    "VOLUME_PROFILE_LOW_VOLUME_ABOVE_VALUE_AREA_BLOCK_ENTRY";
pub(crate) const SHORT_INSIDE_LOW_VOLUME_NODE_ENTRY_REASON: &str =
    "VOLUME_PROFILE_SHORT_INSIDE_LOW_VOLUME_NODE_BLOCK_ENTRY";
const LOW_VOLUME_INSIDE_RANGE_RATIO: f64 = 1.0;
const SHALLOW_BEAR_REBOUND_RATIO: f64 = 0.20;

pub(crate) fn should_block_low_volume_inside_range_entry(
    last: &CandleItem,
    values: &VegasIndicatorSignalValue,
    range: &RangeFilterConfig,
) -> bool {
    if !range.is_open {
        return false;
    }

    let boll = &values.bollinger_value;
    if boll.middle <= 0.0 || boll.upper <= boll.lower {
        return false;
    }

    let bb_width_ratio = (boll.upper - boll.lower) / boll.middle;
    let close_inside_range = last.c() > boll.lower && last.c() < boll.upper;
    let low_volume = values.volume_value.volume_ratio < LOW_VOLUME_INSIDE_RANGE_RATIO;

    bb_width_ratio <= range.bb_width_threshold && close_inside_range && low_volume
}

pub(crate) fn should_block_low_volume_inside_range_long_entry(
    last: &CandleItem,
    values: &VegasIndicatorSignalValue,
    range: &RangeFilterConfig,
) -> bool {
    let fib = &values.fib_retracement_value;
    should_block_low_volume_inside_range_entry(last, values, range)
        && fib.major_bearish
        && fib.retracement_ratio < SHALLOW_BEAR_REBOUND_RATIO
}

pub(crate) fn should_block_opposite_value_area_entry(
    is_long_entry: bool,
    values: &VegasIndicatorSignalValue,
) -> bool {
    let profile = &values.volume_profile_value;
    if is_long_entry {
        profile.close_below_value_area
    } else {
        profile.close_above_value_area
    }
}

pub(crate) fn should_block_low_volume_above_value_area_entry(
    values: &VegasIndicatorSignalValue,
) -> bool {
    let profile = &values.volume_profile_value;
    profile.close_above_value_area && profile.close_on_low_volume_node
}

pub(crate) fn should_block_short_inside_low_volume_node_entry(
    is_long_entry: bool,
    values: &VegasIndicatorSignalValue,
) -> bool {
    let profile = &values.volume_profile_value;
    !is_long_entry && profile.close_inside_value_area && profile.close_on_low_volume_node
}

pub(crate) fn apply_entry_block_reasons(
    signal_result: &mut SignalResult,
    config: &EntryBlockConfig,
    last: &CandleItem,
    values: &VegasIndicatorSignalValue,
    range: Option<&RangeFilterConfig>,
) {
    if config.block_low_volume_inside_range_entry
        && signal_result.should_buy.unwrap_or(false)
        && range.is_some_and(|range| {
            should_block_low_volume_inside_range_long_entry(last, values, range)
        })
    {
        signal_result
            .filter_reasons
            .push(LOW_VOLUME_INSIDE_RANGE_ENTRY_REASON.to_string());
    }

    if config.block_opposite_value_area_entry {
        let is_blocked_long = signal_result.should_buy.unwrap_or(false)
            && should_block_opposite_value_area_entry(true, values);
        let is_blocked_short = signal_result.should_sell.unwrap_or(false)
            && should_block_opposite_value_area_entry(false, values);
        if is_blocked_long || is_blocked_short {
            signal_result
                .filter_reasons
                .push(OPPOSITE_VALUE_AREA_ENTRY_REASON.to_string());
        }
    }

    if config.block_low_volume_above_value_area_entry
        && (signal_result.should_buy.unwrap_or(false) || signal_result.should_sell.unwrap_or(false))
        && should_block_low_volume_above_value_area_entry(values)
    {
        signal_result
            .filter_reasons
            .push(LOW_VOLUME_ABOVE_VALUE_AREA_ENTRY_REASON.to_string());
    }

    if config.block_short_inside_low_volume_node_entry
        && signal_result.should_sell.unwrap_or(false)
        && should_block_short_inside_low_volume_node_entry(false, values)
    {
        signal_result
            .filter_reasons
            .push(SHORT_INSIDE_LOW_VOLUME_NODE_ENTRY_REASON.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trend::vegas::{
        BollingerSignalValue, FibRetracementSignalValue, VegasIndicatorSignalValue,
        VolumeTrendSignalValue,
    };
    use crate::volume::VolumeProfileValue;

    fn candle(o: f64, h: f64, l: f64, c: f64) -> CandleItem {
        CandleItem {
            o,
            h,
            l,
            c,
            ts: 1,
            v: 1.0,
            confirm: 1,
        }
    }

    fn range_config() -> RangeFilterConfig {
        RangeFilterConfig {
            bb_width_threshold: 0.029,
            tp_kline_ratio: 0.56,
            is_open: true,
        }
    }

    #[test]
    fn blocks_low_volume_close_inside_narrow_bollinger_range() {
        let last = candle(2128.67, 2158.07, 2127.61, 2144.9);
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                lower: 2100.340782032374,
                middle: 2124.7441666666587,
                upper: 2149.147551300943,
                ..BollingerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 0.7642821256960192,
                ..VolumeTrendSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(should_block_low_volume_inside_range_entry(
            &last,
            &values,
            &range_config()
        ));
    }

    #[test]
    fn blocks_shallow_major_bearish_long_inside_range() {
        let last = candle(2128.67, 2158.07, 2127.61, 2144.9);
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                lower: 2100.340782032374,
                middle: 2124.7441666666587,
                upper: 2149.147551300943,
                ..BollingerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 0.7642821256960192,
                ..VolumeTrendSignalValue::default()
            },
            fib_retracement_value: FibRetracementSignalValue {
                major_bearish: true,
                retracement_ratio: 0.19809638831377935,
                ..FibRetracementSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(should_block_low_volume_inside_range_long_entry(
            &last,
            &values,
            &range_config()
        ));
    }

    #[test]
    fn allows_deeper_major_bearish_retracement_inside_range() {
        let last = candle(2128.67, 2158.07, 2127.61, 2144.9);
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                lower: 2100.340782032374,
                middle: 2124.7441666666587,
                upper: 2149.147551300943,
                ..BollingerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 0.7642821256960192,
                ..VolumeTrendSignalValue::default()
            },
            fib_retracement_value: FibRetracementSignalValue {
                major_bearish: true,
                retracement_ratio: 0.35,
                ..FibRetracementSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(!should_block_low_volume_inside_range_long_entry(
            &last,
            &values,
            &range_config()
        ));
    }

    #[test]
    fn allows_average_volume_inside_range() {
        let last = candle(2128.67, 2158.07, 2127.61, 2144.9);
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                lower: 2100.340782032374,
                middle: 2124.7441666666587,
                upper: 2149.147551300943,
                ..BollingerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 1.0,
                ..VolumeTrendSignalValue::default()
            },
            fib_retracement_value: FibRetracementSignalValue {
                major_bearish: true,
                retracement_ratio: 0.15,
                ..FibRetracementSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(!should_block_low_volume_inside_range_long_entry(
            &last,
            &values,
            &range_config()
        ));
    }

    #[test]
    fn allows_volume_expansion_inside_narrow_bollinger_range() {
        let last = candle(2128.67, 2158.07, 2127.61, 2144.9);
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                lower: 2100.340782032374,
                middle: 2124.7441666666587,
                upper: 2149.147551300943,
                ..BollingerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 1.2,
                ..VolumeTrendSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(!should_block_low_volume_inside_range_entry(
            &last,
            &values,
            &range_config()
        ));
    }

    #[test]
    fn applies_configured_volume_profile_reasons() {
        let last = candle(100.0, 105.0, 95.0, 104.0);
        let values = VegasIndicatorSignalValue {
            volume_profile_value: VolumeProfileValue {
                close_above_value_area: true,
                close_on_low_volume_node: true,
                ..VolumeProfileValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        let mut signal = SignalResult::empty();
        signal.should_buy = Some(true);
        signal.should_sell = Some(false);
        let config = EntryBlockConfig {
            block_low_volume_above_value_area_entry: true,
            ..EntryBlockConfig::default()
        };

        apply_entry_block_reasons(&mut signal, &config, &last, &values, None);

        assert_eq!(
            signal.filter_reasons,
            vec![LOW_VOLUME_ABOVE_VALUE_AREA_ENTRY_REASON.to_string()]
        );
    }

    #[test]
    fn allows_close_outside_bollinger_range() {
        let last = candle(2148.0, 2188.0, 2140.0, 2162.0);
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                lower: 2100.340782032374,
                middle: 2124.7441666666587,
                upper: 2149.147551300943,
                ..BollingerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 0.7,
                ..VolumeTrendSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(!should_block_low_volume_inside_range_entry(
            &last,
            &values,
            &range_config()
        ));
    }

    #[test]
    fn blocks_long_entry_below_value_area() {
        let values = VegasIndicatorSignalValue {
            volume_profile_value: VolumeProfileValue {
                close_below_value_area: true,
                ..VolumeProfileValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(should_block_opposite_value_area_entry(true, &values));
    }

    #[test]
    fn blocks_short_entry_above_value_area() {
        let values = VegasIndicatorSignalValue {
            volume_profile_value: VolumeProfileValue {
                close_above_value_area: true,
                ..VolumeProfileValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(should_block_opposite_value_area_entry(false, &values));
    }

    #[test]
    fn allows_direction_aligned_value_area_breakout() {
        let long_values = VegasIndicatorSignalValue {
            volume_profile_value: VolumeProfileValue {
                close_above_value_area: true,
                ..VolumeProfileValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        let short_values = VegasIndicatorSignalValue {
            volume_profile_value: VolumeProfileValue {
                close_below_value_area: true,
                ..VolumeProfileValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(!should_block_opposite_value_area_entry(true, &long_values));
        assert!(!should_block_opposite_value_area_entry(
            false,
            &short_values
        ));
    }

    #[test]
    fn blocks_low_volume_node_above_value_area() {
        let values = VegasIndicatorSignalValue {
            volume_profile_value: VolumeProfileValue {
                close_above_value_area: true,
                close_on_low_volume_node: true,
                ..VolumeProfileValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(should_block_low_volume_above_value_area_entry(&values));
    }

    #[test]
    fn allows_high_volume_node_above_value_area() {
        let values = VegasIndicatorSignalValue {
            volume_profile_value: VolumeProfileValue {
                close_above_value_area: true,
                close_on_low_volume_node: false,
                close_on_high_volume_node: true,
                ..VolumeProfileValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(!should_block_low_volume_above_value_area_entry(&values));
    }

    #[test]
    fn blocks_short_inside_low_volume_node() {
        let values = VegasIndicatorSignalValue {
            volume_profile_value: VolumeProfileValue {
                close_inside_value_area: true,
                close_on_low_volume_node: true,
                ..VolumeProfileValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(should_block_short_inside_low_volume_node_entry(
            false, &values
        ));
        assert!(!should_block_short_inside_low_volume_node_entry(
            true, &values
        ));
    }
}
