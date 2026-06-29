mod executor;
mod strategy;
mod types;

pub use executor::BearShortStackStrategyExecutor;
pub use strategy::BearShortStackStrategy;
pub use types::{
    BearShortAction, BearShortDecision, BearShortPreset, BearShortSignalSnapshot,
    BearShortStackBacktestMarketContext, BearShortStackBacktestTuning, BearShortStackConfig,
    BearShortStackThresholds,
};
