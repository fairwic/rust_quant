use core::fmt::{Display, Formatter};

pub enum PLATFORM {
    PlatformOkx,
    PlatformBinance,
    PlatformHuobi,
    PlatformBitget,
    PlatformCoinbase,
}
impl Display for PLATFORM {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PLATFORM::PlatformOkx => "okx",
                PLATFORM::PlatformBinance => "binance",
                PLATFORM::PlatformHuobi => "huobi",
                PLATFORM::PlatformBitget => "bitget",
                PLATFORM::PlatformCoinbase => "coinbase",
            }
        )
    }
}
