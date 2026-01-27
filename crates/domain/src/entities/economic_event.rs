//! 经济日历事件实体
//!
//! 存储经济日历数据，用于分析重要经济事件对市场的影响

use serde::{Deserialize, Serialize};

/// 经济日历事件重要性
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventImportance {
    /// 低重要性 (1)
    Low = 1,
    /// 中等重要性 (2)
    Medium = 2,
    /// 高重要性 (3)
    High = 3,
}

impl From<i32> for EventImportance {
    fn from(value: i32) -> Self {
        match value {
            1 => Self::Low,
            2 => Self::Medium,
            _ => Self::High, // 3 或其他默认高
        }
    }
}

impl From<&str> for EventImportance {
    fn from(value: &str) -> Self {
        match value {
            "1" => Self::Low,
            "2" => Self::Medium,
            _ => Self::High,
        }
    }
}

/// 经济日历事件实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicEvent {
    /// 自增ID
    pub id: Option<i64>,
    /// 经济日历ID (OKX calendar_id)
    pub calendar_id: String,
    /// 计划发布时间 (Unix时间戳毫秒)
    pub event_time: i64,
    /// 事件区域 (如 US, EU, CN)
    pub region: String,
    /// 事件类别
    pub category: String,
    /// 事件名称/指标
    pub event: String,
    /// 事件指向日期 (参考期间)
    pub ref_date: String,
    /// 实际值
    pub actual: Option<String>,
    /// 前值
    pub previous: Option<String>,
    /// 预期值
    pub forecast: Option<String>,
    /// 重要性: 1=低, 2=中, 3=高
    pub importance: i32,
    /// 数据最后更新时间 (Unix时间戳毫秒)
    pub updated_time: i64,
    /// 初始前值
    pub prev_initial: Option<String>,
    /// 货币
    pub currency: String,
    /// 单位
    pub unit: Option<String>,
    /// 创建时间
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl EconomicEvent {
    /// 创建新的经济事件
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        calendar_id: String,
        event_time: i64,
        region: String,
        category: String,
        event: String,
        ref_date: String,
        importance: i32,
        updated_time: i64,
        currency: String,
    ) -> Self {
        Self {
            id: None,
            calendar_id,
            event_time,
            region,
            category,
            event,
            ref_date,
            actual: None,
            previous: None,
            forecast: None,
            importance,
            updated_time,
            prev_initial: None,
            currency,
            unit: None,
            created_at: None,
        }
    }

    /// 判断事件是否为高重要性
    pub fn is_high_importance(&self) -> bool {
        self.importance >= 3
    }

    /// 判断事件是否与加密货币相关 (USD、利率决议等)
    pub fn is_crypto_relevant(&self) -> bool {
        // 高重要性 USD 相关事件对加密货币影响最大
        let relevant_currencies = ["USD", "EUR", "CNY"];
        let relevant_events = [
            "CPI",
            "PPI",
            "GDP",
            "NFP",
            "Unemployment",
            "Interest Rate",
            "FOMC",
            "Fed",
            "Powell",
            "Retail Sales",
            "PCE",
            "ISM",
        ];

        // 货币相关
        if relevant_currencies
            .iter()
            .any(|c| self.currency.contains(c))
        {
            // 如果是高重要性，直接认为相关
            if self.importance >= 3 {
                return true;
            }
            // 中等重要性需要检查事件类型
            if self.importance >= 2 {
                return relevant_events.iter().any(|e| self.event.contains(e));
            }
        }
        false
    }

    /// 检查事件是否在指定时间窗口内
    ///
    /// # Arguments
    /// * `current_time_ms` - 当前时间戳 (毫秒)
    /// * `window_before_ms` - 事件前多少毫秒开始生效
    /// * `window_after_ms` - 事件后多少毫秒仍生效
    pub fn is_within_window(
        &self,
        current_time_ms: i64,
        window_before_ms: i64,
        window_after_ms: i64,
    ) -> bool {
        let start = self.event_time - window_before_ms;
        let end = self.event_time + window_after_ms;
        current_time_ms >= start && current_time_ms <= end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_importance_from_str() {
        assert_eq!(EventImportance::from("1"), EventImportance::Low);
        assert_eq!(EventImportance::from("2"), EventImportance::Medium);
        assert_eq!(EventImportance::from("3"), EventImportance::High);
    }

    #[test]
    fn test_is_crypto_relevant() {
        let event = EconomicEvent {
            id: None,
            calendar_id: "1".to_string(),
            event_time: 1000,
            region: "US".to_string(),
            category: "Inflation".to_string(),
            event: "CPI YoY".to_string(),
            ref_date: "2024-01".to_string(),
            actual: None,
            previous: None,
            forecast: None,
            importance: 3,
            updated_time: 1000,
            prev_initial: None,
            currency: "USD".to_string(),
            unit: None,
            created_at: None,
        };
        assert!(event.is_crypto_relevant());
    }
}
