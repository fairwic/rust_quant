use serde::{Deserialize, Serialize};

/// 止损更新记录
///
/// 记录每次止损价格更新的详细信息,用于分析止损策略的有效性
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopLossUpdate {
    /// 更新序号(从0开始,0表示初始设置,1+表示后续更新)
    pub sequence: i32,

    /// 信号时间戳(毫秒)
    pub signal_ts: i64,

    /// K线时间戳(毫秒)
    pub candle_ts: i64,

    /// 信号来源(Engulfing/KlineHammer/ATR等)
    pub source: String,

    /// 旧止损价(None表示首次设置)
    pub old_price: Option<f64>,

    /// 新止损价
    pub new_price: f64,

    /// 价格变化(new - old, None表示首次设置)
    pub price_change: Option<f64>,
}

impl StopLossUpdate {
    /// 创建初始止损记录
    pub fn initial(signal_ts: i64, candle_ts: i64, source: String, price: f64) -> Self {
        Self {
            sequence: 0,
            signal_ts,
            candle_ts,
            source,
            old_price: None,
            new_price: price,
            price_change: None,
        }
    }

    /// 创建止损更新记录
    pub fn update(
        sequence: i32,
        signal_ts: i64,
        candle_ts: i64,
        source: String,
        old_price: f64,
        new_price: f64,
    ) -> Self {
        Self {
            sequence,
            signal_ts,
            candle_ts,
            source,
            old_price: Some(old_price),
            new_price,
            price_change: Some(new_price - old_price),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_stop_loss() {
        let update = StopLossUpdate::initial(1000, 1000, "Engulfing".to_string(), 100.0);
        assert_eq!(update.sequence, 0);
        assert_eq!(update.new_price, 100.0);
        assert!(update.old_price.is_none());
        assert!(update.price_change.is_none());
    }

    #[test]
    fn test_stop_loss_update() {
        let update = StopLossUpdate::update(1, 2000, 2000, "KlineHammer".to_string(), 100.0, 95.0);
        assert_eq!(update.sequence, 1);
        assert_eq!(update.old_price, Some(100.0));
        assert_eq!(update.new_price, 95.0);
        assert_eq!(update.price_change, Some(-5.0));
    }

    #[test]
    fn test_serialization() {
        let update = StopLossUpdate::initial(1000, 1000, "Engulfing".to_string(), 100.0);
        let json = serde_json::to_string(&update).unwrap();
        let deserialized: StopLossUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sequence, update.sequence);
        assert_eq!(deserialized.new_price, update.new_price);
    }
}
