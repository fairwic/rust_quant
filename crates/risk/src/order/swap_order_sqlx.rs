//! SwapOrder 实体 - sqlx 实现
//! 从 swap_order.rs 迁移 (rbatis → sqlx)

use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_quant_common::utils::time;
use rust_quant_core::database::get_db_pool;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::info;

/// 订单实体
#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct SwapOrderEntity {
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
    // 方向
    pub side: String,
    // 持仓数量
    pub pos_size: String,
    // 持仓方向
    pub pos_side: String,
    // 订单标签
    pub tag: String,
    // 订单详情
    pub detail: String,
    // 平台类型
    pub platform_type: String, //okx,binance,huobi,bitget,
}

impl SwapOrderEntity {
    // 生成订单id
    pub fn gen_order_id(inst_id: &str, period: &str, side: &str, pos_side: &str) -> String {
        let time = time::format_to_period_str(period);
        let order_id = format!("{}_{}_{}_{}_{}", inst_id, period, side, pos_side, time);
        order_id
    }

    /// 插入订单到数据库
    pub async fn insert(&self) -> Result<u64> {
        let pool = get_db_pool();

        let result = sqlx::query(
            "INSERT INTO swap_order 
             (in_order_id, out_order_id, strategy_id, strategy_type, period, 
              inst_id, side, pos_size, pos_side, tag, detail, platform_type)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .execute(pool)
        .await
        .map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;

        info!("订单已插入数据库: in_order_id={}", self.in_order_id);
        Ok(result.last_insert_id())
    }

    /// 根据内部订单ID查询
    pub async fn select_by_in_order_id(in_order_id: &str) -> Result<Vec<Self>> {
        let pool = get_db_pool();

        let orders = sqlx::query_as::<_, Self>("SELECT * FROM swap_order WHERE in_order_id = ?")
            .bind(in_order_id)
            .fetch_all(pool)
            .await
            .map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;

        Ok(orders)
    }

    /// 根据策略ID查询
    pub async fn select_by_strategy_id(strategy_id: i64) -> Result<Vec<Self>> {
        let pool = get_db_pool();

        let orders = sqlx::query_as::<_, Self>("SELECT * FROM swap_order WHERE strategy_id = ?")
            .bind(strategy_id)
            .fetch_all(pool)
            .await
            .map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;

        Ok(orders)
    }

    /// 查询所有订单
    pub async fn select_all() -> Result<Vec<Self>> {
        let pool = get_db_pool();

        let orders = sqlx::query_as::<_, Self>("SELECT * FROM swap_order")
            .fetch_all(pool)
            .await
            .map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;

        Ok(orders)
    }

    /// 更新订单
    pub async fn update(&self) -> Result<u64> {
        let pool = get_db_pool();

        let result = sqlx::query(
            "UPDATE swap_order 
             SET out_order_id = ?, strategy_type = ?, period = ?, 
                 inst_id = ?, side = ?, pos_size = ?, pos_side = ?, 
                 tag = ?, detail = ?, platform_type = ?
             WHERE in_order_id = ?",
        )
        .bind(&self.out_order_id)
        .bind(&self.strategy_type)
        .bind(&self.period)
        .bind(&self.inst_id)
        .bind(&self.side)
        .bind(&self.pos_size)
        .bind(&self.pos_side)
        .bind(&self.tag)
        .bind(&self.detail)
        .bind(&self.platform_type)
        .bind(&self.in_order_id)
        .execute(pool)
        .await
        .map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;

        Ok(result.rows_affected())
    }
}
