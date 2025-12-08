//! 趋势指标

pub mod counter_trend;
pub mod ema;
pub mod ema_indicator;
pub mod nwe; // ⭐ NWE 指标模块（包含indicator_combine）
pub mod nwe_indicator; // 从 src/trading/indicator 迁移
pub mod signal_weight; // 从 src/trading/indicator 迁移
pub mod sma;
pub mod vegas; // 从 src/trading/indicator/vegas_indicator 迁移 // 从 src/trading/indicator 迁移 // 逆势回调逻辑

// 重新导出
pub use ema::EmaIndicator; // 明确导出，避免冲突
pub use nwe::*; // ⭐ 导出 NWE 相关类型
pub use nwe_indicator::*;
pub use signal_weight::*;
pub use sma::*;
pub use vegas as vegas_indicator; // 兼容旧路径
                                  // ema_indicator 与 ema 冲突，不再导出（已被 ema 替代）
