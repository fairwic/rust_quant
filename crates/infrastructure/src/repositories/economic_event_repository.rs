//! 经济日历事件数据访问层实现

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use sqlx::{FromRow, MySql, Pool};
use tracing::error;

use rust_quant_domain::entities::economic_event::EconomicEvent;
use rust_quant_domain::traits::economic_event_repository::EconomicEventRepository;

/// 经济事件数据库实体
#[derive(Debug, Clone, FromRow)]
struct EconomicEventEntity {
    pub id: i64,
    pub calendar_id: String,
    pub event_time: i64,
    pub region: String,
    pub category: String,
    pub event: String,
    pub ref_date: String,
    pub actual: Option<String>,
    pub previous: Option<String>,
    pub forecast: Option<String>,
    pub importance: i32,
    pub updated_time: i64,
    pub prev_initial: Option<String>,
    pub currency: String,
    pub unit: Option<String>,
}

impl EconomicEventEntity {
    fn to_domain(&self) -> EconomicEvent {
        EconomicEvent {
            id: Some(self.id),
            calendar_id: self.calendar_id.clone(),
            event_time: self.event_time,
            region: self.region.clone(),
            category: self.category.clone(),
            event: self.event.clone(),
            ref_date: self.ref_date.clone(),
            actual: self.actual.clone(),
            previous: self.previous.clone(),
            forecast: self.forecast.clone(),
            importance: self.importance,
            updated_time: self.updated_time,
            prev_initial: self.prev_initial.clone(),
            currency: self.currency.clone(),
            unit: self.unit.clone(),
            created_at: None,
        }
    }
}

/// 基于 sqlx 的经济事件仓储实现
pub struct SqlxEconomicEventRepository {
    pool: Pool<MySql>,
}

impl SqlxEconomicEventRepository {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }

    /// 从 OKX DTO 转换为领域实体
    pub fn from_okx_dto(dto: &okx::dto::public_data_dto::EconomicEventOkxRespDto) -> EconomicEvent {
        EconomicEvent {
            id: None,
            calendar_id: dto.calendar_id.clone(),
            event_time: dto.date.parse().unwrap_or(0),
            region: dto.region.clone(),
            category: dto.category.clone(),
            event: dto.event.clone(),
            ref_date: dto.ref_date.clone(),
            actual: if dto.actual.is_empty() {
                None
            } else {
                Some(dto.actual.clone())
            },
            previous: if dto.previous.is_empty() {
                None
            } else {
                Some(dto.previous.clone())
            },
            forecast: if dto.forecast.is_empty() {
                None
            } else {
                Some(dto.forecast.clone())
            },
            importance: dto.importance.parse().unwrap_or(1),
            updated_time: dto.u_time.parse().unwrap_or(0),
            prev_initial: if dto.prev_initial.is_empty() {
                None
            } else {
                Some(dto.prev_initial.clone())
            },
            currency: dto.ccy.clone(),
            unit: if dto.unit.is_empty() {
                None
            } else {
                Some(dto.unit.clone())
            },
            created_at: None,
        }
    }
}

#[async_trait]
impl EconomicEventRepository for SqlxEconomicEventRepository {
    async fn save(&self, event: EconomicEvent) -> Result<()> {
        let query = "
            INSERT INTO economic_events (
                calendar_id, event_time, region, category, event, ref_date,
                actual, previous, forecast, importance, updated_time,
                prev_initial, currency, unit
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON DUPLICATE KEY UPDATE
                event_time = VALUES(event_time),
                region = VALUES(region),
                category = VALUES(category),
                event = VALUES(event),
                ref_date = VALUES(ref_date),
                actual = VALUES(actual),
                previous = VALUES(previous),
                forecast = VALUES(forecast),
                importance = VALUES(importance),
                updated_time = VALUES(updated_time),
                prev_initial = VALUES(prev_initial),
                currency = VALUES(currency),
                unit = VALUES(unit)
        ";

        sqlx::query(query)
            .bind(&event.calendar_id)
            .bind(event.event_time)
            .bind(&event.region)
            .bind(&event.category)
            .bind(&event.event)
            .bind(&event.ref_date)
            .bind(&event.actual)
            .bind(&event.previous)
            .bind(&event.forecast)
            .bind(event.importance)
            .bind(event.updated_time)
            .bind(&event.prev_initial)
            .bind(&event.currency)
            .bind(&event.unit)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("保存经济事件失败: {}", e);
                anyhow!("保存经济事件失败: {}", e)
            })?;

        Ok(())
    }

    async fn save_batch(&self, events: Vec<EconomicEvent>) -> Result<()> {
        for event in events {
            self.save(event).await?;
        }
        Ok(())
    }

    async fn find_by_calendar_id(&self, calendar_id: &str) -> Result<Option<EconomicEvent>> {
        let query = "SELECT * FROM economic_events WHERE calendar_id = ?";

        let entity = sqlx::query_as::<_, EconomicEventEntity>(query)
            .bind(calendar_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!("查询经济事件失败: {}", e);
                anyhow!("查询经济事件失败: {}", e)
            })?;

        Ok(entity.map(|e| e.to_domain()))
    }

    async fn find_by_time_range(
        &self,
        start_time: i64,
        end_time: i64,
        min_importance: Option<i32>,
    ) -> Result<Vec<EconomicEvent>> {
        let min_imp = min_importance.unwrap_or(1);
        let query = "
            SELECT * FROM economic_events 
            WHERE event_time >= ? AND event_time <= ? AND importance >= ?
            ORDER BY event_time ASC
        ";

        let entities = sqlx::query_as::<_, EconomicEventEntity>(query)
            .bind(start_time)
            .bind(end_time)
            .bind(min_imp)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!("查询经济事件时间范围失败: {}", e);
                anyhow!("查询经济事件时间范围失败: {}", e)
            })?;

        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }

    async fn find_latest_event_time(&self) -> Result<Option<i64>> {
        let query = "SELECT MAX(event_time) as max_time FROM economic_events";

        let row: Option<(Option<i64>,)> = sqlx::query_as(query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow!("查询最新事件时间失败: {}", e))?;

        Ok(row.and_then(|r| r.0))
    }

    async fn find_oldest_event_time(&self) -> Result<Option<i64>> {
        let query = "SELECT MIN(event_time) as min_time FROM economic_events";

        let row: Option<(Option<i64>,)> = sqlx::query_as(query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow!("查询最早事件时间失败: {}", e))?;

        Ok(row.and_then(|r| r.0))
    }

    async fn find_upcoming_events(
        &self,
        current_time: i64,
        window_ms: i64,
        min_importance: i32,
    ) -> Result<Vec<EconomicEvent>> {
        let end_time = current_time + window_ms;
        let query = "
            SELECT * FROM economic_events 
            WHERE event_time >= ? AND event_time <= ? AND importance >= ?
            ORDER BY event_time ASC
        ";

        let entities = sqlx::query_as::<_, EconomicEventEntity>(query)
            .bind(current_time)
            .bind(end_time)
            .bind(min_importance)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!("查询即将发生事件失败: {}", e);
                anyhow!("查询即将发生事件失败: {}", e)
            })?;

        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }

    async fn find_active_events(
        &self,
        current_time: i64,
        window_before_ms: i64,
        window_after_ms: i64,
        min_importance: i32,
    ) -> Result<Vec<EconomicEvent>> {
        // 查找 event_time 在 [current_time - window_after, current_time + window_before] 范围内的事件
        // 因为 event 发生在 event_time，如果当前是 event_time + x，那么 x < window_after 时仍在影响窗口内
        // 如果当前是 event_time - y，那么 y < window_before 时事件即将发生
        let start = current_time - window_after_ms;
        let end = current_time + window_before_ms;

        let query = "
            SELECT * FROM economic_events 
            WHERE event_time >= ? AND event_time <= ? AND importance >= ?
            ORDER BY event_time ASC
        ";

        let entities = sqlx::query_as::<_, EconomicEventEntity>(query)
            .bind(start)
            .bind(end)
            .bind(min_importance)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                error!("查询活跃经济事件失败: {}", e);
                anyhow!("查询活跃经济事件失败: {}", e)
            })?;

        Ok(entities.into_iter().map(|e| e.to_domain()).collect())
    }

    async fn count_by_importance(&self, start_time: i64, end_time: i64) -> Result<Vec<(i32, i64)>> {
        let query = "
            SELECT importance, COUNT(*) as cnt 
            FROM economic_events 
            WHERE event_time >= ? AND event_time <= ?
            GROUP BY importance
            ORDER BY importance
        ";

        let rows: Vec<(i32, i64)> = sqlx::query_as(query)
            .bind(start_time)
            .bind(end_time)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| anyhow!("统计经济事件失败: {}", e))?;

        Ok(rows)
    }
}
