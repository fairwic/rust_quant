//! 订单管理模块
//! 
//! ORM 迁移: rbatis → sqlx

// 新版本 (sqlx)
pub mod swap_order_sqlx;
pub mod swap_orders_detail_sqlx;

// 重新导出
pub use swap_order_sqlx::SwapOrderEntity;
pub use swap_orders_detail_sqlx::SwapOrdersDetailEntity;
