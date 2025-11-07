use rust_quant_core::database::get_db_pool;
use rust_quant_core::logger::setup_logging;
use dotenv;
use rust_quant_common::CandleItem;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
/// 公平价值缺口数据
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FvgData {
    pub top: f64,         // 顶部价格
    pub bottom: f64,      // 底部价格
    pub time: i64,        // 时间戳
    pub filled: bool,     // 是否已填补
    pub is_bullish: bool, // 是否是多头缺口
}

/// 公平价值缺口信号值
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FairValueGapValue {
    pub bullish_gaps: Vec<FvgData>, // 多头缺口
    pub bearish_gaps: Vec<FvgData>, // 空头缺口
    pub current_bullish_fvg: bool,  // 当前k线是否形成多头缺口
    pub current_bearish_fvg: bool,  // 当前k线是否形成空头缺口
}

/// 公平价值缺口指标
#[derive(Debug, Clone)]
pub struct FairValueGapIndicator {
    threshold_multiplier: f64, // 阈值乘数
    auto_threshold: bool,      // 自动阈值
    gaps: Vec<FvgData>,        // 存储所有缺口
    // 新增：内部缓冲区
    candle_buffer: VecDeque<CandleItem>, // 内部K线缓冲区
    max_buffer_size: usize,              // 缓冲区最大容量
}

impl FairValueGapIndicator {
    pub fn new(threshold_multiplier: f64, auto_threshold: bool) -> Self {
        // FVG检测需要至少3根K线，保留更多以计算动态阈值
        let max_buffer_size = 30;

        Self {
            threshold_multiplier,
            auto_threshold,
            gaps: Vec::new(),
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
        self.gaps.clear();
    }

    /// 获取当前缓冲区大小
    pub fn buffer_size(&self) -> usize {
        self.candle_buffer.len()
    }

    /// 计算动态阈值
    fn calculate_threshold(&self) -> f64 {
        if self.auto_threshold {
            // 使用最近K线的平均波动范围作为阈值
            let mut sum_range = 0.0;
            let range_bars = 10.min(self.candle_buffer.len());

            if range_bars == 0 {
                return 0.0;
            }

            for i in 0..range_bars {
                let idx = self.candle_buffer.len() - 1 - i;
                if idx < self.candle_buffer.len() {
                    let candle = &self.candle_buffer[idx];
                    sum_range += candle.h - candle.l;
                }
            }

            let avg_range = sum_range / range_bars as f64;
            avg_range * self.threshold_multiplier
        } else {
            // 使用固定阈值
            if let Some(current) = self.candle_buffer.back() {
                let avg_price = (current.c + current.o) / 2.0;
                avg_price * 0.001 * self.threshold_multiplier // 默认0.1%
            } else {
                0.0
            }
        }
    }

    /// 处理新的K线数据
    /// 只需要传入最新的单根K线
    pub fn next(&mut self, candle: &CandleItem) -> FairValueGapValue {
        // 添加新K线到缓冲区
        self.candle_buffer.push_back(candle.clone());

        // 维护缓冲区大小
        while self.candle_buffer.len() > self.max_buffer_size {
            self.candle_buffer.pop_front();
        }

        // 需要至少3根K线才能检测FVG
        if self.candle_buffer.len() < 3 {
            return FairValueGapValue::default();
        }

        // 调用内部处理方法
        self.process_fvg()
    }

    /// 处理FVG检测的核心逻辑
    fn process_fvg(&mut self) -> FairValueGapValue {
        let mut result = FairValueGapValue::default();

        let len = self.candle_buffer.len();

        // 获取最近3根K线
        let current = &self.candle_buffer[len - 1];
        let prev = &self.candle_buffer[len - 2];
        let prev2 = &self.candle_buffer[len - 3];

        // 计算阈值
        let _threshold = self.calculate_threshold();

        // 检测多头FVG：当前K线的最低价高于前两根K线的最高价，且前一根K线收盘价也高于前两根K线最高价
        if current.l > prev2.h && prev.c > prev2.h {
            result.current_bullish_fvg = true;
            let fvg = FvgData {
                top: current.l,
                bottom: prev2.h,
                time: current.ts,
                filled: false,
                is_bullish: true,
            };

            self.gaps.push(fvg);
        }

        // 检测空头FVG：当前K线的最高价低于前两根K线的最低价，且前一根K线收盘价也低于前两根K线最低价
        if current.h < prev2.l && prev.c < prev2.l {
            result.current_bearish_fvg = true;
            let fvg = FvgData {
                top: prev2.l,
                bottom: current.h,
                time: current.ts,
                filled: false,
                is_bullish: false,
            };

            self.gaps.push(fvg);
        }

        // 更新现有缺口状态
        for i in 0..self.gaps.len() {
            let mut gap = self.gaps[i];

            if !gap.filled {
                if gap.is_bullish && (current.l <= gap.bottom || current.c <= gap.bottom) {
                    gap.filled = true;
                } else if !gap.is_bullish && (current.h >= gap.top || current.c >= gap.top) {
                    gap.filled = true;
                }

                self.gaps[i] = gap;
            }
        }

        // 获取未填补的缺口（包括新创建的）
        for gap in &self.gaps {
            if !gap.filled {
                if gap.is_bullish {
                    result.bullish_gaps.push(*gap);
                } else {
                    result.bearish_gaps.push(*gap);
                }
            }
        }

        // 移除已填补的缺口
        self.gaps.retain(|gap| !gap.filled);
        result
    }

    /// 兼容旧API的方法：一次性处理整个K线数组
    pub fn process_all(&mut self, data_items: &[CandleItem]) -> FairValueGapValue {
        self.reset();
        self.init_with_history(data_items);

        if data_items.len() >= 3 {
            self.process_fvg()
        } else {
            FairValueGapValue::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_common;
    use rust_quant_market::models::enums::{SelectTime, TimeDirect};

    #[test]
    fn test_fvg_basic() {
        let mut indicator = FairValueGapIndicator::new(1.0, true);
        // 创建测试数据
        let mut candles = Vec::new();
        // 创建一个明显的多头FVG：k3的低点高于k1的高点，且k2收盘价也高于k1高点
        candles.push(CandleItem {
            // k1 (prev2)
            ts: 1,
            o: 100.0,
            h: 110.0, // 这是关键高点
            l: 95.0,
            c: 105.0,
            v: 1000.0,
            confirm: 0,
        });
        candles.push(CandleItem {
            // k2 (prev) - 收盘价必须高于k1的高点
            ts: 2,
            o: 105.0,
            h: 115.0,
            l: 100.0,
            c: 112.0, // 112.0 > 110.0 ✓
            v: 1000.0,
            confirm: 0,
        });
        candles.push(CandleItem {
            // k3 (current) - 低点必须高于k1的高点
            ts: 3,
            o: 120.0,
            h: 130.0,
            l: 115.0, // 115.0 > 110.0 ✓ 形成多头FVG
            c: 125.0,
            v: 1000.0,
            confirm: 0,
        });
        // 初始化前两根K线作为历史数据
        indicator.init_with_history(&candles[..2]);
        // 处理第三根K线
        let value = indicator.next(&candles[2]);
        // 输出测试结果
        println!("FVG信号值: {:?}", value);
        println!("多头FVG条件检查:");
        println!(
            "  当前K线低点({}) > 前两根K线高点({}): {}",
            115.0,
            110.0,
            115.0 > 110.0
        );
        println!(
            "  前一根K线收盘价({}) > 前两根K线高点({}): {}",
            112.0,
            110.0,
            112.0 > 110.0
        );
        // 应该检测到一个多头FVG
        assert_eq!(value.current_bullish_fvg, true);
        assert_eq!(value.bullish_gaps.len(), 1);
        assert_eq!(value.current_bearish_fvg, false);
        if value.current_bullish_fvg {
            println!("✅ 成功检测到多头FVG");
            let fvg = &value.bullish_gaps[0];
            println!("  FVG顶部: {}, 底部: {}", fvg.top, fvg.bottom);
        }
    }
    #[test]
    fn test_bearish_fvg() {
        let mut indicator = FairValueGapIndicator::new(1.0, true);
        // 创建空头FVG测试数据
        let mut candles = Vec::new();
        // 创建空头FVG：k3的高点低于k1的低点，且k2收盘价也低于k1低点
        candles.push(CandleItem {
            // k1 (prev2)
            ts: 1,
            o: 120.0,
            h: 125.0,
            l: 110.0, // 这是关键低点
            c: 115.0,
            v: 1000.0,
            confirm: 0,
        });
        candles.push(CandleItem {
            // k2 (prev) - 收盘价必须低于k1的低点
            ts: 2,
            o: 115.0,
            h: 118.0,
            l: 105.0,
            c: 108.0, // 108.0 < 110.0 ✓
            v: 1000.0,
            confirm: 0,
        });
        candles.push(CandleItem {
            // k3 (current) - 高点必须低于k1的低点
            ts: 3,
            o: 100.0,
            h: 105.0, // 105.0 < 110.0 ✓ 形成空头FVG
            l: 95.0,
            c: 102.0,
            v: 1000.0,
            confirm: 0,
        });
        // 初始化前两根K线作为历史数据
        indicator.init_with_history(&candles[..2]);
        // 处理第三根K线
        let value = indicator.next(&candles[2]);
        println!("空头FVG信号值: {:?}", value);
        println!("空头FVG条件检查:");
        println!(
            "  当前K线高点({}) < 前两根K线低点({}): {}",
            105.0,
            110.0,
            105.0 < 110.0
        );
        println!(
            "  前一根K线收盘价({}) < 前两根K线低点({}): {}",
            108.0,
            110.0,
            108.0 < 110.0
        );
        assert_eq!(value.current_bearish_fvg, true);
        assert_eq!(value.bearish_gaps.len(), 1);
        assert_eq!(value.current_bullish_fvg, false);
        if value.current_bearish_fvg {
            println!("✅ 成功检测到空头FVG");
            let fvg = &value.bearish_gaps[0];
            println!("  FVG顶部: {}, 底部: {}", fvg.top, fvg.bottom);
        }
    }
    #[tokio::test]
    async fn test_fvg_with_dynamic_threshold() -> anyhow::Result<()> {
        dotenv().ok();
        setup_logging().await?;
        init_db().await;

        let select_time: SelectTime = SelectTime {
            start_time: 1747494000000,
            direct: TimeDirect::BEFORE,
            end_time: None,
        };
        let candles = trading::task::basic::get_candle_data_confirm(
            "BTC-USDT-SWAP",
            "1H",
            300,
            Some(select_time),
        )
        .await?;

        let mut indicator = FairValueGapIndicator::new(1.0, true);

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

        for candle in candle_items {
            let mut candle = candle.clone();

            let value = indicator.next(&candle);
            if value.current_bullish_fvg {
                println!("FVG信号值: {:?}", value);
                println!("✅ 成功检测到多头FVG");
            }
            if value.current_bearish_fvg {
                println!("FVG信号值: {:?}", value);
                println!("✅ 成功检测到空头FVG");
            }
        }
        Ok(())
    }
}
