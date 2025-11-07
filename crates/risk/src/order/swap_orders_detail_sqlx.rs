//! SwapOrdersDetail 实体 - sqlx 实现
//! 从 swap_orders_detail.rs 迁移 (rbatis → sqlx)

use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_quant_core::database::get_db_pool;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::{debug, info};

/// 订单详情实体
#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct SwapOrdersDetailEntity {
    pub id: Option<i64>,
    // 内部订单id  
    pub in_order_id: String,
    // 外部订单id
    pub out_order_id: String,
    // 策略id
    pub strategy_id: i64,
    // 策略类型
    pub strategy_type: String,
    // 周期
    pub period: String,
    // 交易对
    pub inst_id: String,
    // 方向 buy/sell
    pub side: String,
    // 持仓数量
    pub pos_size: String,
    // 持仓方向 long/short  
    pub pos_side: String,
    // 订单标签
    pub tag: String,
    // 订单详情
    pub detail: String,
    // 平台类型
    pub platform_type: String,
    // 订单状态 open/close
    pub status: String,
    // 创建时间
    pub create_time: Option<DateTime<Utc>>,
    // 更新时间  
    pub update_time: Option<DateTime<Utc>>,
}

impl SwapOrdersDetailEntity {
    /// 插入订单详情
    pub async fn insert(&self) -> Result<u64> {
        let pool = get_db_pool();
        
        let result = sqlx::query(
            "INSERT INTO swap_orders_detail 
             (in_order_id, out_order_id, strategy_id, strategy_type, period, 
              inst_id, side, pos_size, pos_side, tag, detail, platform_type, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&self.in_order_id)
        .bind(&self.out_order_id)
        .bind(self.strategy_id)
        .bind(&self.strategy_type)
        .bind(&self.period)
        .bind(&self.inst_id)
        .bind(&self.side)
        .bind(&self.pos_size)
        .bind(&self.pos_side)
        .bind(&self.tag)
        .bind(&self.detail)
        .bind(&self.platform_type)
        .bind(&self.status)
        .execute(pool)
        .await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        
        info!("订单详情已插入: in_order_id={}", self.in_order_id);
        Ok(result.last_insert_id())
    }
    
    /// 根据内部订单ID查询
    pub async fn select_by_in_order_id(in_order_id: &str) -> Result<Vec<Self>> {
        let pool = get_db_pool();
        
        let orders = sqlx::query_as::<_, Self>(
            "SELECT * FROM swap_orders_detail WHERE in_order_id = ?"
        )
        .bind(in_order_id)
        .fetch_all(pool)
        .await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        
        Ok(orders)
    }
    
    /// 根据策略ID和状态查询
    pub async fn select_by_strategy_and_status(
        strategy_id: i64,
        status: &str,
    ) -> Result<Vec<Self>> {
        let pool = get_db_pool();
        
        let orders = sqlx::query_as::<_, Self>(
            "SELECT * FROM swap_orders_detail WHERE strategy_id = ? AND status = ?"
        )
        .bind(strategy_id)
        .bind(status)
        .fetch_all(pool)
        .await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        
        Ok(orders)
    }
    
    /// 获取需要更新的订单ID列表
    pub async fn get_new_update_order_id() -> Result<Vec<String>> {
        let pool = get_db_pool();
        
        let orders: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT in_order_id FROM swap_orders_detail 
             WHERE status = 'open' 
             ORDER BY update_time DESC 
             LIMIT 100"
        )
        .fetch_all(pool)
        .await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        
        Ok(orders.into_iter().map(|r| r.0).collect())
    }
    
    /// 更新订单状态
    pub async fn update_status(&self, status: &str) -> Result<u64> {
        let pool = get_db_pool();
        
        let result = sqlx::query(
            "UPDATE swap_orders_detail 
             SET status = ?, update_time = NOW()
             WHERE in_order_id = ?"
        )
        .bind(status)
        .bind(&self.in_order_id)
        .execute(pool)
        .await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        
        Ok(result.rows_affected())
    }
    
    /// 批量更新 (通过Map参数)
    pub async fn update_by_map(
        in_order_id: &str,
        updates: std::collections::HashMap<String, String>,
    ) -> Result<u64> {
        let pool = get_db_pool();
        
        // 构建动态UPDATE语句
        let mut set_clauses = Vec::new();
        let mut values: Vec<String> = Vec::new();
        
        for (key, value) in updates.iter() {
            set_clauses.push(format!("{} = ?", key));
            values.push(value.clone());
        }
        
        if set_clauses.is_empty() {
            return Ok(0);
        }
        
        let sql = format!(
            "UPDATE swap_orders_detail SET {}, update_time = NOW() WHERE in_order_id = ?",
            set_clauses.join(", ")
        );
        
        debug!("更新订单SQL: {}", sql);
        
        let mut query = sqlx::query(&sql);
        for value in values {
            query = query.bind(value);
        }
        query = query.bind(in_order_id);
        
        let result = query.execute(pool).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        Ok(result.rows_affected())
    }
}

