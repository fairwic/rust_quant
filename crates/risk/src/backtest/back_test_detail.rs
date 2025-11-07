use anyhow::Result;
use chrono::{Local, NaiveDateTime};
use rust_quant_core::database::get_db_pool;
use serde_json::json;
use sqlx::{FromRow, MySql, Pool};
use tracing::{debug, info};

/// 回测详情记录
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, FromRow)]
pub struct BackTestDetail {
    #[sqlx(default)]
    pub id: Option<i64>,
    pub back_test_id: i64,
    pub inst_id: String,
    pub time: String,
    pub strategy_type: String,
    pub option_type: String,
    pub signal_open_position_time: Option<NaiveDateTime>,
    pub open_position_time: NaiveDateTime,
    pub close_position_time: NaiveDateTime,
    pub open_price: String,
    pub close_price: Option<String>,
    #[sqlx(default)]
    pub fee: Option<String>,
    pub profit_loss: String,
    pub quantity: String,
    pub full_close: String,
    pub close_type: String,
    pub signal_status: i32,
    pub signal_value: String,
    pub signal_result: Option<String>,
    #[sqlx(default)]
    pub created_at: Option<NaiveDateTime>,
    pub win_nums: i32,
    pub loss_nums: Option<i32>,
}

/// 基于 sqlx 的 BackTestDetail Model
pub struct BackTestDetailModel;

impl BackTestDetailModel {
    /// 添加单条回测详情记录
    pub async fn add(&self, detail: &BackTestDetail) -> Result<i64> {
        let pool = get_db_pool();
        
        let result = sqlx::query(
            r#"
            INSERT INTO back_test_detail (
                back_test_id, inst_id, time, strategy_type, option_type,
                signal_open_position_time, open_position_time, close_position_time,
                open_price, close_price, fee, profit_loss, quantity, full_close,
                close_type, signal_status, signal_value, signal_result, win_nums, loss_nums
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&detail.back_test_id)
        .bind(&detail.inst_id)
        .bind(&detail.time)
        .bind(&detail.strategy_type)
        .bind(&detail.option_type)
        .bind(&detail.signal_open_position_time)
        .bind(&detail.open_position_time)
        .bind(&detail.close_position_time)
        .bind(&detail.open_price)
        .bind(&detail.close_price)
        .bind(&detail.fee)
        .bind(&detail.profit_loss)
        .bind(&detail.quantity)
        .bind(&detail.full_close)
        .bind(&detail.close_type)
        .bind(&detail.signal_status)
        .bind(&detail.signal_value)
        .bind(&detail.signal_result)
        .bind(&detail.win_nums)
        .bind(&detail.loss_nums)
        .execute(pool)
        .await?;
        
        let last_id = result.last_insert_id() as i64;
        debug!("insert_back_test_detail_result = {}", json!({"id": last_id}));
        Ok(last_id)
    }
    
    /// 批量添加回测详情记录
    pub async fn batch_add(&self, details: Vec<BackTestDetail>) -> Result<u64> {
        if details.is_empty() {
            return Ok(0);
        }
        
        let pool = get_db_pool();
        let start_time = Local::now();
        let mut total_affected = 0u64;
        
        // 分批插入，每批 100 条
        const BATCH_SIZE: usize = 100;
        for chunk in details.chunks(BATCH_SIZE) {
            let mut query_builder = sqlx::QueryBuilder::<MySql>::new(
                r#"INSERT INTO back_test_detail (
                    back_test_id, inst_id, time, strategy_type, option_type,
                    signal_open_position_time, open_position_time, close_position_time,
                    open_price, close_price, fee, profit_loss, quantity, full_close,
                    close_type, signal_status, signal_value, signal_result, win_nums, loss_nums
                ) "#
            );
            
            query_builder.push_values(chunk, |mut b, detail| {
                b.push_bind(&detail.back_test_id)
                    .push_bind(&detail.inst_id)
                    .push_bind(&detail.time)
                    .push_bind(&detail.strategy_type)
                    .push_bind(&detail.option_type)
                    .push_bind(&detail.signal_open_position_time)
                    .push_bind(&detail.open_position_time)
                    .push_bind(&detail.close_position_time)
                    .push_bind(&detail.open_price)
                    .push_bind(&detail.close_price)
                    .push_bind(&detail.fee)
                    .push_bind(&detail.profit_loss)
                    .push_bind(&detail.quantity)
                    .push_bind(&detail.full_close)
                    .push_bind(&detail.close_type)
                    .push_bind(&detail.signal_status)
                    .push_bind(&detail.signal_value)
                    .push_bind(&detail.signal_result)
                    .push_bind(&detail.win_nums)
                    .push_bind(&detail.loss_nums);
            });
            
            let result = query_builder.build().execute(pool).await?;
            total_affected += result.rows_affected();
        }
        
        let duration = Local::now().signed_duration_since(start_time);
        let duration_ms = duration.num_milliseconds();
        
        info!(
            "batch_insert_back_test_detail: 总数={}, 影响行数={}, 耗时={}ms",
            details.len(),
            total_affected,
            duration_ms
        );
        
        Ok(total_affected)
    }
    
    /// 根据 back_test_id 查询详情
    pub async fn find_by_back_test_id(&self, back_test_id: i64) -> Result<Vec<BackTestDetail>> {
        let pool = get_db_pool();
        
        let details = sqlx::query_as::<_, BackTestDetail>(
            "SELECT * FROM back_test_detail WHERE back_test_id = ? ORDER BY open_position_time ASC"
        )
        .bind(back_test_id)
        .fetch_all(pool)
        .await?;
        
        Ok(details)
    }
    
    /// 删除指定 back_test_id 的详情
    pub async fn delete_by_back_test_id(&self, back_test_id: i64) -> Result<u64> {
        let pool = get_db_pool();
        
        let result = sqlx::query("DELETE FROM back_test_detail WHERE back_test_id = ?")
            .bind(back_test_id)
            .execute(pool)
            .await?;
        
        Ok(result.rows_affected())
    }
}
