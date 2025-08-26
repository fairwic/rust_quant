extern crate rbatis;
use rbatis::impl_select;
use rbatis::{crud, impl_update, RBatis};
use serde::{Deserialize, Serialize};

use crate::app_config::db::get_db_client;
/// table
#[derive(Serialize, Deserialize, Debug)]
// #[serde(rename_all = "camelCase")]
#[serde(rename_all = "snake_case")]
pub struct AssetClassificationDetailEntity {
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

crud!(
    AssetClassificationDetailEntity {},
    "asset_classification_detail"
); //crud = insert+select_by_column+update_by_column+delete_by_column

impl_update!(AssetClassificationDetailEntity{update_by_name(name:String) => "`where id = '2'`"},"tickers_data");
impl_select!(AssetClassificationDetailEntity{fetch_list() => "`where inst_id = 'BTC-USDT-SWAP' ORDER BY id DESC` "},"tickers_data");

pub struct AssetClassificationDetailEntityModel {
    db: &'static RBatis,
}

impl AssetClassificationDetailEntityModel {
    pub async fn new() -> Self {
        Self {
            db: get_db_client(),
        }
    }
}
