//! 经济日历数据同步服务
//!
//! 负责从 OKX 获取经济日历数据并持久化到数据库

use anyhow::{anyhow, Result};
use okx::api::api_trait::OkxApiTrait;
use okx::api::public_data::OkxPublicData;
use rust_quant_core::database::get_db_pool;
use rust_quant_domain::entities::economic_event::EconomicEvent;
use rust_quant_domain::traits::economic_event_repository::EconomicEventRepository;
use rust_quant_infrastructure::repositories::economic_event_repository::SqlxEconomicEventRepository;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// 经济日历数据同步服务
///
/// # Architecture
/// services层：协调 OKX API 和 Repository，执行数据同步业务逻辑
pub struct EconomicCalendarSyncService {
    api: OkxPublicData,
    repo: Arc<dyn EconomicEventRepository>,
}

impl EconomicCalendarSyncService {
    /// 创建新的同步服务
    pub fn new() -> Result<Self> {
        let api = OkxPublicData::from_env()?;
        let pool = get_db_pool().clone();
        let repo = Arc::new(SqlxEconomicEventRepository::new(pool));
        Ok(Self { api, repo })
    }

    /// 使用自定义 Repository 创建（用于测试）
    pub fn with_repo(repo: Arc<dyn EconomicEventRepository>) -> Result<Self> {
        let api = OkxPublicData::from_env()?;
        Ok(Self { api, repo })
    }

    /// 执行完整同步（增量 + 历史回填）
    /// 
    /// 只同步 importance=3 的高重要性事件
    pub async fn sync_all(&self) -> Result<()> {
        info!("📅 开始经济日历同步 (仅高重要性事件)");

        // 1. 同步最新数据
        if let Err(e) = self.sync_incremental().await {
            error!("❌ 增量同步失败: {}", e);
        }

        // API 调用间隔（OKX 滑动窗口限流，需要等待足够长时间）
        tokio::time::sleep(Duration::from_millis(5000)).await;

        // 2. 回填历史数据
        if let Err(e) = self.sync_historical().await {
            error!("❌ 历史回填失败: {}", e);
        }

        info!("✅ 经济日历同步完成");
        Ok(())
    }

    /// 增量同步：获取最新的高重要性经济日历事件 (importance=3)
    pub async fn sync_incremental(&self) -> Result<usize> {
        info!("⏩ 经济日历增量同步 (importance=3)...");

        let latest_time = self.repo.find_latest_event_time().await?;

        let events = self
            .api
            .get_economic_calendar(None, Some("3"), None, latest_time, Some(100))
            .await?;

        if events.is_empty() {
            info!("无新的高重要性事件");
            return Ok(0);
        }

        info!("获取到 {} 条高重要性事件", events.len());

        let domain_events: Vec<EconomicEvent> = events
            .iter()
            .map(SqlxEconomicEventRepository::from_okx_dto)
            .collect();

        self.repo.save_batch(domain_events).await?;

        info!("增量同步完成，保存 {} 条事件", events.len());
        Ok(events.len())
    }

    /// 历史回填：获取历史高重要性经济日历数据 (importance=3)
    /// 
    /// OKX API 分页惯例：
    /// - after: 返回 date < after 的数据（更旧）-> 向后翻页
    /// - before: 返回 date > before 的数据（更新）-> 向前翻页
    pub async fn sync_historical(&self) -> Result<usize> {
        info!("📚 经济日历历史回填 (importance=3)...");

        let oldest = self.repo.find_oldest_event_time().await?;
        let mut after_ts = oldest;

        info!("历史回填起始 after={:?}", after_ts);

        let mut total_saved = 0;
        let mut prev_cursor: Option<i64> = None;

        loop {
            // 带重试的 API 调用，用 after 参数获取更旧的数据
            let events = match self.fetch_with_retry(after_ts, 3).await {
                Ok(events) => events,
                Err(e) => {
                    error!("API 调用失败 (已重试): {}", e);
                    tokio::time::sleep(Duration::from_millis(5000)).await;
                    break;
                }
            };

            if events.is_empty() {
                info!("历史回填完成 (无更多数据)");
                break;
            }

            let count = events.len();

            // 取所有事件中最小的时间作为下一次的 after（获取更旧数据）
            let min_ts = events
                .iter()
                .filter_map(|e| e.date.parse::<i64>().ok())
                .min()
                .unwrap_or(0);

            // 防止游标无变化导致无限循环
            if prev_cursor == Some(min_ts) {
                info!("历史回填完成 (游标无变化，已到最早数据)");
                break;
            }

            let domain_events: Vec<EconomicEvent> = events
                .iter()
                .map(SqlxEconomicEventRepository::from_okx_dto)
                .collect();

            self.repo.save_batch(domain_events).await?;
            total_saved += count;

            info!("回填保存 {} 条, cursor updated to {}", count, min_ts);

            prev_cursor = Some(min_ts);
            after_ts = Some(min_ts);

            // OKX API 限流
            tokio::time::sleep(Duration::from_millis(5000)).await;
        }

        info!("历史回填完成，总计保存 {} 条事件", total_saved);
        Ok(total_saved)
    }

    /// 带重试的 API 调用（用 after 参数获取更旧数据）
    async fn fetch_with_retry(
        &self,
        after: Option<i64>,
        max_retries: u32,
    ) -> Result<Vec<okx::dto::public_data_dto::EconomicEventOkxRespDto>> {
        let mut last_error: Option<String> = None;

        for attempt in 0..max_retries {
            // 注意：用 after 参数，before 为 None
            match self
                .api
                .get_economic_calendar(None, Some("3"), None, after, Some(100))
                .await
            {
                Ok(events) => return Ok(events),
                Err(e) => {
                    let err_msg = format!("{}", e);
                    last_error = Some(err_msg.clone());
                    let wait_ms = 2000 * (attempt + 1) as u64;
                    warn!(
                        "API 调用失败: {}，第 {}/{} 次重试，等待 {}ms",
                        err_msg,
                        attempt + 1,
                        max_retries,
                        wait_ms
                    );
                    tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                }
            }
        }

        Err(anyhow!(last_error.unwrap_or_else(|| "API 调用失败".to_string())))
    }

    /// 同步指定区域的经济日历
    pub async fn sync_by_region(&self, region: &str) -> Result<usize> {
        info!("🌍 同步区域 {} 的经济日历", region);

        let events = self
            .api
            .get_economic_calendar(Some(region), Some("3"), None, None, Some(100))
            .await?;

        if events.is_empty() {
            info!("区域 {} 无数据", region);
            return Ok(0);
        }

        let count = events.len();
        let domain_events: Vec<EconomicEvent> = events
            .iter()
            .map(SqlxEconomicEventRepository::from_okx_dto)
            .collect();

        self.repo.save_batch(domain_events).await?;

        info!("区域 {} 同步完成，保存 {} 条", region, count);
        Ok(count)
    }
}

/// 经济事件查询服务
///
/// 提供经济事件的查询接口，用于策略层判断是否处于经济事件影响窗口
pub struct EconomicEventQueryService {
    repo: Arc<dyn EconomicEventRepository>,
}

impl EconomicEventQueryService {
    pub fn new() -> Self {
        let pool = get_db_pool().clone();
        let repo = Arc::new(SqlxEconomicEventRepository::new(pool));
        Self { repo }
    }

    /// 使用自定义 Repository 创建（用于测试）
    pub fn with_repo(repo: Arc<dyn EconomicEventRepository>) -> Self {
        Self { repo }
    }

    /// 检查当前时间是否处于高重要性经济事件影响窗口内
    ///
    /// # Arguments
    /// * `current_time_ms` - 当前时间戳 (毫秒)
    /// * `window_before_ms` - 事件前多少毫秒开始影响 (默认 30 分钟)
    /// * `window_after_ms` - 事件后多少毫秒仍有影响 (默认 60 分钟)
    ///
    /// # Returns
    /// * `Some(events)` - 处于影响窗口内的事件列表
    /// * `None` - 当前没有活跃的高重要性事件
    pub async fn get_active_high_importance_events(
        &self,
        current_time_ms: i64,
        window_before_ms: Option<i64>,
        window_after_ms: Option<i64>,
    ) -> Result<Vec<EconomicEvent>> {
        let before = window_before_ms.unwrap_or(30 * 60 * 1000); // 默认30分钟
        let after = window_after_ms.unwrap_or(60 * 60 * 1000); // 默认60分钟

        self.repo
            .find_active_events(current_time_ms, before, after, 3)
            .await
    }

    /// 检查是否应该暂停追涨追跌
    ///
    /// 在高重要性经济事件发布前后的时间窗口内，应该等待回调再入场
    ///
    /// # Arguments
    /// * `current_time_ms` - 当前时间戳 (毫秒)
    ///
    /// # Returns
    /// * `true` - 当前处于经济事件影响窗口，应等待回调
    /// * `false` - 当前无活跃经济事件，可正常交易
    pub async fn should_wait_for_pullback(&self, current_time_ms: i64) -> Result<bool> {
        let events = self
            .get_active_high_importance_events(current_time_ms, None, None)
            .await?;

        if !events.is_empty() {
            debug!(
                "检测到 {} 个活跃的高重要性经济事件，建议等待回调",
                events.len()
            );
            for event in &events {
                debug!(
                    "  - {}: {} ({}), importance={}",
                    event.region, event.event, event.category, event.importance
                );
            }
        }

        Ok(!events.is_empty())
    }

    /// 获取即将发生的高重要性事件
    ///
    /// # Arguments
    /// * `current_time_ms` - 当前时间戳 (毫秒)
    /// * `lookahead_hours` - 向前查看多少小时
    pub async fn get_upcoming_events(
        &self,
        current_time_ms: i64,
        lookahead_hours: i64,
    ) -> Result<Vec<EconomicEvent>> {
        let window_ms = lookahead_hours * 60 * 60 * 1000;
        self.repo
            .find_upcoming_events(current_time_ms, window_ms, 3)
            .await
    }
}

