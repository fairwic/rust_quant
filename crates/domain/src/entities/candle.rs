//! K线实体 (Candle Aggregate Root)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::Timeframe;
use crate::value_objects::{Price, Volume};

/// K线实体 - 聚合根
///
/// 代表一根K线的完整信息，包含OHLCV数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    /// 交易对符号 (如 "BTC-USDT")
    pub symbol: String,

    /// 时间周期
    pub timeframe: Timeframe,

    /// K线开始时间戳 (毫秒)
    pub timestamp: i64,

    /// K线开始时间 (UTC)
    pub datetime: DateTime<Utc>,

    /// 开盘价
    pub open: Price,

    /// 最高价
    pub high: Price,

    /// 最低价
    pub low: Price,

    /// 收盘价
    pub close: Price,

    /// 成交量
    pub volume: Volume,

    /// 是否已确认 (K线是否已完成)
    pub confirmed: bool,
}

impl Candle {
    /// 创建新的K线
    pub fn new(
        symbol: String,
        timeframe: Timeframe,
        timestamp: i64,
        open: Price,
        high: Price,
        low: Price,
        close: Price,
        volume: Volume,
    ) -> Self {
        let datetime = DateTime::from_timestamp_millis(timestamp).unwrap_or_else(|| Utc::now());

        Self {
            symbol,
            timeframe,
            timestamp,
            datetime,
            open,
            high,
            low,
            close,
            volume,
            confirmed: false,
        }
    }

    /// 标记为已确认
    pub fn confirm(&mut self) {
        self.confirmed = true;
    }

    /// 判断是否为阳线
    pub fn is_bullish(&self) -> bool {
        self.close > self.open
    }

    /// 判断是否为阴线
    pub fn is_bearish(&self) -> bool {
        self.close < self.open
    }

    /// 判断是否为十字星
    pub fn is_doji(&self) -> bool {
        let body = (self.close.value() - self.open.value()).abs();
        let range = self.high.value() - self.low.value();

        if range == 0.0 {
            return true;
        }

        // 实体小于总范围的10%视为十字星
        body / range < 0.1
    }

    /// 获取实体大小
    pub fn body_size(&self) -> f64 {
        (self.close.value() - self.open.value()).abs()
    }

    /// 获取上影线长度
    pub fn upper_shadow(&self) -> f64 {
        let body_high = self.open.value().max(self.close.value());
        self.high.value() - body_high
    }

    /// 获取下影线长度
    pub fn lower_shadow(&self) -> f64 {
        let body_low = self.open.value().min(self.close.value());
        body_low - self.low.value()
    }

    /// 获取总范围 (高-低)
    pub fn range(&self) -> f64 {
        self.high.value() - self.low.value()
    }

    /// 获取中间价
    pub fn middle_price(&self) -> f64 {
        (self.high.value() + self.low.value()) / 2.0
    }

    /// 获取典型价 (Typical Price)
    pub fn typical_price(&self) -> f64 {
        (self.high.value() + self.low.value() + self.close.value()) / 3.0
    }

    /// 计算价格变化百分比
    pub fn price_change_percent(&self) -> f64 {
        if self.open.value() == 0.0 {
            return 0.0;
        }
        ((self.close.value() - self.open.value()) / self.open.value()) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_objects::{Price, Volume};

    #[test]
    fn test_candle_creation() {
        let candle = Candle::new(
            "BTC-USDT".to_string(),
            Timeframe::H1,
            1000000000,
            Price::new(100.0).unwrap(),
            Price::new(110.0).unwrap(),
            Price::new(95.0).unwrap(),
            Price::new(105.0).unwrap(),
            Volume::new(1000.0).unwrap(),
        );

        assert_eq!(candle.symbol, "BTC-USDT");
        assert!(candle.is_bullish());
    }

    #[test]
    fn test_candle_patterns() {
        let bullish = Candle::new(
            "BTC-USDT".to_string(),
            Timeframe::H1,
            1000000000,
            Price::new(100.0).unwrap(),
            Price::new(110.0).unwrap(),
            Price::new(95.0).unwrap(),
            Price::new(105.0).unwrap(),
            Volume::new(1000.0).unwrap(),
        );
        assert!(bullish.is_bullish());
        assert!(!bullish.is_bearish());

        let bearish = Candle::new(
            "BTC-USDT".to_string(),
            Timeframe::H1,
            1000000000,
            Price::new(105.0).unwrap(),
            Price::new(110.0).unwrap(),
            Price::new(95.0).unwrap(),
            Price::new(100.0).unwrap(),
            Volume::new(1000.0).unwrap(),
        );
        assert!(bearish.is_bearish());
        assert!(!bearish.is_bullish());
    }

    #[test]
    fn test_candle_measurements() {
        let candle = Candle::new(
            "BTC-USDT".to_string(),
            Timeframe::H1,
            1000000000,
            Price::new(100.0).unwrap(),
            Price::new(110.0).unwrap(),
            Price::new(95.0).unwrap(),
            Price::new(105.0).unwrap(),
            Volume::new(1000.0).unwrap(),
        );

        assert_eq!(candle.body_size(), 5.0);
        assert_eq!(candle.upper_shadow(), 5.0);
        assert_eq!(candle.lower_shadow(), 5.0);
        assert_eq!(candle.range(), 15.0);
    }
}
