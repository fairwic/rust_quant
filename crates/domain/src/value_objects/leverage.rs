//! 杠杆倍数值对象

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LeverageError {
    #[error("杠杆倍数必须为正数: {0}")]
    MustBePositive(f64),

    #[error("杠杆倍数超出范围: {0} (允许范围: 1-125)")]
    OutOfRange(f64),

    #[error("杠杆倍数无效: {0}")]
    Invalid(String),
}

/// 杠杆倍数值对象
///
/// 业务规则:
/// - 范围: 1x - 125x
/// - 必须为正数
/// - 常见值: 1, 2, 3, 5, 10, 20, 50, 100, 125
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Leverage(f64);

impl Leverage {
    /// 创建杠杆倍数 (带验证)
    pub fn new(value: f64) -> Result<Self, LeverageError> {
        if value <= 0.0 {
            return Err(LeverageError::MustBePositive(value));
        }

        if value > 125.0 {
            return Err(LeverageError::OutOfRange(value));
        }

        if !value.is_finite() {
            return Err(LeverageError::Invalid("杠杆必须是有限数".to_string()));
        }

        Ok(Self(value))
    }

    /// 常用杠杆倍数
    pub fn x1() -> Self {
        Self(1.0)
    }
    pub fn x2() -> Self {
        Self(2.0)
    }
    pub fn x3() -> Self {
        Self(3.0)
    }
    pub fn x5() -> Self {
        Self(5.0)
    }
    pub fn x10() -> Self {
        Self(10.0)
    }
    pub fn x20() -> Self {
        Self(20.0)
    }
    pub fn x50() -> Self {
        Self(50.0)
    }
    pub fn x100() -> Self {
        Self(100.0)
    }
    pub fn x125() -> Self {
        Self(125.0)
    }

    /// 获取杠杆倍数值
    pub fn value(&self) -> f64 {
        self.0
    }

    /// 判断是否为高杠杆 (>10x)
    pub fn is_high_leverage(&self) -> bool {
        self.0 > 10.0
    }

    /// 计算所需保证金
    ///
    /// margin = position_value / leverage
    pub fn calculate_margin(&self, position_value: f64) -> f64 {
        position_value / self.0
    }

    /// 计算最大持仓价值
    ///
    /// max_position_value = available_margin * leverage
    pub fn calculate_max_position(&self, available_margin: f64) -> f64 {
        available_margin * self.0
    }
}

impl fmt::Display for Leverage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x", self.0)
    }
}

impl PartialOrd for Leverage {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_leverage() {
        let lev = Leverage::new(10.0).unwrap();
        assert_eq!(lev.value(), 10.0);
        assert!(!lev.is_high_leverage());
    }

    #[test]
    fn test_high_leverage() {
        let lev = Leverage::new(50.0).unwrap();
        assert!(lev.is_high_leverage());
    }

    #[test]
    fn test_invalid_leverage() {
        // 负数
        assert!(Leverage::new(-5.0).is_err());

        // 零
        assert!(Leverage::new(0.0).is_err());

        // 超出范围
        assert!(Leverage::new(200.0).is_err());
    }

    #[test]
    fn test_margin_calculation() {
        let lev = Leverage::x10();

        // 持仓价值50000，10x杠杆，所需保证金5000
        let margin = lev.calculate_margin(50000.0);
        assert!((margin - 5000.0).abs() < 0.01);
    }

    #[test]
    fn test_max_position_calculation() {
        let lev = Leverage::x10();

        // 可用保证金5000，10x杠杆，最大持仓50000
        let max_pos = lev.calculate_max_position(5000.0);
        assert!((max_pos - 50000.0).abs() < 0.01);
    }

    #[test]
    fn test_common_leverages() {
        assert_eq!(Leverage::x1().value(), 1.0);
        assert_eq!(Leverage::x10().value(), 10.0);
        assert_eq!(Leverage::x100().value(), 100.0);
    }
}
