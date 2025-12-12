//! # Realtime Risk
//!
//! 实盘实时风控模块：
//! - 监听K线/价格事件
//! - 监听策略运行中的持仓事件
//! - 触发风控动作（如：到达 1.5R 后移动止损到开仓价，保本）
//!
//! 说明：
//! - 本模块不直接依赖 services/execution 的具体实现，采用“事件输入 + 执行器注入”的方式。
//! - 真实接入时，上层（runner / service / job）负责把K线与持仓变更事件推送进来。

pub mod types;
pub mod breakeven_stop_loss;
pub mod okx_stop_loss_amender;
pub mod engine;

pub use breakeven_stop_loss::*;
pub use okx_stop_loss_amender::*;
pub use engine::*;
pub use types::*;

