use crate::CandleItem;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 腿部识别系统指标
/// 基于价格高低点识别市场上升/下降腿部
#[derive(Debug, Clone)]
pub struct LegDetectionIndicator {
    size: usize,           // 用于识别腿部的bar数量
    prev_leg: Option<i32>, // 前一个腿部值
    // 新增：内部缓冲区
    candle_buffer: VecDeque<CandleItem>, // 内部K线缓冲区
    max_buffer_size: usize,              // 缓冲区最大容量
}

/// 腿部信号值
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub struct LegDetectionValue {
    pub current_leg: i32,     // 当前腿部 (0=空头腿, 1=多头腿)
    pub is_new_leg: bool,     // 是否是新腿部开始
    pub is_bullish_leg: bool, // 是否是多头腿部
    pub is_bearish_leg: bool, // 是否是空头腿部
}

impl LegDetectionIndicator {
    pub fn new(size: usize) -> Self {
        // 需要保存至少 size + 1 根K线来检测腿部
        let max_buffer_size = (size + 1) * 2;

        Self {
            size,
            prev_leg: None,
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
    }

    /// 重置指标状态
    pub fn reset(&mut self) {
        self.candle_buffer.clear();
        self.prev_leg = None;
    }

    /// 获取当前缓冲区大小
    pub fn buffer_size(&self) -> usize {
        self.candle_buffer.len()
    }

    /// 计算当前腿部
    /// 0 = 空头腿 (BEARISH_LEG)
    /// 1 = 多头腿 (BULLISH_LEG)
    fn calculate_leg(&self) -> i32 {
        if self.candle_buffer.len() <= self.size {
            return self.prev_leg.unwrap_or(0);
        }

        let last_index = self.candle_buffer.len() - 1;
        let target_idx = last_index - self.size;

        // 计算最近size根K线的最高价和最低价（不包括target_idx这根K线）
        let mut highest_in_range = f64::MIN;
        let mut lowest_in_range = f64::MAX;

        for i in 0..self.size {
            let idx = last_index - i;
            if idx < self.candle_buffer.len() {
                let candle = &self.candle_buffer[idx];
                highest_in_range = highest_in_range.max(candle.h);
                lowest_in_range = lowest_in_range.min(candle.l);
            }
        }

        // Pine Script逻辑：
        // newLegHigh = high[size] > ta.highest(size) -> BEARISH_LEG (0)
        // newLegLow = low[size] < ta.lowest(size) -> BULLISH_LEG (1)
        let target_candle = &self.candle_buffer[target_idx];
        let new_leg_high = target_candle.h > highest_in_range;
        let new_leg_low = target_candle.l < lowest_in_range;

        if new_leg_high {
            0 // BEARISH_LEG - 突破高点后开始空头腿
        } else if new_leg_low {
            1 // BULLISH_LEG - 突破低点后开始多头腿
        } else {
            // 如果没有明确的腿部变化，维持之前的状态
            self.prev_leg.unwrap_or(0)
        }
    }

    /// 处理新的K线数据
    /// 只需要传入最新的单根K线
    pub fn next(&mut self, candle: &CandleItem) -> LegDetectionValue {
        // 添加新K线到缓冲区
        self.candle_buffer.push_back(candle.clone());

        // 维护缓冲区大小
        while self.candle_buffer.len() > self.max_buffer_size {
            self.candle_buffer.pop_front();
        }

        let mut result = LegDetectionValue::default();

        // 计算当前腿部
        let current_leg = self.calculate_leg();
        result.current_leg = current_leg;

        // 判断是否是新腿部
        if let Some(prev_leg) = self.prev_leg {
            result.is_new_leg = prev_leg != current_leg;
        }

        // 更新腿部类型
        result.is_bullish_leg = current_leg == 1;
        result.is_bearish_leg = current_leg == 0;

        // 更新上一个腿部
        self.prev_leg = Some(current_leg);

        result
    }

    /// 兼容旧API的方法：一次性处理整个K线数组
    pub fn process_all(&mut self, data_items: &[CandleItem]) -> LegDetectionValue {
        self.reset();
        self.init_with_history(data_items);

        if let Some(last_candle) = data_items.last() {
            self.next(last_candle)
        } else {
            LegDetectionValue::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leg_detection_basic() {
        let mut indicator = LegDetectionIndicator::new(5);

        // 创建测试数据 - 明确的腿部转换模式
        let mut candles = Vec::new();

        // 第一阶段：上升趋势（多头腿）
        for i in 0..8 {
            candles.push(CandleItem {
                ts: i as i64,
                o: 100.0 + i as f64 * 2.0,
                h: 105.0 + i as f64 * 2.0,
                l: 95.0 + i as f64 * 2.0,
                c: 102.0 + i as f64 * 2.0,
                v: 1000.0,
            });
        }

        // 第二阶段：下降趋势（空头腿）
        for i in 0..8 {
            candles.push(CandleItem {
                ts: (i + 8) as i64,
                o: 116.0 - i as f64 * 3.0,
                h: 120.0 - i as f64 * 3.0,
                l: 110.0 - i as f64 * 3.0,
                c: 114.0 - i as f64 * 3.0,
                v: 1000.0,
            });
        }

        // 第三阶段：再次上升（多头腿）
        for i in 0..8 {
            candles.push(CandleItem {
                ts: (i + 16) as i64,
                o: 92.0 + i as f64 * 2.5,
                h: 97.0 + i as f64 * 2.5,
                l: 87.0 + i as f64 * 2.5,
                c: 94.0 + i as f64 * 2.5,
                v: 1000.0,
            });
        }

        // 初始化历史数据
        if candles.len() >= 6 {
            indicator.init_with_history(&candles[..6]);
        }

        // 逐步测试腿部检测
        for i in 6..candles.len() {
            let value = indicator.next(&candles[i]);

            println!(
                "K线 {}: 腿部={}, 新腿部={}, 多头腿={}, 空头腿={}",
                i, value.current_leg, value.is_new_leg, value.is_bullish_leg, value.is_bearish_leg
            );

            if value.is_new_leg {
                println!("  🔄 检测到腿部转换！");
            }
        }

        // 最终测试 - 处理最后一根K线
        let final_value = indicator.next(&candles.last().unwrap());
        println!("\n最终腿部信号值: {:?}", final_value);

        // 验证最后应该是多头腿（因为最后一段是上升的）
        assert_eq!(final_value.is_bullish_leg, true);
        println!("✅ 最终确认为多头腿");
    }

    #[test]
    fn test_leg_transition() {
        let mut indicator = LegDetectionIndicator::new(3);

        // 创建明确的腿部转换：高点突破 -> 空头腿
        let mut candles = Vec::new();

        // 基础K线
        for i in 0..5 {
            candles.push(CandleItem {
                ts: i as i64,
                o: 100.0,
                h: 105.0,
                l: 95.0,
                c: 102.0,
                v: 1000.0,
            });
        }

        // 突破高点的K线（应该触发空头腿）
        candles.push(CandleItem {
            ts: 5,
            o: 102.0,
            h: 120.0, // 明显突破前面的105.0高点
            l: 100.0,
            c: 115.0,
            v: 1000.0,
        });

        // 初始化历史数据
        indicator.init_with_history(&candles[..candles.len() - 1]);

        // 处理最后一根K线
        let value = indicator.next(&candles.last().unwrap());

        println!("腿部转换测试:");
        println!("  突破高点后的腿部: {}", value.current_leg);
        println!("  是否为空头腿: {}", value.is_bearish_leg);
        println!("  是否为新腿部: {}", value.is_new_leg);

        // 根据Pine Script逻辑，突破高点应该是空头腿
        assert_eq!(value.is_bearish_leg, true);
        println!("✅ 突破高点正确识别为空头腿");
    }

    #[tokio::test]
    async fn test_leg_detection_real_data() -> anyhow::Result<()> {
        use crate::app_config::db::init_db;
        use crate::app_config::log::setup_logging;
        use crate::dotenv;
        use crate::trading;
        use crate::trading::model::market::candles::{SelectTime, TimeDirect};

        dotenv().ok();
        setup_logging().await?;
        init_db().await;

        // 获取实际数据 - 使用一段有明显趋势变化的时间段
        let select_time: SelectTime = SelectTime {
            point_time: 1747494000000, // 可以根据需要调整时间戳
            direct: TimeDirect::BEFORE,
        };

        let candles =
            trading::task::basic::get_candle_data("BTC-USDT-SWAP", "1H", 300, Some(select_time))
                .await?;

        println!("总共获取 {} 根K线", candles.len());

        // 将candles转换为CandleItem
        let candle_items: Vec<CandleItem> = candles
            .iter()
            .map(|c| {
                CandleItem::builder()
                    .o(c.o.parse::<f64>().unwrap())
                    .h(c.h.parse::<f64>().unwrap())
                    .l(c.l.parse::<f64>().unwrap())
                    .c(c.c.parse::<f64>().unwrap())
                    .v(c.vol_ccy.parse::<f64>().unwrap())
                    .ts(c.ts)
                    .build()
            })
            .collect::<Result<Vec<_>, _>>()?;

        // 使用不同的size参数来测试
        let size_values = [5, 10, 15];

        for &size in &size_values {
            println!("\n===== 测试腿部检测 (size={}) =====", size);
            let mut indicator = LegDetectionIndicator::new(size);

            // 记录所有腿部转换
            let mut leg_transitions = Vec::new();
            let mut prev_leg = None;

            // 初始化前10根K线作为历史数据
            if candle_items.len() >= 10 {
                indicator.init_with_history(&candle_items[..10]);
            }

            // 从第11根K线开始逐根处理
            println!("K线索引\t价格\t\t腿部\t新腿部\t多头/空头");

            for i in 10..candle_items.len() {
                let current_candle = &candle_items[i];
                let value = indicator.next(current_candle);

                // 只打印部分K线，以免输出过多
                if i % 20 == 0 || value.is_new_leg {
                    println!(
                        "{}\t{:.1}\t\t{}\t{}\t{}",
                        i,
                        current_candle.c,
                        value.current_leg,
                        value.is_new_leg,
                        if value.is_bullish_leg {
                            "多头"
                        } else {
                            "空头"
                        }
                    );
                }

                // 记录腿部转换
                if value.is_new_leg {
                    leg_transitions.push((i, value.current_leg));
                }

                prev_leg = Some(value.current_leg);
            }

            // 打印腿部转换点
            println!("\n腿部转换点 (size={}):", size);
            for (index, leg) in &leg_transitions {
                println!(
                    "K线 {}: 转换为{}",
                    index,
                    if *leg == 1 { "多头腿" } else { "空头腿" }
                );
            }

            println!("检测到 {} 个腿部转换", leg_transitions.len());

            // 分析腿部转换的时间间隔
            if leg_transitions.len() >= 2 {
                let mut intervals = Vec::new();
                for i in 1..leg_transitions.len() {
                    let interval = leg_transitions[i].0 - leg_transitions[i - 1].0;
                    intervals.push(interval);
                }

                let avg_interval = intervals.iter().sum::<usize>() as f64 / intervals.len() as f64;
                println!("平均腿部持续时间: {:.1} 根K线", avg_interval);

                let min_interval = intervals.iter().min().unwrap();
                let max_interval = intervals.iter().max().unwrap();
                println!(
                    "最短腿部: {} 根K线, 最长腿部: {} 根K线",
                    min_interval, max_interval
                );
            }
        }

        Ok(())
    }
}
