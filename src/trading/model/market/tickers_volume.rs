extern crate rbatis;

use anyhow::Result;
use clap::builder::TypedValueParser;
use rbatis::{crud, impl_update, RBatis};
use rbatis::{impl_delete, impl_select};
use rbatis::rbdc::db::ExecResult;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app_config::db::get_db_client;

/// table
#[derive(Serialize, Deserialize, Debug)]
// #[serde(rename_all = "camelCase")]
#[serde(rename_all = "snake_case")]
pub struct TickersVolume {
    pub(crate) inst_id: String,
    pub period: String,
    pub ts: i64,
    pub oi: String,
    pub vol: String,
}

crud!(TickersVolume {}); //crud = insert+select_by_column+update_by_column+delete_by_column
impl_update!(TickersVolume{update_by_name(name:String) => "`where id = '2' `"});
impl_delete!(TickersVolume{delete_by_inst_id(inst_id:&str) => "`where inst_id = #{inst_id} `"});
impl_select!(TickersVolume{fetch_list() => "`where inst_id = 'BTC-USDT-SWAP' ORDER BY id DESC` "});

pub struct TickersVolumeModel {
    db: &'static RBatis,
}

impl TickersVolumeModel {
    pub async fn new() -> Self {
        Self {
            db: get_db_client(),
        }
    }

    pub async fn find_one(&self, inst_id: &str) -> Result<Vec<TickersVolume>> {
        let results: Vec<TickersVolume> =
            TickersVolume::select_by_column(self.db, "inst_id", inst_id).await?;
        Ok(results)
    }

    pub async fn delete_by_inst_id(&self, inst_id: &str) -> Result<u64> {
        let results: ExecResult = TickersVolume::delete_by_inst_id(self.db, inst_id).await?;
        Ok(results.rows_affected)
    }
    pub async fn add(&self, list: Vec<TickersVolume>) -> anyhow::Result<ExecResult> {
        let tickers_db: Vec<TickersVolume> = list
            .iter()
            .map(|ticker| TickersVolume {
                inst_id: ticker.inst_id.clone(),
                oi: ticker.oi.clone(),
                vol: ticker.vol.clone(),
                ts: ticker.ts,
                period: ticker.period.clone(),
            })
            .collect();

        println!("insert_batch = {}", json!(tickers_db));

        let data =
            TickersVolume::insert_batch(self.db, &tickers_db, list.len() as u64).await?;
        println!("insert_batch = {}", json!(data));
        Ok(data)
    }
}
