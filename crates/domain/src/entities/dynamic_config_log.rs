use serde::{Deserialize, Serialize};

/// 动态配置调整日志实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicConfigLog {
    /// 回测ID
    pub backtest_id: i64,
    /// 交易对
    pub inst_id: String,
    /// 周期
    pub period: String,
    /// K线时间
    pub kline_time: String,
    /// 调整标签(JSON)
    pub adjustments: String,
    /// 动态配置快照(JSON)
    pub config_snapshot: Option<String>,
}
