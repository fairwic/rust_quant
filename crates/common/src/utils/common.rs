use core::fmt::{Display, Formatter};
pub enum PLATFORM {
    PlatformOkx,
    PlatformBinance,
    PlatformHuobi,
    PlatformBitget,
    PlatformCoinbase,
}
impl Display for PLATFORM {
    /// 封装当前函数，减少量化核心调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
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
