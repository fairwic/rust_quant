use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VolatilityTier {
    Btc,
    Eth,
    Alt,
}
impl VolatilityTier {
    /// 提供from交易对的集中实现，避免回测策略调用方重复处理相同细节。
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
    /// 提供标签的集中实现，避免回测策略调用方重复处理相同细节。
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
    /// 提供标签的集中实现，避免回测策略调用方重复处理相同细节。
    pub fn label(self) -> &'static str {
        match self {
            Self::Candidate => "可实验",
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
    /// 提供标签的集中实现，避免回测策略调用方重复处理相同细节。
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
    /// backtest ID。
    pub backtest_id: i64,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 周期。
    pub timeframe: String,
    /// 交易方向。
    pub side: String,
    /// 开仓时间。
    pub open_time_ms: i64,
    /// 平仓时间。
    pub close_time_ms: Option<i64>,
    /// 盈亏。
    pub pnl: f64,
    /// 类型标识。
    pub close_type: Option<String>,
    /// 止损来源；为空时使用默认值或表示不限制。
    pub stop_loss_source: Option<String>,
    /// 信号值；为空时表示该条件不启用。
    pub signal_value: Option<String>,
    /// 信号结果；为空时使用默认值或表示不限制。
    pub signal_result: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchFilteredSignalSample {
    /// backtest ID。
    pub backtest_id: i64,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 周期。
    pub timeframe: String,
    /// direction，用于记录新闻或情报分析结果。
    pub direction: String,
    /// 信号生成时间。
    pub signal_time_ms: i64,
    /// theoretical盈亏；为空时表示该条件不启用。
    pub theoretical_pnl: Option<f64>,
    /// trade结果；为空时使用默认值或表示不限制。
    pub trade_result: Option<String>,
    /// 过滤原因列表；为空时表示没有过滤原因。
    pub filter_reasons: Option<String>,
    /// 信号值；为空时表示该条件不启用。
    pub signal_value: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResearchSampleKind {
    Traded,
    Filtered,
}
impl ResearchSampleKind {
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    pub fn label(self) -> &'static str {
        match self {
            Self::Traded => "已成交样本",
            Self::Filtered => "过滤候选",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactorBucketReport {
    /// 名称。
    pub factor_name: String,
    /// 名称。
    pub bucket_name: String,
    /// 类型标识。
    pub sample_kind: ResearchSampleKind,
    /// volatilitytier，用于展示或持久化查询结果。
    pub volatility_tier: VolatilityTier,
    /// scopelabel，用于展示或持久化查询结果。
    pub scope_label: String,
    /// sample数量。
    pub sample_count: usize,
    /// 胜率。
    pub win_rate: f64,
    /// 平均盈亏，用于展示或持久化查询结果。
    pub avg_pnl: f64,
    /// sharpeproxy，用于展示或持久化查询结果。
    pub sharpe_proxy: f64,
    /// 平均mfe，用于展示或持久化查询结果。
    pub avg_mfe: f64,
    /// 平均mae，用于展示或持久化查询结果。
    pub avg_mae: f64,
    /// conclusion，用于展示或持久化查询结果。
    pub conclusion: FactorConclusion,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VegasFactorResearchQuery {
    /// 列表数据。
    pub baseline_ids: Vec<i64>,
    /// 周期。
    pub timeframe: String,
}
impl VegasFactorResearchQuery {
    /// 构建 回测与策略研究 所需实例，并集中初始化依赖和默认状态。
    pub fn new(baseline_ids: Vec<i64>) -> Self {
        Self {
            baseline_ids,
            timeframe: "4H".to_string(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VegasFactorResearchReport {
    /// 列表数据。
    pub trade_samples: Vec<ResearchTradeSample>,
    /// 列表数据。
    pub filtered_signal_samples: Vec<ResearchFilteredSignalSample>,
    /// 列表数据。
    pub factor_buckets: Vec<FactorBucketReport>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathImpactQuery {
    /// baseline ID。
    pub baseline_id: i64,
    /// 列表数据。
    pub experiment_ids: Vec<i64>,
    /// 周期。
    pub timeframe: String,
    /// 交易所合约或现货交易对标识。
    pub inst_id: Option<String>,
    /// topchangedlimit，用于交易策略计算。
    pub top_changed_limit: usize,
}
impl PathImpactQuery {
    /// 构建 回测与策略研究 所需实例，并集中初始化依赖和默认状态。
    pub fn new(baseline_id: i64, experiment_ids: Vec<i64>) -> Self {
        Self {
            baseline_id,
            experiment_ids,
            timeframe: "4H".to_string(),
            inst_id: None,
            top_changed_limit: 10,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathImpactTradeChange {
    /// 类型标识。
    pub change_type: String,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 交易方向。
    pub side: String,
    /// 开仓时间。
    pub open_time_ms: i64,
    /// baseline盈亏；为空时表示该条件不启用。
    pub baseline_pnl: Option<f64>,
    /// experiment盈亏；为空时表示该条件不启用。
    pub experiment_pnl: Option<f64>,
    /// 盈亏delta，用于记录交易或执行状态。
    pub pnl_delta: f64,
    /// 类型标识。
    pub close_type: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathImpactSummary {
    /// baseline ID。
    pub baseline_id: i64,
    /// experiment ID。
    pub experiment_id: i64,
    /// 交易所合约或现货交易对标识。
    pub inst_id: Option<String>,
    /// missing数量。
    pub missing_count: usize,
    /// missing盈亏，用于展示或持久化查询结果。
    pub missing_pnl: f64,
    /// missingwins，用于展示或持久化查询结果。
    pub missing_wins: usize,
    /// missing平均盈亏，用于展示或持久化查询结果。
    pub missing_avg_pnl: f64,
    /// new数量。
    pub new_count: usize,
    /// new盈亏，用于展示或持久化查询结果。
    pub new_pnl: f64,
    /// newwins，用于展示或持久化查询结果。
    pub new_wins: usize,
    /// new平均盈亏，用于展示或持久化查询结果。
    pub new_avg_pnl: f64,
    /// common数量。
    pub common_count: usize,
    /// common盈亏delta，用于展示或持久化查询结果。
    pub common_pnl_delta: f64,
    /// commonimproved数量。
    pub common_improved_count: usize,
    /// total路径delta，用于展示或持久化查询结果。
    pub total_path_delta: f64,
    /// verdict，用于展示或持久化查询结果。
    pub verdict: String,
    /// 列表数据。
    pub top_changes: Vec<PathImpactTradeChange>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathImpactReport {
    /// 列表数据。
    pub summaries: Vec<PathImpactSummary>,
}
