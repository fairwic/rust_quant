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
    
    // ⭐ 新增：兼容旧代码的错误类型
    #[error("数据库错误: {0}")]
    DbError(String),
    
    #[error("业务错误: {0}")]
    BizError(String),
    
    #[error("OKX API错误: {0}")]
    OkxApiError(String),
}

// ⭐ 通用错误转换
impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Unknown(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Unknown(e.to_string())
    }
}
