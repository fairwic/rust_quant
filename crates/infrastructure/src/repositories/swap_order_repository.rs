//! 合约订单仓储实现
//!
//! 对应数据库表 `swap_orders`

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use sqlx::{FromRow, MySql, Pool};
use tracing::{debug, info};

use rust_quant_domain::entities::SwapOrder;
use rust_quant_domain::traits::SwapOrderRepository;

/// 合约订单数据库实体
#[derive(Debug, Clone, FromRow)]
pub struct SwapOrderEntity {
    pub id: i32,
    pub strategy_id: i32,
    pub in_order_id: String,
    pub out_order_id: String,
    pub strategy_type: String,
    pub period: String,
    pub inst_id: String,
    pub side: String,
    pub pos_size: String,
    pub pos_side: String,
    pub tag: String,
    pub platform_type: String,
    pub detail: String,
    pub created_at: chrono::NaiveDateTime,
    pub update_at: Option<chrono::NaiveDateTime>,
}

impl SwapOrderEntity {
    /// 转换为领域实体
    pub fn to_domain(&self) -> SwapOrder {
        SwapOrder {
            id: Some(self.id),
            strategy_id: self.strategy_id,
            in_order_id: self.in_order_id.clone(),
            out_order_id: self.out_order_id.clone(),
            strategy_type: self.strategy_type.clone(),
            period: self.period.clone(),
            inst_id: self.inst_id.clone(),
            side: self.side.clone(),
            pos_size: self.pos_size.clone(),
            pos_side: self.pos_side.clone(),
            tag: self.tag.clone(),
            platform_type: self.platform_type.clone(),
            detail: self.detail.clone(),
            created_at: Utc.from_utc_datetime(&self.created_at),
            update_at: self.update_at.map(|dt| Utc.from_utc_datetime(&dt)),
        }
    }

    /// 从领域实体创建数据库实体
    pub fn from_domain(order: &SwapOrder) -> Self {
        Self {
            id: order.id.unwrap_or(0),
            strategy_id: order.strategy_id,
            in_order_id: order.in_order_id.clone(),
            out_order_id: order.out_order_id.clone(),
            strategy_type: order.strategy_type.clone(),
            period: order.period.clone(),
            inst_id: order.inst_id.clone(),
            side: order.side.clone(),
            pos_size: order.pos_size.clone(),
            pos_side: order.pos_side.clone(),
            tag: order.tag.clone(),
            platform_type: order.platform_type.clone(),
            detail: order.detail.clone(),
            created_at: order.created_at.naive_utc(),
            update_at: order.update_at.map(|dt| dt.naive_utc()),
        }
    }
}

/// 合约订单仓储实现 (基于 sqlx)
pub struct SqlxSwapOrderRepository {
    pool: Pool<MySql>,
}

impl SqlxSwapOrderRepository {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }

    /// 获取数据库连接池引用
    pub fn pool(&self) -> &Pool<MySql> {
        &self.pool
    }
}

#[async_trait]
impl SwapOrderRepository for SqlxSwapOrderRepository {
    async fn find_by_id(&self, id: i32) -> Result<Option<SwapOrder>> {
        debug!("查询合约订单: id={}", id);

        let entity = sqlx::query_as::<_, SwapOrderEntity>(
            r#"SELECT id, strategy_id, in_order_id, out_order_id, strategy_type,
                      period, inst_id, side, pos_size, pos_side, tag,
                      platform_type, detail, created_at, update_at
               FROM swap_orders WHERE id = ? LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(entity.map(|e| e.to_domain()))
    }

    async fn find_by_in_order_id(&self, in_order_id: &str) -> Result<Option<SwapOrder>> {
        debug!("根据内部订单ID查询: in_order_id={}", in_order_id);

        let entity = sqlx::query_as::<_, SwapOrderEntity>(
            r#"SELECT id, strategy_id, in_order_id, out_order_id, strategy_type,
                      period, inst_id, side, pos_size, pos_side, tag,
                      platform_type, detail, created_at, update_at
               FROM swap_orders WHERE in_order_id = ? LIMIT 1"#,
        )
        .bind(in_order_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(entity.map(|e| e.to_domain()))
    }

    async fn find_by_out_order_id(&self, out_order_id: &str) -> Result<Option<SwapOrder>> {
        debug!("根据外部订单ID查询: out_order_id={}", out_order_id);

        let entity = sqlx::query_as::<_, SwapOrderEntity>(
            r#"SELECT id, strategy_id, in_order_id, out_order_id, strategy_type,
                      period, inst_id, side, pos_size, pos_side, tag,
                      platform_type, detail, created_at, update_at
               FROM swap_orders WHERE out_order_id = ? LIMIT 1"#,
        )
        .bind(out_order_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(entity.map(|e| e.to_domain()))
    }

    async fn find_by_inst_id(&self, inst_id: &str, limit: Option<i32>) -> Result<Vec<SwapOrder>> {
        debug!("查询交易对订单: inst_id={}, limit={:?}", inst_id, limit);

        let limit = limit.unwrap_or(100);
        let entities = sqlx::query_as::<_, SwapOrderEntity>(
            r#"SELECT id, strategy_id, in_order_id, out_order_id, strategy_type,
                      period, inst_id, side, pos_size, pos_side, tag,
                      platform_type, detail, created_at, update_at
               FROM swap_orders WHERE inst_id = ? ORDER BY created_at DESC LIMIT ?"#,
        )
        .bind(inst_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }

    async fn find_pending_order(
        &self,
        inst_id: &str,
        period: &str,
        side: &str,
        pos_side: &str,
    ) -> Result<Vec<SwapOrder>> {
        debug!(
            "查询待处理订单: inst_id={}, period={}, side={}, pos_side={}",
            inst_id, period, side, pos_side
        );

        // 查询最近5分钟内相同条件的订单（用于幂等性检查）
        let entities = sqlx::query_as::<_, SwapOrderEntity>(
            r#"SELECT id, strategy_id, in_order_id, out_order_id, strategy_type,
                      period, inst_id, side, pos_size, pos_side, tag,
                      platform_type, detail, created_at, update_at
               FROM swap_orders
               WHERE inst_id = ? AND period = ? AND side = ? AND pos_side = ?
                 AND created_at > DATE_SUB(NOW(), INTERVAL 5 MINUTE)
               ORDER BY created_at DESC"#,
        )
        .bind(inst_id)
        .bind(period)
        .bind(side)
        .bind(pos_side)
        .fetch_all(&self.pool)
        .await?;

        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }

    async fn find_latest_by_strategy_inst_period_pos_side(
        &self,
        strategy_id: i32,
        inst_id: &str,
        period: &str,
        pos_side: &str,
    ) -> Result<Option<SwapOrder>> {
        debug!(
            "查询策略最新订单: strategy_id={}, inst_id={}, period={}, pos_side={}",
            strategy_id, inst_id, period, pos_side
        );

        let entity = sqlx::query_as::<_, SwapOrderEntity>(
            r#"SELECT id, strategy_id, in_order_id, out_order_id, strategy_type,
                      period, inst_id, side, pos_size, pos_side, tag,
                      platform_type, detail, created_at, update_at
               FROM swap_orders
               WHERE strategy_id = ? AND inst_id = ? AND period = ? AND pos_side = ?
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(strategy_id)
        .bind(inst_id)
        .bind(period)
        .bind(pos_side)
        .fetch_optional(&self.pool)
        .await?;

        Ok(entity.map(|e| e.to_domain()))
    }

    async fn save(&self, order: &SwapOrder) -> Result<i32> {
        info!(
            "保存合约订单: in_order_id={}, inst_id={}, side={}, pos_side={}",
            order.in_order_id, order.inst_id, order.side, order.pos_side
        );

        let result = sqlx::query(
            r#"INSERT INTO swap_orders
               (strategy_id, in_order_id, out_order_id, strategy_type, period,
                inst_id, side, pos_size, pos_side, tag, platform_type, detail)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(order.strategy_id)
        .bind(&order.in_order_id)
        .bind(&order.out_order_id)
        .bind(&order.strategy_type)
        .bind(&order.period)
        .bind(&order.inst_id)
        .bind(&order.side)
        .bind(&order.pos_size)
        .bind(&order.pos_side)
        .bind(&order.tag)
        .bind(&order.platform_type)
        .bind(&order.detail)
        .execute(&self.pool)
        .await?;

        let id = result.last_insert_id() as i32;
        info!("合约订单保存成功: id={}", id);

        Ok(id)
    }

    async fn update(&self, order: &SwapOrder) -> Result<()> {
        let id = order.id.ok_or_else(|| anyhow::anyhow!("订单ID不能为空"))?;

        debug!("更新合约订单: id={}", id);

        sqlx::query(
            r#"UPDATE swap_orders SET
               out_order_id = ?, pos_size = ?, detail = ?, update_at = NOW()
               WHERE id = ?"#,
        )
        .bind(&order.out_order_id)
        .bind(&order.pos_size)
        .bind(&order.detail)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_strategy_and_time(
        &self,
        strategy_id: i32,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<SwapOrder>> {
        debug!(
            "查询策略订单: strategy_id={}, start={}, end={}",
            strategy_id, start_time, end_time
        );

        let start_dt = DateTime::from_timestamp_millis(start_time)
            .unwrap_or_else(Utc::now)
            .naive_utc();
        let end_dt = DateTime::from_timestamp_millis(end_time)
            .unwrap_or_else(Utc::now)
            .naive_utc();

        let entities = sqlx::query_as::<_, SwapOrderEntity>(
            r#"SELECT id, strategy_id, in_order_id, out_order_id, strategy_type,
                      period, inst_id, side, pos_size, pos_side, tag,
                      platform_type, detail, created_at, update_at
               FROM swap_orders
               WHERE strategy_id = ? AND created_at BETWEEN ? AND ?
               ORDER BY created_at DESC"#,
        )
        .bind(strategy_id)
        .bind(start_dt)
        .bind(end_dt)
        .fetch_all(&self.pool)
        .await?;

        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_to_domain() {
        let entity = SwapOrderEntity {
            id: 1,
            strategy_id: 100,
            in_order_id: "test_in_123".to_string(),
            out_order_id: "test_out_456".to_string(),
            strategy_type: "nwe".to_string(),
            period: "5m".to_string(),
            inst_id: "BTC-USDT-SWAP".to_string(),
            side: "buy".to_string(),
            pos_size: "0.1".to_string(),
            pos_side: "long".to_string(),
            tag: "test_tag".to_string(),
            platform_type: "okx".to_string(),
            detail: "{}".to_string(),
            created_at: chrono::Utc::now().naive_utc(),
            update_at: None,
        };

        let domain = entity.to_domain();
        assert_eq!(domain.id, Some(1));
        assert_eq!(domain.strategy_id, 100);
        assert_eq!(domain.inst_id, "BTC-USDT-SWAP");
    }
}
