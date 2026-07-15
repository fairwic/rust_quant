use serde::{Deserialize, Serialize};

/// PA 市场结构分类；无法稳定分类时必须进入 Unknown 或 Chaos。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaMarketRegime {
    /// 方向效率和均线斜率同时支持趋势。
    Trend,
    /// 低效率、低均线斜率且具有足够宽度的区间。
    Range,
    /// 数据有效但趋势和区间条件均不稳定。
    Chaos,
    /// 数据不足或结构无法计算。
    Unknown,
}

/// 方向归一化后的交易方向，不包含平仓语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaDirection {
    /// 做多方向。
    Long,
    /// 做空方向。
    Short,
}

/// 独立 PA 候选的确定性来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaCandidateKind {
    /// 趋势中的 EMA20 回撤恢复。
    TrendPullback,
    /// 原趋势回撤 setup 后由下一根已确认 K 线突破确认。
    TrendFollowThrough,
    /// 顺势二次入场；v1 保留类型但不做模糊形态推断。
    TrendSecondEntry,
    /// 区间边界向中点回归。
    RangeBoundary,
}

/// PA Quant Tree 支持的策略标识；每个标识都固定候选语义与周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaStrategyKey {
    /// 15 分钟区间边界回归策略。
    PaRange15m,
    /// 1 小时区间边界回归策略。
    PaRange1h,
    /// 15 分钟趋势回撤策略。
    PaTrend15m,
    /// 1 小时趋势回撤策略。
    PaTrend1h,
    /// 15 分钟趋势跟随确认策略；与原趋势回撤策略保持独立证据。
    PaTrendFollowthrough15m,
    /// 1 小时趋势跟随确认策略；与原趋势回撤策略保持独立证据。
    PaTrendFollowthrough1h,
    /// Vegas 原始候选的 PA 只读 Meta-filter。
    VegasPaMetaFilter,
}

impl PaStrategyKey {
    /// 从外部 strategy_key 解析 v1 白名单，拒绝无版本或未审批的名称。
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "pa_range_15m" => Ok(Self::PaRange15m),
            "pa_range_1h" => Ok(Self::PaRange1h),
            "pa_trend_15m" => Ok(Self::PaTrend15m),
            "pa_trend_1h" => Ok(Self::PaTrend1h),
            "pa_trend_followthrough_15m" => Ok(Self::PaTrendFollowthrough15m),
            "pa_trend_followthrough_1h" => Ok(Self::PaTrendFollowthrough1h),
            "vegas_pa_meta_filter" => Ok(Self::VegasPaMetaFilter),
            _ => Err(format!("unsupported PA strategy key: {value}")),
        }
    }

    /// 返回该策略需要的分钟周期；Meta-filter 不绑定独立 K 线周期。
    pub fn timeframe_minutes(self) -> Option<u32> {
        match self {
            Self::PaRange15m | Self::PaTrend15m | Self::PaTrendFollowthrough15m => Some(15),
            Self::PaRange1h | Self::PaTrend1h | Self::PaTrendFollowthrough1h => Some(60),
            Self::VegasPaMetaFilter => None,
        }
    }

    /// 限制独立 PA 策略只能执行其冻结候选家族。
    pub fn supports_candidate(self, candidate: PaCandidateKind) -> bool {
        match self {
            Self::PaRange15m | Self::PaRange1h => candidate == PaCandidateKind::RangeBoundary,
            Self::PaTrend15m | Self::PaTrend1h => matches!(
                candidate,
                PaCandidateKind::TrendPullback | PaCandidateKind::TrendSecondEntry
            ),
            Self::PaTrendFollowthrough15m | Self::PaTrendFollowthrough1h => {
                candidate == PaCandidateKind::TrendFollowThrough
            }
            Self::VegasPaMetaFilter => false,
        }
    }

    /// 只有独立 v5 策略允许把确认棒作为候选信号时点。
    pub fn uses_followthrough_confirmation(self) -> bool {
        matches!(
            self,
            Self::PaTrendFollowthrough15m | Self::PaTrendFollowthrough1h
        )
    }

    /// Meta-filter 只能进入 Vegas shadow 路径，不能生成独立 PA 交易。
    pub fn is_meta_filter(self) -> bool {
        self == Self::VegasPaMetaFilter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn followthrough_strategy_keys_are_isolated_from_v1_trend_candidates() {
        let strategy = PaStrategyKey::parse("pa_trend_followthrough_15m").unwrap();

        assert_eq!(strategy.timeframe_minutes(), Some(15));
        assert!(strategy.supports_candidate(PaCandidateKind::TrendFollowThrough));
        assert!(!strategy.supports_candidate(PaCandidateKind::TrendPullback));
        assert!(!PaStrategyKey::PaTrend15m.supports_candidate(PaCandidateKind::TrendFollowThrough));
    }
}

/// 每次不交易都必须记录的结构化阻塞原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaBlocker {
    /// K 线数量、确认状态或指标预热不足。
    DataNotReady,
    /// OHLC、成交量或数值合法性失败。
    InvalidCandle,
    /// 市场结构不能确定为允许交易的趋势或区间。
    UnknownRegime,
    /// 结构有效，但没有满足候选事件定义。
    NoCandidate,
    /// 冻结模型拒绝候选。
    QualityRejected,
    /// 原趋势 setup 成立，但唯一允许的下一棒没有通过方向跟随确认。
    ConfirmationRejected,
    /// 下一棒实际入场后的止损、目标或盈亏比不合法。
    RiskPlanInvalid,
}

impl PaBlocker {
    /// 返回稳定的审计代码，供数据库和报告聚合。
    pub fn code(&self) -> &'static str {
        match self {
            Self::DataNotReady => "data_not_ready",
            Self::InvalidCandle => "invalid_candle",
            Self::UnknownRegime => "unknown_regime",
            Self::NoCandidate => "no_candidate",
            Self::QualityRejected => "quality_rejected",
            Self::ConfirmationRejected => "confirmation_rejected",
            Self::RiskPlanInvalid => "risk_plan_invalid",
        }
    }
}

/// 信号时点可见的确定性 PA 特征快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaFeatureSnapshot {
    /// 最后一根已确认 K 线时间戳。
    pub signal_ts: i64,
    /// ATR14。
    pub atr14: f64,
    /// EMA20 当前值。
    pub ema20: f64,
    /// EMA20 五棒变化除以 ATR14。
    pub ema_slope_atr_20_5: f64,
    /// 最近 20 棒方向效率。
    pub range_efficiency_20: f64,
    /// 最近 20 棒最高价。
    pub range_high_20: f64,
    /// 最近 20 棒最低价。
    pub range_low_20: f64,
    /// 当前收盘在 20 棒区间中的位置，范围为 0 到 1。
    pub range_position_20: f64,
    /// 最近八棒相邻区间平均重叠比例。
    pub mean_overlap_ratio_8: f64,
    /// 最近十棒收盘位于 EMA 趋势侧的方向化比例。
    pub always_in_score: f64,
    /// 当前棒实体占整棒长度比例。
    pub signal_body_ratio: f64,
    /// 当前棒收盘在自身高低区间的位置。
    pub close_position: f64,
    /// 最近三棒相对 EMA 的最大回撤深度，单位为 ATR。
    pub pullback_depth_atr_3: f64,
    /// 信号收盘越过 EMA20 的方向化距离，单位为 ATR；非趋势为0。
    pub directional_reclaim_atr: f64,
    /// 信号棒收盘靠近趋势方向端点的比例；非趋势为0.5。
    pub directional_close_strength: f64,
    /// 信号棒全长除以 ATR14，用于区分噪音棒与过度扩张棒。
    pub signal_range_atr: f64,
    /// 最近三棒中收盘位于趋势反侧的比例；非趋势为0。
    pub pullback_close_fraction_3: f64,
    /// 最近三棒是否触碰 EMA20，做多与做空规则共用。
    pub recent_ema_touch: bool,
    /// 当前确定性结构。
    pub regime: PaMarketRegime,
    /// 趋势结构的方向；非趋势时为空。
    pub trend_direction: Option<PaDirection>,
}

/// 在信号棒收盘时冻结、等待下一棒开盘执行的候选。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaCandidate {
    /// 产生候选的 K 线时间戳。
    pub signal_ts: i64,
    /// 跟随确认候选对应的原始 setup 时间戳。
    /// None 表示 v1 趋势/区间候选；序列化时省略以保持旧研究证据稳定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_ts: Option<i64>,
    /// 候选方向。
    pub direction: PaDirection,
    /// 候选事件类型。
    pub kind: PaCandidateKind,
    /// 信号时点已经确定的结构止损。
    pub stop_price: f64,
    /// 区间策略的中点目标；趋势策略在实际入场后按 1.5R 计算。
    pub range_target: Option<f64>,
}

/// 使用下一棒开盘价完成合法性复核后的执行计划。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaExecutionPlan {
    /// 原始信号时间戳。
    pub signal_ts: i64,
    /// 实际入场棒时间戳。
    pub entry_ts: i64,
    /// 执行方向。
    pub direction: PaDirection,
    /// 候选事件类型。
    pub kind: PaCandidateKind,
    /// 下一棒开盘入场价格。
    pub entry_price: f64,
    /// 冻结结构止损价格。
    pub stop_price: f64,
    /// 固定 R 或区间中点目标价格。
    pub target_price: f64,
    /// 入场时的收益风险比。
    pub reward_risk: f64,
}

/// 单次决策的完整可审计结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaDecisionTrace {
    /// 决策对应的信号时间戳。
    pub signal_ts: i64,
    /// 本次使用的 manifest 哈希。
    pub manifest_hash: String,
    /// 冻结模型输出分数。
    pub model_score: Option<f64>,
    /// 信号时点冻结的确定性特征；数据未准备好时为空。
    pub features: Option<PaFeatureSnapshot>,
    /// 识别到的候选。
    pub candidate: Option<PaCandidate>,
    /// 通过下一棒风险复核后的执行计划。
    pub execution: Option<PaExecutionPlan>,
    /// 不交易原因；执行成功时为空。
    pub blocker: Option<PaBlocker>,
}
