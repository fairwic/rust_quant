//! K线数据访问层实现

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::NaiveDateTime;
use sqlx::{FromRow, MySql, PgPool, Pool};
use tracing::{debug, error};

use rust_quant_domain::traits::CandleRepository;
use rust_quant_domain::{Candle, Price, Timeframe, Volume};

/// K线数据库实体
#[derive(Debug, Clone, FromRow)]
struct CandlesEntity {
    #[sqlx(default)]
    pub id: Option<i64>,
    pub ts: i64,
    pub o: String,
    pub h: String,
    pub l: String,
    pub c: String,
    pub vol: String,
    pub vol_ccy: String,
    pub confirm: String,
    #[sqlx(default)]
    pub created_at: Option<NaiveDateTime>,
    #[sqlx(default)]
    pub updated_at: Option<NaiveDateTime>,
}

impl CandlesEntity {
    /// 转换为领域实体
    fn to_domain(&self, symbol: String, timeframe: Timeframe) -> Result<Candle> {
        let open = self
            .o
            .parse::<f64>()
            .map_err(|e| anyhow!("解析开盘价失败: {}", e))
            .and_then(|v| Price::new(v).map_err(|e| anyhow!("{:?}", e)))?;

        let high = self
            .h
            .parse::<f64>()
            .map_err(|e| anyhow!("解析最高价失败: {}", e))
            .and_then(|v| Price::new(v).map_err(|e| anyhow!("{:?}", e)))?;

        let low = self
            .l
            .parse::<f64>()
            .map_err(|e| anyhow!("解析最低价失败: {}", e))
            .and_then(|v| Price::new(v).map_err(|e| anyhow!("{:?}", e)))?;

        let close = self
            .c
            .parse::<f64>()
            .map_err(|e| anyhow!("解析收盘价失败: {}", e))
            .and_then(|v| Price::new(v).map_err(|e| anyhow!("{:?}", e)))?;

        let volume = self
            .vol_ccy
            .parse::<f64>()
            .map_err(|e| anyhow!("解析成交量失败: {}", e))
            .and_then(|v| Volume::new(v).map_err(|e| anyhow!("{:?}", e)))?;

        let mut candle = Candle::new(symbol, timeframe, self.ts, open, high, low, close, volume);

        // 设置确认状态
        if self.confirm == "1" {
            candle.confirm();
        }

        Ok(candle)
    }
}

/// 基于 sqlx 的 K线仓储实现
pub struct SqlxCandleRepository {
    pool: Pool<MySql>,
}

impl SqlxCandleRepository {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }

    /// 获取表名（根据交易对和时间周期）
    fn get_table_name(symbol: &str, timeframe: &Timeframe) -> String {
        // 表名与历史 CandlesModel 保持一致：使用 inst_id 原样（仅小写），允许 `-`，并用反引号包裹执行 SQL。
        // 例：BTC-USDT-SWAP + 4H => `btc-usdt-swap_candles_4h`
        let inst_id = symbol.to_ascii_lowercase();
        let time_interval = match timeframe {
            Timeframe::M1 => "1m",
            Timeframe::M3 => "3m",
            Timeframe::M5 => "5m",
            Timeframe::M15 => "15m",
            Timeframe::M30 => "30m",
            Timeframe::H1 => "1h",
            Timeframe::H2 => "2h",
            Timeframe::H4 => "4h",
            Timeframe::H6 => "6h",
            Timeframe::H12 => "12h",
            Timeframe::D1 => "1d",
            Timeframe::W1 => "1w",
            Timeframe::MN1 => "1M",
        };
        format!("{}_candles_{}", inst_id, time_interval)
    }
}

/// quant_core Postgres K线仓储实现。
///
/// 为了保持现有回测与策略代码的数据形态，Postgres 版本继续使用原来的
/// `{inst_id}_candles_{period}` 分表命名，不改为集中式单表。
pub struct PostgresCandleRepository {
    pool: PgPool,
}

impl PostgresCandleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn ensure_table(&self, symbol: &str, timeframe: Timeframe) -> Result<()> {
        let table_name = Self::quoted_table_name(symbol, timeframe)?;
        let create_table_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
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
            )
            "#,
            table_name
        );

        sqlx::query(&create_table_sql)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("创建 Postgres K线分表失败: {}", e);
                anyhow!("创建 Postgres K线分表失败: {}", e)
            })?;
        self.comment_table(&table_name).await?;

        Ok(())
    }

    async fn comment_table(&self, table_name: &str) -> Result<()> {
        let table_comment_sql = format!("COMMENT ON TABLE {} IS 'K线数据分表'", table_name);
        sqlx::query(&table_comment_sql)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("写入 Postgres K线分表注释失败: {}", e);
                anyhow!("写入 Postgres K线分表注释失败: {}", e)
            })?;

        for (column, comment) in [
            ("id", "主键ID"),
            ("ts", "时间戳"),
            ("o", "开盘价"),
            ("h", "最高价"),
            ("l", "最低价"),
            ("c", "收盘价"),
            ("vol", "成交量"),
            ("vol_ccy", "计价成交量"),
            ("confirm", "K线确认状态"),
            ("created_at", "创建时间"),
            ("updated_at", "更新时间"),
        ] {
            let column_comment_sql = format!(
                "COMMENT ON COLUMN {}.{} IS '{}'",
                table_name, column, comment
            );
            sqlx::query(&column_comment_sql)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    error!("写入 Postgres K线分表字段注释失败: {}", e);
                    anyhow!("写入 Postgres K线分表字段注释失败: {}", e)
                })?;
        }

        Ok(())
    }

    fn get_table_name(symbol: &str, timeframe: Timeframe) -> String {
        format!(
            "{}_candles_{}",
            symbol.to_ascii_lowercase(),
            timeframe.as_str().to_ascii_lowercase()
        )
    }

    pub fn quoted_table_name(symbol: &str, timeframe: Timeframe) -> Result<String> {
        let table_name = Self::get_table_name(symbol, timeframe);
        if table_name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-'))
        {
            Ok(format!("\"{}\"", table_name))
        } else {
            Err(anyhow!("非法 K线分表名: {}", table_name))
        }
    }
}

#[async_trait]
impl CandleRepository for PostgresCandleRepository {
    async fn find_candles(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_time: i64,
        end_time: i64,
        limit: Option<usize>,
    ) -> Result<Vec<Candle>> {
        self.ensure_table(symbol, timeframe).await?;
        let table_name = Self::quoted_table_name(symbol, timeframe)?;
        let limit = limit.unwrap_or(1000);

        let query = format!(
            "SELECT id, ts, o, h, l, c, vol, vol_ccy, confirm, created_at, updated_at
             FROM {}
             WHERE ts >= $1 AND ts <= $2
             ORDER BY ts ASC
             LIMIT $3",
            table_name
        );

        let entities = sqlx::query_as::<_, CandlesEntity>(&query)
            .bind(start_time)
            .bind(end_time)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!("查询 Postgres K线数据失败: {}", e);
                anyhow!("查询 Postgres K线数据失败: {}", e)
            })?;

        let candles = entities
            .into_iter()
            .filter_map(
                |entity| match entity.to_domain(symbol.to_string(), timeframe) {
                    Ok(candle) => Some(candle),
                    Err(e) => {
                        error!("转换 Postgres K线实体失败: {}", e);
                        None
                    }
                },
            )
            .collect();

        Ok(candles)
    }

    async fn get_latest_candle(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<Candle>> {
        self.ensure_table(symbol, timeframe).await?;
        let table_name = Self::quoted_table_name(symbol, timeframe)?;

        let query = format!(
            "SELECT id, ts, o, h, l, c, vol, vol_ccy, confirm, created_at, updated_at
             FROM {}
             ORDER BY ts DESC
             LIMIT 1",
            table_name
        );

        let entity = sqlx::query_as::<_, CandlesEntity>(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!("查询 Postgres 最新K线失败: {}", e);
                anyhow!("查询 Postgres 最新K线失败: {}", e)
            })?;

        match entity {
            Some(e) => Ok(Some(e.to_domain(symbol.to_string(), timeframe)?)),
            None => Ok(None),
        }
    }

    async fn save_candles(&self, candles: Vec<Candle>) -> Result<usize> {
        if candles.is_empty() {
            return Ok(0);
        }

        let first_candle = &candles[0];
        self.ensure_table(&first_candle.symbol, first_candle.timeframe)
            .await?;
        let table_name = Self::quoted_table_name(&first_candle.symbol, first_candle.timeframe)?;
        let mut saved_count = 0;

        for candle in candles {
            let query = format!(
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
                table_name
            );

            let result = sqlx::query(&query)
                .bind(candle.timestamp)
                .bind(candle.open.value().to_string())
                .bind(candle.high.value().to_string())
                .bind(candle.low.value().to_string())
                .bind(candle.close.value().to_string())
                .bind(candle.volume.value().to_string())
                .bind(candle.volume.value().to_string())
                .bind(if candle.confirmed { "1" } else { "0" })
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    error!("保存 Postgres K线数据失败: {}", e);
                    anyhow!("保存 Postgres K线数据失败: {}", e)
                })?;

            saved_count += result.rows_affected() as usize;
        }

        debug!("批量保存 Postgres K线数据，影响行数: {}", saved_count);
        Ok(saved_count)
    }
}

#[async_trait]
impl CandleRepository for SqlxCandleRepository {
    async fn find_candles(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_time: i64,
        end_time: i64,
        limit: Option<usize>,
    ) -> Result<Vec<Candle>> {
        let table_name = Self::get_table_name(symbol, &timeframe);
        let limit = limit.unwrap_or(1000);

        let query = format!(
            "SELECT id, ts, o, h, l, c, vol, vol_ccy, confirm, created_at, updated_at 
             FROM `{}` 
             WHERE ts >= ? AND ts <= ? 
             ORDER BY ts ASC 
             LIMIT ?",
            table_name
        );

        debug!(
            "查询K线数据: symbol={}, timeframe={:?}, start={}, end={}, limit={}",
            symbol, timeframe, start_time, end_time, limit
        );

        let entities = sqlx::query_as::<_, CandlesEntity>(&query)
            .bind(start_time)
            .bind(end_time)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!("查询K线数据失败: {}", e);
                anyhow!("查询K线数据失败: {}", e)
            })?;

        // 转换为领域实体
        let candles = entities
            .into_iter()
            .filter_map(
                |entity| match entity.to_domain(symbol.to_string(), timeframe) {
                    Ok(candle) => Some(candle),
                    Err(e) => {
                        error!("转换K线实体失败: {}", e);
                        None
                    }
                },
            )
            .collect();

        Ok(candles)
    }

    async fn get_latest_candle(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<Candle>> {
        let table_name = Self::get_table_name(symbol, &timeframe);

        let query = format!(
            "SELECT id, ts, o, h, l, c, vol, vol_ccy, confirm, created_at, updated_at 
             FROM `{}` 
             ORDER BY ts DESC 
             LIMIT 1",
            table_name
        );

        debug!("查询最新K线: symbol={}, timeframe={:?}", symbol, timeframe);

        let entity = sqlx::query_as::<_, CandlesEntity>(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!("查询最新K线失败: {}", e);
                anyhow!("查询最新K线失败: {}", e)
            })?;

        match entity {
            Some(e) => Ok(Some(e.to_domain(symbol.to_string(), timeframe)?)),
            None => Ok(None),
        }
    }

    async fn save_candles(&self, candles: Vec<Candle>) -> Result<usize> {
        if candles.is_empty() {
            return Ok(0);
        }

        // 按交易对和时间周期分组
        let mut saved_count = 0;

        // 简化实现：假设所有K线都是同一个交易对和时间周期
        // 实际使用中可能需要按 (symbol, timeframe) 分组处理
        let first_candle = &candles[0];
        let table_name = Self::get_table_name(&first_candle.symbol, &first_candle.timeframe);

        for candle in candles {
            let query = format!(
                "INSERT INTO `{}` (ts, o, h, l, c, vol, vol_ccy, confirm) 
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                 ON DUPLICATE KEY UPDATE 
                    o = VALUES(o),
                    h = VALUES(h),
                    l = VALUES(l),
                    c = VALUES(c),
                    vol = VALUES(vol),
                    vol_ccy = VALUES(vol_ccy),
                    confirm = VALUES(confirm),
                    updated_at = CURRENT_TIMESTAMP",
                table_name
            );

            let result = sqlx::query(&query)
                .bind(candle.timestamp)
                .bind(candle.open.value().to_string())
                .bind(candle.high.value().to_string())
                .bind(candle.low.value().to_string())
                .bind(candle.close.value().to_string())
                .bind(candle.volume.value().to_string())
                .bind(candle.volume.value().to_string())
                .bind(if candle.confirmed { "1" } else { "0" })
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    error!("保存K线数据失败: {}", e);
                    anyhow!("保存K线数据失败: {}", e)
                })?;

            saved_count += result.rows_affected() as usize;
        }

        debug!("批量保存K线数据，影响行数: {}", saved_count);
        Ok(saved_count)
    }
}
