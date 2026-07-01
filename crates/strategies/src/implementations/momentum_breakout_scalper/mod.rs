mod executor;
mod strategy;
mod types;

pub use executor::MomentumBreakoutScalperStrategyExecutor;
pub use strategy::MomentumBreakoutScalperStrategy;
pub use types::{
    MomentumBreakoutAction, MomentumBreakoutBacktestTuning, MomentumBreakoutDecision,
    MomentumBreakoutSignalSnapshot, MomentumBreakoutThresholds,
};
