extern crate rbatis;

use std::sync::Arc;
use tracing::debug;
use rbatis::{crud, impl_insert, impl_update, RBatis};
use rbatis::rbdc::{Date, DateTime};
use rbatis::rbdc::db::ExecResult;
use serde_json::json;
use crate::config::db;
use crate::trading::strategy::StrategyType;

/// CREATE TABLE `back_test_log` (
//   `id` int NOT NULL,
//   `int_type` varchar(255) NOT NULL,
//   `time` varchar(255) NOT NULL,
//   `win_rate` varchar(255) NOT NULL,
//   `Final fund` varchar(255) NOT NULL,
//   `strategy_detail` varchar(255) NOT NULL,
//   `created_at` datetime NOT NULL,
//   PRIMARY KEY (`id`)
// ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StrategyJobSignalLog {
    pub inst_id: String,
    pub time: String,
    pub strategy_type: String,
    pub strategy_result: String,
}

crud!(StrategyJobSignalLog{});
impl_update!(StrategyJobSignalLog{update_by_name(name:&str) => "`where id = '2'`"});

pub struct StrategyJobSignalLogModel {
    db: &'static RBatis,
}

impl StrategyJobSignalLogModel {
    pub async fn new() -> StrategyJobSignalLogModel {
        Self {
            db: db::get_db_client(),
        }
    }
    pub async fn add(&self, list: StrategyJobSignalLog) -> anyhow::Result<ExecResult> {
        let data = StrategyJobSignalLog::insert(self.db, &list).await?;
        debug!("insert_back_test_log_result = {}", json!(data));
        Ok(data)
    }
}
