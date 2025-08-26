// src/indicators/supertrend.rs use ta::{Next, Reset};

use crate::trading::indicator::atr::ATR;
use ta::{Next, Reset};

// 修正后的Supertrend结构体
#[derive(Debug, Clone)]
pub struct Supertrend {
    factor: f64,
    atr: ATR, // 确保ATR使用RMA实现
    prev_upper: f64,
    prev_lower: f64,
    prev_close: f64,
    direction: i8,
    initialized: bool, // 新增初始化标志
}

impl Supertrend {
    pub fn new(factor: f64, atr_period: usize) -> anyhow::Result<Self> {
        Ok(Self {
            factor,
            atr: ATR::new(atr_period)?, // 确保ATR使用RMA
            prev_upper: 0.0,
            prev_lower: 0.0,
            prev_close: 0.0,
            direction: 1,
            initialized: false,
        })
    }

    // 核心逻辑修正
    pub fn next(&mut self, (high, low, close): (f64, f64, f64)) -> (f64, i8) {
        if !self.initialized {
            self.prev_close = close;
            self.initialized = true;
            return (0.0, 1); // 初始化返回默认值
        }

        let src = (high + low) / 2.0;
        let atr = self.atr.next(high, low, close);

        // 计算基础轨道
        let upper = src + self.factor * atr;
        let lower = src - self.factor * atr;

        // Pine Script轨道继承逻辑
        let (final_upper, final_lower) = match self.direction {
            1 => (
                upper.max(self.prev_upper), // 上涨趋势继承更严格的upper
                lower,
            ),
            -1 => (
                upper,
                lower.min(self.prev_lower), // 下跌趋势继承更严格的lower
            ),
            _ => (upper, lower),
        };

        // 方向变化判断（严格遵循Pine Script逻辑）
        let new_direction = if self.prev_close > self.prev_upper {
            if close < final_lower {
                -1
            } else {
                1
            }
        } else {
            if (close > final_upper) {
                1
            } else {
                -1
            }
        };

        // 趋势延续时保持轨道值稳定
        let (final_upper, final_lower) = if new_direction == self.direction {
            (
                final_upper.max(self.prev_upper),
                final_lower.min(self.prev_lower),
            )
        } else {
            (upper, lower) // 趋势反转时重置轨道
        };

        // 更新状态
        self.prev_upper = final_upper;
        self.prev_lower = final_lower;
        self.prev_close = close;
        self.direction = new_direction;

        (
            match new_direction {
                1 => final_upper,
                -1 => final_lower,
                _ => unreachable!(),
            },
            new_direction,
        )
    }
}

impl Reset for Supertrend {
    fn reset(&mut self) {
        self.atr.reset();
        self.prev_upper = 0.0;
        self.prev_lower = 0.0;
        self.prev_close = 0.00; // 重置收盘记录
        self.direction = 1;
    }
}
