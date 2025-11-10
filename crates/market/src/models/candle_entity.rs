use chrono::NaiveDateTime;
use okx::dto::market_dto::CandleOkxRespDto;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// K线数据实体
#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct CandlesEntity {
    #[sqlx(default)]
    pub id: Option<i64>,
    pub ts: i64,         // 开始时间，Unix时间戳的毫秒数格式
    pub o: String,       // 开盘价格
    pub h: String,       // 最高价格
    pub l: String,       // 最低价格
    pub c: String,       // 收盘价格
    pub vol: String,     // 交易量，以张为单位
    pub vol_ccy: String, // 交易量，以币为单位
    pub confirm: String, // K线状态
    #[sqlx(default)]
    pub created_at: Option<NaiveDateTime>,
    #[sqlx(default)]
    pub updated_at: Option<NaiveDateTime>,
}

impl From<&CandleOkxRespDto> for CandlesEntity {
    fn from(candle: &CandleOkxRespDto) -> Self {
        CandlesEntity {
            id: None,
            ts: candle.ts.parse::<i64>().unwrap_or(0),
            o: candle.o.to_string(),
            h: candle.h.to_string(),
            l: candle.l.to_string(),
            c: candle.c.to_string(),
            vol: candle.v.to_string(),
            vol_ccy: candle.vol_ccy.to_string(),
            confirm: candle.confirm.to_string(),
            created_at: None,
            updated_at: None,
        }
    }
}
