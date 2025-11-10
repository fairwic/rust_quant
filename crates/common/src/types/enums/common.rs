use serde::{Deserialize, Serialize};
pub trait EnumAsStrTrait {
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
