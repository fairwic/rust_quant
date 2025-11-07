//! 持仓实体 (Position Aggregate Root)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::value_objects::{Price, Volume};
use crate::enums::{PositionSide, OrderSide};

#[derive(Error, Debug)]
pub enum PositionError {
    #[error("持仓已平仓，无法修改")]
    PositionClosed,
    
    #[error("持仓状态不允许此操作: {0}")]
    InvalidStateTransition(String),
    
    #[error("持仓参数无效: {0}")]
    InvalidParameter(String),
    
    #[error("保证金不足")]
    InsufficientMargin,
}

/// 保证金模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarginMode {
    /// 全仓
    Cross,
    /// 逐仓
    Isolated,
}

/// 持仓状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionStatus {
    /// 持仓中
    Open,
    /// 已平仓
    Closed,
    /// 部分平仓
    PartialClosed,
}

/// 持仓实体 - 聚合根
/// 
/// 代表一个持仓位置，包含持仓信息、盈亏计算等
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// 持仓ID
    pub id: String,
    
    /// 交易对符号
    pub symbol: String,
    
    /// 持仓方向 (Long/Short)
    pub side: PositionSide,
    
    /// 持仓数量
    pub quantity: Volume,
    
    /// 可用数量 (扣除挂单占用)
    pub available_quantity: Volume,
    
    /// 平均开仓价
    pub entry_price: Price,
    
    /// 当前价格
    pub current_price: Price,
    
    /// 未实现盈亏
    pub unrealized_pnl: f64,
    
    /// 已实现盈亏
    pub realized_pnl: f64,
    
    /// 未实现盈亏率
    pub unrealized_pnl_ratio: f64,
    
    /// 杠杆倍数
    pub leverage: f64,
    
    /// 保证金模式
    pub margin_mode: MarginMode,
    
    /// 保证金金额
    pub margin: f64,
    
    /// 持仓状态
    pub status: PositionStatus,
    
    /// 开仓时间
    pub opened_at: DateTime<Utc>,
    
    /// 更新时间
    pub updated_at: DateTime<Utc>,
    
    /// 平仓时间 (可选)
    pub closed_at: Option<DateTime<Utc>>,
}

impl Position {
    /// 创建新持仓
    pub fn new(
        id: String,
        symbol: String,
        side: PositionSide,
        quantity: Volume,
        entry_price: Price,
        leverage: f64,
        margin_mode: MarginMode,
    ) -> Result<Self, PositionError> {
        if quantity.is_zero() {
            return Err(PositionError::InvalidParameter("持仓数量不能为零".to_string()));
        }
        
        if leverage <= 0.0 || leverage > 125.0 {
            return Err(PositionError::InvalidParameter(
                format!("杠杆倍数无效: {}", leverage)
            ));
        }
        
        let now = Utc::now();
        let margin = (entry_price.value() * quantity.value()) / leverage;
        
        Ok(Self {
            id,
            symbol,
            side,
            quantity,
            available_quantity: quantity,
            entry_price,
            current_price: entry_price,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            unrealized_pnl_ratio: 0.0,
            leverage,
            margin_mode,
            margin,
            status: PositionStatus::Open,
            opened_at: now,
            updated_at: now,
            closed_at: None,
        })
    }
    
    /// 更新当前价格并计算盈亏
    pub fn update_price(&mut self, new_price: Price) {
        self.current_price = new_price;
        self.calculate_pnl();
        self.updated_at = Utc::now();
    }
    
    /// 计算盈亏
    pub fn calculate_pnl(&mut self) {
        let price_diff = match self.side {
            PositionSide::Long => {
                self.current_price.value() - self.entry_price.value()
            }
            PositionSide::Short => {
                self.entry_price.value() - self.current_price.value()
            }
            PositionSide::Both => 0.0, // 双向持仓不计算
        };
        
        self.unrealized_pnl = price_diff * self.quantity.value();
        
        if self.margin > 0.0 {
            self.unrealized_pnl_ratio = (self.unrealized_pnl / self.margin) * 100.0;
        }
    }
    
    /// 部分平仓
    pub fn close_partial(&mut self, quantity: Volume) -> Result<(), PositionError> {
        if self.status == PositionStatus::Closed {
            return Err(PositionError::PositionClosed);
        }
        
        if quantity > self.quantity {
            return Err(PositionError::InvalidParameter(
                "平仓数量超过持仓数量".to_string()
            ));
        }
        
        // 计算已实现盈亏
        let ratio = quantity.value() / self.quantity.value();
        self.realized_pnl += self.unrealized_pnl * ratio;
        
        // 更新持仓数量
        self.quantity = Volume::new(self.quantity.value() - quantity.value())
            .map_err(|e| PositionError::InvalidParameter(e.to_string()))?;
        
        if self.quantity.is_zero() {
            self.status = PositionStatus::Closed;
            self.closed_at = Some(Utc::now());
        } else {
            self.status = PositionStatus::PartialClosed;
        }
        
        self.calculate_pnl();
        self.updated_at = Utc::now();
        
        Ok(())
    }
    
    /// 完全平仓
    pub fn close(&mut self) -> Result<(), PositionError> {
        if self.status == PositionStatus::Closed {
            return Err(PositionError::PositionClosed);
        }
        
        self.realized_pnl += self.unrealized_pnl;
        self.unrealized_pnl = 0.0;
        self.unrealized_pnl_ratio = 0.0;
        self.status = PositionStatus::Closed;
        self.closed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        
        Ok(())
    }
    
    /// 判断是否盈利
    pub fn is_profitable(&self) -> bool {
        self.unrealized_pnl > 0.0
    }
    
    /// 获取总盈亏
    pub fn total_pnl(&self) -> f64 {
        self.realized_pnl + self.unrealized_pnl
    }
    
    /// 获取总盈亏率
    pub fn total_pnl_ratio(&self) -> f64 {
        if self.margin == 0.0 {
            return 0.0;
        }
        (self.total_pnl() / self.margin) * 100.0
    }
    
    /// 获取持仓价值
    pub fn position_value(&self) -> f64 {
        self.current_price.value() * self.quantity.value()
    }
    
    /// 获取相反方向的OrderSide (用于平仓)
    pub fn close_side(&self) -> OrderSide {
        match self.side {
            PositionSide::Long => OrderSide::Sell,
            PositionSide::Short => OrderSide::Buy,
            PositionSide::Both => OrderSide::Buy, // 默认
        }
    }
    
    /// 判断是否应该止损
    pub fn should_stop_loss(&self, max_loss_percent: f64) -> bool {
        self.unrealized_pnl_ratio < -max_loss_percent
    }
    
    /// 判断是否应该止盈
    pub fn should_take_profit(&self, target_profit_percent: f64) -> bool {
        self.unrealized_pnl_ratio >= target_profit_percent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_position() -> Position {
        Position::new(
            "POS-001".to_string(),
            "BTC-USDT".to_string(),
            PositionSide::Long,
            Volume::new(1.0).unwrap(),
            Price::new(50000.0).unwrap(),
            10.0,  // 10x杠杆
            MarginMode::Cross,
        ).unwrap()
    }
    
    #[test]
    fn test_position_creation() {
        let pos = create_test_position();
        assert_eq!(pos.side, PositionSide::Long);
        assert_eq!(pos.status, PositionStatus::Open);
        assert_eq!(pos.leverage, 10.0);
    }
    
    #[test]
    fn test_pnl_calculation() {
        let mut pos = create_test_position();
        
        // 价格上涨10%
        pos.update_price(Price::new(55000.0).unwrap());
        
        assert!(pos.is_profitable());
        assert!(pos.unrealized_pnl > 0.0);
    }
    
    #[test]
    fn test_position_close() {
        let mut pos = create_test_position();
        
        // 更新价格
        pos.update_price(Price::new(51000.0).unwrap());
        
        // 平仓
        pos.close().unwrap();
        
        assert_eq!(pos.status, PositionStatus::Closed);
        assert!(pos.closed_at.is_some());
        assert!(pos.realized_pnl > 0.0);
    }
    
    #[test]
    fn test_partial_close() {
        let mut pos = create_test_position();
        pos.update_price(Price::new(51000.0).unwrap());
        
        // 部分平仓50%
        pos.close_partial(Volume::new(0.5).unwrap()).unwrap();
        
        assert_eq!(pos.status, PositionStatus::PartialClosed);
        assert_eq!(pos.quantity.value(), 0.5);
    }
    
    #[test]
    fn test_risk_checks() {
        let mut pos = create_test_position();
        
        // 亏损5%
        pos.update_price(Price::new(47500.0).unwrap());
        
        assert!(pos.should_stop_loss(3.0)); // 超过3%止损线
        assert!(!pos.is_profitable());
    }
}

