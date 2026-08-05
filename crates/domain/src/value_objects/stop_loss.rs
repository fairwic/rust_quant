use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    /// 策略可选的结构化更新证据；旧记录缺失时按 `None` 反序列化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Value>,
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
            evidence: None,
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
            evidence: None,
        }
    }

    /// 为本次更新附加策略证据，不改变旧调用方的构造接口与 JSON 兼容性。
    pub fn with_evidence(mut self, evidence: Option<Value>) -> Self {
        self.evidence = evidence;
        self
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// 提供testinitial止损亏损的集中实现，避免量化核心调用方重复处理相同细节。
    fn test_initial_stop_loss() {
        let update = StopLossUpdate::initial(1000, 1000, "Engulfing".to_string(), 100.0);
        assert_eq!(update.sequence, 0);
        assert_eq!(update.new_price, 100.0);
        assert!(update.old_price.is_none());
        assert!(update.price_change.is_none());
        assert!(update.evidence.is_none());
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
        assert_eq!(deserialized.evidence, None);
    }

    #[test]
    fn old_json_without_evidence_remains_deserializable() {
        let old_json = r#"{
            "sequence":1,
            "signal_ts":2000,
            "candle_ts":2000,
            "source":"Legacy",
            "old_price":100.0,
            "new_price":101.0,
            "price_change":1.0
        }"#;

        let deserialized: StopLossUpdate = serde_json::from_str(old_json).unwrap();

        assert_eq!(deserialized.source, "Legacy");
        assert_eq!(deserialized.evidence, None);
    }
}
