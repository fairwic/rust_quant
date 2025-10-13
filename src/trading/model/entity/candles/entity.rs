use okx::dto::market_dto::CandleOkxRespDto;
use rbatis::rbdc::DateTime;
use serde::{Deserialize, Serialize};

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

impl From<&CandleOkxRespDto> for CandlesEntity {
    fn from(candle: &CandleOkxRespDto) -> Self {
        CandlesEntity {
            ts: candle.ts.parse::<i64>().unwrap(),
            o: candle.o.to_string(),
            h: candle.h.to_string(),
            l: candle.l.to_string(),
            c: candle.c.to_string(),
            vol: candle.v.to_string(),
            vol_ccy: candle.vol_ccy.to_string(),
            confirm: candle.confirm.to_string(),
            updated_at: None,
        }
    }
}
