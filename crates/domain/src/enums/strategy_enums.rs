//! 策略相关枚举

use serde::{Deserialize, Serialize};

/// 策略类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StrategyType {
    /// Vegas 策略
    Vegas,
    /// NWE 策略
    Nwe,
    /// MACD+KDJ 策略
    MacdKdj,
    /// 吞没形态策略
    Engulfing,
    /// 综合策略
    Comprehensive,
    /// 多因子组合策略
    MultCombine,
    /// Squeeze 动量策略
    Squeeze,
    /// UT Boot 策略
    UtBoot,
    /// 顶级合约策略
    TopContract,
    /// 自定义策略
    Custom(u32),
}

impl StrategyType {
    pub fn as_str(&self) -> &str {
        match self {
            StrategyType::Vegas => "vegas",
            StrategyType::Nwe => "nwe",
            StrategyType::MacdKdj => "macd_kdj",
            StrategyType::Engulfing => "engulfing",
            StrategyType::Comprehensive => "comprehensive",
            StrategyType::MultCombine => "mult_combine",
            StrategyType::Squeeze => "squeeze",
            StrategyType::UtBoot => "ut_boot",
            StrategyType::TopContract => "top_contract",
            StrategyType::Custom(_) => "custom",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "vegas" => Some(StrategyType::Vegas),
            "nwe" => Some(StrategyType::Nwe),
            "macd_kdj" => Some(StrategyType::MacdKdj),
            "engulfing" => Some(StrategyType::Engulfing),
            "comprehensive" => Some(StrategyType::Comprehensive),
            "mult_combine" => Some(StrategyType::MultCombine),
            "squeeze" => Some(StrategyType::Squeeze),
            "ut_boot" => Some(StrategyType::UtBoot),
            "top_contract" => Some(StrategyType::TopContract),
            _ => None,
        }
    }
}

/// 策略状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyStatus {
    /// 未启动
    Stopped,
    /// 运行中
    Running,
    /// 暂停
    Paused,
    /// 错误
    Error,
}

impl StrategyStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            StrategyStatus::Stopped => "stopped",
            StrategyStatus::Running => "running",
            StrategyStatus::Paused => "paused",
            StrategyStatus::Error => "error",
        }
    }
}

/// 时间周期
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Timeframe {
    /// 1分钟
    M1,
    /// 3分钟
    M3,
    /// 5分钟
    M5,
    /// 15分钟
    M15,
    /// 30分钟
    M30,
    /// 1小时
    H1,
    /// 2小时
    H2,
    /// 4小时
    H4,
    /// 6小时
    H6,
    /// 12小时
    H12,
    /// 1天
    D1,
    /// 1周
    W1,
    /// 1月
    MN1,
}

impl Timeframe {
    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::M1 => "1m",
            Timeframe::M3 => "3m",
            Timeframe::M5 => "5m",
            Timeframe::M15 => "15m",
            Timeframe::M30 => "30m",
            Timeframe::H1 => "1H",
            Timeframe::H2 => "2H",
            Timeframe::H4 => "4H",
            Timeframe::H6 => "6H",
            Timeframe::H12 => "12H",
            Timeframe::D1 => "1Dutc",
            Timeframe::W1 => "1W",
            Timeframe::MN1 => "1M",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "1m" => Some(Timeframe::M1),
            "3m" => Some(Timeframe::M3),
            "5m" => Some(Timeframe::M5),
            "15m" => Some(Timeframe::M15),
            "30m" => Some(Timeframe::M30),
            "1H" | "1h" => Some(Timeframe::H1),
            "2H" | "2h" => Some(Timeframe::H2),
            "4H" | "4h" => Some(Timeframe::H4),
            "6H" | "6h" => Some(Timeframe::H6),
            "12H" | "12h" => Some(Timeframe::H12),
            "1Dutc" | "1d" => Some(Timeframe::D1),
            "1W" | "1w" => Some(Timeframe::W1),
            "1M" => Some(Timeframe::MN1),
            _ => None,
        }
    }

    /// 获取时间周期对应的分钟数
    pub fn to_minutes(&self) -> i64 {
        match self {
            Timeframe::M1 => 1,
            Timeframe::M3 => 3,
            Timeframe::M5 => 5,
            Timeframe::M15 => 15,
            Timeframe::M30 => 30,
            Timeframe::H1 => 60,
            Timeframe::H2 => 120,
            Timeframe::H4 => 240,
            Timeframe::H6 => 360,
            Timeframe::H12 => 720,
            Timeframe::D1 => 1440,
            Timeframe::W1 => 10080,
            Timeframe::MN1 => 43200,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_type_from_str() {
        assert_eq!(StrategyType::from_str("vegas"), Some(StrategyType::Vegas));
        assert_eq!(StrategyType::from_str("NWE"), Some(StrategyType::Nwe));
        assert_eq!(StrategyType::from_str("unknown"), None);
    }

    #[test]
    fn test_timeframe_conversion() {
        assert_eq!(Timeframe::from_str("1H"), Some(Timeframe::H1));
        assert_eq!(Timeframe::H1.to_minutes(), 60);
        assert_eq!(Timeframe::D1.to_minutes(), 1440);
    }
}
