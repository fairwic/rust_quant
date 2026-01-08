use serde::{Deserialize, Serialize};

/// 资金费率实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRate {
    /// 自增ID
    pub id: Option<i64>,
    /// 产品ID，如 BTC-USD-SWAP
    pub inst_id: String,
    /// 资金费率
    pub funding_rate: f64,
    /// 资金费时间 (Unix时间戳, 毫秒)
    pub funding_time: i64,
    /// 资金费收取逻辑: current_period (当期收), next_period (跨期收)
    pub method: String,
    /// 下一期预测资金费率
    pub next_funding_rate: Option<f64>,
    /// 下一期资金费时间 (Unix时间戳, 毫秒)
    pub next_funding_time: Option<i64>,
    /// 资金费率下限
    pub min_funding_rate: Option<f64>,
    /// 资金费率上限
    pub max_funding_rate: Option<f64>,
    /// 结算资金费率 (settState = processing 时为本轮, settled 为上轮)
    pub sett_funding_rate: Option<f64>,
    /// 资金费率结算状态: processing (结算中), settled (已结算)
    pub sett_state: Option<String>,
    /// 溢价指数
    pub premium: Option<f64>,
    /// 数据更新时间 (Unix时间戳, 毫秒)
    pub ts: i64,
    /// 实际资金费率 (历史数据特有)
    pub realized_rate: Option<f64>,
    /// 利率 (当前资金费率接口特有)
    pub interest_rate: Option<f64>,
}

impl FundingRate {
    /// 创建新的资金费率实体
    pub fn new(
        inst_id: String,
        funding_rate: f64,
        funding_time: i64,
        method: String,
        ts: i64,
    ) -> Self {
        Self {
            id: None,
            inst_id,
            funding_rate,
            funding_time,
            method,
            next_funding_rate: None,
            next_funding_time: None,
            min_funding_rate: None,
            max_funding_rate: None,
            sett_funding_rate: None,
            sett_state: None,
            premium: None,
            ts,
            realized_rate: None,
            interest_rate: None,
        }
    }
}
