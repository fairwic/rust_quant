use crate::trading::model::entity::candles::entity::CandlesEntity;

#[derive(Debug, Clone)]
pub struct Candle {
    pub ts: i64, // 时间戳 (毫秒)
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

impl From<&CandlesEntity> for Candle {
    fn from(entity: &CandlesEntity) -> Self {
        Candle {
            ts: entity.ts,
            open: entity.o.parse::<f64>().unwrap_or(0.0),
            high: entity.h.parse::<f64>().unwrap_or(0.0),
            low: entity.l.parse::<f64>().unwrap_or(0.0),
            close: entity.c.parse::<f64>().unwrap_or(0.0),
        }
    }
}
