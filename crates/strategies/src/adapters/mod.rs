//! 适配器模块
//! 
//! 提供适配器类型来解决孤儿规则问题
//! 
//! ## 为什么需要适配器？
//! 
//! Rust的孤儿规则禁止为外部类型实现外部trait。
//! 例如，我们不能直接为 `CandlesEntity`(来自market包) 实现 `High`/`Low`/`Close`(来自ta库)。
//! 
//! ## 解决方案
//! 
//! 使用Newtype模式创建本地包装器，然后为包装器实现trait。

pub mod candle_adapter;

pub use candle_adapter::{CandleAdapter, adapt, adapt_many};


