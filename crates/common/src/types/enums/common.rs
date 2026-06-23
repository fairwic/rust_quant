use serde::{Deserialize, Serialize};
pub trait EnumAsStrTrait {
    /// 封装当前函数，减少量化核心调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
    fn as_str(&self) -> &'static str;
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PeriodEnum {
    OneMinute,
    FiveMinute,
    FifteenMinute,
    ThirtyMinute,
    OneHour,
    FourHour,
    OneDay,
    OneDayUtc,
}
impl EnumAsStrTrait for PeriodEnum {
    /// 提供转换为字符串的集中实现，避免量化核心调用方重复处理相同细节。
    fn as_str(&self) -> &'static str {
        match self {
            PeriodEnum::OneMinute => "1m",
            PeriodEnum::FiveMinute => "5m",
            PeriodEnum::FifteenMinute => "15m",
            PeriodEnum::ThirtyMinute => "30m",
            PeriodEnum::OneHour => "1H",
            PeriodEnum::FourHour => "4H",
            PeriodEnum::OneDay => "1D",
            PeriodEnum::OneDayUtc => "1Dutc",
        }
    }
}
