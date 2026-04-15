use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VolatilityTier {
    Btc,
    Eth,
    Alt,
}

impl VolatilityTier {
    pub fn from_symbol(inst_id: &str) -> Self {
        let upper = inst_id.to_ascii_uppercase();
        if upper.starts_with("BTC") {
            Self::Btc
        } else if upper.starts_with("ETH") {
            Self::Eth
        } else {
            Self::Alt
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Btc => "BTC",
            Self::Eth => "ETH",
            Self::Alt => "其他币种",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FactorConclusion {
    Candidate,
    Observe,
    Reject,
}

impl FactorConclusion {
    pub fn label(self) -> &'static str {
        match self {
            Self::Candidate => "可回注",
            Self::Observe => "仅观察",
            Self::Reject => "拒绝",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PriceOiState {
    LongBuildup,
    ShortBuildup,
    ShortCovering,
    LongUnwinding,
    Flat,
}

impl PriceOiState {
    pub fn label(self) -> &'static str {
        match self {
            Self::LongBuildup => "long_buildup",
            Self::ShortBuildup => "short_buildup",
            Self::ShortCovering => "short_covering",
            Self::LongUnwinding => "long_unwinding",
            Self::Flat => "flat",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchTradeSample {
    pub backtest_id: i64,
    pub inst_id: String,
    pub timeframe: String,
    pub side: String,
    pub open_time_ms: i64,
    pub close_time_ms: Option<i64>,
    pub pnl: f64,
    pub close_type: Option<String>,
    pub stop_loss_source: Option<String>,
    pub signal_value: Option<String>,
    pub signal_result: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchFilteredSignalSample {
    pub backtest_id: i64,
    pub inst_id: String,
    pub timeframe: String,
    pub direction: String,
    pub signal_time_ms: i64,
    pub theoretical_pnl: Option<f64>,
    pub trade_result: Option<String>,
    pub filter_reasons: Option<String>,
    pub signal_value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResearchSampleKind {
    Traded,
    Filtered,
}

impl ResearchSampleKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Traded => "已成交样本",
            Self::Filtered => "过滤候选",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactorBucketReport {
    pub factor_name: String,
    pub bucket_name: String,
    pub sample_kind: ResearchSampleKind,
    pub volatility_tier: VolatilityTier,
    pub sample_count: usize,
    pub win_rate: f64,
    pub avg_pnl: f64,
    pub sharpe_proxy: f64,
    pub avg_mfe: f64,
    pub avg_mae: f64,
    pub conclusion: FactorConclusion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VegasFactorResearchQuery {
    pub baseline_ids: Vec<i64>,
    pub timeframe: String,
}

impl VegasFactorResearchQuery {
    pub fn new(baseline_ids: Vec<i64>) -> Self {
        Self {
            baseline_ids,
            timeframe: "4H".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VegasFactorResearchReport {
    pub trade_samples: Vec<ResearchTradeSample>,
    pub filtered_signal_samples: Vec<ResearchFilteredSignalSample>,
    pub factor_buckets: Vec<FactorBucketReport>,
}
