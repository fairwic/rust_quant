//! K线数据适配器
//!
//! 解决孤儿规则问题：为外部类型(CandlesEntity)实现外部trait(ta库的High/Low/Close)
//!
//! ## 设计模式
//!
//! 使用Newtype模式创建本地wrapper，然后为wrapper实现trait

use rust_quant_market::models::CandlesEntity;
use ta::{Close, High, Low, Open, Volume};

/// K线数据的适配器包装器
///
/// 用于为 `CandlesEntity` 实现 `ta` 库的 trait
#[derive(Debug, Clone)]
pub struct CandleAdapter {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl From<&CandlesEntity> for CandleAdapter {
    fn from(candle: &CandlesEntity) -> Self {
        Self {
            open: candle.o.parse().unwrap_or(0.0),
            high: candle.h.parse().unwrap_or(0.0),
            low: candle.l.parse().unwrap_or(0.0),
            close: candle.c.parse().unwrap_or(0.0),
            volume: candle.vol.parse().unwrap_or(0.0),
        }
    }
}

impl High for CandleAdapter {
    fn high(&self) -> f64 {
        self.high
    }
}

impl Low for CandleAdapter {
    fn low(&self) -> f64 {
        self.low
    }
}

impl Close for CandleAdapter {
    fn close(&self) -> f64 {
        self.close
    }
}

impl Open for CandleAdapter {
    fn open(&self) -> f64 {
        self.open
    }
}

impl Volume for CandleAdapter {
    fn volume(&self) -> f64 {
        self.volume
    }
}

/// 便捷转换函数
pub fn adapt(candle: &CandlesEntity) -> CandleAdapter {
    CandleAdapter::from(candle)
}

/// 批量转换
pub fn adapt_many(candles: &[CandlesEntity]) -> Vec<CandleAdapter> {
    candles.iter().map(adapt).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_market::models::CandlesEntity;

    fn create_test_candle() -> CandlesEntity {
        CandlesEntity {
            id: Some(1),
            ts: 1609459200000,
            o: "50000.0".to_string(),
            h: "51000.0".to_string(),
            l: "49000.0".to_string(),
            c: "50500.0".to_string(),
            vol: "100.5".to_string(),
            vol_ccy: "5050000.0".to_string(),
            confirm: "1".to_string(),
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn test_candle_adapter_conversion() {
        let candle = create_test_candle();
        let adapter = adapt(&candle);

        assert_eq!(adapter.open(), 50000.0);
        assert_eq!(adapter.high(), 51000.0);
        assert_eq!(adapter.low(), 49000.0);
        assert_eq!(adapter.close(), 50500.0);
        assert_eq!(adapter.volume(), 100.5);
    }

    #[test]
    fn test_adapt_many() {
        let candles = vec![create_test_candle(), create_test_candle()];
        let adapters = adapt_many(&candles);

        assert_eq!(adapters.len(), 2);
        assert_eq!(adapters[0].close(), 50500.0);
    }
}
