//! 合约订单仓储实现
//!
//! 对应数据库表 `swap_orders`
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use rust_quant_domain::entities::SwapOrder;
use rust_quant_domain::traits::SwapOrderRepository;
use sqlx::{FromRow, PgPool};
use tracing::{debug, info};
/// 合约订单数据库实体
#[derive(Debug, Clone, FromRow)]
pub struct SwapOrderEntity {
    /// 唯一标识。
    pub id: i32,
    /// 策略 ID。
    pub strategy_id: i32,
    /// inorder ID。
    pub in_order_id: String,
    /// outorder ID。
    pub out_order_id: String,
    /// 类型标识。
    pub strategy_type: String,
    /// 计算周期。
    pub period: String,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 交易方向。
    pub side: String,
    /// 数量数值。
    pub pos_size: String,
    /// pos方向，用于记录交易或执行状态。
    pub pos_side: String,
    /// 标签。
    pub tag: String,
    /// 类型标识。
    pub platform_type: String,
    /// 详情。
    pub detail: String,
    /// 创建时间。
    pub created_at: chrono::NaiveDateTime,
    /// 时间字段。
    pub update_at: Option<chrono::NaiveDateTime>,
}
impl SwapOrderEntity {
    /// 转换为领域实体
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
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
    /// 数据库连接池。
    pool: PgPool,
}
impl SqlxSwapOrderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
#[async_trait]
impl SwapOrderRepository for SqlxSwapOrderRepository {
    /// 封装当前函数，减少交易执行调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
    async fn find_by_id(&self, id: i32) -> Result<Option<SwapOrder>> {
        debug!("查询合约订单: id={}", id);
        let entity = sqlx::query_as::<_, SwapOrderEntity>(
            r#"SELECT id, strategy_id, in_order_id, out_order_id, strategy_type,
                      period, inst_id, side, pos_size, pos_side, tag,
                      platform_type, detail, created_at, update_at
               FROM swap_orders WHERE id = $1 LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(entity.map(|e| e.to_domain()))
    }
    /// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_by_in_order_id(&self, in_order_id: &str) -> Result<Option<SwapOrder>> {
        debug!("根据内部订单ID查询: in_order_id={}", in_order_id);
        let entity = sqlx::query_as::<_, SwapOrderEntity>(
            r#"SELECT id, strategy_id, in_order_id, out_order_id, strategy_type,
                      period, inst_id, side, pos_size, pos_side, tag,
                      platform_type, detail, created_at, update_at
               FROM swap_orders WHERE in_order_id = $1 LIMIT 1"#,
        )
        .bind(in_order_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(entity.map(|e| e.to_domain()))
    }
    /// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_by_out_order_id(&self, out_order_id: &str) -> Result<Option<SwapOrder>> {
        debug!("根据外部订单ID查询: out_order_id={}", out_order_id);
        let entity = sqlx::query_as::<_, SwapOrderEntity>(
            r#"SELECT id, strategy_id, in_order_id, out_order_id, strategy_type,
                      period, inst_id, side, pos_size, pos_side, tag,
                      platform_type, detail, created_at, update_at
               FROM swap_orders WHERE out_order_id = $1 LIMIT 1"#,
        )
        .bind(out_order_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(entity.map(|e| e.to_domain()))
    }
    /// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_by_inst_id(&self, inst_id: &str, limit: Option<i32>) -> Result<Vec<SwapOrder>> {
        debug!("查询交易对订单: inst_id={}, limit={:?}", inst_id, limit);
        let limit = limit.unwrap_or(100);
        let entities = sqlx::query_as::<_, SwapOrderEntity>(
            r#"SELECT id, strategy_id, in_order_id, out_order_id, strategy_type,
                      period, inst_id, side, pos_size, pos_side, tag,
                      platform_type, detail, created_at, update_at
               FROM swap_orders WHERE inst_id = $1 ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(inst_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }
    /// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
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
               WHERE inst_id = $1 AND period = $2 AND side = $3 AND pos_side = $4
                 AND created_at > NOW() - INTERVAL '5 minutes'
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
    /// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
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
               WHERE strategy_id = $1 AND inst_id = $2 AND period = $3 AND pos_side = $4
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
    /// 提供save的集中实现，避免交易执行调用方重复处理相同细节。
    async fn save(&self, order: &SwapOrder) -> Result<i32> {
        info!(
            "保存合约订单: in_order_id={}, inst_id={}, side={}, pos_side={}",
            order.in_order_id, order.inst_id, order.side, order.pos_side
        );
        let id = sqlx::query_scalar::<_, i32>(
            r#"INSERT INTO swap_orders
               (strategy_id, in_order_id, out_order_id, strategy_type, period,
                inst_id, side, pos_size, pos_side, tag, platform_type, detail)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
               RETURNING id"#,
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
        .fetch_one(&self.pool)
        .await?;
        info!("合约订单保存成功: id={}", id);
        Ok(id)
    }
    /// 执行更新步骤，串起交易执行需要的状态推进和错误处理。
    async fn update(&self, order: &SwapOrder) -> Result<()> {
        let id = order.id.ok_or_else(|| anyhow::anyhow!("订单ID不能为空"))?;
        debug!("更新合约订单: id={}", id);
        sqlx::query(
            r#"UPDATE swap_orders SET
               out_order_id = $1, pos_size = $2, detail = $3, update_at = NOW()
               WHERE id = $4"#,
        )
        .bind(&order.out_order_id)
        .bind(&order.pos_size)
        .bind(&order.detail)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
    /// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
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
               WHERE strategy_id = $1 AND created_at BETWEEN $2 AND $3
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
    /// 封装当前函数，减少交易执行调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
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
