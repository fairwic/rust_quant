use std::cmp::PartialEq;
use serde::{Deserialize, Serialize};
use crate::trading::indicator::candle::Candle;
use crate::trading::indicator::detect_support_resistance::enums::{BreakoutType, LevelType, SupportResistance, TrendState};

/// 在检测过程中，对最近的 pivot 进行合并去噪
fn merge_close_pivots(
    sr_levels: &mut Vec<SupportResistance>,
    new_level: &SupportResistance,
    atr_value: f64,
    merge_ratio: f64, // 譬如 0.5 表示 pivot 距离 < 0.5*ATR 就合并
) -> bool {
    if let Some(last) = sr_levels.last_mut() {
        // 如果两者同为 Support 或同为 Resistance
        // 并且价差 < merge_ratio*ATR，则认为可合并
        if last.level_type == new_level.level_type {
            let dist = (last.price - new_level.price).abs();
            if dist < merge_ratio * atr_value {
                // 这里示意：我们把 price 调整为两者平均，也可以选更高或更低
                last.price = (last.price + new_level.price) / 2.0;
                return true; // 已合并，不需要插入 new_level
            }
        }
    }
    false
}
/// 简单ATR计算，period默认为14
pub fn calc_atr(candles: &[Candle], period: usize) -> Vec<f64> {
    let mut atr_values = vec![0.0; candles.len()];

    if candles.len() < period {
        return atr_values;
    }

    // TR = max(high - low, high - prev_close, prev_close - low)
    // ATR = MA of TR
    let mut tr_values = Vec::with_capacity(candles.len());
    tr_values.push(candles[0].high - candles[0].low); // 第一根没有 prev_close, 简化

    for i in 1..candles.len() {
        let high = candles[i].high;
        let low = candles[i].low;
        let prev_close = candles[i - 1].close;

        let tr = f64::max(high - low, f64::max((high - prev_close).abs(), (prev_close - low).abs()));
        tr_values.push(tr);
    }

    // 简单移动平均
    // 这里做 period 期的滚动平均，写法很直白(可能效率不高，仅作演示)
    for i in 0..tr_values.len() {
        if i >= period {
            let window = &tr_values[i + 1 - period..=i];
            let sum: f64 = window.iter().sum();
            atr_values[i] = sum / period as f64;
        } else {
            // 前期ATR无效
            atr_values[i] = 0.0;
        }
    }

    atr_values
}



pub fn detect_support_resistance_with_bos_choch(
    candles: &[Candle],
    lookback: usize,
    atr_period: usize,
    merge_ratio: f64, // 用于去噪合并
) -> Vec<SupportResistance> {
    let atr_values = calc_atr(candles, atr_period);
    let mut sr_levels = Vec::new();

    // 简易趋势状态
    let mut current_trend = TrendState::Unknown;

    for i in lookback..(candles.len() - lookback) {
        let current_high = candles[i].high;
        let current_low  = candles[i].low;

        let mut is_pivot_high = true;
        let mut is_pivot_low  = true;

        for j in (i - lookback)..=i+lookback {
            if candles[j].high > current_high {
                is_pivot_high = false;
            }
            if candles[j].low < current_low {
                is_pivot_low = false;
            }
        }

        if is_pivot_high {
            let mut sr = SupportResistance {
                index: i,
                ts: candles[i].ts,
                price: current_high,
                level_type: LevelType::Resistance,
                breakout: None,
            };

            // -- 去噪合并 --
            let this_atr = atr_values[i].max(0.000001); // 防止0
            let merged = merge_close_pivots(&mut sr_levels, &sr, this_atr, merge_ratio);
            if !merged {
                // 如果没合并，则插入
                sr_levels.push(sr);
            }
        }

        if is_pivot_low {
            let mut sr = SupportResistance {
                index: i,
                ts: candles[i].ts,
                price: current_low,
                level_type: LevelType::Support,
                breakout: None,
            };

            // -- 去噪合并 --
            let this_atr = atr_values[i].max(0.000001);
            let merged = merge_close_pivots(&mut sr_levels, &sr, this_atr, merge_ratio);
            if !merged {
                sr_levels.push(sr);
            }
        }

        // 下面检测价格是否突破前高/前低
        // ===================================
        // 这里演示一个非常简单的“BOS/CHoCH”判断逻辑
        // （真实策略可能需要更多条件，如确认收盘、或等待下一根K线确认等）
        if i > 0 {
            // 如果当前 close > 前一个 pivot high => 可能出现多头突破(BOS或CHoCH)
            // 我们先找到最近一个阻力 pivot high
            let last_res = sr_levels
                .iter_mut()
                .rev()
                .find(|x| x.level_type == LevelType::Resistance);

            if let Some(resistance) = last_res {
                if candles[i].close > resistance.price {
                    // 如果前面是下降趋势，则这次算 CHoCH，否则是 BOS
                    let bo_type = if current_trend == TrendState::Down {
                        BreakoutType::CHoCH
                    } else {
                        BreakoutType::BOS
                    };
                    resistance.breakout = Some(bo_type);
                    // 更新趋势
                    current_trend = TrendState::Up;
                }
            }

            // 如果当前 close < 最近一个支撑 pivot => 可能出现空头突破
            let last_sup = sr_levels
                .iter_mut()
                .rev()
                .find(|x| x.level_type == LevelType::Support);

            if let Some(support) = last_sup {
                if candles[i].close < support.price {
                    let bo_type = if current_trend == TrendState::Up {
                        BreakoutType::CHoCH
                    } else {
                        BreakoutType::BOS
                    };
                    support.breakout = Some(bo_type);
                    current_trend = TrendState::Down;
                }
            }
        }
    }

    sr_levels
}
