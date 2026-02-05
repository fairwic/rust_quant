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
}

impl std::str::FromStr for StrategyType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vegas" => Ok(StrategyType::Vegas),
            "nwe" => Ok(StrategyType::Nwe),
            "macd_kdj" => Ok(StrategyType::MacdKdj),
            "engulfing" => Ok(StrategyType::Engulfing),
            "comprehensive" => Ok(StrategyType::Comprehensive),
            "mult_combine" => Ok(StrategyType::MultCombine),
            "squeeze" => Ok(StrategyType::Squeeze),
            "ut_boot" => Ok(StrategyType::UtBoot),
            "top_contract" => Ok(StrategyType::TopContract),
            _ => Err(format!("Unknown strategy type: {}", s)),
        }
    }
}

/// 策略状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StrategyStatus {
    /// 未启动
    Stopped,
    /// 运行中
    #[default]
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

impl std::str::FromStr for Timeframe {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1m" => Ok(Timeframe::M1),
            "3m" => Ok(Timeframe::M3),
            "5m" => Ok(Timeframe::M5),
            "15m" => Ok(Timeframe::M15),
            "30m" => Ok(Timeframe::M30),
            "1H" | "1h" => Ok(Timeframe::H1),
            "2H" | "2h" => Ok(Timeframe::H2),
            "4H" | "4h" => Ok(Timeframe::H4),
            "6H" | "6h" => Ok(Timeframe::H6),
            "12H" | "12h" => Ok(Timeframe::H12),
            "1Dutc" | "1d" => Ok(Timeframe::D1),
            "1W" | "1w" => Ok(Timeframe::W1),
            "1M" => Ok(Timeframe::MN1),
            _ => Err(format!("Unknown timeframe: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_type_from_str() {
        use std::str::FromStr;
        assert_eq!(StrategyType::from_str("vegas"), Ok(StrategyType::Vegas));
        assert_eq!(StrategyType::from_str("NWE"), Ok(StrategyType::Nwe));
        assert!(StrategyType::from_str("unknown").is_err());
    }

    #[test]
    fn test_timeframe_conversion() {
        use std::str::FromStr;
        assert_eq!(Timeframe::from_str("1H"), Ok(Timeframe::H1));
        assert_eq!(Timeframe::H1.to_minutes(), 60);
        assert_eq!(Timeframe::D1.to_minutes(), 1440);
    }
}
