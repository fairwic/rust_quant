pub mod executor;
pub mod strategy;
pub mod types;

pub use executor::SuperTrendBacktestAdapter;
pub use strategy::SuperTrendStrategy;
pub use types::{
    SuperTrendAction, SuperTrendBacktestTuning, SuperTrendDecision, SuperTrendDirection,
    SuperTrendSignalSnapshot, SuperTrendThresholds,
};
