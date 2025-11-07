//! 成交量值对象 - 带业务验证的成交量类型

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VolumeError {
    #[error("成交量不能为负: {0}")]
    NegativeVolume(f64),
    
    #[error("成交量无效: {0}")]
    InvalidVolume(String),
}

/// 成交量值对象
/// 
/// 业务规则:
/// - 成交量必须非负
/// - 成交量为0表示无交易
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Volume(f64);

impl Volume {
    /// 创建新的成交量 (带验证)
    pub fn new(value: f64) -> Result<Self, VolumeError> {
        if value < 0.0 {
            return Err(VolumeError::NegativeVolume(value));
        }
        
        if !value.is_finite() {
            return Err(VolumeError::InvalidVolume("成交量必须是有限数".to_string()));
        }
        
        Ok(Self(value))
    }
    
    /// 创建零成交量
    pub fn zero() -> Self {
        Self(0.0)
    }
    
    /// 获取成交量值
    pub fn value(&self) -> f64 {
        self.0
    }
    
    /// 判断是否为零
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }
    
    /// 成交量相加
    pub fn add(&self, other: &Volume) -> Result<Volume, VolumeError> {
        Volume::new(self.0 + other.0)
    }
    
    /// 计算成交量变化比率
    pub fn change_ratio(&self, other: &Volume) -> f64 {
        if self.0 == 0.0 {
            if other.0 == 0.0 {
                return 0.0;
            }
            return f64::INFINITY;
        }
        other.0 / self.0
    }
}

impl fmt::Display for Volume {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.4}", self.0)
    }
}

impl PartialOrd for Volume {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_valid_volume() {
        let volume = Volume::new(1000.0).unwrap();
        assert_eq!(volume.value(), 1000.0);
    }
    
    #[test]
    fn test_zero_volume() {
        let volume = Volume::zero();
        assert!(volume.is_zero());
    }
    
    #[test]
    fn test_negative_volume() {
        let result = Volume::new(-100.0);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_change_ratio() {
        let v1 = Volume::new(100.0).unwrap();
        let v2 = Volume::new(200.0).unwrap();
        
        let ratio = v1.change_ratio(&v2);
        assert!((ratio - 2.0).abs() < 0.001);
    }
}


