use crate::app_config;
use anyhow::Result;
use okx::dto::asset::asset_dto::AssetBalance;
use rbatis::impl_select;
use rbatis::{crud, impl_update, RBatis};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

/// table
#[derive(Serialize, Deserialize, Debug)]
// #[serde(rename_all = "camelCase")]
#[serde(rename_all = "snake_case")]
pub struct AssetEntity {
    // 币种，如 BTC
    pub ccy: String,
    // 余额
    pub bal: String,
    // 冻结余额
    pub frozen_bal: String,
    // 可用余额
    pub avail_bal: String,
}
crud!(AssetEntity {}, "asset"); //crud = insert+select_by_column+update_by_column+delete_by_column
impl_update!(AssetEntity{update_by_name(name:String) => "`where id = '2'`"},"asset");
impl_select!(AssetEntity{fetch_list() => ""},"asset");

pub struct AssetModel {
    db: &'static RBatis,
}

impl AssetModel {
    pub async fn new() -> Self {
        Self {
            db: app_config::db::get_db_client(),
        }
    }
    pub async fn add(&self, list: Vec<AssetBalance>) -> anyhow::Result<()> {
        let tickers_db: Vec<AssetEntity> = list
            .iter()
            .map(|ticker| AssetEntity {
                ccy: ticker.ccy.clone(),
                bal: ticker.bal.clone(),
                frozen_bal: ticker.frozen_bal.clone(),
                avail_bal: ticker.avail_bal.clone(),
            })
            .collect();

        let data = AssetEntity::insert_batch(self.db, &tickers_db, list.len() as u64).await;
        println!("insert_batch = {}", json!(data));
        Ok(())
    }
    pub async fn update(&self, ticker: &AssetBalance) -> anyhow::Result<()> {
        let tickets_data = AssetEntity {
            ccy: ticker.ccy.clone(),
            bal: ticker.bal.clone(),
            frozen_bal: ticker.frozen_bal.clone(),
            avail_bal: ticker.avail_bal.clone(),
        };
        let data = AssetEntity::update_by_column(self.db, &tickets_data, "inst_id").await;
        println!("update_by_column = {}", json!(data));
        // let data = TickersDataDb::update_by_name(&self.db, &tickets_data, ticker.inst_id.clone()).await;
        // println!("update_by_name = {}", json!(data));
        Ok(())
    }
    pub async fn get_all(&self) -> Result<Vec<AssetEntity>> {
        let results: Vec<AssetEntity> = AssetEntity::fetch_list(self.db).await?;
        Ok(results)
    }
}
