// src/strategies/fib_trend.rs
use anyhow::Result;
use serde::{Deserialize, Serialize};
use ta::{Close, High, Low, Next, Reset};

use rust_quant::trading::indicator::atr::ATR;
use rust_quant::trading::indicator::super_trend::Supertrend;

// 策略配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FibTrendConfig {
    pub factor: f64,       // 对应Pine Script的factor参数
    pub atr_period: usize, // 对应Pine Script的atrPeriod
    pub extend_bars: usize,
    pub fib_levels: Vec<f64>,
}

// 斐波那契趋势数据
#[derive(Debug, Clone)]
pub struct FibTrendData {
    pub supertrend: f64,
    pub direction: i8,
    pub upper_band: f64,
    pub lower_band: f64,
    pub fib_levels: Vec<f64>,
    pub extreme_high: f64,
    pub extreme_low: f64,
}

// 策略处理器
pub struct FibTrendStrategy {
    config: FibTrendConfig,
    supertrend: Supertrend,
    atr: ATR,
    current_atr: f64, // 新增字段存储当前ATR值
    extreme_high: f64,
    extreme_low: f64,
    previous_direction: i8,
}

impl FibTrendStrategy {
    pub fn new(config: FibTrendConfig) -> Result<Self> {
        Ok(Self {
            supertrend: Supertrend::new(config.factor, 25)?,
            atr: ATR::new(200).unwrap(),
            current_atr: 0.0, // 初始化ATR值
            extreme_high: 0.0,
            extreme_low: f64::MAX,
            previous_direction: 0,
            config,
        })
    }

    // 处理单个K线
    pub fn next(&mut self, high: f64, low: f64, close: f64) -> FibTrendData {
        // ✅ 正确的极值更新顺序
        let (supertrend_value, direction) = self.supertrend.next((high, low, close));

        // 先更新方向状态
        let direction_changed = direction != self.previous_direction;
        self.previous_direction = direction;

        // 更新极值
        if direction_changed {
            self.extreme_high = high;
            self.extreme_low = low;
        } else {
            self.extreme_high = self.extreme_high.max(high);
            self.extreme_low = self.extreme_low.min(low);
        }

        // ✅ 正确的ATR计算时机（在极值更新后）
        self.current_atr = self.atr.next(high, low, close);

        FibTrendData {
            supertrend: supertrend_value,
            direction,
            upper_band: self.calculate_upper_band(high, direction),
            lower_band: self.calculate_lower_band(low, direction),
            fib_levels: self.calculate_fib_levels(direction),
            extreme_high: self.extreme_high,
            extreme_low: self.extreme_low,
        }
    }

    // ✅ 增强的斐波那契计算
    fn calculate_fib_levels(&self, direction: i8) -> Vec<f64> {
        let (base, target) = match direction {
            1 => (self.extreme_low, self.extreme_high),
            -1 => (self.extreme_high, self.extreme_low),
            _ => return vec![0.0; self.config.fib_levels.len()],
        };

        let range = target - base;
        self.config
            .fib_levels
            .iter()
            .map(|level| base + range * level)
            .collect()
    }

    // 计算上轨（使用当前ATR值）
    fn calculate_upper_band(&self, high: f64, direction: i8) -> f64 {
        match direction {
            1 => high + self.current_atr * 3.0,
            _ => high,
        }
    }

    // 计算下轨（使用当前ATR值）
    fn calculate_lower_band(&self, low: f64, direction: i8) -> f64 {
        match direction {
            -1 => low - self.current_atr * 3.0,
            _ => low,
        }
    }

    // 重置状态
    pub fn reset(&mut self) {
        self.supertrend.reset();
        self.atr.reset();
        self.current_atr = 0.0;
        self.extreme_high = 0.0;
        self.extreme_low = f64::MAX;
        self.previous_direction = 0;
    }
}

// 测试模块
#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;
    use dotenv::dotenv;
    use rust_quant::app_config::db::init_db;
    use rust_quant::app_config::log::setup_logging;
    use rust_quant::trading;
    use rust_quant::trading::indicator::atr::ATR;

    use super::*;

    // 创建测试配置
    fn test_config() -> FibTrendConfig {
        FibTrendConfig {
            factor: 4.0,
            atr_period: 200,
            extend_bars: 20,
            fib_levels: vec![0.236, 0.382, 0.618, 0.786],
        }
    }

    #[tokio::test]
    async fn test_atr_calculation() -> anyhow::Result<()> {
        dotenv().ok();
        setup_logging().await?;
        init_db().await;

        let mut strategy = FibTrendStrategy::new(test_config()).unwrap();
        // 设置参数
        let inst_id = "BTC-USDT-SWAP";
        let period = "4H";
        let min_length = 200;
        let select_time = None;
        let candles =
            trading::task::basic::get_candle_data_confirm(inst_id, period, min_length, select_time)
                .await?;
        let results: Vec<_> = candles
            .iter()
            .map(|item| {
                strategy.next(
                    item.h.parse().unwrap(),
                    item.l.parse().unwrap(),
                    item.c.parse().unwrap(),
                )
            })
            .collect();
        println!("{:#?}", results);
        Ok(())
    }

    #[test]
    fn test_fib_levels() {
        let mut strategy = FibTrendStrategy::new(test_config()).unwrap();
        //
        // // 处理前三根K线建立趋势
        // for &(h, l, c) in &TEST_DATA[..3] {
        //     strategy.next(h, l, c);
        // }
        //
        // let data = strategy.next(TEST_DATA[3].0, TEST_DATA[3].1, TEST_DATA[3].2);

        // 验证斐波那契水平
        let expected_levels = vec![
            7.5 + (11.0 - 7.5) * 0.236,
            7.5 + (11.0 - 7.5) * 0.382,
            7.5 + (11.0 - 7.5) * 0.618,
            7.5 + (11.0 - 7.5) * 0.786,
        ];
        //
        // data.fib_levels.iter()
        //     .zip(expected_levels.iter())
        //     .for_each(|(a, e)| assert_relative_eq!(a, e, epsilon = 0.001));
    }
}
