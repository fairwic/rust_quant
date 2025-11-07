//! 订单相关枚举

use serde::{Deserialize, Serialize};

/// 订单方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    /// 买入 / 做多
    Buy,
    /// 卖出 / 做空
    Sell,
}

impl OrderSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderSide::Buy => "buy",
            OrderSide::Sell => "sell",
        }
    }
    
    /// 反向
    pub fn opposite(&self) -> Self {
        match self {
            OrderSide::Buy => OrderSide::Sell,
            OrderSide::Sell => OrderSide::Buy,
        }
    }
}

/// 订单类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    /// 限价单
    Limit,
    /// 市价单
    Market,
    /// 止损单
    StopLoss,
    /// 止盈单
    TakeProfit,
    /// 追踪止损
    TrailingStop,
}

impl OrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderType::Limit => "limit",
            OrderType::Market => "market",
            OrderType::StopLoss => "stop_loss",
            OrderType::TakeProfit => "take_profit",
            OrderType::TrailingStop => "trailing_stop",
        }
    }
}

/// 订单状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    /// 待提交
    Pending,
    /// 已提交
    Submitted,
    /// 部分成交
    PartiallyFilled,
    /// 完全成交
    Filled,
    /// 已取消
    Cancelled,
    /// 已拒绝
    Rejected,
    /// 已过期
    Expired,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Pending => "pending",
            OrderStatus::Submitted => "submitted",
            OrderStatus::PartiallyFilled => "partially_filled",
            OrderStatus::Filled => "filled",
            OrderStatus::Cancelled => "cancelled",
            OrderStatus::Rejected => "rejected",
            OrderStatus::Expired => "expired",
        }
    }
    
    /// 是否为终态
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderStatus::Filled 
            | OrderStatus::Cancelled 
            | OrderStatus::Rejected 
            | OrderStatus::Expired
        )
    }
    
    /// 是否可以取消
    pub fn can_cancel(&self) -> bool {
        matches!(
            self,
            OrderStatus::Pending 
            | OrderStatus::Submitted 
            | OrderStatus::PartiallyFilled
        )
    }
}

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    /// 多头
    Long,
    /// 空头
    Short,
    /// 双向持仓 (用于支持双向持仓的交易所)
    Both,
}

impl PositionSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            PositionSide::Long => "long",
            PositionSide::Short => "short",
            PositionSide::Both => "both",
        }
    }
    
    pub fn from_order_side(side: OrderSide) -> Self {
        match side {
            OrderSide::Buy => PositionSide::Long,
            OrderSide::Sell => PositionSide::Short,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_order_side_opposite() {
        assert_eq!(OrderSide::Buy.opposite(), OrderSide::Sell);
        assert_eq!(OrderSide::Sell.opposite(), OrderSide::Buy);
    }
    
    #[test]
    fn test_order_status_terminal() {
        assert!(OrderStatus::Filled.is_terminal());
        assert!(OrderStatus::Cancelled.is_terminal());
        assert!(!OrderStatus::Pending.is_terminal());
    }
    
    #[test]
    fn test_order_status_can_cancel() {
        assert!(OrderStatus::Pending.can_cancel());
        assert!(OrderStatus::Submitted.can_cancel());
        assert!(!OrderStatus::Filled.can_cancel());
    }
}


