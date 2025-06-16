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
}

/// 交易类型
pub enum TradeType {
    Open,
    Close,
}
