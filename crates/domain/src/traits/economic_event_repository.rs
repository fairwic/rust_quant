//! 经济日历事件仓储接口

use crate::entities::economic_event::EconomicEvent;
use anyhow::Result;
use async_trait::async_trait;

/// 经济日历事件仓储接口
#[async_trait]
pub trait EconomicEventRepository: Send + Sync {
    /// 保存单个经济事件 (存在则更新)
    async fn save(&self, event: EconomicEvent) -> Result<()>;

    /// 批量保存经济事件
    async fn save_batch(&self, events: Vec<EconomicEvent>) -> Result<()>;

    /// 根据 calendar_id 查找事件
    async fn find_by_calendar_id(&self, calendar_id: &str) -> Result<Option<EconomicEvent>>;

    /// 获取时间范围内的事件
    ///
    /// # Arguments
    /// * `start_time` - 开始时间戳 (毫秒)
    /// * `end_time` - 结束时间戳 (毫秒)
    /// * `min_importance` - 最低重要性 (1-3)
    async fn find_by_time_range(
        &self,
        start_time: i64,
        end_time: i64,
        min_importance: Option<i32>,
    ) -> Result<Vec<EconomicEvent>>;

    /// 获取最新的事件时间戳 (用于增量同步)
    async fn find_latest_event_time(&self) -> Result<Option<i64>>;

    /// 获取最早的事件时间戳 (用于历史回填)
    async fn find_oldest_event_time(&self) -> Result<Option<i64>>;

    /// 查找即将发生的高重要性事件
    ///
    /// # Arguments
    /// * `current_time` - 当前时间戳 (毫秒)
    /// * `window_ms` - 时间窗口 (毫秒)，返回 [current_time, current_time + window_ms] 内的事件
    /// * `min_importance` - 最低重要性
    async fn find_upcoming_events(
        &self,
        current_time: i64,
        window_ms: i64,
        min_importance: i32,
    ) -> Result<Vec<EconomicEvent>>;

    /// 查找当前时间附近的活跃事件
    ///
    /// 用于策略过滤：检查当前是否处于经济事件影响窗口内
    ///
    /// # Arguments
    /// * `current_time` - 当前时间戳 (毫秒)
    /// * `window_before_ms` - 事件前多少毫秒开始生效
    /// * `window_after_ms` - 事件后多少毫秒仍生效
    /// * `min_importance` - 最低重要性
    async fn find_active_events(
        &self,
        current_time: i64,
        window_before_ms: i64,
        window_after_ms: i64,
        min_importance: i32,
    ) -> Result<Vec<EconomicEvent>>;

    /// 统计指定时间范围内各重要性级别的事件数量
    async fn count_by_importance(&self, start_time: i64, end_time: i64) -> Result<Vec<(i32, i64)>>;
}
