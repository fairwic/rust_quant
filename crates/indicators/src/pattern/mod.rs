//! 形态识别指标

pub mod engulfing;
pub mod hammer;
// pub mod support_resistance;  // 从 strategies 移入 - 暂时注释，依赖旧结构需重构

// 从 src/trading/indicator 迁移
pub mod equal_high_low_indicator;
pub mod fair_value_gap_indicator;
pub mod leg_detection_indicator;
pub mod market_structure_indicator;
pub mod premium_discount_indicator;

// 重新导出
pub use engulfing::*;
pub use hammer::*;
pub use equal_high_low_indicator::*;
pub use fair_value_gap_indicator::*;
pub use leg_detection_indicator::*;
pub use market_structure_indicator::*;
pub use premium_discount_indicator::*;

