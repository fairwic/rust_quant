//! # Rust Quant Common
//! 
//! 公共类型、工具函数和常量定义

pub mod types;
pub mod utils;
pub mod constants;
pub mod errors;

// 重新导出常用类型
pub use types::*;
pub use errors::{Result, AppError};
