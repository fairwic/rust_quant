//! 交易信号值对象

use serde::{Deserialize, Serialize};

/// 交易方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalDirection {
    /// 做多信号
    Long,
    /// 做空信号
    Short,
    /// 平仓信号
    Close,
    /// 无信号
    None,
}

/// 信号强度
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SignalStrength(f64);

impl SignalStrength {
    /// 创建信号强度 (0.0 - 1.0)
    pub fn new(value: f64) -> Self {
        Self(value.clamp(0.0, 1.0))
    }
    
    pub fn value(&self) -> f64 {
        self.0
    }
    
    /// 弱信号阈值
    pub fn is_weak(&self) -> bool {
        self.0 < 0.3
    }
    
    /// 中等信号阈值
    pub fn is_moderate(&self) -> bool {
        self.0 >= 0.3 && self.0 < 0.7
    }
    
    /// 强信号阈值
    pub fn is_strong(&self) -> bool {
        self.0 >= 0.7
    }
}

/// 交易信号 - 值对象
/// 
/// 包含信号方向、强度、来源等信息
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradingSignal {
    /// 信号方向
    pub direction: SignalDirection,
    
    /// 信号强度 (0.0 - 1.0)
    pub strength: SignalStrength,
    
    /// 信号来源 (如 "RSI", "MACD", "Vegas")
    pub source: String,
    
    /// 信号生成时间戳 (毫秒)
    pub timestamp: i64,
    
    /// 额外信息
    pub metadata: Option<serde_json::Value>,
}

impl TradingSignal {
    /// 创建新的交易信号
    pub fn new(
        direction: SignalDirection,
        strength: f64,
        source: String,
        timestamp: i64,
    ) -> Self {
        Self {
            direction,
            strength: SignalStrength::new(strength),
            source,
            timestamp,
            metadata: None,
        }
    }
    
    /// 创建做多信号
    pub fn long(strength: f64, source: String, timestamp: i64) -> Self {
        Self::new(SignalDirection::Long, strength, source, timestamp)
    }
    
    /// 创建做空信号
    pub fn short(strength: f64, source: String, timestamp: i64) -> Self {
        Self::new(SignalDirection::Short, strength, source, timestamp)
    }
    
    /// 创建平仓信号
    pub fn close(strength: f64, source: String, timestamp: i64) -> Self {
        Self::new(SignalDirection::Close, strength, source, timestamp)
    }
    
    /// 判断是否为有效信号
    pub fn is_valid(&self) -> bool {
        self.direction != SignalDirection::None && !self.strength.is_weak()
    }
    
    /// 添加元数据
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// 信号结果 - 包含多个信号的组合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalResult {
    /// 最终信号方向
    pub direction: SignalDirection,
    
    /// 综合信号强度
    pub strength: SignalStrength,
    
    /// 组成信号列表
    pub signals: Vec<TradingSignal>,
    
    /// 是否满足开仓条件
    pub can_open: bool,
    
    /// 是否满足平仓条件
    pub should_close: bool,
    
    // === 扩展字段 (兼容现有策略代码) ===
    
    /// 入场价格
    pub entry_price: Option<f64>,
    
    /// 止损价格
    pub stop_loss_price: Option<f64>,
    
    /// 止盈价格
    pub take_profit_price: Option<f64>,
    
    /// 信号K线的止损价格
    pub signal_kline_stop_loss_price: Option<f64>,
    
    /// 信号时间戳
    pub position_time: Option<i64>,
    
    /// 信号K线索引
    pub signal_kline: Option<usize>,
    
    // === NWE 策略扩展字段 ===
    
    /// 时间戳
    pub ts: Option<i64>,
    
    /// 单一值
    pub single_value: Option<f64>,
    
    /// 单一结果
    pub single_result: Option<bool>,
    
    /// 是否应该卖出
    pub should_sell: Option<bool>,
    
    /// 是否应该买入
    pub should_buy: Option<bool>,
    
    /// 开仓价格
    pub open_price: Option<f64>,
    
    /// 最佳止盈价格
    pub best_take_profit_price: Option<f64>,
    
    /// 最佳开仓价格
    pub best_open_price: Option<f64>,
}

impl SignalResult {
    /// 创建空信号结果
    pub fn empty() -> Self {
        Self {
            direction: SignalDirection::None,
            strength: SignalStrength::new(0.0),
            signals: vec![],
            can_open: false,
            should_close: false,
            entry_price: None,
            stop_loss_price: None,
            take_profit_price: None,
            signal_kline_stop_loss_price: None,
            position_time: None,
            signal_kline: None,
            ts: None,
            single_value: None,
            single_result: None,
            should_sell: None,
            should_buy: None,
            open_price: None,
            best_take_profit_price: None,
            best_open_price: None,
        }
    }
    
    /// 从多个信号合并
    pub fn from_signals(signals: Vec<TradingSignal>) -> Self {
        if signals.is_empty() {
            return Self::empty();
        }
        
        // 计算平均强度
        let avg_strength = signals.iter()
            .map(|s| s.strength.value())
            .sum::<f64>() / signals.len() as f64;
        
        // 确定主要方向 (简单多数投票)
        let long_count = signals.iter().filter(|s| s.direction == SignalDirection::Long).count();
        let short_count = signals.iter().filter(|s| s.direction == SignalDirection::Short).count();
        let close_count = signals.iter().filter(|s| s.direction == SignalDirection::Close).count();
        
        let direction = if close_count > signals.len() / 2 {
            SignalDirection::Close
        } else if long_count > short_count {
            SignalDirection::Long
        } else if short_count > long_count {
            SignalDirection::Short
        } else {
            SignalDirection::None
        };
        
        Self {
            direction,
            strength: SignalStrength::new(avg_strength),
            signals,
            can_open: direction == SignalDirection::Long || direction == SignalDirection::Short,
            should_close: direction == SignalDirection::Close,
            entry_price: None,
            stop_loss_price: None,
            take_profit_price: None,
            signal_kline_stop_loss_price: None,
            position_time: None,
            signal_kline: None,
            ts: None,
            single_value: None,
            single_result: None,
            should_sell: None,
            should_buy: None,
            open_price: None,
            best_take_profit_price: None,
            best_open_price: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_signal_strength() {
        let weak = SignalStrength::new(0.2);
        assert!(weak.is_weak());
        
        let moderate = SignalStrength::new(0.5);
        assert!(moderate.is_moderate());
        
        let strong = SignalStrength::new(0.9);
        assert!(strong.is_strong());
    }
    
    #[test]
    fn test_trading_signal() {
        let signal = TradingSignal::long(0.8, "RSI".to_string(), 1000000);
        assert_eq!(signal.direction, SignalDirection::Long);
        assert!(signal.is_valid());
    }
    
    #[test]
    fn test_signal_result_merge() {
        let signals = vec![
            TradingSignal::long(0.7, "RSI".to_string(), 1000000),
            TradingSignal::long(0.8, "MACD".to_string(), 1000000),
            TradingSignal::short(0.5, "Volume".to_string(), 1000000),
        ];
        
        let result = SignalResult::from_signals(signals);
        assert_eq!(result.direction, SignalDirection::Long);
        assert!(result.can_open);
    }
}


