//! 市场事件检测器
//! 
//! 检测重要市场事件：
//! - 政策变化（如美联储利率决议）
//! - 重大新闻（如交易所被黑、项目暴雷）
//! - 社交媒体热点（如 Elon Musk 推文）
//! - 链上异常事件（如大额转账、巨鲸操作）

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::news_collector::NewsArticle;

/// 市场事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    /// 政策变化
    PolicyChange,
    /// 监管动态
    Regulation,
    /// 交易所事件
    ExchangeEvent,
    /// 项目动态
    ProjectUpdate,
    /// 安全事件
    SecurityIncident,
    /// 巨鲸操作
    WhaleMovement,
    /// 社交媒体热点
    SocialTrending,
}

/// 市场事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketEvent {
    pub id: String,
    pub event_type: EventType,
    pub title: String,
    pub description: String,
    pub detected_at: DateTime<Utc>,
    pub source: String,
    
    /// 事件热度（0.0 到 1.0）
    pub heat_score: f64,
    
    /// 市场影响预测（-1.0 到 1.0）
    pub impact_score: f64,
    
    /// 相关资产
    pub related_assets: Vec<String>,
}

/// 事件检测器接口
#[async_trait]
pub trait EventDetector: Send + Sync {
    async fn detect_events(&self, news: &[NewsArticle]) -> anyhow::Result<Vec<MarketEvent>>;
    async fn get_trending_events(&self, hours: u32) -> anyhow::Result<Vec<MarketEvent>>;
}

/// AI 驱动的事件检测器
pub struct AIEventDetector {
    openai_api_key: String,
}

impl AIEventDetector {
    pub fn new(openai_api_key: String) -> Self {
        Self { openai_api_key }
    }
}

#[async_trait]
impl EventDetector for AIEventDetector {
    async fn detect_events(&self, news: &[NewsArticle]) -> anyhow::Result<Vec<MarketEvent>> {
        // TODO: 使用 GPT-4 分析新闻，检测重要事件
        Ok(vec![])
    }
    
    async fn get_trending_events(&self, hours: u32) -> anyhow::Result<Vec<MarketEvent>> {
        // TODO: 从向量数据库检索热点事件
        Ok(vec![])
    }
}

