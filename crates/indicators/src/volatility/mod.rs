//! 波动性指标

pub mod atr;
pub mod atr_stop_loss;
pub mod bollinger;

// 重新导出
pub use atr::ATR; // 明确导出ATR类型（AtrError由atr_stop_loss导出）
pub use atr_stop_loss::*; // ⭐ 导出 ATR Stop Loss（包含AtrError）
pub use bollinger::*;
