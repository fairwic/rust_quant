//! # Rust Quant Indicators
//! 
//! 技术指标库：趋势、动量、波动性、成交量

pub mod trend;
pub mod momentum;
pub mod volatility;
pub mod volume;
pub mod pattern;
pub mod cache;  // 指标缓存模块

// 重新导出所有子模块的类型
pub use trend::*;
pub use momentum::*;
pub use volatility::*;
pub use volume::*;
pub use pattern::*;

// 统一指标接口
pub trait Indicator {
    type Input;
    type Output;
    
    fn update(&mut self, input: Self::Input) -> Self::Output;
    fn reset(&mut self);
}
