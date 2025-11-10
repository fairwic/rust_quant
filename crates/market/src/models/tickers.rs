use anyhow::Result;
use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use okx::dto::market_dto::TickerOkxResDto;
use rust_quant_core::database::get_db_pool;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, MySql, QueryBuilder};
use std::collections::HashMap;
use tracing::debug;

/// Tickers 数据表实体
#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct TickersDataEntity {
    #[sqlx(default)]
    pub id: Option<i64>,
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

/// 查询结果辅助结构
#[derive(Serialize, Deserialize, Debug, FromRow)]
pub struct TickersDataQueryResult {
    pub inst_id: String,
    pub daily_vol: f64,
    pub ts: i64,
}

impl TickersDataQueryResult {
    /// 将时间戳转换为 NaiveDate
    pub fn get_date(&self) -> NaiveDate {
        #[allow(deprecated)]
        NaiveDateTime::from_timestamp_opt(self.ts / 1000, 0)
            .unwrap()
            .date()
    }

    /// 获取 24h 交易量
    pub fn get_vol24h(&self) -> f64 {
        self.daily_vol
    }
}

impl TickersDataEntity {
    /// 将 ts 字段转换为 NaiveDate
    pub fn get_date(&self) -> NaiveDate {
        let naive_datetime = Utc.timestamp_millis_opt(self.ts).unwrap().naive_utc();
        naive_datetime.date()
    }
}

pub struct TicketsModel;

impl TicketsModel {
    pub fn new() -> Self {
        Self
    }

    /// 批量插入 Ticker 数据
    pub async fn add(&self, list: Vec<TickerOkxResDto>) -> Result<u64> {
        if list.is_empty() {
            return Ok(0);
        }

        let pool = get_db_pool();
        let mut query_builder: QueryBuilder<MySql> = QueryBuilder::new(
            "INSERT INTO tickers_data (inst_type, inst_id, last, last_sz, ask_px, ask_sz, \
             bid_px, bid_sz, open24h, high24h, low24h, vol_ccy24h, vol24h, sod_utc0, sod_utc8, ts) "
        );

        query_builder.push_values(list.iter(), |mut b, ticker| {
            b.push_bind(&ticker.inst_type)
                .push_bind(&ticker.inst_id)
                .push_bind(&ticker.last)
                .push_bind(&ticker.last_sz)
                .push_bind(&ticker.ask_px)
                .push_bind(&ticker.ask_sz)
                .push_bind(&ticker.bid_px)
                .push_bind(&ticker.bid_sz)
                .push_bind(&ticker.open24h)
                .push_bind(&ticker.high24h)
                .push_bind(&ticker.low24h)
                .push_bind(&ticker.vol_ccy24h)
                .push_bind(&ticker.vol24h)
                .push_bind(&ticker.sod_utc0)
                .push_bind(&ticker.sod_utc8)
                .push_bind(ticker.ts.parse::<i64>().unwrap_or(0));
        });

        let result = query_builder.build().execute(pool).await?;
        debug!("批量插入 Ticker 数据，影响行数: {}", result.rows_affected());

        Ok(result.rows_affected())
    }

    /// 更新单个 Ticker 数据
    pub async fn update(&self, ticker: &TickerOkxResDto) -> Result<()> {
        let pool = get_db_pool();

        sqlx::query(
            "UPDATE tickers_data SET inst_type = ?, last = ?, last_sz = ?, ask_px = ?, \
             ask_sz = ?, bid_px = ?, bid_sz = ?, open24h = ?, high24h = ?, low24h = ?, \
             vol_ccy24h = ?, vol24h = ?, sod_utc0 = ?, sod_utc8 = ?, ts = ? \
             WHERE inst_id = ?",
        )
        .bind(&ticker.inst_type)
        .bind(&ticker.last)
        .bind(&ticker.last_sz)
        .bind(&ticker.ask_px)
        .bind(&ticker.ask_sz)
        .bind(&ticker.bid_px)
        .bind(&ticker.bid_sz)
        .bind(&ticker.open24h)
        .bind(&ticker.high24h)
        .bind(&ticker.low24h)
        .bind(&ticker.vol_ccy24h)
        .bind(&ticker.vol24h)
        .bind(&ticker.sod_utc0)
        .bind(&ticker.sod_utc8)
        .bind(ticker.ts.parse::<i64>().unwrap_or(0))
        .bind(&ticker.inst_id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// 获取指定合约的全部数据
    pub async fn get_all(&self, inst_ids: &Vec<String>) -> Result<Vec<TickersDataEntity>> {
        if inst_ids.is_empty() {
            return Ok(vec![]);
        }

        let pool = get_db_pool();
        let placeholders = inst_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT * FROM tickers_data WHERE inst_id IN ({}) AND inst_type='SWAP' ORDER BY id DESC",
            placeholders
        );

        let mut query = sqlx::query_as::<_, TickersDataEntity>(&sql);
        for inst_id in inst_ids {
            query = query.bind(inst_id);
        }

        let results = query.fetch_all(pool).await?;
        Ok(results)
    }

    /// 查找单个合约数据
    pub async fn find_one(&self, inst_id: &str) -> Result<Vec<TickersDataEntity>> {
        let pool = get_db_pool();
        let results =
            sqlx::query_as::<_, TickersDataEntity>("SELECT * FROM tickers_data WHERE inst_id = ?")
                .bind(inst_id)
                .fetch_all(pool)
                .await?;

        Ok(results)
    }

    /// 获取每日交易量
    pub async fn get_daily_volumes(
        &self,
        inst_ids: Option<Vec<&str>>,
    ) -> Result<Vec<(String, NaiveDate, f64)>> {
        let pool = get_db_pool();

        let (sql, params): (String, Vec<&str>) = if let Some(inst_ids) = inst_ids {
            let placeholders = inst_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT inst_id, MAX(ts) AS ts, SUM(vol24h) AS daily_vol \
                 FROM tickers_data \
                 WHERE inst_id IN ({}) \
                 GROUP BY inst_id, DATE(FROM_UNIXTIME(ts / 1000)) \
                 ORDER BY ts DESC",
                placeholders
            );
            (sql, inst_ids)
        } else {
            let sql = "SELECT inst_id, MAX(ts) AS ts, SUM(vol24h) AS daily_vol \
                       FROM tickers_data \
                       GROUP BY inst_id, DATE(FROM_UNIXTIME(ts / 1000)) \
                       ORDER BY ts DESC"
                .to_string();
            (sql, vec![])
        };

        let mut query = sqlx::query_as::<_, TickersDataQueryResult>(&sql);
        for param in params {
            query = query.bind(param);
        }

        let results = query.fetch_all(pool).await?;

        // 转换为包含日期和交易量的元组
        let daily_volumes = results
            .into_iter()
            .map(|entry| {
                let date = entry.get_date();
                (entry.inst_id.clone(), date, entry.get_vol24h())
            })
            .collect();

        Ok(daily_volumes)
    }

    /// 计算过去7天的平均交易量
    pub fn calculate_7_day_avg_volume(
        &self,
        daily_volumes: Vec<(String, NaiveDate, f64)>,
    ) -> HashMap<String, f64> {
        let mut daily_vol_map: HashMap<String, Vec<(NaiveDate, f64)>> = HashMap::new();

        // 按 inst_id 和日期分组
        for (inst_id, date, vol) in daily_volumes {
            daily_vol_map
                .entry(inst_id.clone())
                .or_insert_with(Vec::new)
                .push((date, vol));
        }

        // 计算每个合约的7天平均交易量
        let mut avg_volumes: HashMap<String, f64> = HashMap::new();
        for (inst_id, volumes) in daily_vol_map {
            let last_7_days = volumes
                .iter()
                .rev()
                .take(7)
                .map(|(_, vol)| *vol)
                .collect::<Vec<f64>>();

            if last_7_days.len() == 7 {
                let avg_vol = last_7_days.iter().sum::<f64>() / 7.0;
                avg_volumes.insert(inst_id, avg_vol);
            }
        }

        avg_volumes
    }

    /// 判断是否拉升的板块
    pub fn check_for_possible_lift(
        &self,
        daily_volumes: Vec<(String, NaiveDate, f64)>,
        avg_volumes: HashMap<String, f64>,
        threshold: f64,
    ) -> Vec<String> {
        let mut lifted_assets = Vec::new();

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

impl Default for TicketsModel {
    fn default() -> Self {
        Self::new()
    }
}
