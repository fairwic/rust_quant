use super::atr::ATR;
use crate::app_config::db::init_db;
use crate::app_config::log::setup_logging;
use crate::dotenv;
use crate::trading::indicator::vegas_indicator::VegasStrategy;
use crate::trading::model::market::candles::{SelectTime, TimeDirect};
use crate::trading::strategy::strategy_common;
use crate::{trading, CandleItem};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
// use crate::setup_logging;
// use crate::init_db;
// use crate::SelectTime;
// use crate::TimeDirect;
// use crate::trading;

/// 等高/等低点数据
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EqualHighLowData {
    pub price: f64,       // 价格水平
    pub time: i64,        // 时间戳
    pub is_high: bool,    // 是否是高点
    pub is_low: bool,     // 是否是低点
    pub mitigation: bool, // 是否已被缓解
}

/// 等高/等低点元组(a的高点=b的高点)或(a的低点=b的低点)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EqualHighLowTuple(pub EqualHighLowData, pub EqualHighLowData);

impl EqualHighLowTuple {
    /// 获取第一个点
    pub fn first(&self) -> &EqualHighLowData {
        &self.0
    }

    /// 获取第二个点
    pub fn second(&self) -> &EqualHighLowData {
        &self.1
    }

    /// 获取价格差异（绝对值）
    pub fn price_difference(&self) -> f64 {
        (self.0.price - self.1.price).abs()
    }

    /// 获取时间间隔（毫秒）
    pub fn time_interval(&self) -> i64 {
        (self.1.time - self.0.time).abs()
    }

    /// 获取平均价格
    pub fn average_price(&self) -> f64 {
        (self.0.price + self.1.price) / 2.0
    }

    /// 检查是否为等高点元组
    pub fn is_equal_high(&self) -> bool {
        self.0.is_high && self.1.is_high
    }

    /// 检查是否为等低点元组
    pub fn is_equal_low(&self) -> bool {
        self.0.is_low && self.1.is_low
    }

    /// 检查是否有任何一个点被缓解
    pub fn is_mitigated(&self) -> bool {
        self.0.mitigation || self.1.mitigation
    }

    /// 检查是否两个点都被缓解
    pub fn is_fully_mitigated(&self) -> bool {
        self.0.mitigation && self.1.mitigation
    }
}

/// 等高/等低点信号值
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EqualHighLowValue {
    pub equal_highs: Vec<EqualHighLowData>,   // 等高点
    pub equal_lows: Vec<EqualHighLowData>,    // 等低点
    pub equal_tuples: Vec<EqualHighLowTuple>, // 等高/等低点元组
    pub current_equal_high: bool,             // 当前是否形成等高点
    pub current_equal_low: bool,              // 当前是否形成等低点
    pub active_pivot_lows: Vec<EqualHighLowData>,
}

/// 摆动点数据
#[derive(Debug, Clone, Copy, Default)]
struct PivotPoint {
    current_level: f64,
    last_level: f64,
    crossed: bool,
    bar_time: i64,
    bar_index: usize,
}

/// 等高/等低点识别指标（完全按照Pine Script逻辑）
#[derive(Debug, Clone)]
pub struct EqualHighLowIndicator {
    length: usize,                       // 确认K线数量（equalHighsLowsLengthInput）
    threshold: f64,                      // 阈值（equalHighsLowsThresholdInput）
    equal_high: PivotPoint,              // 等高点摆动点
    equal_low: PivotPoint,               // 等低点摆动点
    prev_leg: Option<i32>,               // 前一个腿部状态
    equal_points: Vec<EqualHighLowData>, // 等高/等低点历史
    atr_measure: ATR,                    // ATR值历史
    // 新增：内部缓冲区，存储最近的K线数据
    candle_buffer: VecDeque<CandleItem>, // 内部K线缓冲区
    max_buffer_size: usize,              // 缓冲区最大容量
    active_pivot_lows: Vec<EqualHighLowData>,
    active_pivot_highs: Vec<EqualHighLowData>,
}

impl EqualHighLowIndicator {
    pub fn new(length: usize, threshold: f64) -> Self {
        // 计算所需的最小缓冲区大小
        // 需要至少保存 length + 1 根K线来检测摆动点
        let max_buffer_size = length * 50; // 或者直接不限制，使用完整历史数据

        Self {
            length,
            threshold,
            equal_high: PivotPoint {
                current_level: 0.0,
                last_level: 0.0,
                crossed: false,
                bar_time: 0,
                bar_index: 0,
            },
            equal_low: PivotPoint {
                current_level: 0.0,
                last_level: 0.0,
                crossed: false,
                bar_time: 0,
                bar_index: 0,
            },
            prev_leg: None,
            equal_points: Vec::new(),
            atr_measure: ATR::new(200).unwrap(),
            candle_buffer: VecDeque::with_capacity(max_buffer_size),
            max_buffer_size,
            active_pivot_lows: Vec::new(),
            active_pivot_highs: Vec::new(),
        }
    }

    /// 计算腿部（完全按照Pine Script逻辑）
    fn calculate_leg(&mut self, data_items: &[CandleItem], size: usize) -> i32 {
        if data_items.len() <= size {
            return self.prev_leg.unwrap_or(0);
        }

        let last_index = data_items.len() - 1;
        let target_idx = last_index - size;

        // PineScript腿部检测核心逻辑:
        // newLegHigh = high[length] > ta.highest(high, length)
        // newLegLow = low[length] < ta.lowest(low, length)

        // 计算从当前位置向前size根K线(不包括target_idx)的最高价和最低价
        // 注意: 这对应PineScript中的ta.highest/ta.lowest函数，范围是[last_index-size+1, last_index]
        let mut highest_in_range = f64::MIN;
        let mut lowest_in_range = f64::MAX;

        for i in 0..size {
            // 从last_index往回数，不包括target_idx
            let idx = last_index - i;
            if idx > target_idx && idx < data_items.len() {
                highest_in_range = highest_in_range.max(data_items[idx].h);
                lowest_in_range = lowest_in_range.min(data_items[idx].l);
            }
        }

        // 在PineScript中:
        // high[length]是指从当前位置向前数第length根K线的高点
        // low[length]是指从当前位置向前数第length根K线的低点
        let target_high = data_items[target_idx].h;
        let target_low = data_items[target_idx].l;

        // 调试输出
        println!(
            "腿部计算: 目标K线idx={}, 高={:.2}, 低={:.2}, 区间最高={:.2}, 区间最低={:.2}",
            target_idx, target_high, target_low, highest_in_range, lowest_in_range
        );

        // PineScript逻辑:
        // 当target_idx的高点 > 最近size根K线的最高点时，形成空头腿
        // 当target_idx的低点 < 最近size根K线的最低点时，形成多头腿
        let new_leg_high = target_high > highest_in_range;
        let new_leg_low = target_low < lowest_in_range;

        let current_leg = if new_leg_high {
            println!(
                "形成空头腿: target高点{:.2} > 区间最高{:.2}",
                target_high, highest_in_range
            );
            0 // BEARISH_LEG
        } else if new_leg_low {
            println!(
                "形成多头腿: target低点{:.2} < 区间最低{:.2}",
                target_low, lowest_in_range
            );
            1 // BULLISH_LEG
        } else {
            self.prev_leg.unwrap_or(0)
        };

        current_leg
    }

    /// 检测摆动点变化（Pine Script逻辑）
    fn detect_pivot_change(&mut self, data_items: &[CandleItem]) -> (bool, bool) {
        let current_leg = self.calculate_leg(data_items, self.length);
        let leg_changed = self.prev_leg.map_or(false, |prev| prev != current_leg);

        let pivot_low = leg_changed && current_leg == 1; // 开始多头腿 = 形成低点
        let pivot_high = leg_changed && current_leg == 0; // 开始空头腿 = 形成高点

        if leg_changed {
            println!(
                "腿部变化: 从{:?}到{}, pivot_high={}, pivot_low={}",
                self.prev_leg, current_leg, pivot_high, pivot_low
            );
        }

        self.prev_leg = Some(current_leg);

        (pivot_high, pivot_low)
    }

    /// 处理新的K线数据
    /// 只需要传入最新的单根K线，内部会自动维护历史数据
    pub fn next(&mut self, candle: &CandleItem) -> EqualHighLowValue {
        // 添加新K线到缓冲区
        self.candle_buffer.push_back(candle.clone());

        let atr_measure = self.atr_measure.next(candle.h, candle.l, candle.c);
        println!("atr_measure: {}", atr_measure);
        // 维护缓冲区大小
        while self.candle_buffer.len() > self.max_buffer_size {
            self.candle_buffer.pop_front();
        }
        // 将缓冲区转换为切片来使用现有逻辑
        let data_items: Vec<CandleItem> = self.candle_buffer.iter().cloned().collect();
        // 调用内部方法处理数据
        self.process_data(&data_items, atr_measure)
    }

    /// 处理K线数据的核心逻辑
    fn process_data(&mut self, data_items: &[CandleItem], atr_measure: f64) -> EqualHighLowValue {
        let mut result = EqualHighLowValue::default();

        if data_items.len() < self.length + 1 {
            return result;
        }

        // 如果ATR还没有有效值，使用简单的平均True Range作为备用
        let effective_atr = if atr_measure > 0.0 {
            atr_measure
        } else {
            // 当ATR为0时，使用价格的一定百分比作为备用
            // 计算最近价格的平均值
            let recent_prices: Vec<f64> = data_items
                .iter()
                .rev()
                .take(10)
                .map(|c| (c.h + c.l + c.c) / 3.0)
                .collect();

            let avg_price = if !recent_prices.is_empty() {
                recent_prices.iter().sum::<f64>() / recent_prices.len() as f64
            } else {
                100.0 // 默认值
            };

            // 使用平均价格的1%作为备用ATR
            avg_price * 0.01
        };

        // 计算阈值 = 0.1 * ATR
        let threshold_value = self.threshold * effective_atr;

        // 检测摆动点变化 - 使用更新过的计算腿部函数
        let (pivot_high, pivot_low) = self.detect_pivot_change(data_items);

        // 当前K线索引
        let last_index = data_items.len() - 1;
        let pivot_index = last_index - self.length;

        // 处理新的低点摆动
        if pivot_low {
            let current_low = data_items[pivot_index].l;
            let current_time = data_items[pivot_index].ts;

            // 创建新的摆动低点
            let new_pivot_low = EqualHighLowData {
                price: current_low,
                time: current_time,
                is_high: false,
                is_low: true,
                mitigation: false,
            };

            // 与所有活跃的低点比较，尝试配对
            for &prev_low in &self.active_pivot_lows {
                let price_diff = (prev_low.price - current_low).abs();

                // 只有价格差异在阈值内才配对
                if price_diff <= threshold_value {
                    // 找到等低点
                    result.current_equal_low = true;

                    // 创建等低点数据
                    let equal_low = new_pivot_low;

                    // 创建等低点元组
                    let equal_tuple = EqualHighLowTuple(prev_low, equal_low);

                    // 添加到结果
                    self.equal_points.push(equal_low);
                    result.equal_lows.push(equal_low);
                    result.equal_tuples.push(equal_tuple);
                }
            }

            // 添加当前低点到活跃列表
            self.active_pivot_lows.push(new_pivot_low);

            // 更新当前摆动点（保持原有逻辑兼容）
            self.equal_low.last_level = self.equal_low.current_level;
            self.equal_low.current_level = current_low;
            self.equal_low.crossed = false;
            self.equal_low.bar_time = current_time;
            self.equal_low.bar_index = pivot_index;
        }

        // 处理新的高点摆动
        if pivot_high {
            let current_high = data_items[pivot_index].h;
            let current_time = data_items[pivot_index].ts;

            // 如果前一个高点存在，检查是否是等高点
            if self.equal_high.current_level != 0.0 {
                let price_diff = (self.equal_high.current_level - current_high).abs();

                // 使用PineScript相同的逻辑比较价格差异
                if price_diff <= threshold_value {
                    println!(
                        "✅ 找到等高点! 差异={:.2}, 阈值={:.2}",
                        price_diff, threshold_value
                    );
                    // 找到等高点
                    result.current_equal_high = true;

                    let equal_high = EqualHighLowData {
                        price: current_high,
                        time: current_time,
                        is_high: true,
                        is_low: false,
                        mitigation: false,
                    };

                    // 创建等高点元组（前一个高点和当前高点）
                    let prev_high = EqualHighLowData {
                        price: self.equal_high.current_level,
                        time: self.equal_high.bar_time,
                        is_high: true,
                        is_low: false,
                        mitigation: false,
                    };
                    let equal_tuple = EqualHighLowTuple(prev_high, equal_high);

                    self.equal_points.push(equal_high);
                    result.equal_highs.push(equal_high);
                    result.equal_tuples.push(equal_tuple);
                }
            }

            // 更新等高点摆动点
            self.equal_high.last_level = self.equal_high.current_level;
            self.equal_high.current_level = current_high;
            self.equal_high.crossed = false;
            self.equal_high.bar_time = current_time;
            self.equal_high.bar_index = pivot_index;
        }

        // 检查等高/等低点的缓解状态
        let last_candle = data_items.last().unwrap();

        for i in 0..self.equal_points.len() {
            let mut point = self.equal_points[i];

            if !point.mitigation {
                if point.is_high && last_candle.h > point.price {
                    // 收盘价高于等高点，缓解
                    point.mitigation = true;
                } else if !point.is_high && last_candle.l < point.price {
                    // 收盘价低于等低点，缓解
                    point.mitigation = true;
                }
                self.equal_points[i] = point;
            }
        }

        // 移除已缓解的点
        self.equal_points.retain(|point| !point.mitigation);

        // 在K线结束时，清理被缓解的摆动点
        self.active_pivot_lows
            .retain(|point| !(last_candle.l < point.price)); // 低点被更低的价格缓解
        self.active_pivot_highs
            .retain(|point| !(last_candle.h > point.price)); // 高点被更高的价格缓解

        result
    }

    /// 批量初始化历史K线数据
    /// 在开始使用next()之前，可以先调用此方法来填充历史数据
    pub fn init_with_history(&mut self, history: &[CandleItem]) {
        self.candle_buffer.clear();
        // 只保留最近的数据，确保不超过缓冲区容量
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
        self.equal_high = PivotPoint {
            current_level: 0.0,
            last_level: 0.0,
            crossed: false,
            bar_time: 0,
            bar_index: 0,
        };
        self.equal_low = PivotPoint {
            current_level: 0.0,
            last_level: 0.0,
            crossed: false,
            bar_time: 0,
            bar_index: 0,
        };
        self.prev_leg = None;
        self.equal_points.clear();
        self.active_pivot_lows.clear();
        self.active_pivot_highs.clear();
    }
    /// 获取当前缓冲区大小
    pub fn buffer_size(&self) -> usize {
        self.candle_buffer.len()
    }

    /// 兼容旧API的方法：一次性处理整个K线数组
    /// 推荐使用新的next()方法逐根处理K线
    pub fn process_all(&mut self, data_items: &[CandleItem]) -> EqualHighLowValue {
        // 清空缓冲区并填充新数据
        self.candle_buffer.clear();
        // 只保留需要的数据量
        let start_idx = if data_items.len() > self.max_buffer_size {
            data_items.len() - self.max_buffer_size
        } else {
            0
        };
        for candle in &data_items[start_idx..] {
            self.candle_buffer.push_back(candle.clone());
        }
        // 处理数据
        self.process_data(data_items, 0.00)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time_util;

    /// 测试等高/等低点基本功能
    #[test]
    fn test_equal_high_low_basic() {
        let mut indicator = EqualHighLowIndicator::new(3, 1.0); // 使用3根K线确认，1.0阈值

        // 创建测试数据 - 模拟真实的摆动点模式
        let mut candles = Vec::new();

        // 第一阶段：上升趋势形成高点
        for i in 0..10 {
            candles.push(CandleItem {
                ts: i as i64,
                o: 100.0 + i as f64 * 2.0,
                h: 105.0 + i as f64 * 2.0,
                l: 95.0 + i as f64 * 2.0,
                c: 102.0 + i as f64 * 2.0,
                v: 1000.0,
            });
        }

        // 第二阶段：下降形成低点
        for i in 0..8 {
            candles.push(CandleItem {
                ts: (i + 10) as i64,
                o: 120.0 - i as f64 * 3.0,
                h: 125.0 - i as f64 * 3.0,
                l: 115.0 - i as f64 * 3.0,
                c: 118.0 - i as f64 * 3.0,
                v: 1000.0,
            });
        }

        // 第三阶段：再次上升形成相似高点
        for i in 0..10 {
            candles.push(CandleItem {
                ts: (i + 18) as i64,
                o: 96.0 + i as f64 * 2.5,
                h: 101.0 + i as f64 * 2.5, // 接近之前的高点
                l: 91.0 + i as f64 * 2.5,
                c: 98.0 + i as f64 * 2.5,
                v: 1000.0,
            });
        }

        // 初始化历史数据
        indicator.init_with_history(&candles[..5]);

        // 逐根K线处理
        let mut value = EqualHighLowValue::default();
        for candle in &candles[5..] {
            value = indicator.next(candle);
        }

        // 输出测试结果
        println!("等高/等低点信号值: {:?}", value);
        println!("检测到的等高点数量: {}", value.equal_highs.len());
        println!("检测到的等低点数量: {}", value.equal_lows.len());

        // 验证是否检测到等高点或等低点
        if value.current_equal_high || value.current_equal_low {
            println!("✅ 成功检测到等高/等低点");
        } else {
            println!("❌ 未检测到等高/等低点");
        }
    }

    #[tokio::test]
    async fn test_equal_high_low_real_data() -> anyhow::Result<()> {
        dotenv().ok();
        setup_logging().await?;
        init_db().await;

        // 修改时间戳匹配2025-05-19附近
        let select_time: SelectTime = SelectTime {
            point_time: 1747894800000, // 2025-05-19
            direct: TimeDirect::BEFORE,
        };

        println!("\n===== 等高/等低点真实数据测试 =====");
        println!("目标时间戳: {}", select_time.point_time);

        let candles =
            trading::task::basic::get_candle_data("BTC-USDT-SWAP", "1H", 600, Some(select_time))
                .await?;

        println!("获取了 {} 根K线数据", candles.len());

        // 使用Pine Script中相同的参数：3根K线确认，0.1阈值
        let mut indicator = EqualHighLowIndicator::new(3, 0.1);

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

        println!("总共处理 {} 根K线", candle_items.len());

        // 初始化前4根K线作为历史数据
        if candle_items.len() >= 4 {
            indicator.init_with_history(&candle_items[..4]);
        }

        // 从第5根K线开始逐根处理，重点关注目标时间戳附近
        let mut equal_lows = Vec::new();
        let mut equal_highs = Vec::new();
        let mut equal_tuples = Vec::new();

        // 记录所有关键K线的时间戳和价格，用于后续分析
        let mut key_timestamps = Vec::new();

        println!("\n----- 开始处理K线 -----");
        println!("索引\t时间\t\t价格\t\t高/低点");

        for i in 4..candle_items.len() {
            let current_candle = &candle_items[i];

            // 转换时间戳为可读格式
            let time_str = time_util::mill_time_to_datetime_shanghai(current_candle.ts).unwrap();

            // 处理每根K线
            let value = indicator.next(current_candle);

            // 重点关注图表中标记的EQL点附近
            let is_target_area =
                current_candle.ts >= 1747486800000 && current_candle.ts <= 1747894800000;

            // 记录等高/等低点或目标区域内的K线
            if value.current_equal_high || value.current_equal_low || is_target_area {
                key_timestamps.push((i, current_candle.ts, current_candle.c));

                if value.current_equal_high || value.current_equal_low {
                    println!(
                        "{}\t{}\t{:.1}\t{}",
                        i,
                        time_str,
                        current_candle.c,
                        if value.current_equal_high {
                            "等高点"
                        } else {
                            "等低点"
                        }
                    );

                    if value.current_equal_high {
                        equal_highs.extend(value.equal_highs.clone());
                    }
                    if value.current_equal_low {
                        equal_lows.extend(value.equal_lows.clone());
                    }
                    equal_tuples.extend(value.equal_tuples.clone());
                }
            }
        }

        // 输出结果摘要
        println!("\n===== 最终结果 =====");
        println!("检测到的等高点数量: {}", equal_highs.len());
        println!("检测到的等低点数量: {}", equal_lows.len());
        println!("检测到的等高/等低点元组数量: {}", equal_tuples.len());

        // 打印目标区域内的所有K线
        println!("\n目标区域内的K线 (5月18-19日):");
        for (i, ts, price) in key_timestamps
            .iter()
            .filter(|(_, ts, _)| *ts >= 1747486800000 && *ts <= 1747894800000)
        {
            let time_str = time_util::mill_time_to_datetime_shanghai(*ts).unwrap();

            println!("K线 {}: 时间={}, 价格={:.1}", i, time_str, price);
        }

        // 展示等高/等低点元组详情
        if !equal_tuples.is_empty() {
            println!("\n===== 等高/等低点元组详情 =====");
            for (i, tuple) in equal_tuples.iter().enumerate() {
                println!("元组 {}: ", i + 1);
                println!(
                    "  类型: {}",
                    if tuple.is_equal_high() {
                        "等高点"
                    } else {
                        "等低点"
                    }
                );
                // 转换时间戳为可读格式
                let first_time =
                    time_util::mill_time_to_datetime_shanghai(tuple.first().time).unwrap();

                let second_time =
                    time_util::mill_time_to_datetime_shanghai(tuple.second().time).unwrap();

                println!(
                    "  第一个点: 价格={:.2}, 时间={}",
                    tuple.first().price,
                    first_time
                );
                println!(
                    "  第二个点: 价格={:.2}, 时间={}",
                    tuple.second().price,
                    second_time
                );
                println!("  价格差异: {:.4}", tuple.price_difference());
                println!("  时间间隔: {}小时", tuple.time_interval() / 3600000); // 转换为小时
                println!("  平均价格: {:.2}", tuple.average_price());
                println!("  是否被缓解: {}", tuple.is_mitigated());
                println!();
            }
        }

        Ok(())
    }

    /// 测试完整的等高/等低点检测
    #[test]
    fn test_equal_high_low_detection() {
        let mut indicator = EqualHighLowIndicator::new(3, 0.5); // 使用较大阈值

        // 创建两个相似的低点
        let candles = vec![
            // 第一个低点形成
            CandleItem {
                ts: 0,
                o: 100.0,
                h: 105.0,
                l: 100.0,
                c: 102.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 1,
                o: 102.0,
                h: 103.0,
                l: 95.0,
                c: 96.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 2,
                o: 96.0,
                h: 97.0,
                l: 90.0,
                c: 91.0,
                v: 1000.0,
            }, // 低点1
            CandleItem {
                ts: 3,
                o: 91.0,
                h: 95.0,
                l: 91.0,
                c: 94.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 4,
                o: 94.0,
                h: 98.0,
                l: 93.0,
                c: 97.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 5,
                o: 97.0,
                h: 102.0,
                l: 96.0,
                c: 100.0,
                v: 1000.0,
            },
            // 第二个低点形成
            CandleItem {
                ts: 6,
                o: 100.0,
                h: 101.0,
                l: 95.0,
                c: 96.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 7,
                o: 96.0,
                h: 97.0,
                l: 90.2,
                c: 91.0,
                v: 1000.0,
            }, // 低点2，非常接近低点1（90.2 vs 90.0）
            CandleItem {
                ts: 8,
                o: 91.0,
                h: 95.0,
                l: 91.0,
                c: 94.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 9,
                o: 94.0,
                h: 98.0,
                l: 93.0,
                c: 97.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 10,
                o: 97.0,
                h: 100.0,
                l: 96.0,
                c: 99.0,
                v: 1000.0,
            },
        ];

        let mut equal_low_detected = false;

        // 逐根处理K线
        for (i, candle) in candles.iter().enumerate() {
            println!(
                "\n处理K线 {}: high={}, low={}, close={}",
                i, candle.h, candle.l, candle.c
            );
            let value = indicator.next(candle);

            if value.current_equal_low {
                equal_low_detected = true;
                println!("✅ 检测到等低点!");
                println!("等低点数量: {}", value.equal_lows.len());
                for (j, eq_low) in value.equal_lows.iter().enumerate() {
                    println!(
                        "  等低点 {}: 价格={:.2}, 时间={}",
                        j + 1,
                        eq_low.price,
                        eq_low.time
                    );
                }
            }
        }

        assert!(equal_low_detected, "应该检测到等低点");
    }

    /// 简单测试摆动点检测
    #[test]
    fn test_pivot_detection() {
        let mut indicator = EqualHighLowIndicator::new(3, 0.1);

        // 创建一个明显的V型底部模式
        let candles = vec![
            CandleItem {
                ts: 0,
                o: 100.0,
                h: 105.0,
                l: 95.0,
                c: 100.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 1,
                o: 100.0,
                h: 103.0,
                l: 93.0,
                c: 95.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 2,
                o: 95.0,
                h: 97.0,
                l: 90.0,
                c: 92.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 3,
                o: 92.0,
                h: 94.0,
                l: 88.0,
                c: 90.0,
                v: 1000.0,
            }, // 最低点
            CandleItem {
                ts: 4,
                o: 90.0,
                h: 95.0,
                l: 89.0,
                c: 94.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 5,
                o: 94.0,
                h: 98.0,
                l: 93.0,
                c: 97.0,
                v: 1000.0,
            },
            CandleItem {
                ts: 6,
                o: 97.0,
                h: 102.0,
                l: 96.0,
                c: 100.0,
                v: 1000.0,
            },
        ];

        // 逐根处理K线
        for (i, candle) in candles.iter().enumerate() {
            println!(
                "\n处理K线 {}: high={}, low={}, close={}",
                i, candle.h, candle.l, candle.c
            );
            let value = indicator.next(candle);

            if value.current_equal_high || value.current_equal_low {
                println!("检测到等高/等低点!");
            }
        }
    }
}
