//! 值对象模块
//!
//! 值对象是不可变的、无身份标识的领域概念
//! 它们通过值来识别，而不是通过ID

pub mod leverage;
pub mod percentage;
pub mod price;
pub mod signal;
pub mod symbol;
pub mod volume;

pub use leverage::{Leverage, LeverageError};
pub use percentage::{Percentage, PercentageError};
pub use price::{Price, PriceError};
pub use signal::{SignalDirection, SignalResult, SignalStrength, TradingSignal};
pub use symbol::{Symbol, SymbolError};
pub use volume::{Volume, VolumeError};
