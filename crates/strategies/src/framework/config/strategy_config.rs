// StrategyConfig 已移至 domain 包
// StrategyConfigEntity 已移至 infrastructure 包

// 重新导出供内部使用
pub use rust_quant_domain::StrategyConfig;
pub use rust_quant_infrastructure::{
    StrategyConfigEntity,
    StrategyConfigEntityModel,
};
