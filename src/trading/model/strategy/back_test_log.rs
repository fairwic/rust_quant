extern crate rbatis;

use rbatis::{crud, impl_insert, impl_update, RBatis};
use rbatis::rbdc::{Date, DateTime};
use serde_json::json;
use crate::trading::model::Db;

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
pub struct BackTestLog {
    pub int_type: String,
    pub time: String,
    pub win_rate: String,
    pub final_fund: String,
    pub strategy_detail: Option<String>,
    pub created_at: DateTime,
}

crud!(BackTestLog{});
impl_update!(BackTestLog{update_by_name(name:&str) => "`where id = '2'`"});

