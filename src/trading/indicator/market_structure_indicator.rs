use crate::trading::indicator::leg_detection_indicator::{
    LegDetectionIndicator, LegDetectionValue,
};
use crate::CandleItem;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 转折点结构
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PivotPoint {
    pub price: f64,      // 价格水平
    pub last_price: f64, // 上一个价格水平
    pub time: i64,       // 时间戳
    pub index: usize,    // 索引位置
    pub crossed: bool,   // 是否被穿越
}

/// 市场结构信号值
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketStructureValue {
    pub swing_trend: i32,                  // 摆动趋势 (1=多头, -1=空头, 0=无趋势)
    pub internal_trend: i32,               // 内部趋势 (1=多头, -1=空头, 0=无趋势)
    pub swing_high: Option<PivotPoint>,    // 摆动高点
    pub swing_low: Option<PivotPoint>,     // 摆动低点
    pub internal_high: Option<PivotPoint>, // 内部高点
    pub internal_low: Option<PivotPoint>,  // 内部低点
    pub swing_bullish_bos: bool,           // 摆动多头结构突破
    pub swing_bearish_bos: bool,           // 摆动空头结构突破
    pub swing_bullish_choch: bool,         // 摆动多头特性变化
    pub swing_bearish_choch: bool,         // 摆动空头特性变化
    pub internal_bullish_bos: bool,        // 内部多头结构突破
    pub internal_bearish_bos: bool,        // 内部空头结构突破
    pub internal_bullish_choch: bool,      // 内部多头特性变化
    pub internal_bearish_choch: bool,      // 内部空头特性变化
}

/// 市场结构识别指标
#[derive(Debug, Clone)]
pub struct MarketStructureIndicator {
    swing_length: usize,                          // 摆动结构长度
    internal_length: usize,                       // 内部结构长度
    leg_detector: LegDetectionIndicator,          // 腿部识别器
    internal_leg_detector: LegDetectionIndicator, // 内部腿部识别器
    previous_value: Option<MarketStructureValue>, // 上一次的信号值
    // 新增：内部缓冲区
    candle_buffer: VecDeque<CandleItem>, // 内部K线缓冲区
    max_buffer_size: usize,              // 缓冲区最大容量
}

impl MarketStructureIndicator {
    pub fn new(swing_length: usize, internal_length: usize) -> Self {
        // 需要保存足够的K线来计算结构
        let max_buffer_size = swing_length.max(internal_length) * 3;

        Self {
            swing_length,
            internal_length,
            leg_detector: LegDetectionIndicator::new(swing_length),
            internal_leg_detector: LegDetectionIndicator::new(internal_length),
            previous_value: None,
            candle_buffer: VecDeque::with_capacity(max_buffer_size),
            max_buffer_size,
        }
    }

    /// 批量初始化历史K线数据
    pub fn init_with_history(&mut self, history: &[CandleItem]) {
        self.candle_buffer.clear();

        // 只保留最近的数据
        let start_idx = if history.len() > self.max_buffer_size {
            history.len() - self.max_buffer_size
        } else {
            0
        };

        for candle in &history[start_idx..] {
            self.candle_buffer.push_back(candle.clone());
        }

        // 同时初始化腿部检测器
        self.leg_detector.init_with_history(history);
        self.internal_leg_detector.init_with_history(history);
    }

    /// 重置指标状态
    pub fn reset(&mut self) {
        self.candle_buffer.clear();
        self.leg_detector.reset();
        self.internal_leg_detector.reset();
        self.previous_value = None;
    }

    /// 获取当前缓冲区大小
    pub fn buffer_size(&self) -> usize {
        self.candle_buffer.len()
    }

    /// 更新摆动结构
    fn update_swing_structure(
        &self,
        leg_value: LegDetectionValue,
        structure_value: &mut MarketStructureValue,
    ) {
        if self.candle_buffer.len() < self.swing_length {
            return;
        }

        let last_index = self.candle_buffer.len() - 1;

        // 如果检测到新的多头腿
        if leg_value.is_new_leg && leg_value.is_bullish_leg {
            // 找到摆动低点
            let mut low_idx = last_index - self.swing_length;
            let mut low_price = self.candle_buffer[low_idx].l;

            for i in 1..self.swing_length {
                if low_idx + i < self.candle_buffer.len()
                    && self.candle_buffer[low_idx + i].l < low_price
                {
                    low_idx = low_idx + i;
                    low_price = self.candle_buffer[low_idx].l;
                }
            }

            // 更新摆动低点
            let last_low = structure_value.swing_low.clone().unwrap_or_default();
            structure_value.swing_low = Some(PivotPoint {
                price: low_price,
                last_price: last_low.price,
                time: self.candle_buffer[low_idx].ts,
                index: low_idx,
                crossed: false,
            });
        }

        // 如果检测到新的空头腿
        if leg_value.is_new_leg && leg_value.is_bearish_leg {
            // 找到摆动高点
            let mut high_idx = last_index - self.swing_length;
            let mut high_price = self.candle_buffer[high_idx].h;

            for i in 1..self.swing_length {
                if high_idx + i < self.candle_buffer.len()
                    && self.candle_buffer[high_idx + i].h > high_price
                {
                    high_idx = high_idx + i;
                    high_price = self.candle_buffer[high_idx + i].h;
                }
            }

            // 更新摆动高点
            let last_high = structure_value.swing_high.clone().unwrap_or_default();
            structure_value.swing_high = Some(PivotPoint {
                price: high_price,
                last_price: last_high.price,
                time: self.candle_buffer[high_idx].ts,
                index: high_idx,
                crossed: false,
            });
        }
    }

    /// 更新内部结构
    fn update_internal_structure(
        &self,
        leg_value: LegDetectionValue,
        structure_value: &mut MarketStructureValue,
    ) {
        if self.candle_buffer.len() < self.internal_length {
            return;
        }

        let last_index = self.candle_buffer.len() - 1;

        // 如果检测到新的多头腿
        if leg_value.is_new_leg && leg_value.is_bullish_leg {
            // 找到内部低点
            let mut low_idx = last_index - self.internal_length;
            let mut low_price = self.candle_buffer[low_idx].l;

            for i in 1..self.internal_length {
                if low_idx + i < self.candle_buffer.len()
                    && self.candle_buffer[low_idx + i].l < low_price
                {
                    low_idx = low_idx + i;
                    low_price = self.candle_buffer[low_idx].l;
                }
            }

            // 更新内部低点
            let last_low = structure_value.internal_low.clone().unwrap_or_default();
            structure_value.internal_low = Some(PivotPoint {
                price: low_price,
                last_price: last_low.price,
                time: self.candle_buffer[low_idx].ts,
                index: low_idx,
                crossed: false,
            });
        }

        // 如果检测到新的空头腿
        if leg_value.is_new_leg && leg_value.is_bearish_leg {
            // 找到内部高点
            let mut high_idx = last_index - self.internal_length;
            let mut high_price = self.candle_buffer[high_idx].h;

            for i in 1..self.internal_length {
                if high_idx + i < self.candle_buffer.len()
                    && self.candle_buffer[high_idx + i].h > high_price
                {
                    high_idx = high_idx + i;
                    high_price = self.candle_buffer[high_idx + i].h;
                }
            }

            // 更新内部高点
            let last_high = structure_value.internal_high.clone().unwrap_or_default();
            structure_value.internal_high = Some(PivotPoint {
                price: high_price,
                last_price: last_high.price,
                time: self.candle_buffer[high_idx].ts,
                index: high_idx,
                crossed: false,
            });
        }
    }

    /// 检查结构信号
    fn check_structure_signals(&self, structure_value: &mut MarketStructureValue) {
        if self.candle_buffer.is_empty() {
            return;
        }

        let last_close = self.candle_buffer.back().unwrap().c;

        // 重置所有信号
        structure_value.swing_bullish_bos = false;
        structure_value.swing_bearish_bos = false;
        structure_value.swing_bullish_choch = false;
        structure_value.swing_bearish_choch = false;
        structure_value.internal_bullish_bos = false;
        structure_value.internal_bearish_bos = false;
        structure_value.internal_bullish_choch = false;
        structure_value.internal_bearish_choch = false;

        // 检查摆动结构信号
        if let Some(ref mut swing_high) = structure_value.swing_high {
            if !swing_high.crossed && last_close > swing_high.price {
                // 价格突破摆动高点
                if structure_value.swing_trend == -1 {
                    // 如果之前是空头趋势，那么这是一个CHoCH
                    structure_value.swing_bullish_choch = true;
                } else {
                    // 否则是一个BOS
                    structure_value.swing_bullish_bos = true;
                }
                swing_high.crossed = true;
                structure_value.swing_trend = 1; // 更新为多头趋势
            }
        }

        if let Some(ref mut swing_low) = structure_value.swing_low {
            if !swing_low.crossed && last_close < swing_low.price {
                // 价格突破摆动低点
                if structure_value.swing_trend == 1 {
                    // 如果之前是多头趋势，那么这是一个CHoCH
                    structure_value.swing_bearish_choch = true;
                } else {
                    // 否则是一个BOS
                    structure_value.swing_bearish_bos = true;
                }
                swing_low.crossed = true;
                structure_value.swing_trend = -1; // 更新为空头趋势
            }
        }

        // 检查内部结构信号
        if let Some(ref mut internal_high) = structure_value.internal_high {
            if !internal_high.crossed && last_close > internal_high.price {
                // 价格突破内部高点
                if structure_value.internal_trend == -1 {
                    // 如果之前是空头趋势，那么这是一个CHoCH
                    structure_value.internal_bullish_choch = true;
                } else {
                    // 否则是一个BOS
                    structure_value.internal_bullish_bos = true;
                }
                internal_high.crossed = true;
                structure_value.internal_trend = 1; // 更新为多头趋势
            }
        }

        if let Some(ref mut internal_low) = structure_value.internal_low {
            if !internal_low.crossed && last_close < internal_low.price {
                // 价格突破内部低点
                if structure_value.internal_trend == 1 {
                    // 如果之前是多头趋势，那么这是一个CHoCH
                    structure_value.internal_bearish_choch = true;
                } else {
                    // 否则是一个BOS
                    structure_value.internal_bearish_bos = true;
                }
                internal_low.crossed = true;
                structure_value.internal_trend = -1; // 更新为空头趋势
            }
        }
    }

    /// 处理新的K线数据
    /// 只需要传入最新的单根K线
    pub fn next(&mut self, candle: &CandleItem) -> MarketStructureValue {
        // 添加新K线到缓冲区
        self.candle_buffer.push_back(candle.clone());

        // 维护缓冲区大小
        while self.candle_buffer.len() > self.max_buffer_size {
            self.candle_buffer.pop_front();
        }

        let mut structure_value = match &self.previous_value {
            Some(prev) => prev.clone(),
            None => MarketStructureValue::default(),
        };

        // 获取腿部信号（使用新的API）
        let leg_value = self.leg_detector.next(candle);
        let internal_leg_value = self.internal_leg_detector.next(candle);

        // 更新结构
        self.update_swing_structure(leg_value, &mut structure_value);
        self.update_internal_structure(internal_leg_value, &mut structure_value);

        // 检查信号
        self.check_structure_signals(&mut structure_value);

        // 保存当前值
        self.previous_value = Some(structure_value.clone());

        structure_value
    }

    /// 兼容旧API的方法：一次性处理整个K线数组
    pub fn process_all(&mut self, data_items: &[CandleItem]) -> MarketStructureValue {
        self.reset();
        self.init_with_history(data_items);

        if let Some(last_candle) = data_items.last() {
            self.next(last_candle)
        } else {
            MarketStructureValue::default()
        }
    }

    /// 获取当前市场结构值（用于测试）
    pub fn current_value(&self) -> MarketStructureValue {
        self.previous_value.clone().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_structure_basic() {
        let mut indicator = MarketStructureIndicator::new(10, 5);

        // 创建测试数据 - 明确的市场结构模式
        let mut candles = Vec::new();

        // 第一阶段：建立初始上升趋势
        for i in 0..15 {
            candles.push(CandleItem {
                ts: i as i64,
                o: 100.0 + i as f64 * 1.5,
                h: 105.0 + i as f64 * 1.5,
                l: 95.0 + i as f64 * 1.5,
                c: 102.0 + i as f64 * 1.5,
                v: 1000.0,
                confirm: 0,
            });
        }

        // 第二阶段：回调但不破结构
        for i in 0..8 {
            candles.push(CandleItem {
                ts: (i + 15) as i64,
                o: 123.0 - i as f64 * 0.5,
                h: 127.0 - i as f64 * 0.5,
                l: 118.0 - i as f64 * 0.5,
                c: 121.0 - i as f64 * 0.5,
                v: 1000.0,
                confirm: 0,
            });
        }

        // 第三阶段：强势突破创新高（BOS）
        for i in 0..10 {
            candles.push(CandleItem {
                ts: (i + 23) as i64,
                o: 117.0 + i as f64 * 2.0,
                h: 122.0 + i as f64 * 2.0,
                l: 112.0 + i as f64 * 2.0,
                c: 119.0 + i as f64 * 2.0,
                v: 1000.0,
                confirm: 0,
            });
        }

        // 第四阶段：大幅下跌破结构（CHoCH）
        for i in 0..15 {
            candles.push(CandleItem {
                ts: (i + 33) as i64,
                o: 137.0 - i as f64 * 3.0,
                h: 140.0 - i as f64 * 3.0,
                l: 132.0 - i as f64 * 3.0,
                c: 135.0 - i as f64 * 3.0,
                v: 1000.0,
                confirm: 0,
            });
        }

        // 初始化历史数据
        if candles.len() >= 15 {
            indicator.init_with_history(&candles[..15]);
        }

        // 逐步测试市场结构变化
        println!("=== 市场结构测试 ===");
        for i in 15..candles.len() {
            if i % 5 == 0 {
                // 每5根K线测试一次
                let value = indicator.next(&candles[i]);

                println!(
                    "K线 {}: 摆动趋势={}, 内部趋势={}",
                    i, value.swing_trend, value.internal_trend
                );

                if value.swing_bullish_bos {
                    println!("  📈 摆动多头BOS");
                }
                if value.swing_bullish_choch {
                    println!("  🔄 摆动多头CHoCH");
                }
                if value.swing_bearish_bos {
                    println!("  📉 摆动空头BOS");
                }
                if value.swing_bearish_choch {
                    println!("  🔄 摆动空头CHoCH");
                }

                if value.internal_bullish_bos
                    || value.internal_bearish_bos
                    || value.internal_bullish_choch
                    || value.internal_bearish_choch
                {
                    println!("  🔍 内部结构信号触发");
                }
            } else {
                // 处理其他K线但不打印
                indicator.next(&candles[i]);
            }
        }

        // 最终测试
        let final_value = if !candles.is_empty() {
            indicator.current_value()
        } else {
            MarketStructureValue::default()
        };

        println!("\n最终市场结构信号值:");
        println!("  摆动趋势: {}", final_value.swing_trend);
        println!("  内部趋势: {}", final_value.internal_trend);

        // 验证最终应该是空头趋势（因为最后是大幅下跌）
        assert_eq!(final_value.swing_trend, -1);
        println!("✅ 最终确认为空头趋势");
    }

    #[test]
    fn test_structure_debug() {
        let mut indicator = MarketStructureIndicator::new(5, 3);

        // 创建简单的结构突破模式
        let mut candles = Vec::new();

        // 建立初始高点
        for i in 0..8 {
            let price = 100.0 + (i as f64 * 2.0);
            candles.push(CandleItem {
                ts: i as i64,
                o: price,
                h: price + 3.0,
                l: price - 3.0,
                c: price + 1.0,
                v: 1000.0,
                confirm: 0,
            });
        }

        // 回调
        for i in 0..5 {
            let price = 114.0 - (i as f64 * 1.5);
            candles.push(CandleItem {
                ts: (i + 8) as i64,
                o: price,
                h: price + 2.0,
                l: price - 2.0,
                c: price - 0.5,
                v: 1000.0,
                confirm: 0,
            });
        }

        // 突破前高（应该触发BOS）
        candles.push(CandleItem {
            ts: 13,
            o: 108.0,
            h: 125.0, // 突破前高
            l: 106.0,
            c: 120.0,
            v: 1000.0,
            confirm: 0,
        });

        println!("=== 市场结构调试测试 ===");

        // 初始化并逐步处理每根K线
        indicator.reset();
        for (i, candle) in candles.iter().enumerate() {
            let value = indicator.next(candle);

            // 打印详细信息
            println!(
                "K线 {}: O={:.1}, H={:.1}, L={:.1}, C={:.1}",
                i, candle.o, candle.h, candle.l, candle.c
            );

            // 打印腿部检测信息
            let leg_value = indicator.leg_detector.next(candle);
            println!(
                "  腿部: current={}, new={}, bullish={}, bearish={}",
                leg_value.current_leg,
                leg_value.is_new_leg,
                leg_value.is_bullish_leg,
                leg_value.is_bearish_leg
            );

            // 打印摆动高低点
            if let Some(ref swing_high) = value.swing_high {
                println!(
                    "  摆动高点: price={:.1}, crossed={}, index={}",
                    swing_high.price, swing_high.crossed, swing_high.index
                );
            }

            if let Some(ref swing_low) = value.swing_low {
                println!(
                    "  摆动低点: price={:.1}, crossed={}, index={}",
                    swing_low.price, swing_low.crossed, swing_low.index
                );
            }

            // 打印趋势和信号
            println!(
                "  趋势: swing={}, internal={}",
                value.swing_trend, value.internal_trend
            );

            if value.swing_bullish_bos {
                println!("  📈 摆动多头BOS");
            }
            if value.swing_bullish_choch {
                println!("  🔄 摆动多头CHoCH");
            }
            if value.swing_bearish_bos {
                println!("  📉 摆动空头BOS");
            }
            if value.swing_bearish_choch {
                println!("  🔄 摆动空头CHoCH");
            }

            println!();
        }

        // 分析最终状态
        let final_value = indicator.current_value();
        println!("最终状态:");
        if let Some(ref swing_high) = final_value.swing_high {
            println!("  最后的摆动高点: {:.1}", swing_high.price);
        }
        println!("  最后的收盘价: {:.1}", candles.last().unwrap().c);
        println!(
            "  检测到突破: {}",
            final_value.swing_bullish_bos || final_value.swing_bullish_choch
        );
    }
}
