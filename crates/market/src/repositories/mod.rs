//! 数据持久化仓储

pub mod candle_service;
// TODO: persist_worker 依赖rbatis，暂时注释
// pub mod persist_worker;

// 重新导出
pub use candle_service::*;
