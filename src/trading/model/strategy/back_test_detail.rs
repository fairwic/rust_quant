extern crate rbatis;

use std::sync::Arc;
use tracing::debug;
use rbatis::{crud, impl_insert, impl_update, RBatis};
use rbatis::rbdc::{Date, DateTime};
use rbatis::rbdc::db::ExecResult;
use rbs::Value;
use serde_json::json;
use crate::app_config::db;
use rbatis::impl_select;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BackTestDetail {
    pub option_type: String,
    pub strategy_type: String,
    pub inst_id: String,
    pub time: String,
    pub back_test_id: i64,
    pub open_position_time: String,
    pub close_position_time: String,
    pub open_price: String,
    pub close_price: String,
    pub profit_loss: String,
    pub quantity: String,
    pub full_close: String,
    pub close_type: String,
    pub win_nums: i64,
    pub loss_nums: i64,
    pub signal_detail: String,
}

impl_insert!(BackTestDetail{});
// rbatis::crud!(BackTestDetail{});
impl_update!(BackTestDetail{update_by_name(name:&str) => "`where id = '2'`"});
impl_select!(BackTestDetail{select_positions(back_test_id:i32) => "`where back_test_id = #{back_test_id} and option_type IN ('long', 'SHORT')`"});

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
        // let data = BackTestDetail::insert_batch(&self.db, &list, list.len() as u64).await;
        // debug!("insert_back_test_log_result = {}", json!(data));
        // let data = BackTestLog::insert_batch(&self.db, &v1, 1).await?;
        let table_name = format!("{}", "back_test_detail");
        // 构建批量插入的 SQL 语句
        let mut query = format!("INSERT INTO `{}` (option_type, strategy_type, inst_id, time, back_test_id, open_position_time,\
         close_position_time, open_price,close_price,profit_loss,quantity,full_close,close_type,win_nums,loss_nums,signal_detail) VALUES ", table_name);
        let mut params = Vec::new();

        for candle in list {
            query.push_str("(?, ?, ?, ?, ?, ?, ?, ?,?,?,?,?,?,?,?,?),");
            params.push(candle.option_type.to_string().into());
            params.push(candle.strategy_type.to_string().into());
            params.push(candle.inst_id.to_string().into());
            params.push(candle.time.to_string().into());
            params.push(candle.back_test_id.to_string().into());
            params.push(candle.open_position_time.to_string().into());
            params.push(candle.close_position_time.to_string().into());
            params.push(candle.open_price.to_string().into());
            params.push(candle.close_price.to_string().into());
            params.push(candle.profit_loss.to_string().into());
            params.push(candle.quantity.to_string().into());
            params.push(candle.full_close.to_string().into());
            params.push(candle.close_type.to_string().into());
            params.push(candle.win_nums.to_string().into());
            params.push(candle.loss_nums.to_string().into());
            params.push(candle.signal_detail.to_string().into());
        }

        // 移除最后一个逗号
        query.pop();
        let data = self.db.exec(&query, params).await?;
        // Ok(res
        debug!("insert_back_test_detail_result = {}", json!(data));
        Ok(data.rows_affected)
    }
}
