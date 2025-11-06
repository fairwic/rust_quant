//! # Rust Quant Indicators
//! 
//! 技术指标库：趋势、动量、波动性、成交量

pub mod trend;
pub mod momentum;
pub mod volatility;
pub mod volume;
pub mod pattern;

// 统一指标接口
pub trait Indicator {
    type Input;
    type Output;
    
    fn update(&mut self, input: Self::Input) -> Self::Output;
    fn reset(&mut self);
}
