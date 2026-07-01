pub mod executor;
pub mod strategy;
pub mod types;

pub use executor::RsiDivergenceBacktestAdapter;
pub use strategy::RsiDivergenceStrategy;
pub use types::{
    DivergenceAction, DivergenceType, RsiDivergenceBacktestTuning, RsiDivergenceDecision,
    RsiDivergenceSignalSnapshot, RsiDivergenceThresholds,
};
