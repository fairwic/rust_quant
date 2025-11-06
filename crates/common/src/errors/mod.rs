//! 统一错误类型定义

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("配置错误: {0}")]
    Config(String),
    
    #[error("数据库错误: {0}")]
    Database(String),
    
    #[error("网络错误: {0}")]
    Network(String),
    
    #[error("解析错误: {0}")]
    Parse(String),
    
    #[error("未知错误: {0}")]
    Unknown(String),
}
