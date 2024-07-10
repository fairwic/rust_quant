extern crate rbatis;

use std::sync::Arc;
use rbatis::{crud, impl_insert, impl_update, RBatis};
use rbatis::rbdc::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::trading::okx::market::TickersData;
use crate::config::db;
use anyhow::Result;
use rbatis::rbdc::db::ExecResult;
use tracing::debug;
use rbatis::impl_select;

/// table
#[derive(Serialize, Deserialize, Debug)]
// #[serde(rename_all = "camelCase")]
#[serde(rename_all = "snake_case")]
pub struct TickersDataEntity {
    pub inst_type: String,
    pub inst_id: String,
    pub last: String,
    pub last_sz: String,
    pub ask_px: String,
    pub ask_sz: String,
    pub bid_px: String,
    pub bid_sz: String,
    pub open24h: String,
    pub high24h: String,
    pub low24h: String,
    pub vol_ccy24h: String,
    pub vol24h: String,
    pub sod_utc0: String,
    pub sod_utc8: String,
    pub ts: i64,
}


crud!(TickersDataEntity{},"tickers_data"); //crud = insert+select_by_column+update_by_column+delete_by_column

impl_update!(TickersDataEntity{update_by_name(name:String) => "`where id = '2'`"},"tickers_data");
impl_select!(TickersDataEntity{fetch_list() => "`where inst_id = 'BTC-USDT-SWAP' ORDER BY id DESC` "},"tickers_data");


pub struct TicketsModel {
    db: &'static RBatis,
}

impl TicketsModel {
    pub async fn new() -> Self {
        Self {
            db: db::get_db_client(),
        }
    }


    pub async fn add(&self, list: Vec<TickersData>) -> anyhow::Result<ExecResult> {
        let tickers_db: Vec<TickersDataEntity> = list.iter()
            .map(|ticker| TickersDataEntity {
                inst_type: ticker.inst_type.clone(),
                inst_id: ticker.inst_id.clone(),
                last: ticker.last.clone(),
                last_sz: ticker.last_sz.clone(),
                ask_px: ticker.ask_px.clone(),
                ask_sz: ticker.ask_sz.clone(),
                bid_px: ticker.bid_px.clone(),
                bid_sz: ticker.bid_sz.clone(),
                open24h: ticker.open24h.clone(),
                high24h: ticker.high24h.clone(),
                low24h: ticker.low24h.clone(),
                vol_ccy24h: ticker.vol_ccy24h.clone(),
                vol24h: ticker.vol24h.clone(),
                sod_utc0: ticker.sod_utc0.clone(),
                sod_utc8: ticker.sod_utc8.clone(),
                ts: ticker.ts.parse().unwrap(),
            })
            .collect();

        println!("insert_batch = {}", json!(tickers_db));

        let data = TickersDataEntity::insert_batch(self.db, &tickers_db, list.len() as u64).await?;
        println!("insert_batch = {}", json!(data));
        Ok(data)
    }
    pub async fn update(&self, ticker: &TickersData) -> anyhow::Result<()> {
        let tickets_data = TickersDataEntity {
            inst_type: ticker.inst_type.clone(),
            inst_id: ticker.inst_id.clone(),
            last: ticker.last.clone(),
            last_sz: ticker.last_sz.clone(),
            ask_px: ticker.ask_px.clone(),
            ask_sz: ticker.ask_sz.clone(),
            bid_px: ticker.bid_px.clone(),
            bid_sz: ticker.bid_sz.clone(),
            open24h: ticker.open24h.clone(),
            high24h: ticker.high24h.clone(),
            low24h: ticker.low24h.clone(),
            vol_ccy24h: ticker.vol_ccy24h.clone(),
            vol24h: ticker.vol24h.clone(),
            sod_utc0: ticker.sod_utc0.clone(),
            sod_utc8: ticker.sod_utc8.clone(),
            ts: ticker.ts.parse().unwrap(),
        };
        let data = TickersDataEntity::update_by_column(self.db, &tickets_data, "inst_id").await;
        println!("update_by_column = {}", json!(data));
        // let data = TickersDataDb::update_by_name(&self.db, &tickets_data, ticker.inst_id.clone()).await;
        // println!("update_by_name = {}", json!(data));
        Ok(())
    }
    /*获取全部*/
    pub async fn get_all(&self, inst_ids: Option<&Vec<&str>>) -> Result<Vec<TickersDataEntity>> {
        let sql = if let Some(inst_ids) = inst_ids {
            format!(
                "SELECT * FROM tickers_data WHERE inst_id IN ({}) and inst_type='SWAP' ORDER BY id DESC",
                inst_ids.iter().map(|id| format!("'{}'", id)).collect::<Vec<String>>().join(", ")
            )
        } else {
            "SELECT * FROM tickers_data ORDER BY id DESC".to_string()
        };

        let results: Vec<TickersDataEntity> = self.db.query_decode(sql.as_str(), vec![]).await?;
        Ok(results)
    }

    pub async fn find_one(&self, inst_id: &str) -> Result<Vec<TickersDataEntity>> {
        let results: Vec<TickersDataEntity> = TickersDataEntity::select_by_column(self.db, "inst_id", inst_id).await?;
        Ok(results)
    }
}