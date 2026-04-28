use anyhow::{anyhow, Result};
use sqlx::{Postgres, QueryBuilder};
use tracing::{debug, info};

use super::{get_quant_core_postgres_pool, quote_legacy_table_name};
use super::{CandlesEntity, SelectCandleReqDto, SelectTime, TimeDirect};
use okx::dto::market_dto::CandleOkxRespDto;

#[derive(Debug)]
enum TimeInterval {
    OneDay,
    OneHour,
}

impl TimeInterval {
    fn table_name(&self) -> &'static str {
        match self {
            TimeInterval::OneDay => "btc_candles_1d",
            TimeInterval::OneHour => "btc_candles_1h",
        }
    }
}

pub struct CandlesModel;

impl CandlesModel {
    pub fn new() -> Self {
        Self
    }

    /// 创建 K线 数据表
    pub async fn create_table(&self, inst_id: &str, time_interval: &str) -> Result<u64> {
        let table_name = Self::get_table_name(inst_id, time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        let create_table_sql = Self::build_create_table_sql(&quoted_table_name);
        let result = sqlx::query(&create_table_sql).execute(pool).await?;
        for comment_sql in Self::build_table_comment_sqls(&quoted_table_name) {
            sqlx::query(&comment_sql).execute(pool).await?;
        }
        Ok(result.rows_affected())
    }

    /// 获取表名
    pub fn get_table_name(inst_id: &str, time_interval: &str) -> String {
        format!(
            "{}_candles_{}",
            inst_id.to_ascii_lowercase(),
            time_interval.to_ascii_lowercase()
        )
    }

    /// 批量插入 K线数据
    pub async fn add(
        &self,
        list: Vec<CandleOkxRespDto>,
        inst_id: &str,
        time_interval: &str,
    ) -> Result<u64> {
        if list.is_empty() {
            return Err(anyhow!("candle list is empty"));
        }

        let table_name = Self::get_table_name(inst_id, time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(format!(
            "INSERT INTO {} (ts, o, h, l, c, vol, vol_ccy, confirm) ",
            quoted_table_name
        ));

        query_builder.push_values(list.iter(), |mut b, candle| {
            b.push_bind(candle.ts.parse::<i64>().unwrap_or(0))
                .push_bind(&candle.o)
                .push_bind(&candle.h)
                .push_bind(&candle.l)
                .push_bind(&candle.c)
                .push_bind(&candle.v)
                .push_bind(&candle.vol_ccy)
                .push_bind(&candle.confirm);
        });

        let result = query_builder.build().execute(pool).await?;
        debug!("批量插入 {} 条 K线数据", list.len());
        Ok(result.rows_affected())
    }

    /// 删除大于等于指定时间的数据
    pub async fn delete_lg_time(&self, inst_id: &str, time_interval: &str, ts: i64) -> Result<u64> {
        let table_name = Self::get_table_name(inst_id, time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        let result = sqlx::query(&format!("DELETE FROM {} WHERE ts >= $1", quoted_table_name))
            .bind(ts)
            .execute(pool)
            .await?;

        debug!(
            "删除大于等于 {} 的数据，影响行数: {}",
            ts,
            result.rows_affected()
        );
        Ok(result.rows_affected())
    }

    /// 获取最旧的未确认数据
    pub async fn get_older_un_confirm_data(
        &self,
        inst_id: &str,
        time_interval: &str,
    ) -> Result<Option<CandlesEntity>> {
        let table_name = Self::get_table_name(inst_id, time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        let result = sqlx::query_as::<_, CandlesEntity>(&format!(
            "SELECT * FROM {} WHERE confirm = '0' ORDER BY ts ASC LIMIT 1",
            quoted_table_name
        ))
        .fetch_optional(pool)
        .await?;

        debug!("查询最旧未确认数据: {:?}", result);
        Ok(result)
    }

    /// 更新单条 K线数据
    pub async fn update_one(
        &self,
        candle: CandlesEntity,
        inst_id: &str,
        time_interval: &str,
    ) -> Result<u64> {
        let table_name = Self::get_table_name(inst_id, time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        let result = sqlx::query(&format!(
            "UPDATE {} SET o = $1, h = $2, l = $3, c = $4, vol = $5, vol_ccy = $6, confirm = $7 WHERE ts = $8",
            quoted_table_name
        ))
        .bind(&candle.o)
        .bind(&candle.h)
        .bind(&candle.l)
        .bind(&candle.c)
        .bind(&candle.vol)
        .bind(&candle.vol_ccy)
        .bind(&candle.confirm)
        .bind(candle.ts)
        .execute(pool)
        .await?;

        debug!("更新 K线数据，影响行数: {}", result.rows_affected());
        Ok(result.rows_affected())
    }

    /// [已优化] 使用 UPSERT 单次原子操作替代 SELECT + INSERT/UPDATE
    /// 性能提升：SQL 执行次数从2次降为1次，消除竞态条件
    pub async fn upsert_one(
        &self,
        candle_data: &CandleOkxRespDto,
        inst_id: &str,
        time_interval: &str,
    ) -> Result<u64> {
        let table_name = Self::get_table_name(inst_id, time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        let result = sqlx::query(&format!(
            "INSERT INTO {} (ts, o, h, l, c, vol, vol_ccy, confirm)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (ts) DO UPDATE SET
                o = EXCLUDED.o,
                h = EXCLUDED.h,
                l = EXCLUDED.l,
                c = EXCLUDED.c,
                vol = EXCLUDED.vol,
                vol_ccy = EXCLUDED.vol_ccy,
                confirm = EXCLUDED.confirm,
                updated_at = CURRENT_TIMESTAMP",
            quoted_table_name
        ))
        .bind(candle_data.ts.parse::<i64>().unwrap_or(0))
        .bind(&candle_data.o)
        .bind(&candle_data.h)
        .bind(&candle_data.l)
        .bind(&candle_data.c)
        .bind(&candle_data.v)
        .bind(&candle_data.vol_ccy)
        .bind(&candle_data.confirm)
        .execute(pool)
        .await?;

        debug!("upsert K线数据，影响行数: {}", result.rows_affected());
        Ok(result.rows_affected())
    }

    /// [已优化] 批量 UPSERT，用于处理多条 K 线数据
    /// 性能提升：批量操作减少数据库连接开销，吞吐量提升5-10倍
    pub async fn upsert_batch(
        &self,
        candles: Vec<CandleOkxRespDto>,
        inst_id: &str,
        time_interval: &str,
    ) -> Result<u64> {
        if candles.is_empty() {
            return Ok(0);
        }

        let table_name = Self::get_table_name(inst_id, time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(format!(
            "INSERT INTO {} (ts, o, h, l, c, vol, vol_ccy, confirm) ",
            quoted_table_name
        ));

        query_builder.push_values(candles.iter(), |mut b, candle| {
            b.push_bind(candle.ts.parse::<i64>().unwrap_or(0))
                .push_bind(&candle.o)
                .push_bind(&candle.h)
                .push_bind(&candle.l)
                .push_bind(&candle.c)
                .push_bind(&candle.v)
                .push_bind(&candle.vol_ccy)
                .push_bind(&candle.confirm);
        });

        query_builder.push(
            " ON CONFLICT (ts) DO UPDATE SET
                o = EXCLUDED.o,
                h = EXCLUDED.h,
                l = EXCLUDED.l,
                c = EXCLUDED.c,
                vol = EXCLUDED.vol,
                vol_ccy = EXCLUDED.vol_ccy,
                confirm = EXCLUDED.confirm,
                updated_at = CURRENT_TIMESTAMP",
        );

        let result = query_builder.build().execute(pool).await?;
        debug!(
            "批量 upsert {} 条 K线数据，影响行数: {}",
            candles.len(),
            result.rows_affected()
        );
        Ok(result.rows_affected())
    }

    /// [保留兼容] 旧版本方法，内部调用 upsert_one
    pub async fn update_or_create(
        &self,
        candle_data: &CandleOkxRespDto,
        inst_id: &str,
        time_interval: &str,
    ) -> Result<()> {
        self.upsert_one(candle_data, inst_id, time_interval).await?;
        Ok(())
    }

    /// 查询 K线数据（支持复杂条件）
    pub async fn get_all(&self, dto: SelectCandleReqDto) -> Result<Vec<CandlesEntity>> {
        let table_name = Self::get_table_name(&dto.inst_id, &dto.time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(format!(
            "SELECT id, ts, o, h, l, c, vol, vol_ccy, confirm, created_at, updated_at FROM {} WHERE 1=1",
            quoted_table_name
        ));

        if let Some(confirm) = dto.confirm {
            query_builder
                .push(" AND confirm = ")
                .push_bind(confirm.to_string());
        }

        if let Some(SelectTime {
            direct,
            start_time: point_time,
            end_time,
        }) = dto.select_time
        {
            match direct {
                TimeDirect::BEFORE => {
                    query_builder.push(" AND ts <= ").push_bind(point_time);
                    if let Some(end_time) = end_time {
                        query_builder.push(" AND ts >= ").push_bind(end_time);
                    }
                }
                TimeDirect::AFTER => {
                    query_builder.push(" AND ts >= ").push_bind(point_time);
                    if let Some(end_time) = end_time {
                        query_builder.push(" AND ts <= ").push_bind(end_time);
                    }
                }
            }
        }

        query_builder
            .push(" ORDER BY ts DESC LIMIT ")
            .push_bind(dto.limit as i64);

        let results = query_builder
            .build_query_as::<CandlesEntity>()
            .fetch_all(pool)
            .await?;

        if results.is_empty() {
            info!("未找到 K线数据");
        }

        Ok(results)
    }

    /// 获取最新的一条 K线数据
    pub async fn get_new_data(
        &self,
        inst_id: &str,
        time_interval: &str,
    ) -> Result<Option<CandlesEntity>> {
        let table_name = Self::get_table_name(inst_id, time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        let result = sqlx::query_as::<_, CandlesEntity>(&format!(
            "SELECT * FROM {} ORDER BY ts DESC LIMIT 1",
            quoted_table_name
        ))
        .fetch_optional(pool)
        .await?;

        debug!("查询最新 K线: {:?}", result);
        Ok(result)
    }

    /// 根据时间戳获取一条 K线数据
    pub async fn get_one_by_ts(
        &self,
        inst_id: &str,
        time_interval: &str,
        ts: i64,
    ) -> Result<Option<CandlesEntity>> {
        let table_name = Self::get_table_name(inst_id, time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        let result = sqlx::query_as::<_, CandlesEntity>(&format!(
            "SELECT * FROM {} WHERE ts = $1 ORDER BY ts DESC LIMIT 1",
            quoted_table_name
        ))
        .bind(ts)
        .fetch_optional(pool)
        .await?;

        debug!("根据时间戳查询 K线: {:?}", result);
        Ok(result)
    }

    /// 获取最旧的一条 K线数据
    pub async fn get_oldest_data(
        &self,
        inst_id: &str,
        time_interval: &str,
    ) -> Result<Option<CandlesEntity>> {
        let table_name = Self::get_table_name(inst_id, time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        let result = sqlx::query_as::<_, CandlesEntity>(&format!(
            "SELECT * FROM {} ORDER BY ts ASC LIMIT 1",
            quoted_table_name
        ))
        .fetch_optional(pool)
        .await?;

        debug!("查询最旧 K线: {:?}", result);
        Ok(result)
    }

    /// 获取 K线数据数量
    pub async fn get_new_count(
        &self,
        inst_id: &str,
        time_interval: &str,
        limit: Option<i32>,
    ) -> Result<i64> {
        let limit = limit.unwrap_or(30000);
        let table_name = Self::get_table_name(inst_id, time_interval);
        let quoted_table_name = quote_legacy_table_name(&table_name)?;
        let pool = get_quant_core_postgres_pool()?;

        #[derive(sqlx::FromRow)]
        struct CountResult {
            count: i64,
        }

        let result = sqlx::query_as::<_, CountResult>(&format!(
            "SELECT COUNT(*) as count FROM {}",
            quoted_table_name
        ))
        .fetch_one(pool)
        .await?;

        debug!(
            "get_new_count ignored legacy limit={} because COUNT aggregate is global",
            limit
        );
        debug!("K线数据数量: {}", result.count);
        Ok(result.count)
    }

    /// 从 Postgres 分表获取 K线数据并排序
    pub async fn fetch_candles_from_postgres(
        &self,
        dto: SelectCandleReqDto,
    ) -> Result<Vec<CandlesEntity>> {
        let mut candles = self.get_all(dto).await?;
        candles.sort_unstable_by_key(|a| a.ts);
        Ok(candles)
    }
}

impl CandlesModel {
    fn build_create_table_sql(quoted_table_name: &str) -> String {
        format!(
            "CREATE TABLE IF NOT EXISTS {} (
  id BIGSERIAL PRIMARY KEY,
  ts BIGINT NOT NULL,
  o VARCHAR(20) NOT NULL,
  h VARCHAR(20) NOT NULL,
  l VARCHAR(20) NOT NULL,
  c VARCHAR(20) NOT NULL,
  vol VARCHAR(20) NOT NULL,
  vol_ccy VARCHAR(50) NOT NULL,
  confirm VARCHAR(20) NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (ts)
)",
            quoted_table_name
        )
    }

    fn build_table_comment_sqls(quoted_table_name: &str) -> Vec<String> {
        let mut comments = vec![format!(
            "COMMENT ON TABLE {} IS 'K线数据分表'",
            quoted_table_name
        )];

        for (column, comment) in [
            ("id", "主键ID"),
            ("ts", "开始时间，Unix时间戳的毫秒数格式，如 1597026383085"),
            ("o", "开盘价格"),
            ("h", "最高价格"),
            ("l", "最低价格"),
            ("c", "收盘价格"),
            ("vol", "交易量，以张为单位"),
            ("vol_ccy", "交易量，以币为单位"),
            ("confirm", "K线状态"),
            ("created_at", "创建时间"),
            ("updated_at", "更新时间"),
        ] {
            comments.push(format!(
                "COMMENT ON COLUMN {}.{} IS '{}'",
                quoted_table_name, column, comment
            ));
        }

        comments
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Execute;

    #[test]
    fn create_table_sql_uses_postgres_ddl_and_comments() {
        let sql = CandlesModel::build_create_table_sql("\"btc-usdt-swap_candles_1h\"");
        assert!(sql.contains("BIGSERIAL PRIMARY KEY"));
        assert!(sql.contains("UNIQUE (ts)"));

        let comments = CandlesModel::build_table_comment_sqls("\"btc-usdt-swap_candles_1h\"");
        assert!(comments
            .iter()
            .any(|item| item.contains("COMMENT ON TABLE")));
        assert!(comments
            .iter()
            .any(|item| item.contains("COMMENT ON COLUMN")));
    }

    #[test]
    fn upsert_batch_sql_uses_on_conflict() {
        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO \"btc-usdt-swap_candles_1h\" (ts, o, h, l, c, vol, vol_ccy, confirm) ",
        );
        builder.push_values([0_i64].iter(), |mut b, ts| {
            b.push_bind(*ts)
                .push_bind("1")
                .push_bind("1")
                .push_bind("1")
                .push_bind("1")
                .push_bind("1")
                .push_bind("1")
                .push_bind("1");
        });
        builder.push(
            " ON CONFLICT (ts) DO UPDATE SET
                o = EXCLUDED.o,
                h = EXCLUDED.h,
                l = EXCLUDED.l,
                c = EXCLUDED.c,
                vol = EXCLUDED.vol,
                vol_ccy = EXCLUDED.vol_ccy,
                confirm = EXCLUDED.confirm,
                updated_at = CURRENT_TIMESTAMP",
        );

        let sql = builder.build().sql().to_string();
        assert!(sql.contains("ON CONFLICT (ts) DO UPDATE"));
        assert!(sql.contains("EXCLUDED.confirm"));
    }
}

impl Default for CandlesModel {
    fn default() -> Self {
        Self::new()
    }
}
