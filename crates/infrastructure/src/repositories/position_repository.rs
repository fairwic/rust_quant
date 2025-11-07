//! 持仓仓储实现

use async_trait::async_trait;
use anyhow::Result;
use sqlx::{MySql, Pool, FromRow};
use chrono::{DateTime, Utc};

use rust_quant_domain::{Position, PositionStatus, PositionSide, MarginMode};
use rust_quant_domain::{Price, Volume};
use rust_quant_domain::traits::PositionRepository;

/// 持仓数据库实体
#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct PositionEntity {
    pub id: String,
    pub symbol: String,
    pub side: String,  // long/short/both
    pub quantity: f64,
    pub available_quantity: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl_ratio: f64,
    pub leverage: f64,
    pub margin_mode: String,  // cross/isolated
    pub margin: f64,
    pub status: String,  // open/closed/partial_closed
    pub opened_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

impl PositionEntity {
    /// 转换为领域实体
    pub fn to_domain(&self) -> Result<Position> {
        let side = match self.side.as_str() {
            "long" => PositionSide::Long,
            "short" => PositionSide::Short,
            "both" => PositionSide::Both,
            _ => PositionSide::Long,
        };
        
        let margin_mode = match self.margin_mode.as_str() {
            "cross" => MarginMode::Cross,
            "isolated" => MarginMode::Isolated,
            _ => MarginMode::Cross,
        };
        
        let status = match self.status.as_str() {
            "open" => PositionStatus::Open,
            "closed" => PositionStatus::Closed,
            "partial_closed" => PositionStatus::PartialClosed,
            _ => PositionStatus::Open,
        };
        
        let mut position = Position::new(
            self.id.clone(),
            self.symbol.clone(),
            side,
            Volume::new(self.quantity)?,
            Price::new(self.entry_price)?,
            self.leverage,
            margin_mode,
        )?;
        
        // 更新其他字段
        position.available_quantity = Volume::new(self.available_quantity)?;
        position.current_price = Price::new(self.current_price)?;
        position.unrealized_pnl = self.unrealized_pnl;
        position.realized_pnl = self.realized_pnl;
        position.unrealized_pnl_ratio = self.unrealized_pnl_ratio;
        position.status = status;
        position.updated_at = self.updated_at;
        position.closed_at = self.closed_at;
        
        Ok(position)
    }
    
    /// 从领域实体转换
    pub fn from_domain(position: &Position) -> Self {
        Self {
            id: position.id.clone(),
            symbol: position.symbol.clone(),
            side: position.side.as_str().to_string(),
            quantity: position.quantity.value(),
            available_quantity: position.available_quantity.value(),
            entry_price: position.entry_price.value(),
            current_price: position.current_price.value(),
            unrealized_pnl: position.unrealized_pnl,
            realized_pnl: position.realized_pnl,
            unrealized_pnl_ratio: position.unrealized_pnl_ratio,
            leverage: position.leverage,
            margin_mode: match position.margin_mode {
                MarginMode::Cross => "cross".to_string(),
                MarginMode::Isolated => "isolated".to_string(),
            },
            margin: position.margin,
            status: match position.status {
                PositionStatus::Open => "open".to_string(),
                PositionStatus::Closed => "closed".to_string(),
                PositionStatus::PartialClosed => "partial_closed".to_string(),
            },
            opened_at: position.opened_at,
            updated_at: position.updated_at,
            closed_at: position.closed_at,
        }
    }
}

/// 基于sqlx的持仓仓储实现
pub struct SqlxPositionRepository {
    pool: Pool<MySql>,
}

impl SqlxPositionRepository {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PositionRepository for SqlxPositionRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Position>> {
        let entity = sqlx::query_as::<_, PositionEntity>(
            "SELECT * FROM positions WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        match entity {
            Some(e) => Ok(Some(e.to_domain()?)),
            None => Ok(None),
        }
    }
    
    async fn find_by_symbol(&self, symbol: &str) -> Result<Vec<Position>> {
        let entities = sqlx::query_as::<_, PositionEntity>(
            "SELECT * FROM positions WHERE symbol = ?"
        )
        .bind(symbol)
        .fetch_all(&self.pool)
        .await?;
        
        entities.into_iter()
            .map(|e| e.to_domain())
            .collect()
    }
    
    async fn find_open_positions(&self) -> Result<Vec<Position>> {
        let entities = sqlx::query_as::<_, PositionEntity>(
            "SELECT * FROM positions WHERE status = 'open'"
        )
        .fetch_all(&self.pool)
        .await?;
        
        entities.into_iter()
            .map(|e| e.to_domain())
            .collect()
    }
    
    async fn find_by_status(&self, status: PositionStatus) -> Result<Vec<Position>> {
        let status_str = match status {
            PositionStatus::Open => "open",
            PositionStatus::Closed => "closed",
            PositionStatus::PartialClosed => "partial_closed",
        };
        
        let entities = sqlx::query_as::<_, PositionEntity>(
            "SELECT * FROM positions WHERE status = ?"
        )
        .bind(status_str)
        .fetch_all(&self.pool)
        .await?;
        
        entities.into_iter()
            .map(|e| e.to_domain())
            .collect()
    }
    
    async fn save(&self, position: &Position) -> Result<()> {
        let entity = PositionEntity::from_domain(position);
        
        sqlx::query(
            "INSERT INTO positions 
             (id, symbol, side, quantity, available_quantity, entry_price, 
              current_price, unrealized_pnl, realized_pnl, unrealized_pnl_ratio,
              leverage, margin_mode, margin, status, opened_at, updated_at, closed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&entity.id)
        .bind(&entity.symbol)
        .bind(&entity.side)
        .bind(entity.quantity)
        .bind(entity.available_quantity)
        .bind(entity.entry_price)
        .bind(entity.current_price)
        .bind(entity.unrealized_pnl)
        .bind(entity.realized_pnl)
        .bind(entity.unrealized_pnl_ratio)
        .bind(entity.leverage)
        .bind(&entity.margin_mode)
        .bind(entity.margin)
        .bind(&entity.status)
        .bind(entity.opened_at)
        .bind(entity.updated_at)
        .bind(entity.closed_at)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    async fn update(&self, position: &Position) -> Result<()> {
        let entity = PositionEntity::from_domain(position);
        
        sqlx::query(
            "UPDATE positions 
             SET symbol = ?, side = ?, quantity = ?, available_quantity = ?,
                 entry_price = ?, current_price = ?, unrealized_pnl = ?, 
                 realized_pnl = ?, unrealized_pnl_ratio = ?, leverage = ?,
                 margin_mode = ?, margin = ?, status = ?, updated_at = ?, closed_at = ?
             WHERE id = ?"
        )
        .bind(&entity.symbol)
        .bind(&entity.side)
        .bind(entity.quantity)
        .bind(entity.available_quantity)
        .bind(entity.entry_price)
        .bind(entity.current_price)
        .bind(entity.unrealized_pnl)
        .bind(entity.realized_pnl)
        .bind(entity.unrealized_pnl_ratio)
        .bind(entity.leverage)
        .bind(&entity.margin_mode)
        .bind(entity.margin)
        .bind(&entity.status)
        .bind(entity.updated_at)
        .bind(entity.closed_at)
        .bind(&entity.id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM positions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}

