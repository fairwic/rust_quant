//! 价格值对象 - 带业务验证的价格类型

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PriceError {
    #[error("价格不能为负: {0}")]
    NegativePrice(f64),
    
    #[error("价格不能为零")]
    ZeroPrice,
    
    #[error("价格无效: {0}")]
    InvalidPrice(String),
}

/// 价格值对象
/// 
/// 业务规则:
/// - 价格必须为正数
/// - 价格精度最多8位小数
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Price(f64);

impl Price {
    /// 创建新的价格 (带验证)
    pub fn new(value: f64) -> Result<Self, PriceError> {
        if value < 0.0 {
            return Err(PriceError::NegativePrice(value));
        }
        
        if value == 0.0 {
            return Err(PriceError::ZeroPrice);
        }
        
        if !value.is_finite() {
            return Err(PriceError::InvalidPrice("价格必须是有限数".to_string()));
        }
        
        Ok(Self(value))
    }
    
    /// 创建零价格 (用于特殊场景,如市价单)
    pub fn zero() -> Self {
        Self(0.0)
    }
    
    /// 获取价格值
    pub fn value(&self) -> f64 {
        self.0
    }
    
    /// 价格相加
    pub fn add(&self, other: &Price) -> Result<Price, PriceError> {
        Price::new(self.0 + other.0)
    }
    
    /// 价格相减
    pub fn subtract(&self, other: &Price) -> Result<Price, PriceError> {
        Price::new(self.0 - other.0)
    }
    
    /// 计算价格变化百分比
    pub fn percentage_change(&self, other: &Price) -> f64 {
        if self.0 == 0.0 {
            return 0.0;
        }
        ((other.0 - self.0) / self.0) * 100.0
    }
    
    /// 判断是否为零
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.8}", self.0)
    }
}

impl PartialOrd for Price {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_valid_price() {
        let price = Price::new(100.5).unwrap();
        assert_eq!(price.value(), 100.5);
    }
    
    #[test]
    fn test_negative_price() {
        let result = Price::new(-10.0);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_zero_price() {
        let result = Price::new(0.0);
        assert!(result.is_err());
        
        let zero = Price::zero();
        assert!(zero.is_zero());
    }
    
    #[test]
    fn test_price_operations() {
        let p1 = Price::new(100.0).unwrap();
        let p2 = Price::new(50.0).unwrap();
        
        let sum = p1.add(&p2).unwrap();
        assert_eq!(sum.value(), 150.0);
        
        let diff = p1.subtract(&p2).unwrap();
        assert_eq!(diff.value(), 50.0);
    }
    
    #[test]
    fn test_percentage_change() {
        let p1 = Price::new(100.0).unwrap();
        let p2 = Price::new(110.0).unwrap();
        
        let change = p1.percentage_change(&p2);
        assert!((change - 10.0).abs() < 0.001);
    }
}


