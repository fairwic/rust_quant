//! 策略相关服务模块

pub mod backtest_service;
pub mod live_decision;
pub mod live_parity;
pub mod strategy_config_service;
pub mod strategy_data_service;
pub mod strategy_execution_service;
pub mod vegas_factor_research;

pub use backtest_service::BacktestService;
pub use live_decision::{apply_live_decision, LiveDecisionOutcome};
pub use live_parity::{
    compare_parity_rows, compare_timing_parity, replay_live_with_warmup, to_parity_trade_rows,
    LiveReplayResult, PaperOrderRecord, ParityComparisonReport, ParityDifference, ParityTradeRow,
    TimePair, TimingParityReport,
};
pub use strategy_config_service::StrategyConfigService;
pub use strategy_data_service::StrategyDataService;
pub use strategy_execution_service::StrategyExecutionService;
pub use vegas_factor_research::{
    render_report, FactorBucketReport, FactorConclusion, PriceOiState,
    ResearchFilteredSignalSample, ResearchSampleKind, ResearchTradeSample,
    VegasFactorResearchQuery, VegasFactorResearchReport, VegasFactorResearchService, VolatilityTier,
};
