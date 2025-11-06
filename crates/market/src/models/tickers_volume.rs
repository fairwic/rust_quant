use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, MySql, QueryBuilder};
use rust_quant_core::database::get_db_pool;

/// Tickers Volume 数据表实体
#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct TickersVolume {
    #[sqlx(default)]
    pub id: Option<i64>,
    pub inst_id: String,
    pub period: String,
    pub ts: i64,
    pub oi: String,
    pub vol: String,
}

pub struct TickersVolumeModel;

impl TickersVolumeModel {
    pub fn new() -> Self {
        Self
    }

    /// 根据 inst_id 查询
    pub async fn find_one(&self, inst_id: &str) -> Result<Vec<TickersVolume>> {
        let pool = get_db_pool();
        let results = sqlx::query_as::<_, TickersVolume>(
            "SELECT * FROM tickers_volume WHERE inst_id = ?"
        )
        .bind(inst_id)
        .fetch_all(pool)
        .await?;
        
        Ok(results)
    }

    /// 根据 inst_id 删除
    pub async fn delete_by_inst_id(&self, inst_id: &str) -> Result<u64> {
        let pool = get_db_pool();
        let result = sqlx::query("DELETE FROM tickers_volume WHERE inst_id = ?")
            .bind(inst_id)
            .execute(pool)
            .await?;
        
        Ok(result.rows_affected())
    }

    /// 批量插入
    pub async fn add(&self, list: Vec<TickersVolume>) -> Result<u64> {
        if list.is_empty() {
            return Ok(0);
        }

        let pool = get_db_pool();
        let mut query_builder: QueryBuilder<MySql> = QueryBuilder::new(
            "INSERT INTO tickers_volume (inst_id, period, ts, oi, vol) "
        );

        query_builder.push_values(list.iter(), |mut b, ticker| {
            b.push_bind(&ticker.inst_id)
                .push_bind(&ticker.period)
                .push_bind(ticker.ts)
                .push_bind(&ticker.oi)
                .push_bind(&ticker.vol);
        });

        let result = query_builder.build().execute(pool).await?;
        
        Ok(result.rows_affected())
    }
}

impl Default for TickersVolumeModel {
    fn default() -> Self {
        Self::new()
    }
}
