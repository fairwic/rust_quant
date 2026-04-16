mod report;
mod service;
mod types;

pub use report::{render_path_impact_report, render_report};
pub use service::VegasFactorResearchService;
pub use types::{
    FactorBucketReport, FactorConclusion, PathImpactQuery, PathImpactReport, PathImpactSummary,
    PathImpactTradeChange, PriceOiState, ResearchFilteredSignalSample, ResearchSampleKind,
    ResearchTradeSample, VegasFactorResearchQuery, VegasFactorResearchReport, VolatilityTier,
};
