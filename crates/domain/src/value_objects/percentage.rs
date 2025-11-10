//! 百分比值对象

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PercentageError {
    #[error("百分比超出范围: {0} (允许范围: 0-100)")]
    OutOfRange(f64),

    #[error("百分比无效: {0}")]
    Invalid(String),
}

/// 百分比值对象
///
/// 业务规则:
/// - 范围: 0.0 - 100.0
/// - 用于表示比率、收益率、回撤等
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Percentage(f64);

impl Percentage {
    /// 创建百分比 (带验证)
    ///
    /// 参数: 0-100的值 (如 50.0 表示50%)
    pub fn new(value: f64) -> Result<Self, PercentageError> {
        if value < 0.0 || value > 100.0 {
            return Err(PercentageError::OutOfRange(value));
        }

        if !value.is_finite() {
            return Err(PercentageError::Invalid("百分比必须是有限数".to_string()));
        }

        Ok(Self(value))
    }

    /// 从比率创建百分比
    ///
    /// 参数: 0-1的比率 (如 0.5 表示50%)
    pub fn from_ratio(ratio: f64) -> Result<Self, PercentageError> {
        Self::new(ratio * 100.0)
    }

    /// 常用百分比
    pub fn zero() -> Self {
        Self(0.0)
    }
    pub fn fifty() -> Self {
        Self(50.0)
    }
    pub fn hundred() -> Self {
        Self(100.0)
    }

    /// 获取百分比值 (0-100)
    pub fn value(&self) -> f64 {
        self.0
    }

    /// 获取比率值 (0-1)
    pub fn as_ratio(&self) -> f64 {
        self.0 / 100.0
    }

    /// 判断是否为零
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }

    /// 判断是否为100%
    pub fn is_full(&self) -> bool {
        self.0 == 100.0
    }

    /// 计算百分比对应的金额
    pub fn of(&self, amount: f64) -> f64 {
        amount * self.as_ratio()
    }
}

impl fmt::Display for Percentage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}%", self.0)
    }
}

impl PartialOrd for Percentage {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_percentage() {
        let pct = Percentage::new(50.0).unwrap();
        assert_eq!(pct.value(), 50.0);
        assert_eq!(pct.as_ratio(), 0.5);
    }

    #[test]
    fn test_from_ratio() {
        let pct = Percentage::from_ratio(0.75).unwrap();
        assert_eq!(pct.value(), 75.0);
    }

    #[test]
    fn test_invalid_percentage() {
        // 负数
        assert!(Percentage::new(-10.0).is_err());

        // 超过100
        assert!(Percentage::new(150.0).is_err());
    }

    #[test]
    fn test_percentage_of() {
        let pct = Percentage::new(25.0).unwrap();

        // 25% of 1000 = 250
        assert!((pct.of(1000.0) - 250.0).abs() < 0.01);
    }

    #[test]
    fn test_common_percentages() {
        assert!(Percentage::zero().is_zero());
        assert_eq!(Percentage::fifty().value(), 50.0);
        assert!(Percentage::hundred().is_full());
    }

    #[test]
    fn test_display() {
        let pct = Percentage::new(75.5).unwrap();
        assert_eq!(format!("{}", pct), "75.50%");
    }
}
