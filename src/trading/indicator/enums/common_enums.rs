use serde::{Deserialize, Serialize};

pub enum CandleNums {
    OneMinute,
    FiveMinute,
    FifteenMinute,
    ThirtyMinute,
    OneHour,
    FourHour,
    OneDay,
}

/// 交易方向
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TradeSide {
    Long,
    Short,
    None,
}
impl Default for TradeSide {
    fn default() -> Self {
        TradeSide::None
    }
}

/// 交易类型
pub enum TradeType {
    Open,
    Close,
}
