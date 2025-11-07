//! 订单实体 (Order Aggregate Root)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::value_objects::{Price, Volume};
use crate::enums::{OrderSide, OrderType, OrderStatus};

#[derive(Error, Debug)]
pub enum OrderError {
    #[error("订单已完成，无法修改")]
    OrderCompleted,
    
    #[error("订单状态不允许此操作: {0}")]
    InvalidStateTransition(String),
    
    #[error("订单参数无效: {0}")]
    InvalidParameter(String),
}

/// 订单实体 - 聚合根
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    /// 订单ID
    pub id: String,
    
    /// 交易对符号
    pub symbol: String,
    
    /// 订单方向
    pub side: OrderSide,
    
    /// 订单类型
    pub order_type: OrderType,
    
    /// 订单价格
    pub price: Price,
    
    /// 订单数量
    pub quantity: Volume,
    
    /// 已成交数量
    pub filled_quantity: Volume,
    
    /// 订单状态
    pub status: OrderStatus,
    
    /// 创建时间
    pub created_at: DateTime<Utc>,
    
    /// 更新时间
    pub updated_at: DateTime<Utc>,
    
    /// 完成时间 (可选)
    pub completed_at: Option<DateTime<Utc>>,
    
    /// 平均成交价 (可选)
    pub average_price: Option<Price>,
    
    /// 手续费
    pub fee: f64,
    
    /// 备注
    pub notes: Option<String>,
}

impl Order {
    /// 创建新订单
    pub fn new(
        id: String,
        symbol: String,
        side: OrderSide,
        order_type: OrderType,
        price: Price,
        quantity: Volume,
    ) -> Result<Self, OrderError> {
        if quantity.is_zero() {
            return Err(OrderError::InvalidParameter("订单数量不能为零".to_string()));
        }
        
        let now = Utc::now();
        
        Ok(Self {
            id,
            symbol,
            side,
            order_type,
            price,
            quantity,
            filled_quantity: Volume::zero(),
            status: OrderStatus::Pending,
            created_at: now,
            updated_at: now,
            completed_at: None,
            average_price: None,
            fee: 0.0,
            notes: None,
        })
    }
    
    /// 提交订单
    pub fn submit(&mut self) -> Result<(), OrderError> {
        if self.status != OrderStatus::Pending {
            return Err(OrderError::InvalidStateTransition(
                format!("只能从待提交状态提交订单，当前状态: {:?}", self.status)
            ));
        }
        
        self.status = OrderStatus::Submitted;
        self.updated_at = Utc::now();
        Ok(())
    }
    
    /// 部分成交
    pub fn partially_fill(&mut self, filled: Volume, price: Price) -> Result<(), OrderError> {
        if self.status.is_terminal() {
            return Err(OrderError::OrderCompleted);
        }
        
        self.filled_quantity = self.filled_quantity.add(&filled)
            .map_err(|e| OrderError::InvalidParameter(e.to_string()))?;
        
        // 更新平均成交价
        self.update_average_price(price);
        
        if self.filled_quantity >= self.quantity {
            self.status = OrderStatus::Filled;
            self.completed_at = Some(Utc::now());
        } else {
            self.status = OrderStatus::PartiallyFilled;
        }
        
        self.updated_at = Utc::now();
        Ok(())
    }
    
    /// 完全成交
    pub fn fill(&mut self, price: Price) -> Result<(), OrderError> {
        if self.status.is_terminal() {
            return Err(OrderError::OrderCompleted);
        }
        
        self.filled_quantity = self.quantity;
        self.average_price = Some(price);
        self.status = OrderStatus::Filled;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        
        Ok(())
    }
    
    /// 取消订单
    pub fn cancel(&mut self) -> Result<(), OrderError> {
        if !self.status.can_cancel() {
            return Err(OrderError::InvalidStateTransition(
                format!("订单状态 {:?} 不允许取消", self.status)
            ));
        }
        
        self.status = OrderStatus::Cancelled;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        
        Ok(())
    }
    
    /// 拒绝订单
    pub fn reject(&mut self, reason: String) -> Result<(), OrderError> {
        if self.status.is_terminal() {
            return Err(OrderError::OrderCompleted);
        }
        
        self.status = OrderStatus::Rejected;
        self.notes = Some(reason);
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        
        Ok(())
    }
    
    /// 更新平均成交价
    fn update_average_price(&mut self, new_price: Price) {
        match self.average_price {
            Some(avg) => {
                let total_value = avg.value() * self.filled_quantity.value() 
                    + new_price.value() * self.quantity.value();
                let total_qty = self.filled_quantity.value() + self.quantity.value();
                
                if total_qty > 0.0 {
                    self.average_price = Price::new(total_value / total_qty).ok();
                }
            }
            None => {
                self.average_price = Some(new_price);
            }
        }
    }
    
    /// 获取剩余数量
    pub fn remaining_quantity(&self) -> Volume {
        Volume::new(self.quantity.value() - self.filled_quantity.value())
            .unwrap_or(Volume::zero())
    }
    
    /// 计算总价值
    pub fn total_value(&self) -> f64 {
        self.price.value() * self.quantity.value()
    }
    
    /// 计算已成交价值
    pub fn filled_value(&self) -> f64 {
        self.average_price
            .map(|p| p.value() * self.filled_quantity.value())
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_order() -> Order {
        Order::new(
            "ORDER-001".to_string(),
            "BTC-USDT".to_string(),
            OrderSide::Buy,
            OrderType::Limit,
            Price::new(50000.0).unwrap(),
            Volume::new(1.0).unwrap(),
        ).unwrap()
    }
    
    #[test]
    fn test_order_creation() {
        let order = create_test_order();
        assert_eq!(order.status, OrderStatus::Pending);
        assert_eq!(order.side, OrderSide::Buy);
    }
    
    #[test]
    fn test_order_submit() {
        let mut order = create_test_order();
        order.submit().unwrap();
        assert_eq!(order.status, OrderStatus::Submitted);
    }
    
    #[test]
    fn test_order_fill() {
        let mut order = create_test_order();
        order.submit().unwrap();
        order.fill(Price::new(50000.0).unwrap()).unwrap();
        assert_eq!(order.status, OrderStatus::Filled);
        assert!(order.completed_at.is_some());
    }
    
    #[test]
    fn test_order_cancel() {
        let mut order = create_test_order();
        order.submit().unwrap();
        order.cancel().unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
    }
    
    #[test]
    fn test_cannot_cancel_filled_order() {
        let mut order = create_test_order();
        order.submit().unwrap();
        order.fill(Price::new(50000.0).unwrap()).unwrap();
        
        let result = order.cancel();
        assert!(result.is_err());
    }
}


