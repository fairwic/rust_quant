use anyhow::Result;
use chrono::{Local, NaiveDateTime};
use rust_quant_market::get_quant_core_postgres_pool;
use serde_json::json;
use sqlx::{FromRow, Postgres};
use tracing::{debug, info};
/// 回测详情记录
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, FromRow)]
pub struct BackTestDetail {
    #[sqlx(default)]
    /// 唯一标识。
    pub id: Option<i64>,
    /// backtest ID。
    pub back_test_id: i64,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 时间字段。
    pub time: String,
    /// 类型标识。
    pub strategy_type: String,
    /// 类型标识。
    pub option_type: String,
    /// 开仓时间。
    pub signal_open_position_time: Option<NaiveDateTime>,
    /// 开仓时间。
    pub open_position_time: NaiveDateTime,
    /// 平仓时间。
    pub close_position_time: NaiveDateTime,
    /// 价格数值。
    pub open_price: String,
    /// 离场价格。
    pub close_price: Option<String>,
    #[sqlx(default)]
    /// 手续费。
    pub fee: Option<String>,
    /// 收益亏损，用于交易策略计算。
    pub profit_loss: String,
    /// 数量。
    pub quantity: String,
    /// full收盘，用于交易策略计算。
    pub full_close: String,
    /// 类型标识。
    pub close_type: String,
    /// 状态值。
    pub signal_status: i32,
    /// 信号值，用于交易策略计算。
    pub signal_value: String,
    /// 信号结果；为空时使用默认值或表示不限制。
    pub signal_result: Option<String>,
    #[sqlx(default)]
    /// 创建时间。
    pub created_at: Option<NaiveDateTime>,
    /// winnums，用于交易策略计算。
    pub win_nums: i32,
    /// 亏损数量；为空时表示该条件不启用。
    pub loss_nums: Option<i32>,
}
/// 基于 sqlx 的 BackTestDetail Model
pub struct BackTestDetailModel;
impl BackTestDetailModel {
    /// 添加单条回测详情记录
    /// 封装当前函数，减少风控调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    pub async fn add(&self, detail: &BackTestDetail) -> Result<i64> {
        let pool = get_quant_core_postgres_pool()?;
        let last_id = sqlx::query_scalar(
            r#"
            INSERT INTO back_test_detail (
                back_test_id, inst_id, time, strategy_type, option_type,
                signal_open_position_time, open_position_time, close_position_time,
                open_price, close_price, fee, profit_loss, quantity, full_close,
                close_type, signal_status, signal_value, signal_result, win_nums, loss_nums
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
            RETURNING id::bigint
            "#,
        )
        .bind(detail.back_test_id)
        .bind(detail.inst_id.clone())
        .bind(detail.time.clone())
        .bind(detail.strategy_type.clone())
        .bind(detail.option_type.clone())
        .bind(detail.signal_open_position_time)
        .bind(detail.open_position_time)
        .bind(detail.close_position_time)
        .bind(detail.open_price.clone())
        .bind(detail.close_price.clone())
        .bind(detail.fee.clone())
        .bind(detail.profit_loss.clone())
        .bind(detail.quantity.clone())
        .bind(detail.full_close.clone())
        .bind(detail.close_type.clone())
        .bind(detail.signal_status)
        .bind(detail.signal_value.clone())
        .bind(detail.signal_result.clone())
        .bind(detail.win_nums)
        .bind(detail.loss_nums)
        .fetch_one(pool)
        .await?;
        debug!(
            "insert_back_test_detail_result = {}",
            json!({"id": last_id})
        );
        Ok(last_id)
    }
    /// 批量添加回测详情记录
    pub async fn batch_add(&self, details: Vec<BackTestDetail>) -> Result<u64> {
        if details.is_empty() {
            return Ok(0);
        }
        let pool = get_quant_core_postgres_pool()?;
        let start_time = Local::now();
        let mut total_affected = 0u64;
        // 分批插入，每批 100 条
        const BATCH_SIZE: usize = 100;
        for chunk in details.chunks(BATCH_SIZE) {
            let mut query_builder = sqlx::QueryBuilder::<Postgres>::new(
                r#"INSERT INTO back_test_detail (
                    back_test_id, inst_id, time, strategy_type, option_type,
                    signal_open_position_time, open_position_time, close_position_time,
                    open_price, close_price, fee, profit_loss, quantity, full_close,
                    close_type, signal_status, signal_value, signal_result, win_nums, loss_nums
                ) "#,
            );
            query_builder.push_values(chunk, |mut b, detail| {
                b.push_bind(detail.back_test_id)
                    .push_bind(detail.inst_id.clone())
                    .push_bind(detail.time.clone())
                    .push_bind(detail.strategy_type.clone())
                    .push_bind(detail.option_type.clone())
                    .push_bind(detail.signal_open_position_time)
                    .push_bind(detail.open_position_time)
                    .push_bind(detail.close_position_time)
                    .push_bind(detail.open_price.clone())
                    .push_bind(detail.close_price.clone())
                    .push_bind(detail.fee.clone())
                    .push_bind(detail.profit_loss.clone())
                    .push_bind(detail.quantity.clone())
                    .push_bind(detail.full_close.clone())
                    .push_bind(detail.close_type.clone())
                    .push_bind(detail.signal_status)
                    .push_bind(detail.signal_value.clone())
                    .push_bind(detail.signal_result.clone())
                    .push_bind(detail.win_nums)
                    .push_bind(detail.loss_nums);
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
        let pool = get_quant_core_postgres_pool()?;
        let details = sqlx::query_as::<_, BackTestDetail>(
            "SELECT * FROM back_test_detail WHERE back_test_id = $1 ORDER BY open_position_time ASC",
        )
        .bind(back_test_id)
        .fetch_all(pool)
        .await?;
        Ok(details)
    }
    /// 删除指定 back_test_id 的详情
    pub async fn delete_by_back_test_id(&self, back_test_id: i64) -> Result<u64> {
        let pool = get_quant_core_postgres_pool()?;
        let result = sqlx::query("DELETE FROM back_test_detail WHERE back_test_id = $1")
            .bind(back_test_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}
