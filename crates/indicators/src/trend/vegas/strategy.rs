use crate::signal_weight::{SignalCondition, SignalDirect, SignalType, SignalWeightsConfig};
use crate::volatility::atr::ATR;
use crate::volatility::bollinger::BollingBandsSignalConfig;
use rust_quant_common::enums::common::{EnumAsStrTrait, PeriodEnum};
use rust_quant_common::CandleItem;
use rust_quant_domain::{BacktestResult, BasicRiskStrategyConfig, SignalResult};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::config::*;
use super::ema_filter::{self, EmaDistanceConfig};
use super::indicator_combine::IndicatorCombine;
use super::signal::*;
use super::trend;
use super::utils;
use crate::trend::counter_trend;

/// Vegas综合策略配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VegasStrategy {
    /// 周期
    pub period: String,
    /// 最小需要的k线数量
    pub min_k_line_num: usize,
    /// EMA信号配置
    pub ema_signal: Option<EmaSignalConfig>,
    /// 成交量信号配置
    pub volume_signal: Option<VolumeSignalConfig>,
    /// EMA趋势配置
    pub ema_touch_trend_signal: Option<EmaTouchTrendSignalConfig>,
    /// RSI信号配置
    pub rsi_signal: Option<RsiSignalConfig>,
    /// 布林带信号配置
    pub bolling_signal: Option<BollingBandsSignalConfig>,
    /// 权重配置
    pub signal_weights: Option<SignalWeightsConfig>,
    /// 吞没形态指标
    pub engulfing_signal: Option<EngulfingSignalConfig>,
    /// 锤子形态指标
    pub kline_hammer_signal: Option<KlineHammerConfig>,
    /// 腿部识别配置
    pub leg_detection_signal: Option<LegDetectionConfig>,
    /// 震荡过滤配置（仅调整止盈目标，不作为开仓信号）
    pub range_filter_signal: Option<RangeFilterConfig>,
    /// 极端K线过滤/放行配置
    #[serde(default = "default_extreme_k_filter")]
    pub extreme_k_filter_signal: Option<ExtremeKFilterConfig>,
    /// 大实体止损配置
    #[serde(default = "default_large_entity_stop_loss_config")]
    pub large_entity_stop_loss_config: Option<LargeEntityStopLossConfig>,
    /// 追涨追跌确认配置
    #[serde(default = "default_chase_confirm_config")]
    pub chase_confirm_config: Option<ChaseConfirmConfig>,
    /// MACD 信号配置
    #[serde(default = "default_macd_signal_config")]
    pub macd_signal: Option<MacdSignalConfig>,
    /// Fib 回撤入场配置（趋势回调/反弹入场）
    #[serde(default = "default_fib_retracement_signal_config")]
    pub fib_retracement_signal: Option<FibRetracementSignalConfig>,
    /// EMA 距离过滤配置（控制 TooFar/Ranging 等阈值）
    #[serde(default = "default_ema_distance_config")]
    pub ema_distance_config: EmaDistanceConfig,
    /// ATR 止损倍数（默认 1.5xATR）
    #[serde(default = "default_atr_stop_loss_multiplier")]
    pub atr_stop_loss_multiplier: f64,
    /// 是否输出信号调试信息（single_value/single_result）
    #[serde(default = "default_emit_debug")]
    pub emit_debug: bool,
}

fn default_ema_distance_config() -> EmaDistanceConfig {
    EmaDistanceConfig::default()
}

fn default_atr_stop_loss_multiplier() -> f64 {
    1.5
}

fn default_emit_debug() -> bool {
    true
}

impl VegasStrategy {
    pub fn new(period: String) -> Self {
        Self {
            period,
            min_k_line_num: 7000,
            ema_signal: Some(EmaSignalConfig::default()),
            volume_signal: Some(VolumeSignalConfig::default()),
            ema_touch_trend_signal: Some(EmaTouchTrendSignalConfig::default()),
            rsi_signal: Some(RsiSignalConfig::default()),
            bolling_signal: Some(BollingBandsSignalConfig::default()),
            signal_weights: Some(SignalWeightsConfig::default()),
            engulfing_signal: Some(EngulfingSignalConfig::default()),
            kline_hammer_signal: Some(KlineHammerConfig::default()),
            leg_detection_signal: Some(LegDetectionConfig {
                is_open: false,
                ..LegDetectionConfig::default()
            }),
            range_filter_signal: Some(RangeFilterConfig::default()),

            extreme_k_filter_signal: default_extreme_k_filter(),
            large_entity_stop_loss_config: default_large_entity_stop_loss_config(),
            chase_confirm_config: default_chase_confirm_config(),
            macd_signal: default_macd_signal_config(),
            fib_retracement_signal: default_fib_retracement_signal_config(),
            ema_distance_config: default_ema_distance_config(),
            atr_stop_loss_multiplier: default_atr_stop_loss_multiplier(),
            emit_debug: default_emit_debug(),
        }
    }

    pub fn get_strategy_name() -> String {
        "vegas".to_string()
    }

    /// 获取最小数据长度
    pub fn get_min_data_length(&mut self) -> usize {
        self.min_k_line_num
    }

    /// 获取交易信号
    /// data_items: 数据列表，在突破策略中要考虑到前一根k线
    pub fn get_trade_signal(
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &mut VegasIndicatorSignalValue,
        weights: &SignalWeightsConfig,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        // 输入验证
        if data_items.is_empty() {
            return SignalResult {
                should_buy: Some(false),
                should_sell: Some(false),
                open_price: Some(0.0),
                best_open_price: None,
                atr_take_profit_ratio_price: None,
                atr_stop_loss_price: None,
                long_signal_take_profit_price: None,
                short_signal_take_profit_price: None,
                signal_kline_stop_loss_price: None,
                stop_loss_source: None,
                move_stop_open_price_when_touch_price: None,
                ts: Some(0),
                single_value: None,
                single_result: None,
                counter_trend_pullback_take_profit_price: None,
                // 填充新字段
                direction: rust_quant_domain::SignalDirection::None,
                strength: rust_quant_domain::SignalStrength::new(0.0),
                signals: vec![],
                can_open: false,
                should_close: false,
                entry_price: None,
                stop_loss_price: None,
                take_profit_price: None,
                position_time: None,
                signal_kline: None,
                filter_reasons: vec![],
                dynamic_adjustments: vec![],
                dynamic_config_snapshot: None,
            };
        }

        let last_data_item = match data_items.last() {
            Some(item) => item,
            None => {
                return SignalResult {
                    should_buy: Some(false),
                    should_sell: Some(false),
                    open_price: Some(0.0),
                    best_open_price: None,
                    atr_take_profit_ratio_price: None,
                    atr_stop_loss_price: None,
                    long_signal_take_profit_price: None,
                    short_signal_take_profit_price: None,
                    signal_kline_stop_loss_price: None,
                    stop_loss_source: None,
                    move_stop_open_price_when_touch_price: None,
                    counter_trend_pullback_take_profit_price: None,
                    ts: Some(0),
                    single_value: None,
                    single_result: None,
                    // 填充新字段
                    direction: rust_quant_domain::SignalDirection::None,
                    strength: rust_quant_domain::SignalStrength::new(0.0),
                    signals: vec![],
                    can_open: false,
                    should_close: false,
                    entry_price: None,
                    stop_loss_price: None,
                    take_profit_price: None,
                    position_time: None,
                    signal_kline: None,
                    filter_reasons: vec![],
                    dynamic_adjustments: vec![],
                    dynamic_config_snapshot: None,
                };
            }
        };

        // 初始化交易信号
        let mut signal_result = SignalResult {
            should_buy: Some(false),
            should_sell: Some(false),
            open_price: Some(last_data_item.c),
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            signal_kline_stop_loss_price: None,
            stop_loss_source: None,
            ts: Some(last_data_item.ts),
            single_value: None,
            single_result: None,
            counter_trend_pullback_take_profit_price: None,
            // 填充新字段
            direction: rust_quant_domain::SignalDirection::None,
            strength: rust_quant_domain::SignalStrength::new(0.0),
            signals: vec![],
            can_open: false,
            should_close: false,
            entry_price: None,
            stop_loss_price: None,
            take_profit_price: None,
            position_time: None,
            signal_kline: None,
            move_stop_open_price_when_touch_price: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
        };

        let mut conditions = Vec::with_capacity(10);
        let mut valid_rsi_value: Option<f64> = None;
        let mut dynamic_adjustments: Vec<String> = Vec::new();
        let mut range_snapshot: Option<serde_json::Value> = None;

        // 优先判断成交量
        if let Some(volume_signal) = &self.volume_signal {
            let is_than_vol_ratio =
                self.check_volume_trend(&vegas_indicator_signal_values.volume_value);
            conditions.push((
                SignalType::VolumeTrend,
                SignalCondition::Volume {
                    is_increasing: is_than_vol_ratio,
                    ratio: vegas_indicator_signal_values.volume_value.volume_ratio,
                },
            ));
        }

        // 检查EMA2被突破
        let (price_above, price_below) = self
            .check_breakthrough_conditions(data_items, vegas_indicator_signal_values.ema_values);

        if price_above || price_below {
            conditions.push((
                SignalType::SimpleBreakEma2through,
                SignalCondition::PriceBreakout {
                    price_above,
                    price_below,
                },
            ));
        }

        // 检查EMA排列，回调触碰关键均线位置
        let ema_trend =
            self.check_ema_touch_trend(data_items, vegas_indicator_signal_values.ema_values);
        vegas_indicator_signal_values.ema_touch_value = ema_trend;

        if ema_trend.is_long_signal || ema_trend.is_short_signal {
            conditions.push((
                SignalType::EmaTrend,
                SignalCondition::EmaTouchTrend {
                    is_long_signal: ema_trend.is_long_signal,
                    is_short_signal: ema_trend.is_short_signal,
                },
            ));
        }

        // 计算RSI
        if let Some(rsi_signal) = &self.rsi_signal {
            let current_rsi_opt = self.get_valid_rsi(
                data_items,
                &vegas_indicator_signal_values.rsi_value,
                vegas_indicator_signal_values.ema_values,
            );

            // 如果返回 None，表示检测到极端行情（大利空/利多消息），跳过后续交易信号判断
            let current_rsi = match current_rsi_opt {
                Some(rsi) => rsi,
                None => {
                    // 极端行情，直接返回不交易的信号
                    signal_result
                        .filter_reasons
                        .push("RSI_EXTREME_EVENT".to_string());
                    dynamic_adjustments.push("RSI_EXTREME_EVENT".to_string());
                    signal_result.dynamic_adjustments = dynamic_adjustments.clone();
                    signal_result.dynamic_config_snapshot = Some(
                        json!({
                            "kline_ts": last_data_item.ts,
                            "adjustments": dynamic_adjustments,
                        })
                        .to_string(),
                    );
                    return signal_result;
                }
            };

            valid_rsi_value = Some(current_rsi);

            conditions.push((
                SignalType::Rsi,
                SignalCondition::RsiLevel {
                    current: current_rsi,
                    oversold: rsi_signal.rsi_oversold,
                    overbought: rsi_signal.rsi_overbought,
                    is_valid: true,
                },
            ));
        }

        // 判断布林带
        if let Some(_bollinger_signal) = &self.bolling_signal {
            let bollinger_value =
                self.check_bollinger_signal(data_items, vegas_indicator_signal_values.clone());
            vegas_indicator_signal_values.bollinger_value = bollinger_value;
            conditions.push((
                SignalType::Bolling,
                SignalCondition::Bolling {
                    is_long_signal: bollinger_value.is_long_signal,
                    is_short_signal: bollinger_value.is_short_signal,
                    is_close_signal: bollinger_value.is_close_signal,
                },
            ));
        }

        // 检查突破的持续性
        let _breakthrough_confirmed = self.check_breakthrough_confirmation(data_items, price_above);

        // 计算振幅
        let _k_line_amplitude = utils::calculate_k_line_amplitude(data_items);

        // 计算吞没形态
        self.check_engulfing_signal(
            data_items,
            vegas_indicator_signal_values,
            &mut conditions,
            vegas_indicator_signal_values.ema_values,
        );

        // 添加锤子形态
        self.check_kline_hammer_signal(
            data_items,
            vegas_indicator_signal_values,
            &mut conditions,
            vegas_indicator_signal_values.ema_values,
        );

        // 腿部识别（可选）：只在 is_open 时参与条件打分
        if let Some(leg_detection_signal) = &self.leg_detection_signal {
            if leg_detection_signal.is_open {
                let leg_value = vegas_indicator_signal_values.leg_detection_value;
                if leg_value.is_bullish_leg || leg_value.is_bearish_leg {
                    conditions.push((
                        SignalType::LegDetection,
                        SignalCondition::LegDetection {
                            is_bullish_leg: leg_value.is_bullish_leg,
                            is_bearish_leg: leg_value.is_bearish_leg,
                            is_new_leg: leg_value.is_new_leg,
                        },
                    ));
                }
            }
        }

        // ================================================================
        // 【新增】EMA距离过滤
        // ================================================================
        let ema_distance_config = self.ema_distance_config;
        let ema_distance_filter = ema_filter::apply_ema_distance_filter(
            last_data_item.c,
            &vegas_indicator_signal_values.ema_values,
            &ema_distance_config,
        );
        vegas_indicator_signal_values.ema_distance_filter = ema_distance_filter;

        // ================================================================
        // 【新增】MACD 计算
        // ================================================================
        if let Some(macd_cfg) = &self.macd_signal {
            if macd_cfg.is_open && data_items.len() > macd_cfg.slow_period + macd_cfg.signal_period
            {
                use ta::indicators::MovingAverageConvergenceDivergence;
                use ta::Next;

                let mut macd = MovingAverageConvergenceDivergence::new(
                    macd_cfg.fast_period,
                    macd_cfg.slow_period,
                    macd_cfg.signal_period,
                )
                .unwrap();

                let mut prev_macd = 0.0f64;
                let mut prev_signal = 0.0f64;
                let mut prev_histogram = 0.0f64;
                let mut prev_prev_histogram = 0.0f64;

                // 计算所有 K 线的 MACD
                for item in data_items.iter() {
                    let macd_output = macd.next(item.c);
                    prev_prev_histogram = prev_histogram;
                    prev_histogram = macd_output.macd - macd_output.signal;
                    prev_signal = macd_output.signal;
                    prev_macd = macd_output.macd;
                }

                let histogram = prev_macd - prev_signal;

                // 判断金叉死叉：当前 histogram > 0 且前一根 < 0
                let is_golden_cross = histogram > 0.0 && prev_prev_histogram <= 0.0;
                let is_death_cross = histogram < 0.0 && prev_prev_histogram >= 0.0;

                // 判断柱状图趋势
                let histogram_increasing = histogram > prev_prev_histogram;
                let histogram_decreasing = histogram < prev_prev_histogram;
                // 判断动量是否正在改善（用于识别触底反弹）
                // 对于负区域：histogram > prev_histogram 表示负值在变小，动量改善
                let histogram_improving = histogram > prev_histogram;

                vegas_indicator_signal_values.macd_value = super::signal::MacdSignalValue {
                    macd_line: prev_macd,
                    signal_line: prev_signal,
                    histogram,
                    is_golden_cross,
                    is_death_cross,
                    histogram_increasing,
                    histogram_decreasing,
                    above_zero: prev_macd > 0.0,
                    prev_histogram: prev_prev_histogram,
                    histogram_improving,
                };
            }
        }

        // ================================================================
        // 【新增】Fib 回撤入场信号（Swing + Fib + 放量）
        // ================================================================
        let fib_cfg = self.fib_retracement_signal.unwrap_or_default();
        if fib_cfg.is_open {
            vegas_indicator_signal_values.fib_retracement_value =
                super::swing_fib::generate_fib_retracement_signal(
                    data_items,
                    &vegas_indicator_signal_values.ema_values,
                    &vegas_indicator_signal_values.leg_detection_value,
                    vegas_indicator_signal_values.volume_value.volume_ratio,
                    &fib_cfg,
                );
        } else {
            vegas_indicator_signal_values
                .fib_retracement_value
                .volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
        }

        // ================================================================
        // 计算得分
        // ================================================================
        let score = weights.calculate_score(conditions.clone());

        // 计算分数到达指定值
        // 计算分数到达指定值
        let mut signal_direction = weights.is_signal_valid(&score);
        if fib_cfg.is_open {
            let fib_val = vegas_indicator_signal_values.fib_retracement_value;
            let fib_direction = if fib_val.is_long_signal {
                Some(SignalDirect::IsLong)
            } else if fib_val.is_short_signal {
                Some(SignalDirect::IsShort)
            } else {
                None
            };

            // Fib 触发时优先使用 Fib 方向（即使原权重系统没有达到阈值）
            if fib_direction.is_some() {
                signal_direction = fib_direction;
            } else if fib_cfg.only_on_fib {
                // 仅Fib模式：未触发Fib则不允许开仓
                signal_direction = None;
            }
        }

        if let Some(signal_direction) = signal_direction {
            // 计算 ATR 用于止损价格
            let mut atr = ATR::new(14).unwrap();
            for item in data_items.iter() {
                atr.next(item.h, item.l, item.c);
            }
            let atr_value = atr.value();
            let atr_multiplier = self.atr_stop_loss_multiplier.max(0.0);

            // 检查大实体（Large Entity）状态
            let mut is_large_entity = false;
            let mut large_entity_retracement_sl: Option<f64> = None;

            if let Some(large_entity_cfg) = &self.large_entity_stop_loss_config {
                if large_entity_cfg.is_open {
                    let body_ratio = last_data_item.body_ratio();
                    let move_pct =
                        (last_data_item.c - last_data_item.o).abs() / last_data_item.o.max(1e-9);
                    let range = last_data_item.h - last_data_item.l;

                    if body_ratio >= large_entity_cfg.min_body_ratio
                        && move_pct >= large_entity_cfg.min_move_pct
                    {
                        is_large_entity = true;
                        // 计算基于回撤比例的止损
                        match signal_direction {
                            SignalDirect::IsLong => {
                                // 做多：High - Range * ratio (容忍从高点回撤一定比例)
                                let sl =
                                    last_data_item.h - range * large_entity_cfg.retracement_ratio;
                                // 确保止损不高于入场价(Close) - 保护性
                                large_entity_retracement_sl = Some(sl.min(last_data_item.c));
                            }
                            SignalDirect::IsShort => {
                                // 做空：Low + Range * ratio (容忍从低点反弹一定比例)
                                let sl =
                                    last_data_item.l + range * large_entity_cfg.retracement_ratio;
                                // 确保止损不低于入场价(Close) - 保护性
                                large_entity_retracement_sl = Some(sl.max(last_data_item.c));
                            }
                        }
                    }
                }
            }

            match signal_direction {
                SignalDirect::IsLong => {
                    signal_result.should_buy = Some(true);
                    signal_result.direction = rust_quant_domain::SignalDirection::Long;
                    // 做多止损: 入场价 - ATR * multiplier
                    if atr_value > 0.0 {
                        signal_result.atr_stop_loss_price =
                            Some(last_data_item.c - atr_value * atr_multiplier);
                    }

                    // Fib 回撤入场：优先写入 swing 止损（可配置）
                    if fib_cfg.is_open
                        && fib_cfg.use_swing_stop_loss
                        && vegas_indicator_signal_values
                            .fib_retracement_value
                            .is_long_signal
                        && signal_result.signal_kline_stop_loss_price.is_none()
                    {
                        let sl = vegas_indicator_signal_values
                            .fib_retracement_value
                            .suggested_stop_loss;
                        if sl > 0.0 {
                            signal_result.signal_kline_stop_loss_price =
                                Some(sl.min(last_data_item.c));
                            signal_result.stop_loss_source = Some("FibRetracement".to_string());
                        }
                    }

                    // 【成交量确认形态止损】只在成交量放大时启用形态止损
                    let volume_confirmed =
                        vegas_indicator_signal_values.volume_value.volume_ratio > 1.5;

                    // 1. 优先检查大实体止损（强趋势保护）
                    // 用户规则优化：如果macd是绿柱（histogram > 0），且快线大于慢线（macd > signal），就不启用大实体止损
                    let macd_val = &vegas_indicator_signal_values.macd_value;
                    let macd_strong_bullish =
                        macd_val.histogram > 0.0 && macd_val.macd_line > macd_val.signal_line;

                    if is_large_entity
                        && large_entity_retracement_sl.is_some()
                        && !macd_strong_bullish
                    {
                        signal_result.signal_kline_stop_loss_price = large_entity_retracement_sl;
                        signal_result.stop_loss_source =
                            Some("LargeEntity_Retracement".to_string());
                    }
                    // 2. 其次检查吞没形态 + 成交量确认
                    else if vegas_indicator_signal_values.engulfing_value.is_engulfing {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.o);
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Rejected".to_string());
                        }
                    }

                    // 3. 最后检查锤子线形态 + 成交量确认(如果还没有设置止损)
                    if signal_result.signal_kline_stop_loss_price.is_none()
                        && vegas_indicator_signal_values
                            .kline_hammer_value
                            .is_long_signal
                    {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.l);
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Rejected".to_string());
                        }
                    }
                }
                SignalDirect::IsShort => {
                    signal_result.should_sell = Some(true);
                    signal_result.direction = rust_quant_domain::SignalDirection::Short;
                    // 做空止损: 入场价 + ATR * multiplier
                    if atr_value > 0.0 {
                        signal_result.atr_stop_loss_price =
                            Some(last_data_item.c + atr_value * atr_multiplier);
                    }

                    // Fib 回撤入场：优先写入 swing 止损（可配置）
                    if fib_cfg.is_open
                        && fib_cfg.use_swing_stop_loss
                        && vegas_indicator_signal_values
                            .fib_retracement_value
                            .is_short_signal
                        && signal_result.signal_kline_stop_loss_price.is_none()
                    {
                        let sl = vegas_indicator_signal_values
                            .fib_retracement_value
                            .suggested_stop_loss;
                        if sl > 0.0 {
                            signal_result.signal_kline_stop_loss_price =
                                Some(sl.max(last_data_item.c));
                            signal_result.stop_loss_source = Some("FibRetracement".to_string());
                        }
                    }

                    // 【成交量确认形态止损】只在成交量放大时启用形态止损
                    let volume_confirmed =
                        vegas_indicator_signal_values.volume_value.volume_ratio > 1.5;

                    // 1. 优先检查大实体止损（强趋势保护）
                    // if is_large_entity && large_entity_retracement_sl.is_some() {
                    //    signal_result.signal_kline_stop_loss_price = large_entity_retracement_sl;
                    //    signal_result.stop_loss_source =
                    //        Some("LargeEntity_Retracement".to_string());
                    // }
                    // 2. 其次检查吞没形态 + 成交量确认
                    if vegas_indicator_signal_values.engulfing_value.is_engulfing {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.o);
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("Engulfing_Volume_Rejected".to_string());
                        }
                    }

                    // 3. 最后检查锤子线形态 + 成交量确认(如果还没有设置止损)
                    if signal_result.signal_kline_stop_loss_price.is_none()
                        && vegas_indicator_signal_values
                            .kline_hammer_value
                            .is_short_signal
                    {
                        if volume_confirmed {
                            signal_result.signal_kline_stop_loss_price = Some(last_data_item.h);
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Confirmed".to_string());
                        } else {
                            signal_result.stop_loss_source =
                                Some("KlineHammer_Volume_Rejected".to_string());
                        }
                    }
                }
            }

            // 信号产生时立即记录指标快照（在过滤逻辑之前）
            // 这样即使信号后续被过滤，filtered_signal_log 也能记录当时的指标状态

            signal_result.single_value = Some(json!(vegas_indicator_signal_values).to_string());
            signal_result.single_result = Some(json!(conditions).to_string());
        }

        // ================================================================
        // Fib 严格大趋势过滤：禁开反向仓
        // 只有当 swing 波动幅度足够大时，才应用此过滤，避免窄幅震荡中过度过滤
        // ================================================================
        if fib_cfg.is_open && fib_cfg.strict_major_trend {
            let major_bull =
                trend::is_major_bullish_trend(&vegas_indicator_signal_values.ema_values);
            let major_bear =
                trend::is_major_bearish_trend(&vegas_indicator_signal_values.ema_values);

            // 计算 swing 波动幅度
            let swing_high = vegas_indicator_signal_values
                .fib_retracement_value
                .swing_high;
            let swing_low = vegas_indicator_signal_values
                .fib_retracement_value
                .swing_low;
            let swing_move_pct = if swing_low > 0.0 {
                (swing_high - swing_low) / swing_low
            } else {
                0.0
            };

            // 只有在 swing 数据有效且波动幅度足够大时才应用过滤
            let is_trend_move_significant =
                swing_low > 0.0 && swing_move_pct >= fib_cfg.min_trend_move_pct;

            // 注意：这里仅记录"禁止开仓"的原因，不直接清空 should_buy/should_sell。
            // 这样回测/实盘可以在 backtest/position 层实现"反向信号仅平仓，不反手开仓"的行为。
            if is_trend_move_significant {
                if major_bear && signal_result.should_buy.unwrap_or(false) {
                    signal_result.filter_reasons.push(format!(
                        "FIB_STRICT_MAJOR_BEAR_BLOCK_LONG(swing_pct={:.2}%)",
                        swing_move_pct * 100.0
                    ));
                }
                if major_bull && signal_result.should_sell.unwrap_or(false) {
                    signal_result.filter_reasons.push(format!(
                        "FIB_STRICT_MAJOR_BULL_BLOCK_SHORT(swing_pct={:.2}%)",
                        swing_move_pct * 100.0
                    ));
                }
            }
        }

        // ================================================================
        // 应用EMA距离过滤（仅空头分支）
        // - 过远状态且空头排列：拒绝做空
        // ================================================================
        if ema_distance_filter.should_filter_short && signal_result.should_sell.unwrap_or(false) {
            signal_result.should_sell = Some(false);
            signal_result
                .filter_reasons
                .push("EMA_DISTANCE_FILTER_SHORT".to_string());
        }

        // ================================================================
        // 【追涨/追跌确认K线条件】
        // 当价格远离EMA144时，要求额外的确认条件才能开仓
        // 回测验证: ID 5988, profit +57%, sharpe 1.53→1.89, max_dd 57.7%→55.5%
        // ================================================================
        let chase_cfg = self.chase_confirm_config.unwrap_or_default();
        if chase_cfg.enabled {
            let ema144 = vegas_indicator_signal_values.ema_values.ema2_value;
            if ema144 > 0.0 {
                let price_vs_ema144 = (last_data_item.c - ema144) / ema144;

                // 追涨确认：price > EMA144*(1+threshold) 时要求额外确认
                if price_vs_ema144 > chase_cfg.long_threshold
                    && signal_result.should_buy.unwrap_or(false)
                {
                    let body_ratio = last_data_item.body_ratio();
                    let is_bullish = last_data_item.c > last_data_item.o;

                    // 确认条件（任一满足）
                    let pullback_touch = {
                        let low_vs_ema144 = (last_data_item.l - ema144) / ema144;
                        low_vs_ema144.abs() <= chase_cfg.pullback_touch_threshold
                    };
                    let bullish_close = is_bullish && body_ratio > chase_cfg.min_body_ratio;
                    let has_engulfing = vegas_indicator_signal_values
                        .engulfing_value
                        .is_valid_engulfing;

                    let confirmed = pullback_touch || bullish_close || has_engulfing;
                    if !confirmed {
                        signal_result.should_buy = Some(false);
                        signal_result
                            .filter_reasons
                            .push("CHASE_CONFIRM_FILTER_LONG".to_string());
                    }
                }

                // 追跌确认：price < EMA144*(1-threshold) 时要求额外确认
                if price_vs_ema144 < -chase_cfg.short_threshold
                    && signal_result.should_sell.unwrap_or(false)
                {
                    let body_ratio = last_data_item.body_ratio();
                    let is_bearish = last_data_item.c < last_data_item.o;

                    // 确认条件（任一满足）
                    let bounce_touch = {
                        let high_vs_ema144 = (last_data_item.h - ema144) / ema144;
                        high_vs_ema144.abs() <= chase_cfg.pullback_touch_threshold
                    };
                    let bearish_close = is_bearish && body_ratio > chase_cfg.min_body_ratio;
                    let has_engulfing = vegas_indicator_signal_values
                        .engulfing_value
                        .is_valid_engulfing;

                    let confirmed = bounce_touch || bearish_close || has_engulfing;
                    if !confirmed {
                        signal_result.should_sell = Some(false);
                        signal_result
                            .filter_reasons
                            .push("CHASE_CONFIRM_FILTER_SHORT".to_string());
                    }
                }
            }
        }

        // ================================================================
        // 【新增】极端K线过滤/放行：
        // - 大实体且一次跨越多条EMA时，仅顺势放行；反向信号直接过滤
        // - 方向冲突时撤销信号，避免追入假突破
        // ================================================================
        if let Some(extreme_cfg) = self.extreme_k_filter_signal.as_ref() {
            if extreme_cfg.is_open {
                let body_ratio = last_data_item.body_ratio();
                let body_move_pct =
                    ((last_data_item.c - last_data_item.o).abs()) / last_data_item.o.max(1e-9);
                let cross_count = Self::count_crossed_emas(
                    last_data_item.o,
                    last_data_item.c,
                    &vegas_indicator_signal_values.ema_values,
                );

                let is_extreme = body_ratio >= extreme_cfg.min_body_ratio
                    && body_move_pct >= extreme_cfg.min_move_pct
                    && cross_count >= extreme_cfg.min_cross_ema_count;

                if is_extreme {
                    let is_bull = last_data_item.c > last_data_item.o;
                    let is_bear = last_data_item.c < last_data_item.o;

                    if is_bull && signal_result.should_sell.unwrap_or(false) {
                        signal_result.should_sell = Some(false);
                        signal_result
                            .filter_reasons
                            .push("EXTREME_K_FILTER_CONFLICT_SHORT".to_string());
                    }
                    if is_bear && signal_result.should_buy.unwrap_or(false) {
                        signal_result.should_buy = Some(false);
                        signal_result
                            .filter_reasons
                            .push("EXTREME_K_FILTER_CONFLICT_LONG".to_string());
                    }

                    // 仅顺势放行，逆势则拦截
                    if signal_result.should_buy.unwrap_or(false) {
                        // 如果是大趋势多头且极端K线也是多头，则放行（忽略小趋势）
                        let allow_by_major = trend::is_major_bullish_trend(
                            &vegas_indicator_signal_values.ema_values,
                        ) && is_bull;

                        if !allow_by_major {
                            // 否则必须满足小趋势多头
                            if !trend::is_bullish_trend(&vegas_indicator_signal_values.ema_values) {
                                signal_result.should_buy = Some(false);
                                signal_result
                                    .filter_reasons
                                    .push("EXTREME_K_FILTER_TREND_LONG".to_string());
                            }
                        }
                    }

                    if signal_result.should_sell.unwrap_or(false) {
                        // 如果是大趋势空头且极端K线也是空头，则放行（忽略小趋势）
                        let allow_by_major = trend::is_major_bearish_trend(
                            &vegas_indicator_signal_values.ema_values,
                        ) && is_bear;

                        if !allow_by_major {
                            // 否则必须满足小趋势空头
                            if !trend::is_bearish_trend(&vegas_indicator_signal_values.ema_values) {
                                signal_result.should_sell = Some(false);
                                signal_result
                                    .filter_reasons
                                    .push("EXTREME_K_FILTER_TREND_SHORT".to_string());
                            }
                        }
                    }
                }
            }
        }

        // ================================================================
        // 震荡过滤：震荡时降低止盈目标（不影响开仓，只影响 TP）
        // 震荡区间: RSI 中性 + 缩量或 MACD 近零轴 -> 1:1 止盈
        // ================================================================
        if let Some(range_filter_signal) = &self.range_filter_signal {
            if range_filter_signal.is_open && self.bolling_signal.is_some() {
                let bb_value = &vegas_indicator_signal_values.bollinger_value;
                let mid = bb_value.middle;
                let width = bb_value.upper - bb_value.lower;
                if mid > 0.0 && width > 0.0 {
                    let bb_width_ratio = width / mid;
                    if bb_width_ratio <= range_filter_signal.bb_width_threshold {
                        let k_range = (last_data_item.h - last_data_item.l)
                            .abs()
                            .max(last_data_item.c * 0.001);
                        let tp_ratio = range_filter_signal.tp_kline_ratio.max(0.0);
                        let entry_price = signal_result.open_price.unwrap_or(last_data_item.c);
                        let volume_ratio = vegas_indicator_signal_values.volume_value.volume_ratio;
                        let rsi_in_range = valid_rsi_value
                            .map(|rsi| (46.0..=54.0).contains(&rsi))
                            .unwrap_or(false);
                        let macd_near_zero = self.macd_signal.as_ref().is_some_and(|macd_cfg| {
                            if !macd_cfg.is_open {
                                return false;
                            }
                            let macd_val = &vegas_indicator_signal_values.macd_value;
                            macd_val.macd_line.abs() <= entry_price * 0.001
                        });
                        let is_ultra_narrow =
                            bb_width_ratio <= range_filter_signal.bb_width_threshold * 0.85;
                        let is_indecision = last_data_item.is_small_body_and_big_up_down_shadow();
                        let use_one_to_one = rsi_in_range
                            && (volume_ratio < 1.05 || macd_near_zero || is_indecision)
                            && is_ultra_narrow;
                        range_snapshot = Some(json!({
                            "enabled": true,
                            "bb_width_ratio": bb_width_ratio,
                            "bb_width_threshold": range_filter_signal.bb_width_threshold,
                            "tp_ratio": tp_ratio,
                            "use_one_to_one": use_one_to_one,
                            "volume_ratio": volume_ratio,
                            "rsi": valid_rsi_value,
                            "macd_near_zero": macd_near_zero,
                            "is_indecision": is_indecision,
                        }));
                        if use_one_to_one {
                            dynamic_adjustments.push("RANGE_TP_ONE_TO_ONE".to_string());
                        } else {
                            dynamic_adjustments.push("RANGE_TP_RATIO".to_string());
                        }

                        let take_profit_diff = if use_one_to_one {
                            let stop_price = signal_result
                                .signal_kline_stop_loss_price
                                .or(signal_result.atr_stop_loss_price);
                            let diff = stop_price
                                .map(|price| (entry_price - price).abs())
                                .unwrap_or(0.0);
                            if diff > 0.0 {
                                diff
                            } else {
                                k_range * tp_ratio
                            }
                        } else {
                            k_range * tp_ratio
                        };

                        if signal_result.should_buy.unwrap_or(false) {
                            signal_result.long_signal_take_profit_price =
                                Some(entry_price + take_profit_diff);
                        }
                        if signal_result.should_sell.unwrap_or(false) {
                            signal_result.short_signal_take_profit_price =
                                Some(entry_price - take_profit_diff);
                        }
                    }
                }
            }
        }

        // ================================================================
        // 【新增】MACD 动量反转过滤 (Momentum Turn Filter)
        // 核心逻辑：允许 MACD 反向入场（抄底/摸顶），但要求动量必须改善（拐点已现）
        // 1. 如果 MACD 与交易方向一致 -> 放行（顺势）
        // 2. 如果 MACD 与交易方向相反（逆势）：
        //    - 柱状图继续恶化（接飞刀） -> 过滤
        //    - 柱状图开始改善（企稳） -> 放行
        // ================================================================
        if let Some(macd_cfg) = &self.macd_signal {
            if macd_cfg.is_open {
                let macd_val = &vegas_indicator_signal_values.macd_value;

                // 做多过滤
                if signal_result.should_buy.unwrap_or(false) {
                    let mut should_filter = false;

                    if macd_cfg.filter_falling_knife {
                        // 如果 MACD 柱状图为负（处于空头动量区）
                        if macd_val.histogram < 0.0 {
                            // 且 柱状图在递减（负值变更大，动量加速向下）
                            if macd_val.histogram_decreasing {
                                should_filter = true; // 正在接飞刀，过滤
                                signal_result
                                    .filter_reasons
                                    .push("MACD_FALLING_KNIFE_LONG".to_string());
                            }
                        }
                    }

                    if should_filter {
                        signal_result.should_buy = Some(false);
                    }
                }

                // 做空过滤
                if signal_result.should_sell.unwrap_or(false) {
                    let mut should_filter = false;

                    if macd_cfg.filter_falling_knife {
                        // 如果 MACD 柱状图为正（处于多头动量区）
                        if macd_val.histogram > 0.0 {
                            // 且 柱状图在递增（正值变更大，动量加速向上）
                            if macd_val.histogram_increasing {
                                should_filter = true; // 正在逆势摸顶（涨势未尽），过滤
                                signal_result
                                    .filter_reasons
                                    .push("MACD_FALLING_KNIFE_SHORT".to_string());
                            }
                        }
                    }

                    if should_filter {
                        signal_result.should_sell = Some(false);
                    }
                }
            }
        }

        if signal_result.signal_kline_stop_loss_price.is_some() {
            dynamic_adjustments.push("STOP_LOSS_SIGNAL_KLINE".to_string());
        }
        if signal_result.atr_stop_loss_price.is_some() {
            dynamic_adjustments.push("STOP_LOSS_ATR".to_string());
        }
        if signal_result.long_signal_take_profit_price.is_some() {
            dynamic_adjustments.push("TP_DYNAMIC_LONG".to_string());
        }
        if signal_result.short_signal_take_profit_price.is_some() {
            dynamic_adjustments.push("TP_DYNAMIC_SHORT".to_string());
        }
        if signal_result
            .counter_trend_pullback_take_profit_price
            .is_some()
        {
            dynamic_adjustments.push("TP_COUNTER_TREND".to_string());
        }

        signal_result.dynamic_adjustments = dynamic_adjustments.clone();
        signal_result.dynamic_config_snapshot = Some(
            json!({
                "kline_ts": last_data_item.ts,
                "adjustments": dynamic_adjustments,
                "range_tp": range_snapshot,
                "stop_loss": {
                    "signal_kline": signal_result.signal_kline_stop_loss_price,
                    "atr": signal_result.atr_stop_loss_price,
                    "source": signal_result.stop_loss_source.clone(),
                },
                "take_profit": {
                    "long": signal_result.long_signal_take_profit_price,
                    "short": signal_result.short_signal_take_profit_price,
                    "atr_ratio": signal_result.atr_take_profit_ratio_price,
                    "counter_trend": signal_result.counter_trend_pullback_take_profit_price,
                }
            })
            .to_string(),
        );

        // 可选：添加详细信息到结果中
        if self.emit_debug
            && (signal_result.should_buy.unwrap_or(false)
                || signal_result.should_sell.unwrap_or(false))
        {
            //如果有使用信号k线止损
            if risk_config.is_used_signal_k_line_stop_loss.unwrap_or(false) {
                self.calculate_best_stop_loss_price(
                    last_data_item,
                    &mut signal_result,
                    &conditions,
                );
            }
            //如果有使用逆势回调止盈
            if risk_config
                .is_counter_trend_pullback_take_profit
                .unwrap_or(false)
            {
                counter_trend::calculate_counter_trend_pullback_take_profit_price(
                    data_items,
                    &mut signal_result,
                    &conditions,
                    vegas_indicator_signal_values.ema_values.ema1_value,
                );
            }
            signal_result.single_value = Some(json!(vegas_indicator_signal_values).to_string());
            signal_result.single_result = Some(json!(conditions).to_string());
        }

        signal_result
    }

    /// 获取指标组合
    pub fn get_indicator_combine(&self) -> IndicatorCombine {
        use crate::ema_indicator::EmaIndicator;
        use crate::leg_detection_indicator::LegDetectionIndicator;
        use crate::momentum::rsi::RsiIndicator;
        use crate::pattern::engulfing::KlineEngulfingIndicator;
        use crate::pattern::hammer::KlineHammerIndicator;
        use crate::volatility::bollinger::BollingBandsPlusIndicator;
        use crate::volume_indicator::VolumeRatioIndicator;

        let mut indicator_combine = IndicatorCombine::default();

        // 添加吞没形态
        if let Some(_engulfing_signal) = &self.engulfing_signal {
            indicator_combine.engulfing_indicator = Some(KlineEngulfingIndicator::new());
        }

        // 添加EMA
        if let Some(ema_signal) = &self.ema_signal {
            indicator_combine.ema_indicator = Some(EmaIndicator::new(
                ema_signal.ema1_length,
                ema_signal.ema2_length,
                ema_signal.ema3_length,
                ema_signal.ema4_length,
                ema_signal.ema5_length,
                ema_signal.ema6_length,
                ema_signal.ema7_length,
            ));
        }

        // 添加成交量
        if let Some(volume_signal) = &self.volume_signal {
            indicator_combine.volume_indicator = Some(VolumeRatioIndicator::new(
                volume_signal.volume_bar_num,
                true,
            ));
        }

        // 添加RSI
        if let Some(rsi_signal) = &self.rsi_signal {
            indicator_combine.rsi_indicator = Some(RsiIndicator::new(rsi_signal.rsi_length));
        }

        // 添加布林带
        if let Some(bolling_signal) = &self.bolling_signal {
            indicator_combine.bollinger_indicator = Some(BollingBandsPlusIndicator::new(
                bolling_signal.period,
                bolling_signal.multiplier,
                bolling_signal.consecutive_touch_times,
            ));
        }

        // 添加锤子形态
        if let Some(kline_hammer_signal) = &self.kline_hammer_signal {
            indicator_combine.kline_hammer_indicator = Some(KlineHammerIndicator::new(
                kline_hammer_signal.up_shadow_ratio,
                kline_hammer_signal.down_shadow_ratio,
            ));
        }

        // 添加腿部识别（可选）
        if let Some(leg_detection_signal) = &self.leg_detection_signal {
            if leg_detection_signal.is_open {
                indicator_combine.leg_detection_indicator =
                    Some(LegDetectionIndicator::new(leg_detection_signal.size));
            }
        }

        indicator_combine
    }

    /// 运行回测
    ///
    /// 注意：此方法不能在 indicators 包中完整实现，因为 BacktestResult 在不同包中定义不同
    /// 实际回测逻辑应在 strategies 或 orchestration 包中调用，使用 get_indicator_combine() 和 get_trade_signal()
    pub fn run_test(
        &mut self,
        _candles: &[CandleItem],
        _risk_strategy_config: BasicRiskStrategyConfig,
    ) -> BacktestResult {
        // 由于架构分层，indicators 包的 BacktestResult 与 strategies 包不同
        // 此方法仅作占位，实际回测在 orchestration/backtest_executor.rs 中实现
        unimplemented!(
            "VegasStrategy::run_test 应在 orchestration 包中调用，\
            使用 get_indicator_combine() 和 get_trade_signal() 方法"
        )
    }

    // 私有辅助方法
    fn check_volume_trend(&self, volume_trend: &VolumeTrendSignalValue) -> bool {
        if let Some(volume_signal_config) = &self.volume_signal {
            return volume_trend.volume_ratio > volume_signal_config.volume_increase_ratio;
        }
        false
    }

    fn check_breakthrough_conditions(
        &self,
        data_items: &[CandleItem],
        ema_value: EmaSignalValue,
    ) -> (bool, bool) {
        if let Some(ema_signal) = &self.ema_signal {
            trend::check_breakthrough_conditions(
                data_items,
                ema_value,
                ema_signal.ema_breakthrough_threshold,
            )
        } else {
            (false, false)
        }
    }

    fn check_ema_touch_trend(
        &self,
        data_items: &[CandleItem],
        ema_value: EmaSignalValue,
    ) -> EmaTouchTrendSignalValue {
        if let Some(ema_touch_trend_signal) = &self.ema_touch_trend_signal {
            trend::check_ema_touch_trend(data_items, ema_value, ema_touch_trend_signal)
        } else {
            EmaTouchTrendSignalValue::default()
        }
    }

    fn get_valid_rsi(
        &self,
        data_items: &[CandleItem],
        rsi_value: &RsiSignalValue,
        ema_value: EmaSignalValue,
    ) -> Option<f64> {
        trend::get_valid_rsi(data_items, rsi_value.rsi_value, ema_value)
    }

    fn check_breakthrough_confirmation(&self, data_items: &[CandleItem], is_upward: bool) -> bool {
        trend::check_breakthrough_confirmation(data_items, is_upward)
    }

    fn check_bollinger_signal(
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_value: VegasIndicatorSignalValue,
    ) -> BollingerSignalValue {
        let mut bolling_bands = vegas_indicator_signal_value.bollinger_value;
        // if data_items.last().expect("数据不能为空").ts == 1756051200000 {
        //     print!("bolling_bands: {:?}", bolling_bands);
        //     print!("data_items: {:?}", data_items.last());
        // }
        if let Some(_bollinger_signal) = &self.bolling_signal {
            let ema_signal_values = vegas_indicator_signal_value.ema_values;
            let data_item = data_items.last().expect("数据不能为空");

            if bolling_bands.lower > data_item.l() {
                bolling_bands.is_long_signal = true;
            }
            if bolling_bands.upper < data_item.h() {
                bolling_bands.is_short_signal = true;
            }

            //过滤逻辑,如果虽然触发了bollinger的信号，但是k线的收盘价，依然大于em1值,则认为bollinger的信号是无效的(除了对4H周期，其他的周期的提升非常大,特别是日线级别)
            if (bolling_bands.is_long_signal || bolling_bands.is_short_signal)
                && self.period != PeriodEnum::FourHour.as_str()
            {
                if bolling_bands.is_long_signal
                    && data_items.last().expect("数据不能为空").c < ema_signal_values.ema1_value
                {
                    bolling_bands.is_long_signal = false;
                    bolling_bands.is_force_filter_signal = true;
                }

                if bolling_bands.is_short_signal
                    && data_items.last().expect("数据不能为空").c > ema_signal_values.ema1_value
                {
                    bolling_bands.is_short_signal = false;
                    bolling_bands.is_force_filter_signal = true;
                }
            }
            //todo 加入过滤逻辑，如果出发点了布林带低点或者高点，但是k线是大阳线或者大阴线(实体站百分60以上)&&且刚开始形成死叉或者金叉的 表示很强势，不能直接做多，或者做空
            //todo 如何收盘价在支撑位置的下方，则不能做多，反之不能做空
            //todo 当均线空头排列时候。止盈 eth止盈为之前n根下跌k线的30%的位置，而且从最低点到最高点不能超过12%的收益
            //todo 如果上下引线都大于实体部分，说明此时不能开仓，因为此时趋势不明显，而且容易亏损
            //如果价格
            //判断k线的实体部分占比是否大于60%

            let body_ratio = data_items.last().expect("数据不能为空").body_ratio();
            if bolling_bands.is_long_signal || bolling_bands.is_short_signal {
                // if data_items.last().unwrap().ts == 1763049600000 {
                //     println!("data_items: {:?}", data_items.last().unwrap());
                //    println!("body_ratio: {:?}", data_items.last().unwrap().body_ratio());
                // }
                // if body_ratio > 0.8 {
                //     bolling_bands.is_force_filter_signal = true;
                //     bolling_bands.is_long_signal = false;
                //     bolling_bands.is_short_signal = false;
                // }
                if data_items
                    .last()
                    .expect("数据不能为空")
                    .is_small_body_and_big_up_down_shadow()
                {
                    bolling_bands.is_force_filter_signal = true;
                    bolling_bands.is_long_signal = false;
                    bolling_bands.is_short_signal = false;
                }
            }
        }

        bolling_bands
    }

    fn check_engulfing_signal(
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_value: &mut VegasIndicatorSignalValue,
        conditions: &mut Vec<(SignalType, SignalCondition)>,
        _ema_value: EmaSignalValue,
    ) {
        let mut is_engulfing = false;
        let last_data_item = data_items.last().expect("数据不能为空");

        if let Some(engulfing_signal) = &self.engulfing_signal {
            if vegas_indicator_signal_value.engulfing_value.is_engulfing
                && vegas_indicator_signal_value.engulfing_value.body_ratio
                    > engulfing_signal.body_ratio
            {
                vegas_indicator_signal_value
                    .engulfing_value
                    .is_valid_engulfing = true;
                is_engulfing = true;
            }
        }

        if is_engulfing {
            let is_long_signal = last_data_item.c() > last_data_item.o();
            let is_short_signal = !is_long_signal;

            conditions.push((
                SignalType::Engulfing,
                SignalCondition::Engulfing {
                    is_long_signal,
                    is_short_signal,
                },
            ));
        }
    }

    fn check_kline_hammer_signal(
        &self,
        data_items: &[CandleItem],
        vegas_indicator_signal_values: &mut VegasIndicatorSignalValue,
        conditions: &mut Vec<(SignalType, SignalCondition)>,
        ema_value: EmaSignalValue,
    ) {
        if let Some(_kline_hammer_signal) = &self.kline_hammer_signal {
            let is_hammer = vegas_indicator_signal_values.kline_hammer_value.is_hammer;
            let is_hanging_man = vegas_indicator_signal_values
                .kline_hammer_value
                .is_hanging_man;

            // 如果有长上影线，且振幅>0.5，则才能判断是有效的
            if is_hammer && utils::calculate_k_line_amplitude(data_items) >= 0.6 {
                vegas_indicator_signal_values
                    .kline_hammer_value
                    .is_long_signal = true;

                // 过滤条件
                if ema_value.is_short_trend
                    && data_items.last().expect("数据不能为空").c < ema_value.ema1_value
                    && data_items.last().expect("数据不能为空").v < 5000.0
                {
                    vegas_indicator_signal_values
                        .kline_hammer_value
                        .is_long_signal = false;
                }
            }

            if is_hanging_man && utils::calculate_k_line_amplitude(data_items) >= 0.6 {
                vegas_indicator_signal_values
                    .kline_hammer_value
                    .is_short_signal = true;

                // 过滤条件
                if ema_value.is_long_trend
                    && data_items.last().expect("数据不能为空").c > ema_value.ema1_value
                    && data_items.last().expect("数据不能为空").v < 5000.0
                {
                    vegas_indicator_signal_values
                        .kline_hammer_value
                        .is_short_signal = false;
                }
            }
            // //如何没有长上影线和长下影线的长影线，但是此时如何实体特别大，且是放量的大实体，则标记为上涨
            // if !is_hanging_man
            //     && !is_hammer
            //     && vegas_indicator_signal_values.kline_hammer_value.body_ratio > 0.9
            //     && vegas_indicator_signal_values.volume_value.volume_ratio > 1.7
            // {
            //     println!("time:{}",time_util::mill_time_to_datetime_shanghai(data_items.last().unwrap().ts).unwrap());
            //     if data_items.last().unwrap().c > data_items.last().unwrap().o() {
            //         vegas_indicator_signal_values
            //             .kline_hammer_value
            //             .is_long_signal = true;
            //     } else {
            //         vegas_indicator_signal_values
            //             .kline_hammer_value
            //             .is_long_signal = false;
            //     }
            // }
        }

        if vegas_indicator_signal_values
            .kline_hammer_value
            .is_long_signal
            || vegas_indicator_signal_values
                .kline_hammer_value
                .is_short_signal
        {
            conditions.push((
                SignalType::KlineHammer,
                SignalCondition::KlineHammer {
                    is_long_signal: vegas_indicator_signal_values
                        .kline_hammer_value
                        .is_long_signal,
                    is_short_signal: vegas_indicator_signal_values
                        .kline_hammer_value
                        .is_short_signal,
                },
            ));
        }
    }

    /// 统计极端K线一次跨越的EMA条数（开盘价与收盘价之间包含的EMA数量）
    fn count_crossed_emas(open: f64, close: f64, ema_values: &EmaSignalValue) -> usize {
        let (low, high) = if open < close {
            (open, close)
        } else {
            (close, open)
        };
        let emas = [
            ema_values.ema1_value,
            ema_values.ema2_value,
            ema_values.ema3_value,
            ema_values.ema4_value,
            ema_values.ema5_value,
        ];
        emas.iter()
            .filter(|ema| **ema >= low && **ema <= high)
            .count()
    }

    fn calculate_best_stop_loss_price(
        &self,
        last_data_item: &CandleItem,
        signal_result: &mut SignalResult,
        conditions: &[(SignalType, SignalCondition)],
    ) {
        // 检查是否有吞没形态信号
        let has_engulfing_signal = conditions
            .iter()
            .any(|(signal_type, _)| matches!(signal_type, SignalType::Engulfing));

        // 如果是吞没形态信号，使用开盘价作为止损价格
        if has_engulfing_signal {
            signal_result.signal_kline_stop_loss_price = Some(last_data_item.o());
        }

        // 【已禁用】只保留吞没形态止损，其他情况不设置信号线止损
        // if let Some(stop_loss_price) = utils::calculate_best_stop_loss_price(
        //     last_data_item,
        //     signal_result.should_buy.unwrap_or(false),
        //     signal_result.should_sell.unwrap_or(false),
        // ) {
        //     signal_result.signal_kline_stop_loss_price = Some(stop_loss_price);
        // }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EmaSignalValue, FibRetracementSignalConfig, RsiSignalConfig, SignalType,
        SignalWeightsConfig, VegasIndicatorSignalValue, VegasStrategy, VolumeSignalConfig,
    };
    use rust_quant_common::CandleItem;
    use rust_quant_domain::BasicRiskStrategyConfig;

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
    fn fib_strict_reason_includes_swing_pct_suffix() {
        let mut strategy = VegasStrategy {
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

        let mut indicator_values = VegasIndicatorSignalValue::default();
        indicator_values.ema_values = EmaSignalValue {
            ema1_value: 90.0,
            ema2_value: 95.0,
            ema3_value: 96.0,
            ema4_value: 100.0,
            ..EmaSignalValue::default()
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
}
