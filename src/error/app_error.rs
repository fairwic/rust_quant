use std::fmt;
use thiserror::Error;
/// 应用错误
#[derive(Error, Debug)]
pub enum AppError {
    /// 业务错误
    #[error("业务错误: {0}")]
    BizError(String),

    /// 数据库错误
    #[error("数据库错误: {0}")]
    DbError(String),

    #[error("OKX API错误: {0}")]
    OkxApiError(String),

    /// 未知错误
    #[error("未知错误: {0}")]
    Unknown(String),
}

/// OKX API特定错误码
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiErrorCode {
    /// 操作成功
    Ok = 0,
    /// 未知错误
    Unknown = 99999,
}

impl fmt::Display for ApiErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} ({})", self, *self as i32)
    }
}

impl ApiErrorCode {
    /// 从错误码获取ApiErrorCode枚举
    pub fn from_code(code: u32) -> Self {
        match code {
            0 => Self::Ok,
            _ => Self::Unknown,
        }
    }
}

/// 把任何错误转换为Error类型的结果
pub fn to_err<E: std::error::Error + Send + Sync + 'static>(err: E) -> AppError {
    AppError::Unknown(err.to_string())
}

/// 把okx的错误转换为AppError
impl From<okx::error::Error> for AppError {
    fn from(err: okx::error::Error) -> Self {
        AppError::OkxApiError(err.to_string())
    }
}
