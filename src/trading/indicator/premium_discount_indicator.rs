use crate::trading::indicator::market_structure_indicator::{
    MarketStructureIndicator, MarketStructureValue,
};
use crate::CandleItem;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 溢价/折扣区域数据
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PremiumDiscountData {
    pub top: f64,             // 顶部价格
    pub bottom: f64,          // 底部价格
    pub time: i64,            // 时间戳
    pub is_premium: bool,     // 是否是溢价区域
    pub is_discount: bool,    // 是否是折扣区域
    pub is_equilibrium: bool, // 是否是平衡区域
}

/// 溢价/折扣区域信号值
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PremiumDiscountValue {
    pub premium_zone: Option<PremiumDiscountData>, // 溢价区域
    pub discount_zone: Option<PremiumDiscountData>, // 折扣区域
    pub equilibrium_zone: Option<PremiumDiscountData>, // 平衡区域
    pub in_premium_zone: bool,                     // 当前价格是否在溢价区域
    pub in_discount_zone: bool,                    // 当前价格是否在折扣区域
    pub in_equilibrium_zone: bool,                 // 当前价格是否在平衡区域
}

/// 溢价/折扣区域指标
/// 基于市场结构的摆动高低点计算溢价/折扣区域
/// 与PineScript完全一致的实现
#[derive(Debug, Clone)]
pub struct PremiumDiscountIndicator {
    market_structure: MarketStructureIndicator, // 市场结构指标
    swing_length: usize,                        // 摆动结构长度
    internal_length: usize,                     // 内部结构长度
    premium_zone: Option<PremiumDiscountData>,  // 当前溢价区域
    discount_zone: Option<PremiumDiscountData>, // 当前折扣区域
    equilibrium_zone: Option<PremiumDiscountData>, // 当前平衡区域
    // 内部缓冲区
    candle_buffer: VecDeque<CandleItem>, // 内部K线缓冲区
    max_buffer_size: usize,              // 缓冲区最大容量
}

impl PremiumDiscountIndicator {
    pub fn new(swing_length: usize, internal_length: usize) -> Self {
        // 保留足够的K线用于计算
        let max_buffer_size = swing_length * 3;

        Self {
            market_structure: MarketStructureIndicator::new(swing_length, internal_length),
            swing_length,
            internal_length,
            premium_zone: None,
            discount_zone: None,
            equilibrium_zone: None,
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

        // 同时初始化市场结构指标
        self.market_structure.init_with_history(history);
    }

    /// 重置指标状态
    pub fn reset(&mut self) {
        self.candle_buffer.clear();
        self.market_structure.reset();
        self.premium_zone = None;
        self.discount_zone = None;
        self.equilibrium_zone = None;
    }

    /// 获取当前缓冲区大小
    pub fn buffer_size(&self) -> usize {
        self.candle_buffer.len()
    }

    /// 使用摆动高低点计算溢价/折扣/平衡区域
    fn calculate_zones(
        &mut self,
        structure_value: &MarketStructureValue,
        candle: &CandleItem,
    ) -> PremiumDiscountValue {
        let mut result = PremiumDiscountValue::default();

        // 获取摆动高低点
        let swing_high = match &structure_value.swing_high {
            Some(high) => high.price,
            None => return result, // 如果没有摆动高点，直接返回空结果
        };

        let swing_low = match &structure_value.swing_low {
            Some(low) => low.price,
            None => return result, // 如果没有摆动低点，直接返回空结果
        };

        // 计算各区域边界 - 完全按照PineScript逻辑
        // 溢价区域：从顶部到"95%顶部+5%底部"
        let premium_top = swing_high;
        let premium_bottom = 0.95 * swing_high + 0.05 * swing_low;

        // 平衡区域：从"52.5%顶部+47.5%底部"到"52.5%底部+47.5%顶部"
        let equilibrium_top = 0.525 * swing_high + 0.475 * swing_low;
        let equilibrium_bottom = 0.525 * swing_low + 0.475 * swing_high;

        // 折扣区域：从"95%底部+5%顶部"到底部
        let discount_top = 0.95 * swing_low + 0.05 * swing_high;
        let discount_bottom = swing_low;

        // 更新区域数据
        let premium_data = PremiumDiscountData {
            top: premium_top,
            bottom: premium_bottom,
            time: candle.ts,
            is_premium: true,
            is_discount: false,
            is_equilibrium: false,
        };

        let equilibrium_data = PremiumDiscountData {
            top: equilibrium_top,
            bottom: equilibrium_bottom,
            time: candle.ts,
            is_premium: false,
            is_discount: false,
            is_equilibrium: true,
        };

        let discount_data = PremiumDiscountData {
            top: discount_top,
            bottom: discount_bottom,
            time: candle.ts,
            is_premium: false,
            is_discount: true,
            is_equilibrium: false,
        };

        // 更新内部状态
        self.premium_zone = Some(premium_data);
        self.equilibrium_zone = Some(equilibrium_data);
        self.discount_zone = Some(discount_data);

        // 检查当前价格所在区域
        let current_price = candle.c;

        // 扩展区域定义：
        // - 如果价格高于溢价区域顶部(摆动高点)，也将其视为溢价区域
        // - 如果价格低于折扣区域底部(摆动低点)，也将其视为折扣区域
        if current_price >= premium_bottom {
            result.in_premium_zone = true;
        } else if current_price <= discount_top && current_price >= discount_bottom {
            result.in_discount_zone = true;
        } else if current_price < discount_bottom {
            result.in_discount_zone = true;
        } else if current_price >= equilibrium_bottom && current_price <= equilibrium_top {
            result.in_equilibrium_zone = true;
        }

        // 返回结果
        result.premium_zone = Some(premium_data);
        result.equilibrium_zone = Some(equilibrium_data);
        result.discount_zone = Some(discount_data);

        result
    }

    /// 处理新的K线数据
    /// 只需要传入最新的单根K线
    pub fn next(&mut self, candle: &CandleItem) -> PremiumDiscountValue {
        // 添加新K线到缓冲区
        self.candle_buffer.push_back(candle.clone());

        // 维护缓冲区大小
        while self.candle_buffer.len() > self.max_buffer_size {
            self.candle_buffer.pop_front();
        }

        // 获取最新的市场结构信息
        let structure_value = self.market_structure.next(candle);

        // 基于市场结构计算溢价/折扣区域
        self.calculate_zones(&structure_value, candle)
    }

    /// 兼容旧API的方法：一次性处理整个K线数组
    pub fn process_all(&mut self, data_items: &[CandleItem]) -> PremiumDiscountValue {
        self.reset();
        self.init_with_history(data_items);

        if let Some(last_candle) = data_items.last() {
            self.next(last_candle)
        } else {
            PremiumDiscountValue::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_premium_discount_zones() {
        let mut indicator = PremiumDiscountIndicator::new(10, 5);

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
            });
        }

        // 第三阶段：强势突破创新高
        for i in 0..10 {
            candles.push(CandleItem {
                ts: (i + 23) as i64,
                o: 117.0 + i as f64 * 2.0,
                h: 122.0 + i as f64 * 2.0,
                l: 112.0 + i as f64 * 2.0,
                c: 119.0 + i as f64 * 2.0,
                v: 1000.0,
            });
        }

        // 初始化历史数据
        if candles.len() >= 15 {
            indicator.init_with_history(&candles[..15]);
        }

        // 逐步测试区域识别
        println!("=== 溢价/折扣区域测试 ===");
        for i in 15..candles.len() {
            if i % 5 == 0 {
                // 每5根K线测试一次
                let value = indicator.next(&candles[i]);

                println!("K线 {}: 价格={:.1}", i, candles[i].c);

                if let Some(premium) = &value.premium_zone {
                    println!("  溢价区域: {:.1} - {:.1}", premium.bottom, premium.top);
                }

                if let Some(equilibrium) = &value.equilibrium_zone {
                    println!(
                        "  平衡区域: {:.1} - {:.1}",
                        equilibrium.bottom, equilibrium.top
                    );
                }

                if let Some(discount) = &value.discount_zone {
                    println!("  折扣区域: {:.1} - {:.1}", discount.bottom, discount.top);
                }

                println!(
                    "  当前区域: {} {} {}",
                    if value.in_premium_zone { "溢价" } else { "" },
                    if value.in_equilibrium_zone {
                        "平衡"
                    } else {
                        ""
                    },
                    if value.in_discount_zone { "折扣" } else { "" }
                );

                println!();
            } else {
                // 处理其他K线但不打印
                indicator.next(&candles[i]);
            }
        }

        // 最终测试
        let final_value = indicator.next(&candles.last().unwrap());

        println!("\n最终区域信号值:");
        println!("  处理了{}根K线", candles.len());

        if let Some(premium) = &final_value.premium_zone {
            println!("  溢价区域: {:.1} - {:.1}", premium.bottom, premium.top);
        }

        if let Some(equilibrium) = &final_value.equilibrium_zone {
            println!(
                "  平衡区域: {:.1} - {:.1}",
                equilibrium.bottom, equilibrium.top
            );
        }

        if let Some(discount) = &final_value.discount_zone {
            println!("  折扣区域: {:.1} - {:.1}", discount.bottom, discount.top);
        }

        println!(
            "  当前区域: {} {} {}",
            if final_value.in_premium_zone {
                "溢价"
            } else {
                ""
            },
            if final_value.in_equilibrium_zone {
                "平衡"
            } else {
                ""
            },
            if final_value.in_discount_zone {
                "折扣"
            } else {
                ""
            }
        );

        // 添加最后一根K线的价格与区域比较，以便调试
        let last_price = candles.last().unwrap().c;
        println!("\n调试信息:");
        println!("  最后K线价格: {:.1}", last_price);

        if let Some(premium) = &final_value.premium_zone {
            println!(
                "  是否在溢价区域: {} <= {:.1} <= {} ({})",
                premium.bottom,
                last_price,
                premium.top,
                last_price >= premium.bottom && last_price <= premium.top
            );
        }

        if let Some(equilibrium) = &final_value.equilibrium_zone {
            println!(
                "  是否在平衡区域: {} <= {:.1} <= {} ({})",
                equilibrium.bottom,
                last_price,
                equilibrium.top,
                last_price >= equilibrium.bottom && last_price <= equilibrium.top
            );
        }

        if let Some(discount) = &final_value.discount_zone {
            println!(
                "  是否在折扣区域: {} <= {:.1} <= {} ({})",
                discount.bottom,
                last_price,
                discount.top,
                last_price >= discount.bottom && last_price <= discount.top
            );
        }
    }
}
