extern crate rbatis;

use std::sync::Arc;
use std::vec;
use anyhow::anyhow;
use rbatis::{crud, impl_insert, RBatis};
use rbs::Value;
use serde_json::json;
use tracing::debug;

use crate::app_config::db;

/// table
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BackTestLog {
    pub strategy_type: String,
    pub inst_type: String,
    pub time: String,
    pub win_rate: String,
    pub final_fund: String,
    pub open_positions_num: i32,
    pub strategy_detail: Option<String>,
    pub profit: String,
}
crud!(BackTestLog{});



pub struct BackTestLogModel {
    db: &'static RBatis,
}

impl BackTestLogModel {
    pub async fn new() -> BackTestLogModel {
        Self {
            db: db::get_db_client(),
        }
    }
    pub async fn add(&self, list: &BackTestLog) -> anyhow::Result<i64> {
        // println!("111111111 list:{:#?}", list);
        // println!("db:{:#?}", self.db);
        let mut v1 = vec::Vec::new();
        v1.push(list.clone());

        // let data = BackTestLog::insert_batch(&self.db, &v1, 1).await?;
        let table_name = format!("{}", "back_test_log");
        // 构建批量插入的 SQL 语句
        let mut query = format!("INSERT INTO `{}` (strategy_type, inst_type, time, win_rate, final_fund, open_positions_num, strategy_detail, profit) VALUES ", table_name);
        let mut params = Vec::new();

        for candle in v1 {
            query.push_str("(?, ?, ?, ?, ?, ?, ?, ?),");
            params.push(candle.strategy_type.to_string().into());
            params.push(candle.inst_type.to_string().into());
            params.push(candle.time.to_string().into());
            params.push(candle.win_rate.to_string().into());
            params.push(candle.final_fund.to_string().into());
            params.push(candle.open_positions_num.to_string().into());
            params.push(candle.strategy_detail.unwrap().to_string().into());
            params.push(candle.profit.to_string().into());
        }

        // 移除最后一个逗号
        query.pop();
        let data = self.db.exec(&query, params).await?;
        // Ok(res
        debug!("insert_back_test_log_result = {}", json!(data));
        Ok(data.last_insert_id.as_i64().unwrap())
    }
}
