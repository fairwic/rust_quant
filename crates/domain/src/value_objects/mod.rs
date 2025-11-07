//! 值对象模块
//! 
//! 值对象是不可变的、无身份标识的领域概念
//! 它们通过值来识别，而不是通过ID

pub mod price;
pub mod volume;
pub mod signal;

pub use price::{Price, PriceError};
pub use volume::{Volume, VolumeError};
pub use signal::{
    SignalDirection, 
    SignalStrength, 
    TradingSignal, 
    SignalResult
};


