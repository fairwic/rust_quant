use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_quant_domain::entities::{
    FundFlowAlert, FundFlowSide, MarketAnomaly, MarketRankEvent, MarketRankSnapshot,
    MarketVelocityEpisode, MarketVelocityEpisodeWrite,
};
use rust_quant_domain::traits::fund_monitoring_repository::{
    FundFlowAlertRepository, MarketAnomalyRepository,
};
use sqlx::{PgPool, Row};

pub struct SqlxMarketAnomalyRepository {
    pool: PgPool,
}

impl SqlxMarketAnomalyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn rank_improved(new_rank: Option<i32>, old_best_rank: Option<i32>) -> bool {
    match (new_rank, old_best_rank) {
        (Some(new_rank), Some(old_best_rank)) => new_rank < old_best_rank,
        (Some(_), None) => true,
        _ => false,
    }
}

fn delta_improved(new_delta: Option<i32>, old_max_delta: Option<i32>) -> bool {
    match (new_delta, old_max_delta) {
        (Some(new_delta), Some(old_max_delta)) => new_delta > old_max_delta,
        (Some(_), None) => true,
        _ => false,
    }
}

#[async_trait]
impl MarketAnomalyRepository for SqlxMarketAnomalyRepository {
    async fn save(&self, anomaly: &MarketAnomaly) -> Result<i64> {
        let inserted_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO market_anomalies 
                (symbol, current_rank, rank_15m_ago, rank_4h_ago, rank_24h_ago, 
                 delta_15m, delta_4h, delta_24h, volume_24h, updated_at, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (symbol) DO UPDATE SET
                current_rank = EXCLUDED.current_rank,
                rank_15m_ago = EXCLUDED.rank_15m_ago,
                rank_4h_ago = EXCLUDED.rank_4h_ago,
                rank_24h_ago = EXCLUDED.rank_24h_ago,
                delta_15m = EXCLUDED.delta_15m,
                delta_4h = EXCLUDED.delta_4h,
                delta_24h = EXCLUDED.delta_24h,
                volume_24h = EXCLUDED.volume_24h,
                updated_at = EXCLUDED.updated_at,
                status = EXCLUDED.status
            RETURNING id
            "#,
        )
        .bind(&anomaly.symbol)
        .bind(anomaly.current_rank)
        .bind(anomaly.rank_15m_ago)
        .bind(anomaly.rank_4h_ago)
        .bind(anomaly.rank_24h_ago)
        .bind(anomaly.delta_15m)
        .bind(anomaly.delta_4h)
        .bind(anomaly.delta_24h)
        .bind(anomaly.volume_24h)
        .bind(anomaly.updated_at)
        .bind(&anomaly.status)
        .fetch_one(&self.pool)
        .await?;

        Ok(inserted_id)
    }

    async fn mark_exited(&self, symbol: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE market_anomalies SET status = 'EXITED', updated_at = NOW()
            WHERE symbol = $1
            "#,
        )
        .bind(symbol)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_latest_update_time(&self) -> Result<Option<DateTime<Utc>>> {
        let row = sqlx::query(
            r#"SELECT MAX(updated_at) as max_time FROM market_anomalies WHERE status = 'ACTIVE'"#,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let max_time: Option<DateTime<Utc>> = row.try_get("max_time").ok();
            Ok(max_time)
        } else {
            Ok(None)
        }
    }

    async fn get_all_active(&self) -> Result<Vec<MarketAnomaly>> {
        let rows = sqlx::query(
            r#"
            SELECT id, symbol, current_rank, rank_15m_ago, rank_4h_ago, rank_24h_ago,
                   delta_15m, delta_4h, delta_24h, volume_24h, updated_at, status
            FROM market_anomalies 
            WHERE status = 'ACTIVE'
            ORDER BY current_rank ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            result.push(MarketAnomaly {
                id: row.try_get("id").ok(),
                symbol: row.try_get("symbol")?,
                current_rank: row.try_get("current_rank")?,
                rank_15m_ago: row.try_get("rank_15m_ago").ok(),
                rank_4h_ago: row.try_get("rank_4h_ago").ok(),
                rank_24h_ago: row.try_get("rank_24h_ago").ok(),
                delta_15m: row.try_get("delta_15m").ok(),
                delta_4h: row.try_get("delta_4h").ok(),
                delta_24h: row.try_get("delta_24h").ok(),
                volume_24h: row
                    .try_get::<Option<Decimal>, _>("volume_24h")
                    .unwrap_or(None),
                updated_at: row.try_get("updated_at")?,
                status: row.try_get("status")?,
            });
        }
        Ok(result)
    }

    async fn clear_stale_period_data(
        &self,
        clear_15m: bool,
        clear_4h: bool,
        clear_24h: bool,
    ) -> Result<()> {
        let mut updates = Vec::new();
        if clear_15m {
            updates.push("rank_15m_ago = NULL, delta_15m = NULL");
        }
        if clear_4h {
            updates.push("rank_4h_ago = NULL, delta_4h = NULL");
        }
        if clear_24h {
            updates.push("rank_24h_ago = NULL, delta_24h = NULL");
        }

        if updates.is_empty() {
            return Ok(());
        }

        let sql = format!(
            "UPDATE market_anomalies SET {} WHERE status = 'ACTIVE'",
            updates.join(", ")
        );
        sqlx::query(&sql).execute(&self.pool).await?;
        Ok(())
    }

    async fn save_rank_event(&self, event: &MarketRankEvent) -> Result<i64> {
        let technical_snapshot = event.technical_snapshot.as_ref();
        let inserted_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO market_rank_events
                (exchange, symbol, event_type, timeframe, old_rank, new_rank, delta_rank,
                 volume_24h_quote, current_price, previous_price, price_change_pct,
                 price_direction, technical_timeframe, technical_period, technical_close_price,
                 technical_ma_value, technical_ema_value, technical_ma_distance_pct,
                 technical_ema_distance_pct, technical_ma_state, technical_ema_state,
                 technical_candle_count, technical_snapshot_at, technical_snapshot_status,
                 detected_at, source, notification_state)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                    $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                    $21, $22, $23, $24, $25, $26, $27)
            RETURNING id
            "#,
        )
        .bind(&event.exchange)
        .bind(&event.symbol)
        .bind(event.event_type.as_str())
        .bind(&event.timeframe)
        .bind(event.old_rank)
        .bind(event.new_rank)
        .bind(event.delta_rank)
        .bind(event.volume_24h_quote)
        .bind(event.current_price)
        .bind(event.previous_price)
        .bind(event.price_change_pct)
        .bind(&event.price_direction)
        .bind(technical_snapshot.map(|snapshot| snapshot.timeframe.as_str()))
        .bind(technical_snapshot.map(|snapshot| snapshot.period))
        .bind(technical_snapshot.map(|snapshot| snapshot.close_price))
        .bind(technical_snapshot.map(|snapshot| snapshot.ma_value))
        .bind(technical_snapshot.map(|snapshot| snapshot.ema_value))
        .bind(technical_snapshot.map(|snapshot| snapshot.ma_distance_pct))
        .bind(technical_snapshot.map(|snapshot| snapshot.ema_distance_pct))
        .bind(technical_snapshot.map(|snapshot| snapshot.ma_state.as_str()))
        .bind(technical_snapshot.map(|snapshot| snapshot.ema_state.as_str()))
        .bind(technical_snapshot.map(|snapshot| snapshot.candle_count))
        .bind(technical_snapshot.map(|snapshot| snapshot.snapshot_at))
        .bind(&event.technical_snapshot_status)
        .bind(event.detected_at)
        .bind(&event.source)
        .bind(&event.notification_state)
        .fetch_one(&self.pool)
        .await?;

        Ok(inserted_id)
    }

    async fn upsert_market_velocity_episode(
        &self,
        episode: &MarketVelocityEpisode,
    ) -> Result<(i64, MarketVelocityEpisodeWrite)> {
        let mut tx = self.pool.begin().await?;
        let existing = sqlx::query(
            r#"
            SELECT id, best_new_rank, max_delta_rank
              FROM market_velocity_episodes
             WHERE LOWER(exchange) = LOWER($1)
               AND symbol = $2
               AND event_type = $3
               AND COALESCE(timeframe, '') = COALESCE($4, '')
               AND status = 'active'
             FOR UPDATE
            "#,
        )
        .bind(&episode.exchange)
        .bind(&episode.symbol)
        .bind(episode.event_type.as_str())
        .bind(&episode.timeframe)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = existing else {
            let id = sqlx::query_scalar::<_, i64>(
                r#"
                INSERT INTO market_velocity_episodes
                    (exchange, symbol, event_type, timeframe, status, started_at, last_seen_at,
                     first_old_rank, latest_old_rank, latest_new_rank, best_new_rank,
                     latest_delta_rank, max_delta_rank, hit_count, volume_24h_quote,
                     current_price, previous_price, price_change_pct, price_direction,
                     technical_snapshot_status)
                VALUES ($1, $2, $3, $4, 'active', $5, $6, $7, $7, $8, $8,
                        $9, $9, 1, $10, $11, $12, $13, $14, $15)
                RETURNING id
                "#,
            )
            .bind(&episode.exchange)
            .bind(&episode.symbol)
            .bind(episode.event_type.as_str())
            .bind(&episode.timeframe)
            .bind(episode.started_at)
            .bind(episode.last_seen_at)
            .bind(episode.first_old_rank)
            .bind(episode.latest_new_rank)
            .bind(episode.latest_delta_rank)
            .bind(episode.volume_24h_quote)
            .bind(episode.current_price)
            .bind(episode.previous_price)
            .bind(episode.price_change_pct)
            .bind(&episode.price_direction)
            .bind(&episode.technical_snapshot_status)
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok((id, MarketVelocityEpisodeWrite::Created));
        };

        let id: i64 = row.try_get("id")?;
        let old_best_rank: Option<i32> = row.try_get("best_new_rank")?;
        let old_max_delta: Option<i32> = row.try_get("max_delta_rank")?;
        let escalated = rank_improved(episode.latest_new_rank, old_best_rank)
            || delta_improved(episode.latest_delta_rank, old_max_delta);

        sqlx::query(
            r#"
            UPDATE market_velocity_episodes
               SET last_seen_at = $2,
                   latest_old_rank = $3,
                   latest_new_rank = $4,
                   best_new_rank = CASE
                       WHEN best_new_rank IS NULL THEN $4
                       WHEN $4::INTEGER IS NULL THEN best_new_rank
                       ELSE LEAST(best_new_rank, $4)
                   END,
                   latest_delta_rank = $5,
                   max_delta_rank = CASE
                       WHEN max_delta_rank IS NULL THEN $5
                       WHEN $5::INTEGER IS NULL THEN max_delta_rank
                       ELSE GREATEST(max_delta_rank, $5)
                   END,
                   hit_count = hit_count + 1,
                   volume_24h_quote = $6,
                   current_price = $7,
                   previous_price = $8,
                   price_change_pct = $9,
                   price_direction = $10,
                   technical_snapshot_status = $11,
                   updated_at = NOW()
             WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(episode.last_seen_at)
        .bind(episode.latest_old_rank)
        .bind(episode.latest_new_rank)
        .bind(episode.latest_delta_rank)
        .bind(episode.volume_24h_quote)
        .bind(episode.current_price)
        .bind(episode.previous_price)
        .bind(episode.price_change_pct)
        .bind(&episode.price_direction)
        .bind(&episode.technical_snapshot_status)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        let write = if escalated {
            MarketVelocityEpisodeWrite::Escalated
        } else {
            MarketVelocityEpisodeWrite::Updated
        };
        Ok((id, write))
    }

    async fn attach_rank_event_to_market_velocity_episode(
        &self,
        episode_id: i64,
        rank_event_id: i64,
        escalated_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE market_velocity_episodes
               SET last_rank_event_id = $2,
                   last_escalated_at = $3,
                   updated_at = NOW()
             WHERE id = $1
            "#,
        )
        .bind(episode_id)
        .bind(rank_event_id)
        .bind(escalated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn close_stale_market_velocity_episodes(
        &self,
        exchange: &str,
        stale_before: DateTime<Utc>,
    ) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE market_velocity_episodes
               SET status = 'closed',
                   updated_at = NOW()
             WHERE LOWER(exchange) = LOWER($1)
               AND status = 'active'
               AND last_seen_at < $2
            "#,
        )
        .bind(exchange)
        .bind(stale_before)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn save_rank_snapshots(&self, snapshots: &[MarketRankSnapshot]) -> Result<()> {
        if snapshots.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;
        for snapshot in snapshots {
            sqlx::query(
                r#"
                INSERT INTO market_rank_snapshots
                    (exchange, symbol, rank, price, volume_24h_quote, captured_at)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (exchange, symbol, captured_at) DO UPDATE SET
                    rank = EXCLUDED.rank,
                    price = EXCLUDED.price,
                    volume_24h_quote = EXCLUDED.volume_24h_quote
                "#,
            )
            .bind(&snapshot.exchange)
            .bind(&snapshot.symbol)
            .bind(snapshot.rank)
            .bind(snapshot.price)
            .bind(snapshot.volume_24h_quote)
            .bind(snapshot.captured_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn load_recent_rank_snapshots(
        &self,
        exchange: &str,
        since: DateTime<Utc>,
    ) -> Result<Vec<MarketRankSnapshot>> {
        let rows = sqlx::query(
            r#"
            SELECT id, exchange, symbol, rank, price, volume_24h_quote, captured_at, created_at
            FROM market_rank_snapshots
            WHERE LOWER(exchange) = LOWER($1)
              AND captured_at >= $2
            ORDER BY captured_at ASC, rank ASC, symbol ASC
            "#,
        )
        .bind(exchange)
        .bind(since)
        .fetch_all(&self.pool)
        .await?;

        let mut snapshots = Vec::with_capacity(rows.len());
        for row in rows {
            snapshots.push(MarketRankSnapshot {
                id: row.try_get("id").ok(),
                exchange: row.try_get("exchange")?,
                symbol: row.try_get("symbol")?,
                rank: row.try_get("rank")?,
                price: row.try_get("price")?,
                volume_24h_quote: row.try_get("volume_24h_quote")?,
                captured_at: row.try_get("captured_at")?,
                created_at: row.try_get("created_at")?,
            });
        }
        Ok(snapshots)
    }

    async fn delete_rank_snapshots_before(&self, before: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM market_rank_snapshots
            WHERE captured_at < $1
            "#,
        )
        .bind(before)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

pub struct SqlxFundFlowAlertRepository {
    pool: PgPool,
}

impl SqlxFundFlowAlertRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FundFlowAlertRepository for SqlxFundFlowAlertRepository {
    async fn save(&self, alert: &FundFlowAlert) -> Result<i64> {
        let side_str = match alert.side {
            FundFlowSide::Inflow => "INFLOW",
            FundFlowSide::Outflow => "OUTFLOW",
        };

        let inserted_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO fund_flow_alerts (symbol, net_inflow, total_volume, side, window_secs, alert_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(&alert.symbol)
        .bind(alert.net_inflow)
        .bind(alert.total_volume)
        .bind(side_str)
        .bind(alert.window_secs)
        .bind(alert.alert_at)
        .fetch_one(&self.pool)
        .await?;

        Ok(inserted_id)
    }
}
