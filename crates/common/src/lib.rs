//! # Rust Quant Common
//!
//! 公共类型、工具函数和常量定义

pub mod constants;
pub mod errors;
pub mod types;
pub mod utils;

// 重新导出常用类型
pub use errors::{AppError, Result};
pub use types::*;
