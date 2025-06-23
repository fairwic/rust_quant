use ta::indicators::{ExponentialMovingAverage, MovingAverageConvergenceDivergence};

/// 突破方向枚举
#[derive(Debug, Clone, PartialEq)]
pub enum BreakDirection {
    Up,   // 向上突破
    Down, // 向下突破
}

/// 突破指标
/// 检测价格突破前期高点或低点的策略，只有连续n根K线振幅小于指定阈值时才进行突破判断
#[derive(Debug, Clone)]
pub struct BreakIndicator {
    prev_highs: Vec<f64>,               // 历史最高价
    prev_lows: Vec<f64>,                // 历史最低价
    break_period: usize,                // 突破检测周期
    amplitude_period: usize,            // 振幅检测周期
    amplitude_threshold: f64,           // 振幅阈值（百分比）
    amplitude_history: Vec<f64>,        // 振幅历史记录
    highest_in_period: f64,             // 当前周期最高点
    lowest_in_period: f64,              // 当前周期最低点
    last_break: Option<BreakDirection>, // 上一次突破方向
    close_history: Vec<f64>,            // 收盘价历史
}

impl BreakIndicator {
    /// 创建突破指标实例
    pub fn new(period: usize, amplitude_period: usize, amplitude_threshold: f64) -> Self {
        Self {
            prev_highs: Vec::with_capacity(period),
            prev_lows: Vec::with_capacity(period),
            break_period: period,
            amplitude_period,
            amplitude_threshold,
            amplitude_history: Vec::with_capacity(amplitude_period),
            highest_in_period: 0.0,
            lowest_in_period: f64::MAX,
            last_break: None,
            close_history: Vec::with_capacity(period),
        }
    }

    /// 使用默认振幅参数创建实例
    pub fn new_with_default_amplitude(period: usize) -> Self {
        Self::new(period, 3, 0.8)
    }

    /// 计算突破信号，返回 (是否突破高点, 是否突破低点, 突破强度)
    pub fn next(
        &mut self,
        current_high: f64,
        current_low: f64,
        current_close: f64,
    ) -> (bool, bool, f64) {
        // 健壮性检查
        if current_high < current_low || current_close <= 0.0 {
            return (false, false, 0.0);
        }
        let current_amplitude = ((current_high - current_low) / current_close) * 100.0;
        println!("current_amplitude: {:?}", current_amplitude);

        // 先补齐窗口
        if self.prev_highs.len() < self.break_period {
            self.prev_highs.push(current_high);
            self.prev_lows.push(current_low);
            self.amplitude_history.push(current_amplitude);
            self.close_history.push(current_close);
            self.highest_in_period = self.prev_highs.iter().cloned().fold(0.0, f64::max);
            self.lowest_in_period = self.prev_lows.iter().cloned().fold(f64::MAX, f64::min);
            self.last_break = None;
            return (false, false, 0.0);
        }

        // 新增：判断收盘价是否连续上涨
        let mut is_all_up = true;
        if self.close_history.len() == self.break_period {
            for i in 1..self.close_history.len() {
                if self.close_history[i] <= self.close_history[i - 1] {
                    is_all_up = false;
                    break;
                }
            }
        }
        if is_all_up && self.close_history.len() == self.break_period {
            self.prev_highs.clear();
            self.prev_lows.clear();
            self.amplitude_history.clear();
            self.close_history.clear();
            self.highest_in_period = 0.0;
            self.lowest_in_period = f64::MAX;
            self.last_break = None;
            return (false, false, 0.0);
        }

        // 判断窗口内振幅是否都小于阈值
        let amplitude_ok = self.amplitude_history.iter().all(|&amp| amp < self.amplitude_threshold);

        if amplitude_ok {
            // 判断突破
            let highest = self.prev_highs.iter().cloned().fold(0.0, f64::max);
            let lowest = self.prev_lows.iter().cloned().fold(f64::MAX, f64::min);
            let high_break = current_close > highest;
            let low_break = current_close < lowest;
            let break_strength = if high_break {
                (current_close - highest) / highest * 100.0
            } else if low_break {
                (lowest - current_close) / lowest * 100.0
            } else {
                0.0
            };
            self.last_break = if high_break {
                Some(BreakDirection::Up)
            } else if low_break {
                Some(BreakDirection::Down)
            } else {
                None
            };

            //没有突破且当前振幅大于阈值,表示突破失败了. 但是因为振幅过大，所以直接忽视着一根k线
            if self.last_break.is_none()
                && current_amplitude > self.amplitude_threshold
                && (current_high > self.highest_in_period || current_low < self.lowest_in_period)
            {
                self.close_history.push(current_close);
                if self.close_history.len() > self.break_period {
                    self.close_history.remove(0);
                }
                return (false, false, 0.0);
            }

            // 如果突破，重置窗口
            if high_break || low_break {
                self.prev_highs.clear();
                self.prev_lows.clear();
                self.amplitude_history.clear();
                self.close_history.clear();
                self.highest_in_period = 0.0;
                self.lowest_in_period = f64::MAX;
            } else {
                // 未突破则滑动窗口
                self.prev_highs.push(current_high);
                self.prev_lows.push(current_low);
                self.amplitude_history.push(current_amplitude);
                self.close_history.push(current_close);
                if self.prev_highs.len() > self.break_period {
                    self.prev_highs.remove(0);
                }
                if self.prev_lows.len() > self.break_period {
                    self.prev_lows.remove(0);
                }
                if self.amplitude_history.len() > self.break_period {
                    self.amplitude_history.remove(0);
                }
                if self.close_history.len() > self.break_period {
                    self.close_history.remove(0);
                }
                self.highest_in_period = self.prev_highs.iter().cloned().fold(0.0, f64::max);
                self.lowest_in_period = self.prev_lows.iter().cloned().fold(f64::MAX, f64::min);
            }
            return (high_break, low_break, break_strength);
        } else {
            // 振幅条件不满足，滑动窗口
            self.prev_highs.push(current_high);
            self.prev_lows.push(current_low);
            self.amplitude_history.push(current_amplitude);
            self.close_history.push(current_close);
            if self.prev_highs.len() > self.break_period {
                self.prev_highs.remove(0);
            }
            if self.prev_lows.len() > self.break_period {
                self.prev_lows.remove(0);
            }
            if self.amplitude_history.len() > self.break_period {
                self.amplitude_history.remove(0);
            }
            if self.close_history.len() > self.break_period {
                self.close_history.remove(0);
            }
            self.highest_in_period = self.prev_highs.iter().cloned().fold(0.0, f64::max);
            self.lowest_in_period = self.prev_lows.iter().cloned().fold(f64::MAX, f64::min);
            self.last_break = None;
            return (false, false, 0.0);
        }
    }

    /// 更新历史高低点数据
    fn update_history(&mut self, current_high: f64, current_low: f64) {
        if self.prev_highs.len() >= self.break_period {
            self.prev_highs.remove(0);
        }
        self.prev_highs.push(current_high);
        if self.prev_lows.len() >= self.break_period {
            self.prev_lows.remove(0);
        }
        self.prev_lows.push(current_low);

        // 更新当前周期最高点和最低点
        self.highest_in_period = self.prev_highs.iter().cloned().fold(0.0, f64::max);
        // 更新当前周期最低点
        self.lowest_in_period = self.prev_lows.iter().cloned().fold(f64::MAX, f64::min);
    }

    /// 更新振幅历史记录
    fn update_amplitude_history(&mut self, amplitude: f64) {
        if self.amplitude_history.len() >= self.amplitude_period {
            self.amplitude_history.remove(0);
        }
        self.amplitude_history.push(amplitude);
    }

    /// 检查振幅条件：连续n根K线振幅都小于阈值
    fn is_amplitude_condition_met(&self) -> bool {
        self.amplitude_history.len() == self.amplitude_period
            && self
                .amplitude_history
                .iter()
                .all(|&amp| amp < self.amplitude_threshold)
    }

    /// 获取当前高点
    pub fn get_current_high(&self) -> f64 {
        self.highest_in_period
    }
    /// 获取当前低点
    pub fn get_current_low(&self) -> f64 {
        self.lowest_in_period
    }
    /// 获取最近一次突破方向
    pub fn get_last_break_direction(&self) -> Option<BreakDirection> {
        self.last_break.clone()
    }
    /// 获取当前振幅历史
    pub fn get_amplitude_history(&self) -> &Vec<f64> {
        &self.amplitude_history
    }
    /// 获取振幅条件状态和平均振幅
    pub fn get_amplitude_condition_status(&self) -> (bool, f64) {
        let met = self.is_amplitude_condition_met();
        let avg = if self.amplitude_history.is_empty() {
            0.0
        } else {
            self.amplitude_history.iter().sum::<f64>() / self.amplitude_history.len() as f64
        };
        (met, avg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_break_indicator_new() {
        let indicator = BreakIndicator::new(5, 3, 0.8);
        assert_eq!(indicator.break_period, 5);
        assert_eq!(indicator.amplitude_period, 3);
        assert_eq!(indicator.amplitude_threshold, 0.8);
        assert_eq!(indicator.prev_highs.len(), 0);
        assert_eq!(indicator.prev_lows.len(), 0);
    }

    #[test]
    fn test_break_indicator_new_with_default_amplitude() {
        let indicator = BreakIndicator::new_with_default_amplitude(5);
        assert_eq!(indicator.break_period, 5);
        assert_eq!(indicator.amplitude_period, 3);
        assert_eq!(indicator.amplitude_threshold, 0.8);
    }

    #[test]
    fn test_amplitude_condition_check() {
        let mut indicator = BreakIndicator::new(3, 3, 0.8);
        indicator.update_amplitude_history(0.5);
        indicator.update_amplitude_history(0.6);
        indicator.update_amplitude_history(0.7);
        indicator.update_amplitude_history(0.75);
        assert!(indicator.is_amplitude_condition_met());
        let mut indicator = BreakIndicator::new(3, 3, 0.8);
        indicator.update_amplitude_history(0.5);
        indicator.update_amplitude_history(0.6);
        indicator.update_amplitude_history(1.0);
        assert!(!indicator.is_amplitude_condition_met());
    }

    #[test]
    fn test_break_indicator_with_amplitude_condition() {
        let mut indicator = BreakIndicator::new(3, 3, 0.8);
        indicator.next(100.0, 99.5, 99.8);
        indicator.next(105.0, 104.5, 104.8);
        indicator.next(110.0, 109.5, 109.8);
        println!(
            "amplitude_condition_status: {:?}",
            indicator.get_amplitude_condition_status()
        );
        println!("amplitude_history: {:?}", indicator.get_amplitude_history());
        println!("highest_in_period: {:?}", indicator.get_current_high());
        println!("lowest_in_period: {:?}", indicator.get_current_low());
        println!("last_break: {:?}", indicator.get_last_break_direction());
        println!("--------------------------------");
        let (high_break, low_break, strength) = indicator.next(115.0, 114.5, 114.8);
        println!(
            "amplitude_condition_status: {:?}",
            indicator.get_amplitude_condition_status()
        );
        println!("amplitude_history: {:?}", indicator.get_amplitude_history());
        println!("highest_in_period: {:?}", indicator.get_current_high());
        println!("lowest_in_period: {:?}", indicator.get_current_low());
        println!("last_break: {:?}", indicator.get_last_break_direction());
        println!("--------------------------------");
        assert!(high_break);
        assert!(!low_break);
        assert!(strength > 0.0);
    }

    #[test]
    fn test_break_indicator_amplitude_condition_not_met() {
        let mut indicator = BreakIndicator::new(3, 3, 0.8);
        indicator.next(100.0, 99.5, 99.8);
        indicator.next(105.0, 104.5, 104.8);
        indicator.next(110.0, 109.5, 109.8);
        println!(
            "amplitude_condition_status: {:?}",
            indicator.get_amplitude_condition_status()
        );
        println!("amplitude_history: {:?}", indicator.get_amplitude_history());
        println!("highest_in_period: {:?}", indicator.get_current_high());
        println!("lowest_in_period: {:?}", indicator.get_current_low());
        println!("last_break: {:?}", indicator.get_last_break_direction());
        println!("--------------------------------");

        let (high_break, low_break, strength) = indicator.next(120.0, 115.0, 117.5);
        println!(
            "amplitude_condition_status: {:?}",
            indicator.get_amplitude_condition_status()
        );
        println!("amplitude_history: {:?}", indicator.get_amplitude_history());
        println!("highest_in_period: {:?}", indicator.get_current_high());
        println!("lowest_in_period: {:?}", indicator.get_current_low());
        println!("last_break: {:?}", indicator.get_last_break_direction());
        println!("--------------------------------");
        assert!(!high_break);
        assert!(!low_break);
        assert_eq!(strength, 0.0);
    }

    #[test]
    fn test_break_indicator_high_break() {
        let mut indicator = BreakIndicator::new(3, 3, 0.8);
        indicator.next(100.0, 99.5, 99.8);
        indicator.next(105.0, 104.5, 104.8);
        indicator.next(110.0, 109.5, 109.8);
        let (high_break, low_break, strength) = indicator.next(115.0, 114.5, 114.8);
        assert!(high_break);
        assert!(!low_break);
        assert!(strength > 0.0);
        assert_eq!(
            indicator.get_last_break_direction(),
            Some(BreakDirection::Up)
        );
    }

    #[test]
    fn test_break_indicator_low_break() {
        let mut indicator = BreakIndicator::new(3, 3, 0.8);
        indicator.next(110.0, 100.0, 105.0);
        indicator.next(105.0, 95.0, 100.0);
        indicator.next(100.0, 90.0, 95.0);
        let (high_break, low_break, strength) = indicator.next(95.0, 85.0, 88.0);
        assert!(!high_break);
        assert!(low_break);
        assert!(strength > 0.0);
        assert_eq!(
            indicator.get_last_break_direction(),
            Some(BreakDirection::Down)
        );
    }

    #[test]
    fn test_break_indicator_no_break() {
        let mut indicator = BreakIndicator::new(3, 3, 0.8);
        indicator.next(110.0, 100.0, 105.0);
        indicator.next(105.0, 95.0, 100.0);
        indicator.next(100.0, 90.0, 95.0);
        let (high_break, low_break, strength) = indicator.next(105.0, 95.0, 100.0);
        assert!(!high_break);
        assert!(!low_break);
        assert_eq!(strength, 0.0);
        assert_eq!(indicator.get_last_break_direction(), None);
    }

    #[test]
    fn test_break_indicator_get_current_levels() {
        let mut indicator = BreakIndicator::new(3, 3, 0.8);
        indicator.next(110.0, 100.0, 105.0);
        indicator.next(105.0, 95.0, 100.0);
        indicator.next(100.0, 90.0, 95.0);
        assert_eq!(indicator.get_current_high(), 110.0);
        assert_eq!(indicator.get_current_low(), 90.0);
    }

    #[test]
    fn test_get_amplitude_condition_status() {
        let mut indicator = BreakIndicator::new(3, 3, 0.8);
        indicator.update_amplitude_history(0.5);
        indicator.update_amplitude_history(0.6);
        indicator.update_amplitude_history(0.7);
        let (condition_met, avg_amplitude) = indicator.get_amplitude_condition_status();
        assert!(condition_met);
        assert!((avg_amplitude - 0.6).abs() < 0.01);
    }

    #[test]
    fn test_break_indicator_consecutive_monotonic_closes() {
        // 连续收盘价单调递增，窗口应被重置，不会判断为突破
        let mut indicator = BreakIndicator::new(3, 3, 0.8);
        // 补齐窗口
        indicator.next(100.0, 99.5, 100.0);
        indicator.next(101.0, 100.5, 101.0);
        indicator.next(102.0, 101.5, 102.0);
        // 这时窗口内收盘价为[100.0, 101.0, 102.0]，单调递增，窗口应被重置
        assert_eq!(indicator.prev_highs.len(), 0);
        assert_eq!(indicator.close_history.len(), 0);
        // 再加一根，窗口重新补齐
        indicator.next(103.0, 102.5, 103.0);
        assert_eq!(indicator.prev_highs.len(), 1);
        assert_eq!(indicator.close_history.len(), 1);
    }

    #[test]
    fn test_break_indicator_large_amplitude_but_no_break() {
        // 测试中间出现振幅很大但收盘价未突破最高价，突破失败
        let mut indicator = BreakIndicator::new(3, 3, 0.8);
        // 补齐窗口，振幅都小于0.8
        indicator.next(100.0, 99.5, 100.0);
        indicator.next(101.0, 100.5, 101.0);
        indicator.next(102.0, 101.5, 100.0);
        // 此时窗口已满，最高价为102.0
        // 新K线振幅很大，但收盘价没有突破最高价
        let (high_break, low_break, strength) = indicator.next(110.0, 90.0, 101.5); // 振幅大于0.8，但收盘价101.5 < 102.0
        assert!(!high_break);
        assert!(!low_break);
        assert_eq!(strength, 0.0);
        // 窗口滑动，最高价应为102.0, 101.0, 110.0
        assert_eq!(indicator.get_current_high(), 102.0);
        // 再来一根正常K线，收盘价突破最高价
        let (high_break2, _, strength2) = indicator.next(120.0, 119.0, 120.5);
        assert!(high_break2);
        assert!(strength2 > 0.0);
    }
}
