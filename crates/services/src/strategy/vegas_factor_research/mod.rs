mod report;
mod service;
mod types;

pub use report::render_report;
pub use service::VegasFactorResearchService;
pub use types::{
    FactorBucketReport, FactorConclusion, PriceOiState, ResearchFilteredSignalSample,
    ResearchSampleKind, ResearchTradeSample, VegasFactorResearchQuery, VegasFactorResearchReport,
    VolatilityTier,
};
