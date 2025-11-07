//! 策略框架类型定义
//! 
//! 提供策略特有的类型定义

use serde::{Deserialize, Serialize};
use rust_quant_domain::OrderSide;

// ⭐ 类型别名：统一命名
pub use rust_quant_domain::BasicRiskConfig as BasicRiskStrategyConfig;
pub use rust_quant_domain::BacktestResult as BackTestResult;

/// 交易方向（策略层使用）
/// 
/// 提供更语义化的方向命名
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TradeSide {
    /// 做多 / 买入
    #[default]
    Long,
    /// 做空 / 卖出
    Short,
}

impl TradeSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeSide::Long => "long",
            TradeSide::Short => "short",
        }
    }
    
    /// 转换为 OrderSide
    pub fn to_order_side(&self) -> OrderSide {
        match self {
            TradeSide::Long => OrderSide::Buy,
            TradeSide::Short => OrderSide::Sell,
        }
    }
    
    /// 从 OrderSide 转换
    pub fn from_order_side(side: OrderSide) -> Self {
        match side {
            OrderSide::Buy => TradeSide::Long,
            OrderSide::Sell => TradeSide::Short,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_side_conversion() {
        assert_eq!(TradeSide::Long.to_order_side(), OrderSide::Buy);
        assert_eq!(TradeSide::Short.to_order_side(), OrderSide::Sell);
        
        assert_eq!(TradeSide::from_order_side(OrderSide::Buy), TradeSide::Long);
        assert_eq!(TradeSide::from_order_side(OrderSide::Sell), TradeSide::Short);
    }
}

