extern crate rbatis;

use tracing::debug;
use rbatis::{crud, impl_insert, impl_update, RBatis};
use rbatis::rbdc::{Date, DateTime};
use rbatis::rbdc::db::ExecResult;
use rbs::Value;
use serde_json::json;
use crate::trading::model::Db;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BackTestDetail {
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
    pub full_close: bool,
    pub close_type: String,
}

crud!(BackTestDetail{});
impl_update!(BackTestDetail{update_by_name(name:&str) => "`where id = '2'`"});

pub struct BackTestDetailModel {
    db: RBatis,
}

impl BackTestDetailModel {
    pub async fn new() -> BackTestDetailModel {
        Self {
            db: Db::get_db_client().await,
        }
    }
    pub async fn add(&self, list: BackTestDetail) -> anyhow::Result<i64> {
        let data = BackTestDetail::insert(&self.db, &list).await;
        debug!("insert_back_test_log_result = {}", json!(data));
        Ok(data.unwrap().last_insert_id.as_i64().unwrap())
    }
    pub async fn batch_add(&self, list: Vec<BackTestDetail>) -> anyhow::Result<u64> {
        let data = BackTestDetail::insert_batch(&self.db, &list, list.len() as u64).await;
        debug!("insert_back_test_log_result = {}", json!(data));
        Ok(data.unwrap().rows_affected)
    }
}
