extern crate rbatis;

use std::convert::TryInto;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rbatis::{crud, impl_update, RBatis};
use rbatis::rbdc::db::ExecResult;
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::time_util;
use crate::app_config::db;
use rbatis::impl_select;

/// table
#[derive(Serialize, Deserialize, Debug, Clone)]
// #[serde(rename_all = "camelCase")]
#[serde(rename_all = "snake_case")]
pub struct SwapOrderEntity {
    pub uuid: String,
    pub strategy_type: String,
    pub period: String,
    pub inst_id: String,  // 使用 Vec<u8> 来表示 VARBINARY
    pub side: String,
    pub pos_side: String,
    pub okx_ord_id: String,
    pub tag: String,
    pub detail: String,
}

impl SwapOrderEntity {
    pub fn gen_uuid(inst_id: &str, period: &str, side: String, pos_side: String) -> String {
        let time = time_util::format_to_period(period, None);
        format!("{}+{}+{}+{}+{}", time, inst_id, period, side, pos_side)
    }
}


crud!(SwapOrderEntity{},"swap_orders"); //crud = insert+select_by_column+update_by_column+delete_by_column

impl_update!(SwapOrderEntity{update_by_name(name:String) => "`where id = '2'`"},"swap_orders");
impl_select!(SwapOrderEntity{fetch_list() => ""},"swap_orders");


#[derive(Debug)]
enum TimeInterval {
    OneDay,
    OneHour,
    // 其他时间类型可以在这里添加
}

impl TimeInterval {
    fn table_name(&self) -> &'static str {
        match self {
            TimeInterval::OneDay => "btc_candles_1d",
            TimeInterval::OneHour => "btc_candles_1h",
            // 其他时间类型映射到对应的表名
        }
    }
}


pub struct SwapOrderEntityModel {
    db: &'static RBatis,
}

impl SwapOrderEntityModel {
    pub async fn new() -> Self {
        Self {
            db: db::get_db_client(),
        }
    }
        
    pub async fn add(&self, swap_order_entity: SwapOrderEntity) -> anyhow::Result<ExecResult> {
        let data = SwapOrderEntity::insert(self.db, &swap_order_entity).await?;
        println!("insert_batch = {}", json!(data));
        Ok(data)
    }
    pub async fn getOne(&self, inst_id: &str, time: &str, side: String, pos_side: String) -> anyhow::Result<Vec<SwapOrderEntity>> {
        let uuid = SwapOrderEntity::gen_uuid(inst_id, time, side, pos_side);
        let data = SwapOrderEntity::select_by_column(self.db, "uuid", uuid.as_str()).await?;
        println!("query swap_oder uuid = {},result:{}", uuid, json!(data));
        Ok(data)
    }
    //
    // pub async fn update(&self, ticker: &TickersData) -> anyhow::Result<()> {
    //     let tickets_data = SwapOrderEntity {
    //         inst_type: ticker.inst_type.clone(),
    //         inst_id: ticker.inst_id.clone(),
    //         last: ticker.last.clone(),
    //         last_sz: ticker.last_sz.clone(),
    //         ask_px: ticker.ask_px.clone(),
    //         ask_sz: ticker.ask_sz.clone(),
    //         bid_px: ticker.bid_px.clone(),
    //         bid_sz: ticker.bid_sz.clone(),
    //         open24h: ticker.open24h.clone(),
    //         high24h: ticker.high24h.clone(),
    //         low24h: ticker.low24h.clone(),
    //         vol_ccy24h: ticker.vol_ccy24h.clone(),
    //         vol24h: ticker.vol24h.clone(),
    //         sod_utc0: ticker.sod_utc0.clone(),
    //         sod_utc8: ticker.sod_utc8.clone(),
    //         ts: ticker.ts.parse().unwrap(),
    //     };
    //     let data = SwapOrderEntity::update_by_column(&self.db, &tickets_data, "inst_id").await;
    //     println!("update_by_column = {}", json!(data));
    //     // let data = TickersDataDb::update_by_name(&self.db, &tickets_data, ticker.inst_id.clone()).await;
    //     // println!("update_by_name = {}", json!(data));
    //     Ok(())
    // }
    // /*获取全部*/
    // pub async fn get_all(&self, inst_ids: Option<&Vec<&str>>) -> Result<Vec<SwapOrderEntity>> {
    //     let sql = if let Some(inst_ids) = inst_ids {
    //         format!(
    //             "SELECT * FROM tickers_data WHERE inst_id IN ({}) and inst_type='SWAP' ORDER BY id DESC",
    //             inst_ids.iter().map(|id| format!("'{}'", id)).collect::<Vec<String>>().join(", ")
    //         )
    //     } else {
    //         "SELECT * FROM tickers_data ORDER BY id DESC".to_string()
    //     };
    //
    //     let results: Vec<SwapOrderEntity> = self.db.query_decode(sql.as_str(), vec![]).await?;
    //     Ok(results)
    // }
}