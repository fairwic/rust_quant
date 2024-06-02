extern crate rbatis;
use rbatis::{crud, impl_insert, impl_update, RBatis};
use rbatis::rbdc::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::trading::okx::market::TickersData;
use crate::trading::okx::model::Db;

/// table
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct TickersDataDb {
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
    pub ts: String,
}


crud!(TickersDataDb{},"tickers_data");
impl_update!(TickersDataDb{update_by_name(name:String) => "`where id = '2'`"},"tickers_data");


pub struct TicketsModel {
    db: RBatis,
}

impl TicketsModel {
    pub async fn new() -> Self {
        Self {
            db: Db::get_db_client().await,
        }
    }
    pub async fn add(&self, list: Vec<TickersData>) -> anyhow::Result<()> {
        let tickers_db: Vec<TickersDataDb> = list.iter()
            .map(|ticker| TickersDataDb {
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
                ts: ticker.ts.clone(),
            })
            .collect();

        let data = TickersDataDb::insert_batch(&self.db, &tickers_db, list.len() as u64).await;
        println!("insert_batch = {}", json!(data));
        Ok(())
    }
    pub async fn update(&self, ticker: &TickersData) -> anyhow::Result<()> {
        let tickets_data = TickersDataDb {
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
            ts: ticker.ts.clone(),
        };
        let data = TickersDataDb::update_by_column(&self.db, &tickets_data, "inst_id").await;
        println!("update_by_column = {}", json!(data));

        // let data = TickersDataDb::update_by_name(&self.db, &tickets_data, ticker.inst_id.clone()).await;
        // println!("update_by_name = {}", json!(data));
        Ok(())
    }
}