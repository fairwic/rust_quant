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
pub enum TimeDirect {
    BEFORE,
    AFTER,
}

pub struct SelectTime {
    //选择开始时间
    pub start_time: i64,
    //选择结束时间
    pub end_time: Option<i64>,
    //选择方向1 正
    pub direct: TimeDirect,
}

crud!(CandlesEntity {}, "tickers_data"); //crud = insert+select_by_column+update_by_column+delete_by_column

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
    db: &'static RBatis,
}

impl CandlesModel {
    pub async fn new() -> Self {
        Self {
            db: db::get_db_client(),
        }
    }

    pub async fn create_table(&self, inst_id: &str, time_interval: &str) -> Result<ExecResult> {
        let table_name = CandlesModel::get_tale_name(inst_id, time_interval);
        let create_table_sql = format!(
            "CREATE TABLE IF NOT EXISTS `{}` (
  `id` int NOT NULL AUTO_INCREMENT,
  `ts` bigint NOT NULL COMMENT '开始时间，Unix时间戳的毫秒数格式，如 1597026383085',
  `o` varchar(20) NOT NULL COMMENT '开盘价格',
  `h` varchar(20) NOT NULL COMMENT '最高价格',
  `l` varchar(20) NOT NULL COMMENT '最低价格',
  `c` varchar(20) NOT NULL COMMENT '收盘价格',
  `vol` varchar(20) NOT NULL COMMENT '交易量，以张为单位',
  `vol_ccy` varchar(50) NOT NULL COMMENT '交易量，以币为单位',
  `confirm` varchar(20) NOT NULL COMMENT 'K线状态',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `ts` (`ts` DESC) USING BTREE,
  KEY `vol_ccy` (`vol_ccy` DESC)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 ;",
            table_name
        );
        //   `vol_ccy_quote` varchar(50) NOT NULL COMMENT '交易量，以计价货币为单位',
        // println!("create_table_sql = {}", create_table_sql);
        let res = self.db.exec(&create_table_sql, vec![]).await?;
        Ok(res)
    }

    pub fn get_tale_name(inst_id: &str, time_interval: &str) -> String {
        // println!("inst_id{},time_interval{}",inst_id,time_interval);
        let table_name = format!("{}_candles_{}", inst_id, time_interval);
        table_name
    }
    pub async fn add(
        &self,
        list: Vec<CandleOkxRespDto>,
        inst_id: &str,
        time_interval: &str,
    ) -> anyhow::Result<ExecResult> {
        // let items :Vec<CandlesEntity>= list.iter().map(|candle| {
        //     CandlesEntity {
        //         ts: candle.ts.parse::<i64>().unwrap(),
        //         o: candle.o.to_string(),
        //         h: candle.h.to_string(),
        //         l: candle.l.to_string(),
        //         c: candle.c.to_string(),
        //         vol: candle.v.to_string(),
        //         vol_ccy: candle.vol_ccy.to_string(),
        //         // vol_ccy_quote: candle.vol_ccy_quote.to_string(),
        //         confirm: candle.confirm.to_string(),
        //     }
        // }).collect();
        // let data = CandlesEntity::insert_batch(self.db, &items, items.len() as u64).await?;
        // println!("insert_batch = {}", json!(data));
        // Ok(data)

        //自定义表名
        let table_name = format!("{}_candles_{}", inst_id, time_interval);
        // 构建批量插入的 SQL 语句
        let mut query = format!(
            "INSERT INTO `{}` (ts, o, h, l, c, vol, vol_ccy, confirm) VALUES ",
            table_name
        );
        let mut params = Vec::new();

        for candle in list {
            query.push_str("(?, ?, ?, ?, ?, ?,?,?),");
            params.push(candle.ts.into());
            params.push(candle.o.into());
            params.push(candle.h.into());
            params.push(candle.l.into());
            params.push(candle.c.into());
            params.push(candle.v.into());
            params.push(candle.vol_ccy.into());
            // params.push(candle.vol_ccy_quote.into());
            params.push(candle.confirm.into());
        }

        // 移除最后一个逗号
        query.pop();
        debug!("query: {}", query);
        debug!("parmas: {:?}", params);
        if params.is_empty() {
            //抛出错误
            return Err(anyhow!("params is empty"));
        } else {
            let res = self.db.exec(&query, params).await?;
            Ok(res)
        }
    }

    pub(crate) async fn delete_lg_time(
        &self,
        inst_id: &str,
        time_interval: &str,
        ts: i64,
    ) -> anyhow::Result<ExecResult> {
        let table_name = Self::get_tale_name(inst_id, time_interval);
        let query = format!("DELETE  FROM `{}` WHERE ts >= ?", table_name);
        let params = vec![ts.into()];

        debug!("delete lg confirm >0 data sql:{}", query);
        debug!("delete lg confirm >0 data param:{:?}", params);
        let result = self.db.exec(&query, params).await?;
        Ok(result)
    }

    pub(crate) async fn get_older_un_confirm_data(
        &self,
        inst_id: &str,
        time_interval: &str,
    ) -> anyhow::Result<Option<CandlesEntity>> {
        let table_name = Self::get_tale_name(inst_id, time_interval);
        let query = format!(
            "select * from `{}` WHERE confirm = 0 order by ts asc limit 1",
            table_name
        );
        debug!("update candle db sql:{}", query);
        let result: Option<CandlesEntity> = self.db.query_decode(&query, vec![]).await?;
        Ok(result)
    }

    pub(crate) async fn update_one(
        &self,
        candle: CandlesEntity,
        inst_id: &str,
        time_interval: &str,
    ) -> anyhow::Result<u64> {
        let table_name = format!("{}_candles_{}", inst_id, time_interval);
        let query = format!(
            "UPDATE `{}` SET o = ?, h = ?, l = ?, c = ?, vol = ?, vol_ccy = ?, confirm = ? WHERE ts = ?",
            table_name
        );

        let params = vec![
            candle.o.into(),
            candle.h.into(),
            candle.l.into(),
            candle.c.into(),
            candle.vol.into(),
            candle.vol_ccy.into(),
            // candle.vol_ccy_quote.into(),
            candle.confirm.into(),
            candle.ts.into(),
        ];

        if params.is_empty() {
            return Err(anyhow!("params is empty"));
        } else {
            debug!("update candle db sql:{}", query);
            let result = self.db.exec(&query, params).await?;
            Ok(result.rows_affected)
        }
    }

    pub async fn update_or_create(
        &self,
        candle_data: &CandleOkxRespDto,
        inst_id: &str,
        time_interval: &str,
    ) -> anyhow::Result<()> {
        //查询是否存在
        //不存在写入，存在且confirm==0 更新
        let existing_record: Option<CandlesEntity> = self
            .get_one_by_ts(
                inst_id,
                time_interval,
                candle_data.ts.parse::<i64>().unwrap(),
            )
            .await?;
        if existing_record.is_none() {
            let res = self
                .add(vec![candle_data.clone()], inst_id, time_interval)
                .await?;
        } else {
            let data = CandlesEntity {
                ts: candle_data.ts.parse::<i64>().unwrap(),
                o: candle_data.o.to_string(),
                h: candle_data.h.to_string(),
                l: candle_data.l.to_string(),
                c: candle_data.c.to_string(),
                vol: candle_data.v.to_string(),
                vol_ccy: candle_data.vol_ccy.to_string(),
                // vol_ccy_quote: candle_data.vol_ccy_quote.to_string(),
                confirm: candle_data.confirm.to_string(),
            };
            let exec_result: u64 = self.update_one(data, inst_id, time_interval).await?;
        }
        Ok(())
    }

    pub async fn get_all(
        &self,
        inst_id: &str,
        time_interval: &str,
        limit: usize,
        select_time: Option<SelectTime>,
    ) -> Result<Vec<CandlesEntity>> {
        let mut query = format!(
            "SELECT ts,o,h,l,c,vol,vol_ccy,confirm FROM `{}` ",
            Self::get_tale_name(inst_id, time_interval)
        );
        //如果指定了时间
        if let Some(SelectTime {
            direct,
            start_time: point_time,
            end_time: end_time,
        }) = select_time
        {
            match direct {
                TimeDirect::BEFORE => {
                    query = format!("{} where ts<= {} ", query, point_time);
                    if let Some(end_time) = end_time {
                        query = format!("{} and ts>= {} ", query, end_time);
                    }
                }
                TimeDirect::AFTER => {
                    query = format!("{} where ts>= {} ", query, point_time);
                    if let Some(end_time) = end_time {
                        query = format!("{} and ts<= {} ", query, end_time);
                    }
                }
            }
        }
        //默认取最后的条数
        query = format!("{} order by ts DESC limit {}", query, limit);
        info!("query get candle SQL: {}", query);

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

    pub async fn get_new_data(
        &self,
        inst_id: &str,
        time_interval: &str,
    ) -> Result<Option<CandlesEntity>> {
        let mut query = format!(
            "select * from  `{}` ORDER BY ts DESC limit 1; ",
            Self::get_tale_name(inst_id, time_interval)
        );
        debug!("query: {}", query);
        let res: Option<CandlesEntity> = self.db.query_decode(&query, vec![]).await?;
        debug!("result: {:?}", res);
        Ok(res)
    }

    pub async fn get_one_by_ts(
        &self,
        inst_id: &str,
        time_interval: &str,
        ts: i64,
    ) -> Result<Option<CandlesEntity>> {
        let mut query = format!(
            "select * from  `{}` where `ts` = {} ORDER BY ts DESC limit 1; ",
            Self::get_tale_name(inst_id, time_interval),
            ts
        );
        debug!("query: {}", query);
        let res: Option<CandlesEntity> = self.db.query_decode(&query, vec![]).await?;
        debug!("result: {:?}", res);
        Ok(res)
    }

    pub async fn get_oldest_data(
        &self,
        inst_id: &str,
        time_interval: &str,
    ) -> Result<Option<CandlesEntity>> {
        let mut query = format!(
            "select * from  `{}` ORDER BY ts ASC limit 1; ",
            Self::get_tale_name(inst_id, time_interval)
        );
        debug!("query: {}", query);
        let res: Option<CandlesEntity> = self.db.query_decode(&query, vec![]).await?;
        debug!("result: {:?}", res);
        Ok(res)
    }

    pub async fn get_new_count(
        &self,
        inst_id: &str,
        time_interval: &str,
        mut limit: Option<i32>,
    ) -> Result<u64> {
        if limit.is_none() {
            limit = Option::from(30000);
        }
        let mut query = format!(
            "select count(*) from  `{}` ORDER BY ts DESC limit {};",
            Self::get_tale_name(inst_id, time_interval),
            limit.unwrap()
        );
        debug!("query: {}", query);
        let res: u64 = self.db.query_decode(&query, vec![]).await?;
        debug!("result: {:?}", res);
        Ok(res)
    }

    pub async fn fetch_candles_from_mysql(
        &self,
        ins_id: &str,
        time: &str,
        limit: usize,
        select_time: Option<SelectTime>,
    ) -> anyhow::Result<Vec<CandlesEntity>> {
        let candles_model = CandlesModel::new().await;
        let candles = candles_model
            .get_all(ins_id, time, limit, select_time)
            .await;
        match candles {
            Ok(mut data) => {
                data.sort_unstable_by(|a, b| a.ts.cmp(&b.ts));
                Ok(data)
            }
            Err(e) => {
                error!("Error fetching candles from MySQL: {}", e);
                Err(anyhow::anyhow!("Error fetching candles from MySQL"))
            }
        }
    }
}
