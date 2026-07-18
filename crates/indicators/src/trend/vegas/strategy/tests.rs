    use super::super::ema_filter::EmaDistanceFilter;
    use super::super::signal::{
        BollingerSignalValue, EmaTouchTrendSignalValue, EngulfingSignalValue,
        KlineHammerSignalValue, MacdSignalValue, RsiSignalValue, VolumeTrendSignalValue,
    };
    use super::{
        EmaDistanceState, EmaSignalValue, EntryBlockConfig, FibRetracementSignalConfig,
        FibRetracementSignalValue, RsiSignalConfig, SignalCondition, SignalType,
        SignalWeightsConfig,
        VegasIndicatorSignalValue, VegasStrategy, VolumeSignalConfig,
    };
    use crate::leg_detection_indicator::LegDetectionValue;
    use rust_quant_common::CandleItem;
    use rust_quant_domain::{BasicRiskStrategyConfig, SignalResult};
    fn candle(o: f64, h: f64, l: f64, c: f64, ts: i64) -> CandleItem {
        CandleItem {
            o,
            h,
            l,
            c,
            ts,
            v: 1.0,
            confirm: 1,
        }
    }
    #[test]
    fn weak_bollinger_filter_uses_normalized_atr_instead_of_symbol_name() {
        let strategy = VegasStrategy {
            entry_block_config: EntryBlockConfig {
                weak_bollinger_min_atr_ratio: 0.0225,
                ..EntryBlockConfig::default()
            },
            ..VegasStrategy::default()
        };
        let low_volatility = (0..15)
            .map(|ts| candle(100.0, 100.2, 99.8, 100.0, ts))
            .collect::<Vec<_>>();
        let high_volatility = (0..15)
            .map(|ts| candle(100.0, 103.0, 97.0, 100.0, ts))
            .collect::<Vec<_>>();

        assert!(!strategy.weak_bollinger_volatility_allows_filter(&low_volatility));
        assert!(strategy.weak_bollinger_volatility_allows_filter(&high_volatility));
    }
    #[test]
    fn fib_strict_reason_includes_swing_pct_suffix() {
        let strategy = VegasStrategy {
            period: "4H".to_string(),
            volume_signal: Some(VolumeSignalConfig {
                volume_bar_num: 4,
                volume_increase_ratio: 2.0,
                volume_decrease_ratio: 2.0,
                is_open: true,
            }),
            rsi_signal: Some(RsiSignalConfig {
                rsi_length: 14,
                rsi_oversold: 15.0,
                rsi_overbought: 85.0,
                is_open: true,
            }),
            fib_retracement_signal: Some(FibRetracementSignalConfig {
                is_open: true,
                only_on_fib: false,
                swing_lookback: 5,
                fib_trigger_low: 0.328,
                fib_trigger_high: 0.618,
                min_volume_ratio: 10.0,
                require_leg_confirmation: false,
                strict_major_trend: true,
                stop_loss_buffer_ratio: 0.01,
                use_swing_stop_loss: false,
                min_trend_move_pct: 0.1,
            }),
            ..VegasStrategy::default()
        };
        let candles = vec![
            candle(10.0, 10.0, 9.0, 9.5, 1),
            candle(9.5, 9.7, 8.5, 9.0, 2),
            candle(9.0, 9.2, 8.0, 8.4, 3),
            candle(8.4, 8.8, 8.2, 8.6, 4),
            candle(8.6, 9.0, 8.4, 8.8, 5),
        ];
        let mut indicator_values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                ema1_value: 90.0,
                ema2_value: 95.0,
                ema3_value: 96.0,
                ema4_value: 100.0,
                ..EmaSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        indicator_values.volume_value.volume_ratio = 3.0;
        indicator_values.rsi_value.rsi_value = 10.0;
        let weights = SignalWeightsConfig {
            weights: vec![(SignalType::VolumeTrend, 1.0), (SignalType::Rsi, 1.0)],
            min_total_weight: 2.0,
        };
        let result = strategy.get_trade_signal(
            &candles,
            &mut indicator_values,
            &weights,
            &BasicRiskStrategyConfig::default(),
        );
        let reason = result
            .filter_reasons
            .iter()
            .find(|r| r.starts_with("FIB_STRICT_MAJOR_BEAR_BLOCK_LONG"))
            .expect("expected fib strict reason");
        assert!(
            reason.contains("swing_pct="),
            "reason should include swing_pct suffix, got: {}",
            reason
        );
    }
    #[test]
    fn bearish_fvg_pressure_long_block_detects_rejection_inside_overhead_gap() {
        let candles = vec![
            candle(120.0, 122.0, 110.0, 111.0, 1),
            candle(111.0, 112.0, 100.0, 101.0, 2),
            candle(100.0, 101.0, 90.0, 95.0, 3),
            candle(96.0, 99.0, 94.0, 98.0, 4),
            candle(98.0, 103.0, 97.0, 102.0, 5),
        ];

        assert!(VegasStrategy::should_block_bearish_fvg_pressure_long(
            &candles, 20, 0.005, 0.0
        ));
    }
    #[test]
    fn bearish_fvg_pressure_long_block_regresses_2026_07_04_eth_signal() {
        let candles = vec![
            candle(1820.0, 1840.0, 1810.30, 1812.0, 1781539200000),
            candle(1812.0, 1816.0, 1804.0, 1806.0, 1781553600000),
            candle(1800.0, 1802.73, 1784.0, 1790.0, 1781568000000),
            candle(1757.48, 1764.74, 1754.0, 1757.27, 1783152000000),
            candle(1757.28, 1803.33, 1757.28, 1790.57, 1783166400000),
        ];

        assert!(VegasStrategy::should_block_bearish_fvg_pressure_long(
            &candles, 240, 0.003, 0.001
        ));
    }
    #[test]
    fn bearish_fvg_pressure_long_block_allows_clean_close_above_gap() {
        let candles = vec![
            candle(120.0, 122.0, 110.0, 111.0, 1),
            candle(111.0, 112.0, 100.0, 101.0, 2),
            candle(100.0, 101.0, 90.0, 95.0, 3),
            candle(96.0, 99.0, 94.0, 98.0, 4),
            candle(98.0, 112.0, 97.0, 111.0, 5),
        ];

        assert!(!VegasStrategy::should_block_bearish_fvg_pressure_long(
            &candles, 20, 0.005, 0.0
        ));
    }
    #[test]
    fn bearish_fvg_pressure_long_context_requires_conflicting_chase_shape() {
        let values = VegasIndicatorSignalValue {
            engulfing_value: EngulfingSignalValue {
                is_valid_engulfing: true,
                ..EngulfingSignalValue::default()
            },
            bollinger_value: BollingerSignalValue {
                is_short_signal: true,
                ..BollingerSignalValue::default()
            },
            leg_detection_value: LegDetectionValue {
                is_bullish_leg: true,
                ..LegDetectionValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                should_filter_long: true,
                ..EmaDistanceFilter::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(VegasStrategy::is_bearish_fvg_pressure_chase_long_context(
            &values
        ));
    }
    #[test]
    fn bearish_fvg_pressure_long_context_ignores_non_bollinger_pressure() {
        let values = VegasIndicatorSignalValue {
            engulfing_value: EngulfingSignalValue {
                is_valid_engulfing: true,
                ..EngulfingSignalValue::default()
            },
            leg_detection_value: LegDetectionValue {
                is_bullish_leg: true,
                ..LegDetectionValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                should_filter_long: true,
                ..EmaDistanceFilter::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(!VegasStrategy::is_bearish_fvg_pressure_chase_long_context(
            &values
        ));
    }
    #[test]
    fn weak_bollinger_context_blocks_long_against_upper_pressure() {
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                is_short_signal: true,
                ..BollingerSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(VegasStrategy::should_block_weak_bollinger_context_long(
            &values
        ));
    }
    #[test]
    fn weak_bollinger_context_blocks_short_without_bollinger_edge() {
        let values = VegasIndicatorSignalValue::default();

        assert!(VegasStrategy::should_block_weak_bollinger_context_short(
            &values
        ));
    }
    #[test]
    fn weak_bollinger_context_keeps_short_with_bollinger_short_edge() {
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                is_short_signal: true,
                ..BollingerSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };

        assert!(!VegasStrategy::should_block_weak_bollinger_context_short(
            &values
        ));
    }
    #[test]
    fn entry_block_config_enables_weak_bollinger_context_filter_without_env() {
        let strategy = VegasStrategy {
            entry_block_config: EntryBlockConfig {
                block_weak_bollinger_context_entry: true,
                ..EntryBlockConfig::default()
            },
            ..VegasStrategy::default()
        };
        let candles = vec![candle(100.0, 102.0, 99.0, 101.0, 1)];
        let mut signal = SignalResult {
            should_buy: Some(true),
            open_price: Some(101.0),
            ..SignalResult::empty()
        };
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                is_short_signal: true,
                ..BollingerSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        let mut dynamic_adjustments = Vec::new();
        let mut range_snapshot = None;

        strategy.apply_post_signal_entry_filters(
            &candles,
            candles.last().expect("last candle"),
            &values,
            &[],
            FibRetracementSignalConfig {
                is_open: false,
                strict_major_trend: false,
                ..FibRetracementSignalConfig::default()
            },
            EmaDistanceFilter::default(),
            None,
            &mut signal,
            &mut dynamic_adjustments,
            &mut range_snapshot,
        );

        assert_eq!(signal.should_buy, Some(false));
        assert!(signal
            .filter_reasons
            .contains(&"WEAK_BOLLINGER_CONTEXT_LONG_BLOCK".to_string()));
    }
    #[test]
    fn entry_block_config_keeps_weak_bollinger_filter_off_by_default() {
        let strategy = VegasStrategy::default();
        let candles = vec![candle(100.0, 102.0, 99.0, 101.0, 1)];
        let mut signal = SignalResult {
            should_buy: Some(true),
            open_price: Some(101.0),
            ..SignalResult::empty()
        };
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                is_short_signal: true,
                ..BollingerSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        let mut dynamic_adjustments = Vec::new();
        let mut range_snapshot = None;

        strategy.apply_post_signal_entry_filters(
            &candles,
            candles.last().expect("last candle"),
            &values,
            &[],
            FibRetracementSignalConfig {
                is_open: false,
                strict_major_trend: false,
                ..FibRetracementSignalConfig::default()
            },
            EmaDistanceFilter::default(),
            None,
            &mut signal,
            &mut dynamic_adjustments,
            &mut range_snapshot,
        );

        assert_eq!(signal.should_buy, Some(true));
        assert!(!signal
            .filter_reasons
            .contains(&"WEAK_BOLLINGER_CONTEXT_LONG_BLOCK".to_string()));
    }
    #[test]
    fn entry_block_config_enables_bearish_fvg_pressure_filter_without_env() {
        let strategy = VegasStrategy {
            entry_block_config: EntryBlockConfig {
                block_bearish_fvg_pressure_long: true,
                ..EntryBlockConfig::default()
            },
            ..VegasStrategy::default()
        };
        let candles = vec![
            candle(1820.0, 1840.0, 1810.30, 1812.0, 1781539200000),
            candle(1812.0, 1816.0, 1804.0, 1806.0, 1781553600000),
            candle(1800.0, 1802.73, 1784.0, 1790.0, 1781568000000),
            candle(1757.48, 1764.74, 1754.0, 1757.27, 1783152000000),
            candle(1757.28, 1803.33, 1757.28, 1790.57, 1783166400000),
        ];
        let mut signal = SignalResult {
            should_buy: Some(true),
            open_price: Some(1790.57),
            ..SignalResult::empty()
        };
        let values = VegasIndicatorSignalValue {
            engulfing_value: EngulfingSignalValue {
                is_valid_engulfing: true,
                ..EngulfingSignalValue::default()
            },
            bollinger_value: BollingerSignalValue {
                is_short_signal: true,
                ..BollingerSignalValue::default()
            },
            leg_detection_value: LegDetectionValue {
                is_bullish_leg: true,
                ..LegDetectionValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                should_filter_long: true,
                ..EmaDistanceFilter::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        let mut dynamic_adjustments = Vec::new();
        let mut range_snapshot = None;

        strategy.apply_post_signal_entry_filters(
            &candles,
            candles.last().expect("last candle"),
            &values,
            &[],
            FibRetracementSignalConfig {
                is_open: false,
                strict_major_trend: false,
                ..FibRetracementSignalConfig::default()
            },
            EmaDistanceFilter::default(),
            None,
            &mut signal,
            &mut dynamic_adjustments,
            &mut range_snapshot,
        );

        assert_eq!(signal.should_buy, Some(false));
        assert!(signal
            .filter_reasons
            .contains(&"BEARISH_FVG_PRESSURE_LONG_BLOCK".to_string()));
    }
    #[test]
    fn deep_negative_hammer_long_candidate_helper_matches_expected_shape() {
        let values = VegasIndicatorSignalValue {
            bollinger_value: BollingerSignalValue {
                is_long_signal: true,
                ..BollingerSignalValue::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                ..KlineHammerSignalValue::default()
            },
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 1.4,
                ..VolumeTrendSignalValue::default()
            },
            rsi_value: RsiSignalValue {
                rsi_value: 39.0,
                ..RsiSignalValue::default()
            },
            macd_value: MacdSignalValue {
                macd_line: -31.0,
                signal_line: -11.0,
                histogram: -21.0,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(VegasStrategy::is_deep_negative_hammer_long_candidate(
            &values
        ));
    }
    #[test]
    fn repair_long_candidate_helper_matches_expected_shape() {
        let values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 1.6,
                ..VolumeTrendSignalValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: -1.0,
                histogram_increasing: true,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(VegasStrategy::is_repair_long_candidate(&values, Some(44.0)));
        assert!(!VegasStrategy::is_repair_long_candidate(
            &values,
            Some(46.0)
        ));
    }
    #[test]
    fn counter_trend_hammer_long_new_leg_positive_macd_candidate_matches_expected_shape() {
        let values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                body_ratio: 0.16,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 2.86,
                ..VolumeTrendSignalValue::default()
            },
            leg_detection_value: crate::leg_detection_indicator::LegDetectionValue {
                is_bearish_leg: true,
                is_new_leg: true,
                ..crate::leg_detection_indicator::LegDetectionValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 1.95,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(
            VegasStrategy::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                &values,
                Some(36.0)
            )
        );
    }
    #[test]
    fn counter_trend_hammer_long_new_leg_positive_macd_candidate_requires_non_negative_histogram() {
        let values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                ..KlineHammerSignalValue::default()
            },
            leg_detection_value: crate::leg_detection_indicator::LegDetectionValue {
                is_bearish_leg: true,
                is_new_leg: true,
                ..crate::leg_detection_indicator::LegDetectionValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: -0.1,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(
            !VegasStrategy::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                &values,
                Some(36.0)
            )
        );
    }
    #[test]
    fn counter_trend_hammer_long_new_leg_positive_macd_candidate_rejects_extreme_histogram_or_weak_body(
    ) {
        let extreme_hist_values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                body_ratio: 0.18,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 2.0,
                ..VolumeTrendSignalValue::default()
            },
            leg_detection_value: crate::leg_detection_indicator::LegDetectionValue {
                is_bearish_leg: true,
                is_new_leg: true,
                ..crate::leg_detection_indicator::LegDetectionValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 12.0,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(
            !VegasStrategy::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                &extreme_hist_values,
                Some(36.0)
            )
        );
        let weak_body_values = VegasIndicatorSignalValue {
            kline_hammer_value: KlineHammerSignalValue {
                body_ratio: 0.08,
                ..extreme_hist_values.kline_hammer_value
            },
            macd_value: MacdSignalValue {
                histogram: 1.5,
                ..MacdSignalValue::default()
            },
            ..extreme_hist_values
        };
        assert!(
            !VegasStrategy::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                &weak_body_values,
                Some(36.0)
            )
        );
        let extreme_volume_values = VegasIndicatorSignalValue {
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_long_signal: true,
                body_ratio: 0.18,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 4.2,
                ..VolumeTrendSignalValue::default()
            },
            leg_detection_value: crate::leg_detection_indicator::LegDetectionValue {
                is_bearish_leg: true,
                is_new_leg: true,
                ..crate::leg_detection_indicator::LegDetectionValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 1.5,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(
            !VegasStrategy::is_counter_trend_hammer_long_new_leg_positive_macd_candidate(
                &extreme_volume_values,
                Some(36.0)
            )
        );
    }
    #[test]
    fn weak_ema_trend_entry_without_pattern_below_fib_midline_should_be_blocked() {
        let conditions = vec![
            (
                SignalType::VolumeTrend,
                SignalCondition::Volume {
                    is_increasing: true,
                    ratio: 2.8,
                },
            ),
            (
                SignalType::EmaTrend,
                SignalCondition::EmaTouchTrend {
                    is_long_signal: true,
                    is_short_signal: false,
                },
            ),
            (
                SignalType::Rsi,
                SignalCondition::RsiLevel {
                    current: 48.0,
                    oversold: 15.0,
                    overbought: 85.0,
                    is_valid: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: true,
                    is_bearish_leg: false,
                    is_new_leg: false,
                },
            ),
        ];
        let fib_value = FibRetracementSignalValue {
            is_long_signal: true,
            in_zone: true,
            retracement_ratio: 0.49,
            swing_high: 120.0,
            swing_low: 100.0,
            ..FibRetracementSignalValue::default()
        };
        assert!(VegasStrategy::should_block_weak_ema_trend_entry(
            &conditions,
            &fib_value,
            true,
        ));
    }
    #[test]
    fn weak_ema_trend_entry_with_engulfing_confirmation_should_stay_allowed() {
        let conditions = vec![
            (
                SignalType::EmaTrend,
                SignalCondition::EmaTouchTrend {
                    is_long_signal: true,
                    is_short_signal: false,
                },
            ),
            (
                SignalType::Engulfing,
                SignalCondition::Engulfing {
                    is_long_signal: true,
                    is_short_signal: false,
                },
            ),
        ];
        let fib_value = FibRetracementSignalValue {
            is_long_signal: true,
            in_zone: true,
            retracement_ratio: 0.42,
            swing_high: 120.0,
            swing_low: 100.0,
            ..FibRetracementSignalValue::default()
        };
        assert!(!VegasStrategy::should_block_weak_ema_trend_entry(
            &conditions,
            &fib_value,
            true,
        ));
    }
    #[test]
    fn weak_structure_breakout_long_without_bos_should_be_blocked() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: true,
                    price_below: false,
                },
            ),
            (
                SignalType::Rsi,
                SignalCondition::RsiLevel {
                    current: 55.0,
                    oversold: 15.0,
                    overbought: 85.0,
                    is_valid: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: true,
                    is_bearish_leg: false,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: false,
                    is_bearish_bos: false,
                    is_bullish_choch: true,
                    is_bearish_choch: false,
                    is_internal: true,
                },
            ),
        ];
        assert!(VegasStrategy::should_block_weak_structure_breakout_long(
            &conditions,
            Some(55.0),
        ));
    }
    #[test]
    fn weak_structure_breakout_long_with_bos_should_stay_allowed() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: true,
                    price_below: false,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: true,
                    is_bearish_leg: false,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: true,
                    is_bearish_bos: false,
                    is_bullish_choch: true,
                    is_bearish_choch: false,
                    is_internal: true,
                },
            ),
        ];
        assert!(!VegasStrategy::should_block_weak_structure_breakout_long(
            &conditions,
            Some(58.0),
        ));
    }
    #[test]
    fn conflicting_bullish_structure_short_should_be_blocked_when_too_far() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: true,
                    price_below: false,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: false,
                    is_bearish_bos: false,
                    is_bullish_choch: true,
                    is_bearish_choch: false,
                    is_internal: true,
                },
            ),
        ];
        assert!(
            VegasStrategy::should_block_conflicting_structure_breakout_short(
                &conditions,
                EmaDistanceState::TooFar,
            )
        );
    }
    #[test]
    fn conflicting_bullish_structure_short_should_stay_allowed_when_not_too_far() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: true,
                    price_below: false,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: true,
                    is_bearish_bos: false,
                    is_bullish_choch: false,
                    is_bearish_choch: false,
                    is_internal: true,
                },
            ),
        ];
        assert!(
            !VegasStrategy::should_block_conflicting_structure_breakout_short(
                &conditions,
                EmaDistanceState::Normal,
            )
        );
    }
    #[test]
    fn shallow_fib_breakdown_short_should_be_blocked_when_too_far() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: false,
                    price_below: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: false,
                    is_bearish_bos: true,
                    is_bullish_choch: false,
                    is_bearish_choch: false,
                    is_internal: true,
                },
            ),
        ];
        let fib_value = FibRetracementSignalValue {
            in_zone: false,
            retracement_ratio: 0.26,
            swing_high: 120.0,
            swing_low: 100.0,
            ..FibRetracementSignalValue::default()
        };
        assert!(VegasStrategy::should_block_shallow_fib_breakdown_short(
            &conditions,
            EmaDistanceState::TooFar,
            &fib_value,
        ));
    }
    #[test]
    fn shallow_fib_breakdown_short_should_stay_allowed_in_fib_zone() {
        let conditions = vec![
            (
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above: false,
                    price_below: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: false,
                },
            ),
            (
                SignalType::MarketStructure,
                SignalCondition::MarketStructure {
                    is_bullish_bos: false,
                    is_bearish_bos: false,
                    is_bullish_choch: false,
                    is_bearish_choch: true,
                    is_internal: true,
                },
            ),
        ];
        let fib_value = FibRetracementSignalValue {
            in_zone: true,
            retracement_ratio: 0.26,
            swing_high: 120.0,
            swing_low: 100.0,
            ..FibRetracementSignalValue::default()
        };
        assert!(!VegasStrategy::should_block_shallow_fib_breakdown_short(
            &conditions,
            EmaDistanceState::TooFar,
            &fib_value,
        ));
    }
    #[test]
    fn conflicting_too_far_new_bear_leg_short_should_be_blocked_with_low_volume() {
        let conditions = vec![
            (
                SignalType::Bolling,
                SignalCondition::Bolling {
                    is_long_signal: true,
                    is_short_signal: false,
                    is_close_signal: false,
                },
            ),
            (
                SignalType::Engulfing,
                SignalCondition::Engulfing {
                    is_long_signal: false,
                    is_short_signal: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: true,
                },
            ),
        ];
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            fib_retracement_value: FibRetracementSignalValue {
                in_zone: true,
                ..FibRetracementSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 1.06,
                ..VolumeTrendSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(
            VegasStrategy::should_block_conflicting_too_far_new_bear_leg_short(
                &conditions,
                &signal_values,
            )
        );
    }
    #[test]
    fn conflicting_too_far_new_bear_leg_short_should_stay_allowed_with_high_volume() {
        let conditions = vec![
            (
                SignalType::Bolling,
                SignalCondition::Bolling {
                    is_long_signal: true,
                    is_short_signal: false,
                    is_close_signal: false,
                },
            ),
            (
                SignalType::Engulfing,
                SignalCondition::Engulfing {
                    is_long_signal: false,
                    is_short_signal: true,
                },
            ),
            (
                SignalType::LegDetection,
                SignalCondition::LegDetection {
                    is_bullish_leg: false,
                    is_bearish_leg: true,
                    is_new_leg: true,
                },
            ),
        ];
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            fib_retracement_value: FibRetracementSignalValue {
                in_zone: true,
                ..FibRetracementSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 2.3,
                ..VolumeTrendSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(
            !VegasStrategy::should_block_conflicting_too_far_new_bear_leg_short(
                &conditions,
                &signal_values,
            )
        );
    }
    #[test]
    fn macd_near_zero_short_stop_should_tighten_to_midpoint() {
        let macd = MacdSignalValue {
            histogram: 1.2,
            ..MacdSignalValue::default()
        };
        let tightened =
            VegasStrategy::tighten_short_signal_stop_near_zero_macd(100.0, 110.0, &macd);
        assert_eq!(Some(105.0), tightened);
    }
    #[test]
    fn macd_far_from_zero_short_stop_should_stay_unadjusted() {
        let macd = MacdSignalValue {
            histogram: 3.5,
            ..MacdSignalValue::default()
        };
        let tightened =
            VegasStrategy::tighten_short_signal_stop_near_zero_macd(100.0, 110.0, &macd);
        assert_eq!(None, tightened);
    }
    #[test]
    fn macd_near_zero_weak_hammer_short_should_be_blocked_when_too_far_and_low_volume() {
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_short_signal: true,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 0.85,
                ..VolumeTrendSignalValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 0.63,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(VegasStrategy::should_block_macd_near_zero_weak_hammer_short(&signal_values));
    }
    #[test]
    fn macd_near_zero_weak_hammer_short_should_stay_allowed_with_higher_volume() {
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            ema_values: EmaSignalValue {
                is_short_trend: true,
                ..EmaSignalValue::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_short_signal: true,
                ..KlineHammerSignalValue::default()
            },
            volume_value: VolumeTrendSignalValue {
                volume_ratio: 1.05,
                ..VolumeTrendSignalValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 0.63,
                ..MacdSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(!VegasStrategy::should_block_macd_near_zero_weak_hammer_short(&signal_values));
    }
    #[test]
    fn too_far_uptrend_opposing_hammer_short_should_be_blocked() {
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            ema_touch_value: EmaTouchTrendSignalValue {
                is_uptrend: true,
                ..EmaTouchTrendSignalValue::default()
            },
            ema_values: EmaSignalValue {
                is_long_trend: true,
                is_short_trend: false,
                ..EmaSignalValue::default()
            },
            fib_retracement_value: FibRetracementSignalValue {
                in_zone: false,
                ..FibRetracementSignalValue::default()
            },
            bollinger_value: BollingerSignalValue {
                is_long_signal: false,
                is_short_signal: true,
                ..BollingerSignalValue::default()
            },
            leg_detection_value: LegDetectionValue {
                is_bullish_leg: true,
                is_bearish_leg: false,
                is_new_leg: false,
                ..LegDetectionValue::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_short_signal: true,
                ..KlineHammerSignalValue::default()
            },
            engulfing_value: EngulfingSignalValue {
                is_valid_engulfing: false,
                ..EngulfingSignalValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 2.4,
                ..MacdSignalValue::default()
            },
            rsi_value: RsiSignalValue {
                rsi_value: 62.0,
                ..RsiSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(VegasStrategy::should_block_too_far_uptrend_opposing_hammer_short(&signal_values));
    }
    #[test]
    fn too_far_uptrend_opposing_hammer_short_should_stay_allowed_in_fib_zone() {
        let signal_values = VegasIndicatorSignalValue {
            ema_distance_filter: EmaDistanceFilter {
                state: EmaDistanceState::TooFar,
                ..EmaDistanceFilter::default()
            },
            ema_touch_value: EmaTouchTrendSignalValue {
                is_uptrend: true,
                ..EmaTouchTrendSignalValue::default()
            },
            ema_values: EmaSignalValue {
                is_long_trend: true,
                is_short_trend: false,
                ..EmaSignalValue::default()
            },
            fib_retracement_value: FibRetracementSignalValue {
                in_zone: true,
                ..FibRetracementSignalValue::default()
            },
            bollinger_value: BollingerSignalValue {
                is_long_signal: false,
                is_short_signal: true,
                ..BollingerSignalValue::default()
            },
            leg_detection_value: LegDetectionValue {
                is_bullish_leg: true,
                is_bearish_leg: false,
                is_new_leg: false,
                ..LegDetectionValue::default()
            },
            kline_hammer_value: KlineHammerSignalValue {
                is_short_signal: true,
                ..KlineHammerSignalValue::default()
            },
            engulfing_value: EngulfingSignalValue {
                is_valid_engulfing: false,
                ..EngulfingSignalValue::default()
            },
            macd_value: MacdSignalValue {
                histogram: 2.4,
                ..MacdSignalValue::default()
            },
            rsi_value: RsiSignalValue {
                rsi_value: 62.0,
                ..RsiSignalValue::default()
            },
            ..VegasIndicatorSignalValue::default()
        };
        assert!(
            !VegasStrategy::should_block_too_far_uptrend_opposing_hammer_short(&signal_values,)
        );
    }
