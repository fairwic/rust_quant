extern crate rbatis;

use anyhow::{anyhow, Result};
use rbatis::rbdc::db::ExecResult;
use rbatis::{crud, impl_update, RBatis};
use rbs::Value;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, error, info};

use crate::app_config::db;
use okx::dto::market_dto::CandleOkxRespDto;
use rbatis::impl_select;

/// table
#[derive(Serialize, Deserialize, Debug, Clone)]
// #[serde(rename_all = "camelCase")]
#[serde(rename_all = "snake_case")]
pub struct CandlesEntity {
    pub ts: i64,         // 开始时间，Unix时间戳的毫秒数格式
    pub o: String,       // 开盘价格
    pub h: String,       // 最高价格
    pub l: String,       // 最低价格
    pub c: String,       // 收盘价格
    pub vol: String,     // 交易量，以张为单位
    pub vol_ccy: String, // 交易量，以币为单位
    // pub vol_ccy_quote: String, // 交易量，以计价货币为单位
    pub confirm: String, // K线状态
}

crud!(CandlesEntity {}, "tickers_data"); //crud = insert+select_by_column+update_by_column+delete_by_column

impl_update!(CandlesEntity{update_by_name(name:String) => "`where id = '2'`"},"tickers_data");
impl_select!(CandlesEntity{fetch_list() => ""},"tickers_data");
