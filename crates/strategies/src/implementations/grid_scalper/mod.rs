pub mod executor;
pub mod strategy;
pub mod types;

pub use executor::{run_grid_backtest, GridScalperBacktestAdapter};
pub use strategy::GridScalperStrategy;
pub use types::{
    GridAction, GridScalperBacktestTuning, GridScalperDecision, GridScalperSignalSnapshot,
    GridScalperThresholds,
};
