extern crate rbatis;

use std::collections::HashMap;
use anyhow::Result;
use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use clap::builder::TypedValueParser;
use rbatis::{crud, impl_update, RBatis};
use rbatis::impl_select;
use rbatis::rbdc::db::ExecResult;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app_config::db::get_db_client;
use crate::trading::okx::market::TickersData;

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
#[derive(Serialize, Deserialize, Debug)]
pub struct TickersDataQueryResult {
    pub inst_id: String,
    // pub ts: i64,    // 存储 Unix 时间戳
    pub daily_vol: f64, // 存储 24小时交易量
    pub ts: i64
}

impl TickersDataQueryResult {
    // 将 ts 转换为 NaiveDate
    // 将时间戳转换为 NaiveDate（按日期分组）
    pub fn get_date(&self) -> NaiveDate {
        // 转换时间戳为 NaiveDate
        NaiveDateTime::from_timestamp(self.ts / 1000, 0).date()  // /1000 是将毫秒转换为秒
    }


    // 将 vol24h 转换为 f64
    pub fn get_vol24h(&self) -> f64 {
        self.daily_vol
    }
}


crud!(TickersDataEntity{},"tickers_data"); //crud = insert+select_by_column+update_by_column+delete_by_column

impl_update!(TickersDataEntity{update_by_name(name:String) => "`where id = '2'`"},"tickers_data");
impl_select!(TickersDataEntity{fetch_list() => "`where inst_id = 'BTC-USDT-SWAP' ORDER BY id DESC` "},"tickers_data");

impl TickersDataEntity {

    // 将 ts 字段转换为 NaiveDate
    pub fn get_date(&self) -> NaiveDate {
        let naive_datetime = Utc.timestamp_millis(self.ts).naive_utc();
        naive_datetime.date()
    }
}
pub struct TicketsModel {
    db: &'static RBatis,
}

impl TicketsModel {
    pub async fn new() -> Self {
        Self {
            db: get_db_client(),
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

    pub async fn get_daily_volumes(&self, inst_ids: Option<Vec<&str>>) -> Result<Vec<(String, NaiveDate, f64)>> {
        // 构造查询
        let sql = if let Some(inst_ids) = inst_ids {
            format!(
                "SELECT inst_id, MAX(ts) AS ts, SUM(vol24h) AS daily_vol
                 FROM tickers_data
                 WHERE inst_id IN ({})
                 GROUP BY inst_id, DATE(FROM_UNIXTIME(ts / 1000))
                 ORDER BY ts DESC",
                inst_ids.iter().map(|id| format!("'{}'", id)).collect::<Vec<String>>().join(", ")
            )
        } else {
            "SELECT inst_id, MAX(ts) AS ts, SUM(vol24h) AS daily_vol
             FROM tickers_data
             GROUP BY inst_id, DATE(FROM_UNIXTIME(ts / 1000))
             ORDER BY ts DESC".to_string()
        };

        // 查询并反序列化到中间结构体
        let results: Vec<TickersDataQueryResult> = self.db.query_decode(sql.as_str(), vec![]).await?;

        // 将查询结果转换为包含日期和交易量的元组
        let daily_volumes = results.into_iter().map(|entry| {
            let date = entry.get_date();  // 将 ts 转换为 NaiveDate
            (entry.inst_id.clone(), date, entry.get_vol24h())
        }).collect();

        Ok(daily_volumes)
    }


    // 计算过去7天的平均交易量
    pub fn calculate_7_day_avg_volume(
        &self,
        daily_volumes: Vec<(String, NaiveDate, f64)>
    ) -> HashMap<String, f64> {
        let mut daily_vol_map: HashMap<String, Vec<(NaiveDate, f64)>> = HashMap::new();

        // 将交易量数据按 inst_id 和日期分组
        for (inst_id, date, vol) in daily_volumes {
            daily_vol_map.entry(inst_id.clone())
                .or_insert_with(Vec::new)
                .push((date, vol));
        }

        // 计算每个板块的7天平均交易量
        let mut avg_volumes: HashMap<String, f64> = HashMap::new();
        for (inst_id, volumes) in daily_vol_map {
            let last_7_days = volumes.iter()
                .rev()
                .take(7)  // 取最近7天的交易量
                .map(|(_, vol)| *vol)
                .collect::<Vec<f64>>();

            if last_7_days.len() == 7 {
                let avg_vol = last_7_days.iter().sum::<f64>() / 7.0;
                avg_volumes.insert(inst_id, avg_vol);
            }
        }

        avg_volumes
    }

    // 判断是否拉升的板块
    pub fn check_for_possible_lift(
        &self,
        daily_volumes: Vec<(String, NaiveDate, f64)>,
        avg_volumes: HashMap<String, f64>,
        threshold: f64,
    ) -> Vec<String> {
        let mut lifted_assets = Vec::new();

        // 遍历每个板块的交易量数据
        for (inst_id, _, current_vol) in daily_volumes {
            if let Some(avg_vol) = avg_volumes.get(&inst_id) {
                if current_vol > *avg_vol * threshold {
                    lifted_assets.push(inst_id);
                }
            }
        }

        lifted_assets
    }
}