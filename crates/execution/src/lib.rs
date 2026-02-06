//! # Rust Quant Execution
//!
//! 订单执行：订单管理、持仓管理
pub mod order_manager;
pub mod position_manager;

// ⭐ 不能在这里实现From<okx::Error>，因为违反孤儿规则
// 已添加 OkxApiError 变体到 AppError，使用 .map_err() 转换
