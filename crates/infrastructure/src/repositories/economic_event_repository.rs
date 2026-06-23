//! 经济日历事件数据访问层实现
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rust_quant_domain::entities::economic_event::EconomicEvent;
use rust_quant_domain::traits::economic_event_repository::EconomicEventRepository;
use sqlx::{FromRow, PgPool};
use tracing::error;
/// 经济事件数据库实体
#[derive(Debug, Clone, FromRow)]
struct EconomicEventEntity {
    /// 唯一标识。
    pub id: i64,
    /// calendar ID。
    pub calendar_id: String,
    /// 时间字段。
    pub event_time: i64,
    /// region，用于运行时配置或基础设施依赖。
    pub region: String,
    /// 分类。
    pub category: String,
    /// event，用于运行时配置或基础设施依赖。
    pub event: String,
    /// ref日期，用于运行时配置或基础设施依赖。
    pub ref_date: String,
    /// actual；为空时表示该条件不启用。
    pub actual: Option<String>,
    /// 上一期；为空时表示该条件不启用。
    pub previous: Option<String>,
    /// forecast；为空时表示该条件不启用。
    pub forecast: Option<String>,
    /// importance，用于运行时配置或基础设施依赖。
    pub importance: i32,
    /// 时间字段。
    pub updated_time: i64,
    /// 前值初始读数；为空时表示没有前值。
    pub prev_initial: Option<String>,
    /// currency，用于运行时配置或基础设施依赖。
    pub currency: String,
    /// unit；为空时表示该条件不启用。
    pub unit: Option<String>,
}
impl EconomicEventEntity {
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
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
    /// 数据库连接池。
    pool: PgPool,
}
impl SqlxEconomicEventRepository {
    pub fn new(pool: PgPool) -> Self {
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
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
    async fn save(&self, event: EconomicEvent) -> Result<()> {
        let query = "
            INSERT INTO economic_events (
                calendar_id, event_time, region, category, event, ref_date,
                actual, previous, forecast, importance, updated_time,
                prev_initial, currency, unit
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ON CONFLICT (calendar_id) DO UPDATE SET
                event_time = EXCLUDED.event_time,
                region = EXCLUDED.region,
                category = EXCLUDED.category,
                event = EXCLUDED.event,
                ref_date = EXCLUDED.ref_date,
                actual = EXCLUDED.actual,
                previous = EXCLUDED.previous,
                forecast = EXCLUDED.forecast,
                importance = EXCLUDED.importance,
                updated_time = EXCLUDED.updated_time,
                prev_initial = EXCLUDED.prev_initial,
                currency = EXCLUDED.currency,
                unit = EXCLUDED.unit
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
    /// 持久化 配置、基础设施和运行时 结果，保证写入路径和幂等语义集中处理。
    async fn save_batch(&self, events: Vec<EconomicEvent>) -> Result<()> {
        for event in events {
            self.save(event).await?;
        }
        Ok(())
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_by_calendar_id(&self, calendar_id: &str) -> Result<Option<EconomicEvent>> {
        let query = "SELECT * FROM economic_events WHERE calendar_id = $1";
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
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_by_time_range(
        &self,
        start_time: i64,
        end_time: i64,
        min_importance: Option<i32>,
    ) -> Result<Vec<EconomicEvent>> {
        let min_imp = min_importance.unwrap_or(1);
        let query = "
            SELECT * FROM economic_events 
            WHERE event_time >= $1 AND event_time <= $2 AND importance >= $3
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
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_latest_event_time(&self) -> Result<Option<i64>> {
        let query = "SELECT MAX(event_time) as max_time FROM economic_events";
        let row: Option<(Option<i64>,)> = sqlx::query_as(query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow!("查询最新事件时间失败: {}", e))?;
        Ok(row.and_then(|r| r.0))
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_oldest_event_time(&self) -> Result<Option<i64>> {
        let query = "SELECT MIN(event_time) as min_time FROM economic_events";
        let row: Option<(Option<i64>,)> = sqlx::query_as(query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow!("查询最早事件时间失败: {}", e))?;
        Ok(row.and_then(|r| r.0))
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn find_upcoming_events(
        &self,
        current_time: i64,
        window_ms: i64,
        min_importance: i32,
    ) -> Result<Vec<EconomicEvent>> {
        let end_time = current_time + window_ms;
        let query = "
            SELECT * FROM economic_events 
            WHERE event_time >= $1 AND event_time <= $2 AND importance >= $3
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
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
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
            WHERE event_time >= $1 AND event_time <= $2 AND importance >= $3
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
    /// 提供数量byimportance的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn count_by_importance(&self, start_time: i64, end_time: i64) -> Result<Vec<(i32, i64)>> {
        let query = "
            SELECT importance, COUNT(*) as cnt 
            FROM economic_events 
            WHERE event_time >= $1 AND event_time <= $2
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
