extern crate rbatis;

use anyhow::{anyhow, Result};
use rbatis::{crud, impl_update, RBatis};
use rbatis::rbdc::db::ExecResult;
use rbs::Value;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

use crate::trading::model::{Db, Model};
use crate::trading::okx::market::TickersData;

/// table
#[derive(Serialize, Deserialize, Debug)]
// #[serde(rename_all = "camelCase")]
#[serde(rename_all = "snake_case")]
pub struct CandlesEntity {
    pub ts: String, // 开始时间，Unix时间戳的毫秒数格式
    pub o: String, // 开盘价格
    pub h: String, // 最高价格
    pub l: String, // 最低价格
    pub c: String, // 收盘价格
    pub vol: String, // 交易量，以张为单位
    pub vol_ccy: String, // 交易量，以币为单位
    pub vol_ccy_quote: String, // 交易量，以计价货币为单位
    pub confirm: String, // K线状态
}


crud!(CandlesEntity{},"tickers_data"); //crud = insert+select_by_column+update_by_column+delete_by_column

impl_update!(CandlesEntity{update_by_name(name:String) => "`where id = '2'`"},"tickers_data");
impl_select!(CandlesEntity{fetch_list() => ""},"tickers_data");


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


pub struct CandlesModel {
    db: RBatis,
}

impl CandlesModel {
    pub async fn new() -> Self {
        Self {
            db: Db::get_db_client().await,
        }
    }

    pub async fn create_table(&self, inst_id: &str, time_interval: &str) -> Result<ExecResult> {
        let table_name = self.get_tale_name(inst_id, time_interval);
        let create_table_sql = format!(
            "CREATE TABLE IF NOT EXISTS `{}` (
            `id` int NOT NULL AUTO_INCREMENT,
            `ts` varchar(20) NOT NULL COMMENT '开始时间，Unix时间戳的毫秒数格式，如 1597026383085',
            `o` varchar(20) NOT NULL COMMENT '开盘价格',
            `h` varchar(20) NOT NULL COMMENT '最高价格',
            `l` varchar(20) NOT NULL COMMENT '最低价格',
            `c` varchar(20) NOT NULL COMMENT '收盘价格',
            `vol` varchar(20) NOT NULL COMMENT '交易量，以张为单位',
            `vol_ccy` varchar(20) NOT NULL COMMENT '交易量，以币为单位',
            `vol_ccy_quote` varchar(20) NOT NULL COMMENT '交易量，以计价货币为单位',
            `confirm` varchar(20) NOT NULL COMMENT 'K线状态',
            `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
            `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP,
            PRIMARY KEY (`id`)
        ) ENGINE=InnoDB AUTO_INCREMENT=1 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;",
            table_name
        );
        let res = self.db.exec(&create_table_sql, vec![]).await?;
        Ok(res)
    }

    fn get_tale_name(&self, inst_id: &str, time_interval: &str) -> String {
        let table_name = format!("{}_candles_{}", inst_id, time_interval);
        table_name
    }
    pub(crate) async fn add(&self, list: Vec<CandlesEntity>, inst_id: &str, time_interval: &str) -> anyhow::Result<ExecResult> {
        // let data = CandlesEntity::insert_batch(&self.db, &list, list.len() as u64).await;
        // println!("insert_batch = {}", json!(data));

        let table_name = format!("{}_candles_{}", inst_id, time_interval);
        // 构建批量插入的 SQL 语句
        let mut query = format!("INSERT INTO {} (ts, o, h, l, c, vol, vol_ccy, vol_ccy_quote, confirm) VALUES ", table_name);
        let mut params = Vec::new();

        for candle in list {
            query.push_str("(?, ?, ?, ?, ?, ?, ?, ?, ?),");
            params.push(candle.ts.into());
            params.push(candle.o.into());
            params.push(candle.h.into());
            params.push(candle.l.into());
            params.push(candle.c.into());
            params.push(candle.vol.into());
            params.push(candle.vol_ccy.into());
            params.push(candle.vol_ccy_quote.into());
            params.push(candle.confirm.into());
        }

        // 移除最后一个逗号
        query.pop();
        println!("query: {}", query);
        println!("parmas: {:?}", params);
        if params.is_empty() {
            //抛出错误
            return Err(anyhow!("params is empty"));
        } else {
            let res = self.db.exec(&query, params).await?;
            Ok(res)
        }
    }
    // pub async fn update(&self, ticker: &TickersData) -> anyhow::Result<()> {
    //     let tickets_data = CandlesEntity {
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
    //         ts: ticker.ts.clone(),
    //     };
    //     let data = CandlesEntity::update_by_column(&self.db, &tickets_data, "inst_id").await;
    //     println!("update_by_column = {}", json!(data));
    //     // let data = TickersDataDb::update_by_name(&self.db, &tickets_data, ticker.inst_id.clone()).await;
    //     // println!("update_by_name = {}", json!(data));
    //     Ok(())
    // }
    pub async fn get_all(&self, inst_id: &str, time_interval: &str) -> Result<Vec<CandlesEntity>> {
        let mut query = format!("select * from  {} order by ts desc limit 1000 ", self.get_tale_name(inst_id, time_interval));
        println!("query: {}", query);
        let res: Value = self.db.query(&query, vec![]).await?;


        if res.is_array() && res.as_array().unwrap().is_empty() {
            info!("No candles found in MySQL");
            return Ok(vec![]);
        }

        // 将 rbatis::core::value::Value 转换为 serde_json::Value
        let json_value: serde_json::Value = serde_json::from_str(&res.to_string())?;

        // 将 serde_json::Value 转换为 Vec<CandlesEntity>
        let candles: Vec<CandlesEntity> = serde_json::from_value(json_value)?;
        // let res:Vec<CandlesEntity>=serde_json::from_value(res);
        // let results: Vec<CandlesEntity> = CandlesEntity::fetch_list(&self.db).await?;
        Ok(candles)
    }
}