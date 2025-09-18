use serde::{Deserialize, Serialize};
use rbatis::rbdc::DateTime;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct CandlesEntity {
    pub ts: i64,         // 开始时间，Unix时间戳的毫秒数格式
    pub o: String,       // 开盘价格
    pub h: String,       // 最高价格
    pub l: String,       // 最低价格
    pub c: String,       // 收盘价格
    pub vol: String,     // 交易量，以张为单位
    pub vol_ccy: String, // 交易量，以币为单位
    // pub vol_ccy_quote: String, // 交易量，以计价货币为单位
    pub confirm: String, // K线状态
    pub updated_at: Option<DateTime>,
}

