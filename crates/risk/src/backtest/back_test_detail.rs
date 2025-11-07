use rust_quant_core::database;
use chrono::Local;
// TODO: 迁移到 sqlx 后移除 rbatis 宏
// use rbdc::db::ExecResult;
// use rbdc::{Date, DateTime};
// use rbatis::{crud, impl_insert, impl_update, RBatis};
// use rbs::Value;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, info};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BackTestDetail {
    pub option_type: String,
    pub strategy_type: String,
    pub inst_id: String,
    pub time: String,
    pub back_test_id: i64,
    pub open_position_time: String,
    //信号触发时间
    pub signal_open_position_time: Option<String>,
    //信号状态 0使用信号正常 -1信号错过 1使用信号的最优价格
    pub signal_status: i32,
    pub close_position_time: String,
    pub open_price: String,
    pub close_price: Option<String>,
    pub profit_loss: String,
    pub quantity: String,
    pub full_close: String,
    pub close_type: String,
    pub win_nums: i64,
    pub loss_nums: i64,
    pub signal_value: String,
    pub signal_result: String,
}

impl_insert!(BackTestDetail {});
// rbatis::// ORM迁移TODO
// ORM迁移TODO
// ORM迁移TODO

pub struct BackTestDetailModel {
    db: &'static RBatis,
}

impl BackTestDetailModel {
    pub async fn new() -> BackTestDetailModel {
        Self {
            db: db::get_db_client(),
        }
    }
    pub async fn add(&self, list: BackTestDetail) -> anyhow::Result<i64> {
        let data = BackTestDetail::insert(self.db, &list).await;
        debug!("insert_back_test_log_result = {}", json!(data));
        Ok(data.unwrap().last_insert_id.as_i64().unwrap())
    }
    pub async fn batch_add(&self, list: Vec<BackTestDetail>) -> anyhow::Result<u64> {
        let data = BackTestDetail::insert_batch(self.db, &list, list.len() as u64).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        debug!("insert_back_test_log_result = {}", json!(data));
        Ok(data.rows_affected)

        // let table_name = format!("{}", "back_test_detail");
        // // 构建批量插入的 SQL 语句
        // let mut query = format!("INSERT INTO `{}` (option_type, strategy_type, inst_id, time, back_test_id, open_position_time,\
        //  close_position_time, open_price,close_price,profit_loss,quantity,full_close,close_type,win_nums,loss_nums,signal_status,signal_open_position_time,signal_value,signal_result) VALUES ", table_name);
        // let mut params = Vec::new();

        // for candle in list {
        //     query.push_str("(?, ?, ?, ?, ?, ?, ?, ?,?,?,?,?,?,?,?,?,?,?,?),");
        //     params.push(candle.option_type.to_string().into());
        //     params.push(candle.strategy_type.to_string().into());
        //     params.push(candle.inst_id.to_string().into());
        //     params.push(candle.time.to_string().into());
        //     params.push(candle.back_test_id.to_string().into());
        //     params.push(candle.open_position_time.to_string().into());
        //     params.push(candle.close_position_time.to_string().into());
        //     params.push(candle.open_price.to_string().into());
        //     params.push(candle.close_price.to_string().into());
        //     params.push(candle.profit_loss.to_string().into());
        //     params.push(candle.quantity.to_string().into());
        //     params.push(candle.full_close.to_string().into());
        //     params.push(candle.close_type.to_string().into());
        //     params.push(candle.win_nums.to_string().into());
        //     params.push(candle.loss_nums.to_string().into());

        //     params.push(candle.signal_status.to_string().into());
        //     params.push(candle.signal_open_position_time.clone().unwrap_or_else(|| "null".to_string()).into());

        //     params.push(candle.signal_value.to_string().into());
        //     params.push(candle.signal_result.to_string().into());
        // }

        // // 移除最后一个逗号
        // query.pop();
        // let time = Local::now();
        // let data = self.db.exec(&query, params).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        // //记录执行所的时间
        // let duration = Local::now().signed_duration_since(time);
        // let duration_ms = duration.num_milliseconds();

        // // let res = format!("insert_back_test_detail_result = 执行时间{}毫秒 影响行数{}", duration_ms, data.rows_affected);
        // // info!("{}", res);
        // Ok(data.rows_affected)
    }
}
