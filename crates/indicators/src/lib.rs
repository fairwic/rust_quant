//! # Rust Quant Indicators
//!
//! 技术指标库：趋势、动量、波动性、成交量
pub mod cache;
pub mod momentum;
pub mod pattern;
pub mod trend;
pub mod volatility;
pub mod volume; // 指标缓存模块
                // 重新导出所有子模块的类型
pub use momentum::*;
pub use pattern::*;
pub use trend::*;
pub use volatility::*;
pub use volume::*;
// 统一指标接口
pub trait Indicator {
    type Input;
    type Output;
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
    fn update(&mut self, input: Self::Input) -> Self::Output;
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
    fn reset(&mut self);
}
