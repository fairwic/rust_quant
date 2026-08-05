use serde::Serialize;

/// 冻结 Pine V1 研究版的源码身份，防止历史 Rust 结果与后续图表版本混在一起。
pub const PINE_SOURCE_FNV1A32: &str = "66d3937e";

/// Rust V1 对照实现的独立研究身份；该身份不注册到 Paper 或 Live。
pub const STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v1";

/// Pine V2 冻结快照去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V2_PINE_SOURCE_FNV1A32: &str = "3cbbc9d8";

/// Pine V2 对照实现的 Research 身份，历史报告继续绑定该版本。
pub const V2_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v2";

/// 冻结 V3 候选 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const CURRENT_PINE_SOURCE_FNV1A32: &str = "7827654b";

/// V3 只作为独立 Research 候选，不替换 V1/V2 的历史入口。
pub const CURRENT_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v3";

/// V4 主 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V4_PINE_SOURCE_FNV1A32: &str = "9ab73288";

/// V4 逆势退出与背离形态门禁的独立 Research 身份。
pub const V4_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v4";

/// V5 主 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V5_PINE_SOURCE_FNV1A32: &str = "a36f0e19";

/// V5 年龄化纯 RSI 逆势退出的独立 Research 身份。
pub const V5_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v5";

/// V6 冻结 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V6_PINE_SOURCE_FNV1A32: &str = "60d9e838";

/// V6 EMA 趋势多突破接受的独立 Research 身份。
pub const V6_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v6";

/// V7 冻结 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V7_PINE_SOURCE_FNV1A32: &str = "aa8a1e37";

/// V7 RSI 反向长影门禁的独立 Research 身份。
pub const V7_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v7";

/// V8 冻结 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V8_PINE_SOURCE_FNV1A32: &str = "252225ec";

/// V8 慢均线带收复空单门禁的独立 Research 身份。
pub const V8_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v8";

/// V9 冻结 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V9_PINE_SOURCE_FNV1A32: &str = "0e7a1393";

/// V9 可调指标默认值同步的独立 Research 身份。
pub const V9_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v9";

/// V10 冻结 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V10_PINE_SOURCE_FNV1A32: &str = "06973f3c";

/// V10 五类低质量入场门禁的独立 Research 身份。
pub const V10_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v10";

/// V11 冻结 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V11_PINE_SOURCE_FNV1A32: &str = "53ba4291";

/// V11 对 V10 剩余四类负期望样本增加预注册残差门禁。
pub const V11_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v11";

/// V12 冻结 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V12_PINE_SOURCE_FNV1A32: &str = "34752685";

/// V12 把五类过度耦合入场拆为 setup 与 2～4 根确认，只用于 Research 对照。
pub const V12_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v12";

/// V13 冻结 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V13_PINE_SOURCE_FNV1A32: &str = "b81e5d25";

/// V13 只拆分 EMA 压缩释放、放量突破和回踩接受，不改变其他 V11 家族。
pub const V13_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v13";

/// V14 冻结 Pine 去掉单个文件末尾换行后的 UTF-16 FNV-1a 身份。
pub const V14_PINE_SOURCE_FNV1A32: &str = "45391eac";

/// V14 把 EMA 压缩改为无方向 setup，方向仅由后续有限窗口 impulse 决定。
pub const V14_STRATEGY_VERSION: &str = "tradingview_velocity_parity_15m_research_v14";

/// V15 独立真实箱体突破接受 Pine 的 UTF-16 FNV-1a 身份。
pub const V15_PINE_SOURCE_FNV1A32: &str = "28f2817f";

/// V15 不继承组合策略，只研究真实箱体收缩、冻结突破与回踩接受。
pub const V15_STRATEGY_VERSION: &str = "range_squeeze_break_acceptance_15m_research_v15";

/// V16 独立右侧触发 Pine 的 UTF-16 FNV-1a 身份。
pub const V16_PINE_SOURCE_FNV1A32: &str = "5ac357c1";

/// V16 只替换 V15 的入场时序与可交易性门槛，退出合同保持冻结。
pub const V16_STRATEGY_VERSION: &str = "range_squeeze_right_side_trigger_15m_research_v16";

/// V17 纯右侧触发消融 Pine 的 UTF-16 FNV-1a 身份。
pub const V17_PINE_SOURCE_FNV1A32: &str = "7097ee03";

/// V17 保留 V15 接受资格，只移除 V16 在 stop entry 上新增的经济门禁。
pub const V17_STRATEGY_VERSION: &str = "range_squeeze_right_side_trigger_ablation_15m_research_v17";

/// V18 组合主 Pine 的 UTF-16 FNV-1a 身份。
pub const V18_PINE_SOURCE_FNV1A32: &str = "9f26295a";

/// V18 保留 V11 主策略，并把冻结 V17 作为低优先级补充家族。
pub const V18_STRATEGY_VERSION: &str =
    "tradingview_velocity_v11_plus_range_squeeze_v17_15m_research_v18";

/// V19 组合主 Pine 的 UTF-16 FNV-1a 身份。
pub const V19_PINE_SOURCE_FNV1A32: &str = "406cde87";

/// V19 冻结 V18，只拒绝带显著长下影的锚区假突破空单。
pub const V19_STRATEGY_VERSION: &str =
    "tradingview_velocity_v18_plus_false_breakout_lower_wick_guard_15m_research_v19";

/// V20 组合主 Pine 的 UTF-16 FNV-1a 身份。
pub const V20_PINE_SOURCE_FNV1A32: &str = "a755168d";

/// V20 冻结 V19，并新增突破后扫高失败接受的独立早期空单家族。
pub const V20_STRATEGY_VERSION: &str =
    "volume_anchor_upthrust_failed_acceptance_short_15m_research_v20";

/// V21 只把 V20 扫高拒绝改为紧邻下一根完成棒的右侧确认。
pub const V21_STRATEGY_VERSION: &str =
    "volume_anchor_upthrust_failed_acceptance_right_side_short_15m_research_v21";

/// V22A 在 V21 上限制确认棒最多消耗冻结结构奖励的 25%。
pub const V22A_STRATEGY_VERSION: &str =
    "volume_anchor_upthrust_failed_acceptance_target_consumption_cap_25_15m_research_v22a";

/// V22B 是预注册主候选，最多允许确认棒消耗冻结结构奖励的 33%。
pub const V22B_STRATEGY_VERSION: &str =
    "volume_anchor_upthrust_failed_acceptance_target_consumption_cap_33_15m_research_v22b";

/// V22C 放宽至 50%，用于检查目标消耗门禁的参数方向是否稳定。
pub const V22C_STRATEGY_VERSION: &str =
    "volume_anchor_upthrust_failed_acceptance_target_consumption_cap_50_15m_research_v22c";

/// V23 只替换 V20 的锚区来源，继续使用原有第 1～2 根失败确认和冻结风险合同。
pub const V23_STRATEGY_VERSION: &str =
    "volume_recent_horizontal_first_break_upthrust_failed_acceptance_short_15m_research_v23";

/// V24 保留 V23 横盘锚点，但跌回上沿时不再要求确认棒超过突破棒高点。
pub const V24_STRATEGY_VERSION: &str =
    "volume_recent_horizontal_first_break_close_back_short_15m_research_v24";

/// V25A 在 V24 上把 8 根横盘收盘方向效率限制为 0.30。
pub const V25A_STRATEGY_VERSION: &str =
    "volume_recent_horizontal_direction_efficiency_30_first_break_close_back_short_15m_research_v25a";

/// V25B 是预注册主候选，把 8 根横盘收盘方向效率限制为 0.35。
pub const V25B_STRATEGY_VERSION: &str =
    "volume_recent_horizontal_direction_efficiency_35_first_break_close_back_short_15m_research_v25b";

/// V25C 是同一变量的邻域上界，把方向效率限制为 0.40。
pub const V25C_STRATEGY_VERSION: &str =
    "volume_recent_horizontal_direction_efficiency_40_first_break_close_back_short_15m_research_v25c";

/// V26 以 V25B 为基线，只把固定 8 根旧窗口替换为紧贴突破前的最长有效父横盘。
pub const V26_STRATEGY_VERSION: &str =
    "volume_active_parent_horizontal_first_break_close_back_short_15m_research_v26";

/// V27 保留 V26 父横盘，只要求确认棒完全否定突破棒上涨实体。
pub const V27_STRATEGY_VERSION: &str =
    "volume_active_parent_horizontal_breakout_body_rejection_short_15m_research_v27";

/// V28 保留 V27，并要求实体否定深度至少达到父横盘高度的 10%。
pub const V28_STRATEGY_VERSION: &str =
    "volume_active_parent_horizontal_normalized_body_rejection_10pct_short_15m_research_v28";

/// V29 保留 V27，只接受突破收盘超出父横盘上沿不超过区间高度 10% 的浅突破。
pub const V29_STRATEGY_VERSION: &str =
    "volume_active_parent_horizontal_shallow_breakout_excess_10pct_short_15m_research_v29";

/// V30 保留 V29，并要求父横盘在突破前至少完成三次上下边界交替。
pub const V30_STRATEGY_VERSION: &str =
    "volume_active_parent_horizontal_edge_transitions_3_shallow_breakout_excess_10pct_short_15m_research_v30";

/// 严格视觉横盘首次放量收盘上破的独立 Research 身份；不覆盖 V20 或 V29。
pub const STRICT_VISUAL_BREAKOUT_STRATEGY_VERSION: &str =
    "volume_strict_visual_consolidation_breakout_long_15m_research_v1";

/// V2 只把 V1 的直接入场延后到冻结上沿的三棒回踩接受确认。
pub const STRICT_VISUAL_BREAKOUT_RETEST_ACCEPTANCE_STRATEGY_VERSION: &str =
    "volume_strict_visual_consolidation_breakout_retest_acceptance_long_15m_research_v2";

/// V3 保留 V2，只接受确认收盘仍守在冻结突破实体中点之上的严格子集。
pub const STRICT_VISUAL_BREAKOUT_BODY_MIDPOINT_HOLD_STRATEGY_VERSION: &str =
    "volume_strict_visual_consolidation_breakout_body_midpoint_hold_long_15m_research_v3";

/// V4 保留 V3 信号，只把不超过 32 根的冻结横盘 Fixed 目标缩短为 1R。
pub const STRICT_VISUAL_BREAKOUT_SHORT_RANGE_ONE_R_STRATEGY_VERSION: &str =
    "volume_strict_visual_consolidation_breakout_body_midpoint_hold_short_range_32_one_r_long_15m_research_v4";

/// V5 回到 V3 的退出合同，只新增突破源实体占比与方向位移强度门禁。
pub const STRICT_VISUAL_BREAKOUT_BODY_STRENGTH_STRATEGY_VERSION: &str =
    "volume_strict_visual_consolidation_breakout_body_strength_60pct_25bps_body_midpoint_hold_long_15m_research_v5";

/// V6 保留 V5 强突破门禁，只把弱离区立即消费改为紧邻一根完成棒观察期。
pub const STRICT_VISUAL_BREAKOUT_WEAK_DEPARTURE_PROBATION_STRATEGY_VERSION: &str =
    "volume_strict_visual_consolidation_weak_departure_one_bar_probation_body_strength_long_15m_research_v6";

/// V8 保留 V6 全部合同，只要求首次合法确认收盘至少高出冻结上沿 0.40 个来源 ATR。
pub const STRICT_VISUAL_BREAKOUT_ACCEPTANCE_MARGIN_STRATEGY_VERSION: &str =
    "volume_strict_visual_consolidation_stronger_acceptance_margin_0_40_atr_long_15m_research_v8";

/// V9 保留 V8 全部合同，只拒绝未收盘越过尚未解决外部结构高点的突破来源。
pub const STRICT_VISUAL_BREAKOUT_EXTERNAL_STRUCTURE_CLEARANCE_STRATEGY_VERSION: &str =
    "volume_strict_visual_consolidation_external_structure_clearance_long_15m_research_v9";

/// 双向、无量能门禁且按越界幅度保留 25% 的独立 Research 合同；不覆盖 V1～V9。
pub const STRICT_VISUAL_SYMMETRIC_RETAINED_BREAKOUT_STRATEGY_VERSION: &str =
    "strict_visual_consolidation_symmetric_retained_breakout_15m_research_v1";
/// V11 只替换 V10 初始保护位：多头放在突破棒低点外一 tick，空头完全镜像。
pub const STRICT_VISUAL_BREAKOUT_CANDLE_EXTREME_STOP_STRATEGY_VERSION: &str =
    "strict_visual_consolidation_symmetric_breakout_candle_extreme_stop_15m_research_v1";

/// Candidate V20 上严格视觉横盘做多家族的独立入场时序；不对应新的 Pine 快照。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrictVisualBreakoutResearchVariant {
    /// 原样回放 Candidate V20，不计算或追加严格视觉横盘交易。
    #[default]
    Baseline,
    /// 活动横盘首次放量阳线收盘上破时，沿用主策略下一开盘与 ATR 风险合同做多。
    V1,
    /// 冻结 V1 突破证据，等待后三根内回踩上沿并收盘守稳后再生成做多意图。
    V2RetestAcceptance,
    /// 在 V2 首次确认棒上要求收盘守住冻结突破实体中点；失败后不得等待补开。
    V3BodyMidpointHold,
    /// 保留 V3 入场；冻结横盘不超过 32 根时，既有 Fixed 目标改为初始止损对应的 1R。
    V4ShortRangeOneR,
    /// 直接继承 V3，并要求突破源实体占比至少 60%、方向实体位移至少 25 bps。
    V5BreakoutBodyStrength,
    /// 保留 V5；弱离区等待紧邻下一根完成棒，回区则恢复，否则消费且不补锚点。
    V6WeakDepartureProbation,
    /// 保留 V6；首次合法确认收盘距冻结上沿不足 0.40 个来源 ATR 时消费来源。
    V8AcceptanceMargin40Atr,
    /// 保留 V8；突破收盘还必须越过横盘前自适应窗口内尚未解决的结构高点。
    V9ExternalStructureClearance,
    /// 双向镜像；弱离区观察一棒，强突破后五棒内首次回踩须保留 25% 越界幅度。
    V10SymmetricRetainedBreakout,
    /// 保留 V10 全部信号与目标，只把初始止损冻结在突破棒整根极值外一 tick。
    V11BreakoutCandleExtremeStop,
    /// 保留 V11 结构止损，并在实际开盘时保证最终风险距离至少为一个确认信号 ATR。
    V12ExtremeStopMinOneAtr,
}

impl StrictVisualBreakoutResearchVariant {
    /// 返回稳定 CLI/报告名称，避免用布尔值丢失候选身份。
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::V1 => "v1",
            Self::V2RetestAcceptance => "v2_retest_acceptance",
            Self::V3BodyMidpointHold => "v3_body_midpoint_hold",
            Self::V4ShortRangeOneR => "v4_short_range_32_one_r",
            Self::V5BreakoutBodyStrength => "v5_breakout_body_strength_60pct_25bps",
            Self::V6WeakDepartureProbation => "v6_weak_departure_one_bar_probation",
            Self::V8AcceptanceMargin40Atr => "v8_acceptance_margin_0_40_atr",
            Self::V9ExternalStructureClearance => "v9_external_structure_clearance",
            Self::V10SymmetricRetainedBreakout => "v10_symmetric_retained_breakout_25pct",
            Self::V11BreakoutCandleExtremeStop => "v11_breakout_candle_extreme_stop",
            Self::V12ExtremeStopMinOneAtr => "v12_breakout_candle_extreme_stop_min_1atr",
        }
    }
    /// `true` 只表示启用严格横盘 Research 家族；不会注册生产或模拟运行入口。
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Self::Baseline)
    }
    /// `true` 表示突破棒只冻结来源，必须等待后续完成棒验证价格接受。
    pub const fn requires_retest_acceptance(self) -> bool {
        matches!(
            self,
            Self::V2RetestAcceptance
                | Self::V3BodyMidpointHold
                | Self::V4ShortRangeOneR
                | Self::V5BreakoutBodyStrength
                | Self::V6WeakDepartureProbation
                | Self::V8AcceptanceMargin40Atr
                | Self::V9ExternalStructureClearance
                | Self::V10SymmetricRetainedBreakout
                | Self::V11BreakoutCandleExtremeStop
                | Self::V12ExtremeStopMinOneAtr
        )
    }

    /// `true` 表示 V2 首次可确认棒还必须守住突破实体中点，失败即消费来源。
    pub const fn requires_breakout_body_midpoint_hold(self) -> bool {
        matches!(
            self,
            Self::V3BodyMidpointHold
                | Self::V4ShortRangeOneR
                | Self::V5BreakoutBodyStrength
                | Self::V6WeakDepartureProbation
                | Self::V8AcceptanceMargin40Atr
                | Self::V9ExternalStructureClearance
        )
    }

    /// `true` 表示只启用 V4 的冻结短区间 1R 退出变量，不改变 V3 信号状态机。
    pub const fn uses_short_range_one_r_target(self) -> bool {
        matches!(self, Self::V4ShortRangeOneR)
    }
    /// `true` 表示突破源必须同时通过冻结的 60% 实体占比与 25 bps 方向位移。
    pub const fn requires_breakout_body_strength(self) -> bool {
        matches!(
            self,
            Self::V5BreakoutBodyStrength
                | Self::V6WeakDepartureProbation
                | Self::V8AcceptanceMargin40Atr
                | Self::V9ExternalStructureClearance
                | Self::V10SymmetricRetainedBreakout
                | Self::V11BreakoutCandleExtremeStop
                | Self::V12ExtremeStopMinOneAtr
        )
    }

    /// `true` 表示弱离区只等待紧邻下一根完成棒决定恢复或消费旧横盘。
    pub const fn uses_weak_departure_probation(self) -> bool {
        matches!(
            self,
            Self::V6WeakDepartureProbation
                | Self::V8AcceptanceMargin40Atr
                | Self::V9ExternalStructureClearance
                | Self::V10SymmetricRetainedBreakout
                | Self::V11BreakoutCandleExtremeStop
                | Self::V12ExtremeStopMinOneAtr
        )
    }

    /// `true` 表示在 V6 首次合法确认棒上执行冻结来源 ATR 的 0.40 接受余量门禁。
    pub const fn requires_acceptance_margin_40_atr(self) -> bool {
        matches!(
            self,
            Self::V8AcceptanceMargin40Atr | Self::V9ExternalStructureClearance
        )
    }

    /// `true` 表示 V8 原本确认信号时还要检查突破棒冻结的外部结构上沿。
    pub const fn requires_external_structure_clearance(self) -> bool {
        matches!(self, Self::V9ExternalStructureClearance)
    }

    /// `true` 表示使用本轮双向、五棒、25% 越界保留且无量能门禁的完整新合同。
    pub const fn uses_symmetric_retained_breakout_contract(self) -> bool {
        matches!(
            self,
            Self::V10SymmetricRetainedBreakout
                | Self::V11BreakoutCandleExtremeStop
                | Self::V12ExtremeStopMinOneAtr
        )
    }

    /// `true` 时突破棒完成即冻结整根极值外一 tick，确认等待期间不得重算。
    pub const fn uses_breakout_candle_extreme_stop(self) -> bool {
        matches!(
            self,
            Self::V11BreakoutCandleExtremeStop | Self::V12ExtremeStopMinOneAtr
        )
    }

    /// 候选使用独立策略身份；基线继续返回所选冻结 Pine/Rust 版本。
    pub const fn strategy_version(self, base: ParityRuleVersion) -> &'static str {
        match self {
            Self::Baseline => base.strategy_version(),
            Self::V1 => STRICT_VISUAL_BREAKOUT_STRATEGY_VERSION,
            Self::V2RetestAcceptance => STRICT_VISUAL_BREAKOUT_RETEST_ACCEPTANCE_STRATEGY_VERSION,
            Self::V3BodyMidpointHold => STRICT_VISUAL_BREAKOUT_BODY_MIDPOINT_HOLD_STRATEGY_VERSION,
            Self::V4ShortRangeOneR => STRICT_VISUAL_BREAKOUT_SHORT_RANGE_ONE_R_STRATEGY_VERSION,
            Self::V5BreakoutBodyStrength => STRICT_VISUAL_BREAKOUT_BODY_STRENGTH_STRATEGY_VERSION,
            Self::V6WeakDepartureProbation => {
                STRICT_VISUAL_BREAKOUT_WEAK_DEPARTURE_PROBATION_STRATEGY_VERSION
            }
            Self::V8AcceptanceMargin40Atr => {
                STRICT_VISUAL_BREAKOUT_ACCEPTANCE_MARGIN_STRATEGY_VERSION
            }
            Self::V9ExternalStructureClearance => {
                STRICT_VISUAL_BREAKOUT_EXTERNAL_STRUCTURE_CLEARANCE_STRATEGY_VERSION
            }
            Self::V10SymmetricRetainedBreakout => {
                STRICT_VISUAL_SYMMETRIC_RETAINED_BREAKOUT_STRATEGY_VERSION
            }
            Self::V11BreakoutCandleExtremeStop => {
                STRICT_VISUAL_BREAKOUT_CANDLE_EXTREME_STOP_STRATEGY_VERSION
            }
            Self::V12ExtremeStopMinOneAtr => {
                "strict_visual_consolidation_symmetric_breakout_candle_extreme_stop_min_1atr_15m_research_v1"
            }
        }
    }
}

/// V20 扫高失败家族的单变量锚区或入场时序实验；不对应新的 Pine 快照。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorUpthrustResearchVariant {
    /// 原样回放 V20，在拒绝 K 线收盘确认并于下一根开盘入场。
    #[default]
    Baseline,
    /// 把拒绝棒冻结为 setup，下一根收盘跌破其低点后才确认。
    RightSideConfirmation,
    /// V21 右侧确认仍成立，但确认棒最多消耗原冻结结构奖励的 25%。
    TargetConsumptionCap25,
    /// V21 右侧确认仍成立，但确认棒最多消耗原冻结结构奖励的 33%。
    TargetConsumptionCap33,
    /// V21 右侧确认仍成立，但确认棒最多消耗原冻结结构奖励的 50%。
    TargetConsumptionCap50,
    /// 只接受最近有效横盘的首次收盘上破，拒绝固定窗口新高和已经离开横盘的趋势延续。
    RecentHorizontalFirstBreak,
    /// 保留 V23 锚点，只以第 1～2 根跌回冻结上沿确认失败，不要求再次超过突破棒高点。
    RecentHorizontalFirstBreakCloseBack,
    /// V24 时序保持不变，只接受方向效率不超过 0.30 的 8 根横盘。
    RecentHorizontalDirectionEfficiency30,
    /// V24 时序保持不变，只接受方向效率不超过 0.35 的 8 根横盘。
    RecentHorizontalDirectionEfficiency35,
    /// V24 时序保持不变，只接受方向效率不超过 0.40 的 8 根横盘。
    RecentHorizontalDirectionEfficiency40,
    /// V25B 其余合同不变，8 根改为成形下限，并优先冻结最长有效父横盘。
    ActiveParentHorizontal,
    /// V26 其余合同不变，确认收盘必须不高于已冻结的突破棒开盘价。
    ActiveParentHorizontalBreakoutBodyRejection,
    /// V27 其余合同不变，实体否定深度至少达到父横盘高度的 10%。
    ActiveParentHorizontalNormalizedBodyRejection10Pct,
    /// V27 其余合同不变，突破收盘超出父横盘上沿不得超过区间高度的 10%。
    ActiveParentHorizontalShallowBreakoutExcess10Pct,
    /// V29 其余合同不变，父横盘必须在突破前完成至少三次上下边界切换。
    ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct,
}

impl AnchorUpthrustResearchVariant {
    /// CLI 与报告共用的稳定标识。
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::RightSideConfirmation => "right_side_confirmation",
            Self::TargetConsumptionCap25 => "target_consumption_cap_25",
            Self::TargetConsumptionCap33 => "target_consumption_cap_33",
            Self::TargetConsumptionCap50 => "target_consumption_cap_50",
            Self::RecentHorizontalFirstBreak => "recent_horizontal_first_break",
            Self::RecentHorizontalFirstBreakCloseBack => "recent_horizontal_first_break_close_back",
            Self::RecentHorizontalDirectionEfficiency30 => {
                "recent_horizontal_direction_efficiency_30"
            }
            Self::RecentHorizontalDirectionEfficiency35 => {
                "recent_horizontal_direction_efficiency_35"
            }
            Self::RecentHorizontalDirectionEfficiency40 => {
                "recent_horizontal_direction_efficiency_40"
            }
            Self::ActiveParentHorizontal => "active_parent_horizontal",
            Self::ActiveParentHorizontalBreakoutBodyRejection => {
                "active_parent_horizontal_breakout_body_rejection"
            }
            Self::ActiveParentHorizontalNormalizedBodyRejection10Pct => {
                "active_parent_horizontal_normalized_body_rejection_10pct"
            }
            Self::ActiveParentHorizontalShallowBreakoutExcess10Pct => {
                "active_parent_horizontal_shallow_breakout_excess_10pct"
            }
            Self::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct => {
                "active_parent_horizontal_edge_transitions_3_shallow_breakout_excess_10pct"
            }
        }
    }

    /// 返回独立 Research-only 身份；基线继续绑定冻结 V20。
    pub const fn strategy_version(self, base: ParityRuleVersion) -> &'static str {
        match self {
            Self::Baseline => base.strategy_version(),
            Self::RightSideConfirmation => V21_STRATEGY_VERSION,
            Self::TargetConsumptionCap25 => V22A_STRATEGY_VERSION,
            Self::TargetConsumptionCap33 => V22B_STRATEGY_VERSION,
            Self::TargetConsumptionCap50 => V22C_STRATEGY_VERSION,
            Self::RecentHorizontalFirstBreak => V23_STRATEGY_VERSION,
            Self::RecentHorizontalFirstBreakCloseBack => V24_STRATEGY_VERSION,
            Self::RecentHorizontalDirectionEfficiency30 => V25A_STRATEGY_VERSION,
            Self::RecentHorizontalDirectionEfficiency35 => V25B_STRATEGY_VERSION,
            Self::RecentHorizontalDirectionEfficiency40 => V25C_STRATEGY_VERSION,
            Self::ActiveParentHorizontal => V26_STRATEGY_VERSION,
            Self::ActiveParentHorizontalBreakoutBodyRejection => V27_STRATEGY_VERSION,
            Self::ActiveParentHorizontalNormalizedBodyRejection10Pct => V28_STRATEGY_VERSION,
            Self::ActiveParentHorizontalShallowBreakoutExcess10Pct => V29_STRATEGY_VERSION,
            Self::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct => {
                V30_STRATEGY_VERSION
            }
        }
    }

    /// 是否启用独立于冻结 V20 的 Research-only 行为。
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Self::Baseline)
    }

    /// V21/V22 才等待紧邻下一根完成棒；V23～V30 保留 V20 的第 1～2 根即时确认。
    pub const fn requires_right_side_confirmation(self) -> bool {
        matches!(
            self,
            Self::RightSideConfirmation
                | Self::TargetConsumptionCap25
                | Self::TargetConsumptionCap33
                | Self::TargetConsumptionCap50
        )
    }

    /// V23～V30 使用最近横盘首次突破的独立 pending，不能改写旧假突破家族状态。
    pub const fn uses_recent_horizontal_first_break_anchor(self) -> bool {
        matches!(
            self,
            Self::RecentHorizontalFirstBreak
                | Self::RecentHorizontalFirstBreakCloseBack
                | Self::RecentHorizontalDirectionEfficiency30
                | Self::RecentHorizontalDirectionEfficiency35
                | Self::RecentHorizontalDirectionEfficiency40
                | Self::ActiveParentHorizontal
                | Self::ActiveParentHorizontalBreakoutBodyRejection
                | Self::ActiveParentHorizontalNormalizedBodyRejection10Pct
                | Self::ActiveParentHorizontalShallowBreakoutExcess10Pct
                | Self::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct
        )
    }

    /// V26～V30 启用最长有效父横盘；旧版本继续使用冻结的固定 8 根选择器。
    pub const fn uses_active_parent_horizontal_anchor(self) -> bool {
        matches!(
            self,
            Self::ActiveParentHorizontal
                | Self::ActiveParentHorizontalBreakoutBodyRejection
                | Self::ActiveParentHorizontalNormalizedBodyRejection10Pct
                | Self::ActiveParentHorizontalShallowBreakoutExcess10Pct
                | Self::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct
        )
    }

    /// 冻结版本都保留扫过突破高点；V24～V30 删除该门槛，避免静默改写 V23 结果。
    pub const fn requires_breakout_high_sweep(self) -> bool {
        !matches!(
            self,
            Self::RecentHorizontalFirstBreakCloseBack
                | Self::RecentHorizontalDirectionEfficiency30
                | Self::RecentHorizontalDirectionEfficiency35
                | Self::RecentHorizontalDirectionEfficiency40
                | Self::ActiveParentHorizontal
                | Self::ActiveParentHorizontalBreakoutBodyRejection
                | Self::ActiveParentHorizontalNormalizedBodyRejection10Pct
                | Self::ActiveParentHorizontalShallowBreakoutExcess10Pct
                | Self::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct
        )
    }

    /// 返回 V25～V30 沿用的方向效率上限；更早版本返回 `None` 以保持历史结果。
    pub const fn horizontal_direction_efficiency_max(self) -> Option<f64> {
        match self {
            Self::RecentHorizontalDirectionEfficiency30 => Some(0.30),
            Self::RecentHorizontalDirectionEfficiency35 => Some(0.35),
            Self::RecentHorizontalDirectionEfficiency40 => Some(0.40),
            Self::ActiveParentHorizontal => Some(0.35),
            Self::ActiveParentHorizontalBreakoutBodyRejection => Some(0.35),
            Self::ActiveParentHorizontalNormalizedBodyRejection10Pct => Some(0.35),
            Self::ActiveParentHorizontalShallowBreakoutExcess10Pct => Some(0.35),
            Self::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct => Some(0.35),
            _ => None,
        }
    }

    /// V27～V30 要求确认收盘完全否定突破棒实体，V26 原候选身份保持不变。
    pub const fn requires_breakout_body_rejection(self) -> bool {
        matches!(
            self,
            Self::ActiveParentHorizontalBreakoutBodyRejection
                | Self::ActiveParentHorizontalNormalizedBodyRejection10Pct
                | Self::ActiveParentHorizontalShallowBreakoutExcess10Pct
                | Self::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct
        )
    }

    /// V28 的唯一新增门禁；`None` 表示不限制父横盘归一化实体否定深度。
    pub const fn normalized_breakout_body_rejection_min(self) -> Option<f64> {
        match self {
            Self::ActiveParentHorizontalNormalizedBodyRejection10Pct => Some(0.10),
            _ => None,
        }
    }

    /// V29 的唯一新增门禁；`None` 表示不限制突破收盘相对父横盘高度的超幅。
    pub const fn normalized_breakout_excess_max(self) -> Option<f64> {
        match self {
            Self::ActiveParentHorizontalShallowBreakoutExcess10Pct
            | Self::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct => Some(0.10),
            _ => None,
        }
    }

    /// V30 的唯一新增门禁；`None` 表示不限制父横盘的上下边界交替次数。
    pub const fn minimum_horizontal_edge_transitions(self) -> Option<usize> {
        match self {
            Self::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct => Some(3),
            _ => None,
        }
    }

    /// 返回确认棒最多可消耗的冻结结构奖励比例；`None` 表示 V21 不追加该门禁。
    pub const fn target_consumption_cap(self) -> Option<f64> {
        match self {
            Self::Baseline
            | Self::RightSideConfirmation
            | Self::RecentHorizontalFirstBreak
            | Self::RecentHorizontalFirstBreakCloseBack
            | Self::RecentHorizontalDirectionEfficiency30
            | Self::RecentHorizontalDirectionEfficiency35
            | Self::RecentHorizontalDirectionEfficiency40
            | Self::ActiveParentHorizontal
            | Self::ActiveParentHorizontalBreakoutBodyRejection
            | Self::ActiveParentHorizontalNormalizedBodyRejection10Pct
            | Self::ActiveParentHorizontalShallowBreakoutExcess10Pct
            | Self::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct => None,
            Self::TargetConsumptionCap25 => Some(0.25),
            Self::TargetConsumptionCap33 => Some(0.33),
            Self::TargetConsumptionCap50 => Some(0.50),
        }
    }
}

/// V19 的 EMA 空头趋势单变量消融；仅改变该家族，未对应新的 Pine 快照。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmaShortResearchVariant {
    /// 原样回放 V19，作为所有单变量的同源基线。
    #[default]
    Baseline,
    /// 要求四条 EMA 同步下斜且三个空头间距扩张。
    SlopeSpread,
    /// 要求信号收盘跌破前 20 根已完成 K 线的最低价。
    StructureBreak,
    /// 要求信号收盘至少向下突破前 20 根低点 0.10 ATR。
    StructureBreakDepth10,
    /// 要求信号收盘至少向下突破前 20 根低点 0.20 ATR。
    StructureBreakDepth20,
    /// 在严格跌破前 20 根低点之外，要求实际 EMA676 相对 20 根前下降。
    StructureBreakEma676Falling20,
    /// 冻结来源棒，等待后续 1～3 根对 EMA12 的回抽失败。
    RightSideRetest,
    /// 拒绝收盘距离 EMA12 超过 0.8 ATR 的延伸追空。
    DistanceGuard,
    /// 量比达到 10 倍时必须同时获得向下结构接受。
    ExtremeVolumeAcceptance,
}

impl EmaShortResearchVariant {
    /// CLI 与报告共用的稳定标识，避免不同消融结果写成同一策略版本。
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::SlopeSpread => "slope_spread",
            Self::StructureBreak => "structure_break",
            Self::StructureBreakDepth10 => "structure_break_depth_0_10_atr",
            Self::StructureBreakDepth20 => "structure_break_depth_0_20_atr",
            Self::StructureBreakEma676Falling20 => "structure_break_ema676_falling_20",
            Self::RightSideRetest => "right_side_retest",
            Self::DistanceGuard => "distance_guard",
            Self::ExtremeVolumeAcceptance => "extreme_volume_acceptance",
        }
    }

    /// 返回 Research-only 策略身份；基线继续使用冻结 V19 身份。
    pub const fn strategy_version(self, base: ParityRuleVersion) -> &'static str {
        match self {
            Self::Baseline => base.strategy_version(),
            Self::SlopeSpread => "tradingview_velocity_v19_ema_short_slope_spread_ablation_v1",
            Self::StructureBreak => {
                "tradingview_velocity_v19_ema_short_structure_break_ablation_v1"
            }
            Self::StructureBreakDepth10 => {
                "tradingview_velocity_v19_ema_short_structure_break_depth_0_10_atr_v2"
            }
            Self::StructureBreakDepth20 => {
                "tradingview_velocity_v19_ema_short_structure_break_depth_0_20_atr_v2"
            }
            Self::StructureBreakEma676Falling20 => {
                "tradingview_velocity_v19_ema_short_structure_break_ema676_falling_20_v3"
            }
            Self::RightSideRetest => {
                "tradingview_velocity_v19_ema_short_right_side_retest_ablation_v1"
            }
            Self::DistanceGuard => "tradingview_velocity_v19_ema_short_distance_guard_ablation_v1",
            Self::ExtremeVolumeAcceptance => {
                "tradingview_velocity_v19_ema_short_extreme_volume_acceptance_ablation_v1"
            }
        }
    }
}

/// V19 EMA 趋势多的逐层门槛实验；只新增保守补充来源，不改变其他信号家族。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmaTrendLongResearchVariant {
    /// 原样回放 V19，作为逐层补齐目标 K 线的同源基线。
    #[default]
    Baseline,
    /// 补充来源只把周内量能门槛从 nearest-rank P90 调整为 P80。
    WeeklyVolumeP80,
    /// 在 P80 基础上，把最低 2.7 ATR 目标所需量比从 3.0 调整为 2.5。
    WeeklyP80TakeProfitFloor25,
    /// 再把补充来源的实体/开盘价下限从 1% 调整为 0.3%。
    WeeklyP80TakeProfitFloor25Body003,
    /// 在 0.3% 实体基线上，要求收盘至少越过冻结前高 0.20 ATR。
    WeeklyP80TakeProfitFloor25Body003BreakDepth20,
    /// 在 0.3% 实体基线上，要求收盘至少越过冻结前高 0.30 ATR。
    WeeklyP80TakeProfitFloor25Body003BreakDepth30,
    /// 冻结 0.30 ATR 与 1.25 ATR 距离，只要求补充来源的回踩接受 K 收阳。
    WeeklyP80TakeProfitFloor25Body003BreakDepth30BullishAcceptance,
    /// 冻结 0.30 ATR 突破深度，只把补充来源距 EMA12 上限调整为 1.35 ATR。
    WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance135,
    /// 冻结 0.30 ATR 突破深度，只把补充来源距 EMA12 上限调整为 1.50 ATR。
    WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance150,
    /// 在 0.3% 实体基线上，要求收盘至少越过冻结前高 0.40 ATR。
    WeeklyP80TakeProfitFloor25Body003BreakDepth40,
    /// 最后把补充来源距 EMA12 上限从 1.25 ATR 调整为 1.50 ATR。
    WeeklyP80TakeProfitFloor25Body003Distance15,
    /// 只补四项均落在原门槛边缘的保守样本，避免把任一宽松条件扩散到所有趋势多。
    ConservativeTargetGap,
}

impl EmaTrendLongResearchVariant {
    /// CLI 与报告共用的稳定标识，名称直接表达逐层累计的门槛。
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::WeeklyVolumeP80 => "weekly_volume_p80",
            Self::WeeklyP80TakeProfitFloor25 => "weekly_p80_take_profit_floor_2_5",
            Self::WeeklyP80TakeProfitFloor25Body003 => {
                "weekly_p80_take_profit_floor_2_5_body_0_003"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth20 => {
                "weekly_p80_take_profit_floor_2_5_body_0_003_break_depth_0_2_atr"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30 => {
                "weekly_p80_take_profit_floor_2_5_body_0_003_break_depth_0_3_atr"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30BullishAcceptance => {
                "weekly_p80_take_profit_floor_2_5_body_0_003_break_depth_0_3_atr_bullish_acceptance"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance135 => {
                "weekly_p80_take_profit_floor_2_5_body_0_003_break_depth_0_3_atr_distance_1_35_atr"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance150 => {
                "weekly_p80_take_profit_floor_2_5_body_0_003_break_depth_0_3_atr_distance_1_5_atr"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth40 => {
                "weekly_p80_take_profit_floor_2_5_body_0_003_break_depth_0_4_atr"
            }
            Self::WeeklyP80TakeProfitFloor25Body003Distance15 => {
                "weekly_p80_take_profit_floor_2_5_body_0_003_distance_1_5_atr"
            }
            Self::ConservativeTargetGap => "conservative_target_gap",
        }
    }

    /// 返回 Research-only 身份；基线继续绑定冻结 V19，补充路径不进入 Paper 或 Live。
    pub const fn strategy_version(self, base: ParityRuleVersion) -> &'static str {
        match self {
            Self::Baseline => base.strategy_version(),
            Self::WeeklyVolumeP80 => "tradingview_velocity_v19_ema_long_weekly_p80_ladder_step_1",
            Self::WeeklyP80TakeProfitFloor25 => {
                "tradingview_velocity_v19_ema_long_weekly_p80_tp25_ladder_step_2"
            }
            Self::WeeklyP80TakeProfitFloor25Body003 => {
                "tradingview_velocity_v19_ema_long_weekly_p80_tp25_body003_ladder_step_3"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth20 => {
                "tradingview_velocity_v19_ema_long_body003_break_depth_0_20_atr_v1"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30 => {
                "tradingview_velocity_v19_ema_long_body003_break_depth_0_30_atr_v1"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30BullishAcceptance => {
                "tradingview_velocity_v19_ema_long_body003_break_depth_0_30_atr_bullish_acceptance_v1"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance135 => {
                "tradingview_velocity_v19_ema_long_body003_break_depth_0_30_atr_distance_1_35_atr_v1"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance150 => {
                "tradingview_velocity_v19_ema_long_body003_break_depth_0_30_atr_distance_1_50_atr_v1"
            }
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth40 => {
                "tradingview_velocity_v19_ema_long_body003_break_depth_0_40_atr_v1"
            }
            Self::WeeklyP80TakeProfitFloor25Body003Distance15 => {
                "tradingview_velocity_v19_ema_long_weekly_p80_tp25_body003_distance15_ladder_step_4"
            }
            Self::ConservativeTargetGap => {
                "tradingview_velocity_v19_ema_long_conservative_target_gap_v1"
            }
        }
    }

    /// 补充路径是否已经启用；基线不得产生任何额外来源棒。
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Self::Baseline)
    }

    /// 补充路径是否把最低 ATR 目标量比降到 2.5。
    pub const fn uses_take_profit_floor_25(self) -> bool {
        matches!(
            self,
            Self::WeeklyP80TakeProfitFloor25
                | Self::WeeklyP80TakeProfitFloor25Body003
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth20
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30BullishAcceptance
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance135
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance150
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth40
                | Self::WeeklyP80TakeProfitFloor25Body003Distance15
                | Self::ConservativeTargetGap
        )
    }

    /// 补充路径是否把实体/开盘价下限降到 0.3%。
    pub const fn uses_body_open_min_003(self) -> bool {
        matches!(
            self,
            Self::WeeklyP80TakeProfitFloor25Body003
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth20
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30BullishAcceptance
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance135
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance150
                | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth40
                | Self::WeeklyP80TakeProfitFloor25Body003Distance15
                | Self::ConservativeTargetGap
        )
    }

    /// 补充来源允许偏离 EMA12 的最大 ATR 倍数。
    pub const fn source_distance_atr_max(self) -> f64 {
        match self {
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance135 => 1.35,
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance150 => 1.50,
            Self::WeeklyP80TakeProfitFloor25Body003Distance15 | Self::ConservativeTargetGap => 1.50,
            _ => 1.25,
        }
    }

    /// 返回补充来源越过冻结前高的最小 ATR 深度；其他版本保持原始 `> 0` 合同。
    pub const fn source_break_depth_atr_min(self) -> f64 {
        match self {
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth20 => 0.20,
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30
            | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30BullishAcceptance
            | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance135
            | Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance150 => 0.30,
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth40 => 0.40,
            _ => 0.0,
        }
    }

    /// 保守补丁只接受同时位于四项原始门槛缺口中的样本。
    pub const fn requires_all_target_gaps(self) -> bool {
        matches!(self, Self::ConservativeTargetGap)
    }

    /// 补充来源是否必须等待阳线回踩确认；原 V19 来源不受该 Research 门禁影响。
    pub const fn requires_bullish_retest(self) -> bool {
        matches!(
            self,
            Self::WeeklyP80TakeProfitFloor25Body003BreakDepth30BullishAcceptance
        )
    }
}

/// 放量杀跌后低位拒绝与强阳收回的独立 Research-only 开关。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SellClimaxBaseReclaimResearchVariant {
    /// 不新增信号，保持 V19 Conservative Target Gap 基线。
    #[default]
    Baseline,
    /// 启用冻结的 `sell climax -> base -> reclaim` V1 多头形态。
    V1,
}

impl SellClimaxBaseReclaimResearchVariant {
    /// CLI 与报告共用的稳定标识。
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::V1 => "v1",
        }
    }

    /// 返回独立研究身份；该版本不会注册到 Paper 或 Live。
    pub const fn strategy_version(self, base: ParityRuleVersion) -> &'static str {
        match self {
            Self::Baseline => base.strategy_version(),
            Self::V1 => {
                "tradingview_velocity_v19_conservative_target_gap_plus_volume_sell_climax_base_reclaim_long_15m_v1"
            }
        }
    }

    /// 只有 V1 才允许组合器追加该信号家族。
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::V1)
    }
}

/// 选择本次回放使用的冻结规则集；版本只影响策略规则，不改变行情或成交成本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParityRuleVersion {
    Frozen66d3937e,
    Current3cbbc9d8,
    CandidateV3,
    CandidateV4,
    CandidateV5,
    CandidateV6,
    CandidateV7,
    CandidateV8,
    CandidateV9,
    CandidateV10,
    CandidateV11,
    CandidateV12,
    CandidateV13,
    CandidateV14,
    CandidateV15,
    CandidateV16,
    CandidateV17,
    CandidateV18,
    CandidateV19,
    CandidateV20,
}

impl ParityRuleVersion {
    /// 返回可审计的 Rust Research 策略身份。
    pub const fn strategy_version(self) -> &'static str {
        match self {
            Self::Frozen66d3937e => STRATEGY_VERSION,
            Self::Current3cbbc9d8 => V2_STRATEGY_VERSION,
            Self::CandidateV3 => CURRENT_STRATEGY_VERSION,
            Self::CandidateV4 => V4_STRATEGY_VERSION,
            Self::CandidateV5 => V5_STRATEGY_VERSION,
            Self::CandidateV6 => V6_STRATEGY_VERSION,
            Self::CandidateV7 => V7_STRATEGY_VERSION,
            Self::CandidateV8 => V8_STRATEGY_VERSION,
            Self::CandidateV9 => V9_STRATEGY_VERSION,
            Self::CandidateV10 => V10_STRATEGY_VERSION,
            Self::CandidateV11 => V11_STRATEGY_VERSION,
            Self::CandidateV12 => V12_STRATEGY_VERSION,
            Self::CandidateV13 => V13_STRATEGY_VERSION,
            Self::CandidateV14 => V14_STRATEGY_VERSION,
            Self::CandidateV15 => V15_STRATEGY_VERSION,
            Self::CandidateV16 => V16_STRATEGY_VERSION,
            Self::CandidateV17 => V17_STRATEGY_VERSION,
            Self::CandidateV18 => V18_STRATEGY_VERSION,
            Self::CandidateV19 => V19_STRATEGY_VERSION,
            Self::CandidateV20 => V20_STRATEGY_VERSION,
        }
    }

    /// 返回该规则集唯一对应的 Pine 源码身份。
    pub const fn pine_source_fnv1a32(self) -> &'static str {
        match self {
            Self::Frozen66d3937e => PINE_SOURCE_FNV1A32,
            Self::Current3cbbc9d8 => V2_PINE_SOURCE_FNV1A32,
            Self::CandidateV3 => CURRENT_PINE_SOURCE_FNV1A32,
            Self::CandidateV4 => V4_PINE_SOURCE_FNV1A32,
            Self::CandidateV5 => V5_PINE_SOURCE_FNV1A32,
            Self::CandidateV6 => V6_PINE_SOURCE_FNV1A32,
            Self::CandidateV7 => V7_PINE_SOURCE_FNV1A32,
            Self::CandidateV8 => V8_PINE_SOURCE_FNV1A32,
            Self::CandidateV9 => V9_PINE_SOURCE_FNV1A32,
            Self::CandidateV10 => V10_PINE_SOURCE_FNV1A32,
            Self::CandidateV11 => V11_PINE_SOURCE_FNV1A32,
            Self::CandidateV12 => V12_PINE_SOURCE_FNV1A32,
            Self::CandidateV13 => V13_PINE_SOURCE_FNV1A32,
            Self::CandidateV14 => V14_PINE_SOURCE_FNV1A32,
            Self::CandidateV15 => V15_PINE_SOURCE_FNV1A32,
            Self::CandidateV16 => V16_PINE_SOURCE_FNV1A32,
            Self::CandidateV17 => V17_PINE_SOURCE_FNV1A32,
            Self::CandidateV18 => V18_PINE_SOURCE_FNV1A32,
            Self::CandidateV19 => V19_PINE_SOURCE_FNV1A32,
            Self::CandidateV20 => V20_PINE_SOURCE_FNV1A32,
        }
    }

    /// V2 新增的独立信号家族在 V3 中继续存在，避免候选版意外退回 V1 规则。
    pub const fn includes_v2_additions(self) -> bool {
        matches!(
            self,
            Self::Current3cbbc9d8
                | Self::CandidateV3
                | Self::CandidateV4
                | Self::CandidateV5
                | Self::CandidateV6
                | Self::CandidateV7
                | Self::CandidateV8
                | Self::CandidateV9
                | Self::CandidateV10
                | Self::CandidateV11
                | Self::CandidateV12
                | Self::CandidateV13
                | Self::CandidateV14
                | Self::CandidateV18
                | Self::CandidateV19
                | Self::CandidateV20
        )
    }

    /// V3 及后续候选继承结构与交叉保护，不污染冻结的 V1/V2。
    pub const fn includes_v3_guards(self) -> bool {
        matches!(
            self,
            Self::CandidateV3
                | Self::CandidateV4
                | Self::CandidateV5
                | Self::CandidateV6
                | Self::CandidateV7
                | Self::CandidateV8
                | Self::CandidateV9
                | Self::CandidateV10
                | Self::CandidateV11
                | Self::CandidateV12
                | Self::CandidateV13
                | Self::CandidateV14
                | Self::CandidateV18
                | Self::CandidateV19
                | Self::CandidateV20
        )
    }

    /// V4 的逆势移动保护、破位目标分层和背离形态门禁不回写历史版本。
    pub const fn includes_v4_guards(self) -> bool {
        matches!(
            self,
            Self::CandidateV4
                | Self::CandidateV5
                | Self::CandidateV6
                | Self::CandidateV7
                | Self::CandidateV8
                | Self::CandidateV9
                | Self::CandidateV10
                | Self::CandidateV11
                | Self::CandidateV12
                | Self::CandidateV13
                | Self::CandidateV14
                | Self::CandidateV18
                | Self::CandidateV19
                | Self::CandidateV20
        )
    }

    /// V5 只把纯 RSI 严格逆势仓切到年龄化结构退出，不改写历史候选。
    pub const fn includes_v5_guards(self) -> bool {
        matches!(
            self,
            Self::CandidateV5
                | Self::CandidateV6
                | Self::CandidateV7
                | Self::CandidateV8
                | Self::CandidateV9
                | Self::CandidateV10
                | Self::CandidateV11
                | Self::CandidateV12
                | Self::CandidateV13
                | Self::CandidateV14
                | Self::CandidateV18
                | Self::CandidateV19
                | Self::CandidateV20
        )
    }

    /// V6 只替换 EMA 趋势多的单棒追入，其他家族继续继承 V5。
    pub const fn includes_v6_guards(self) -> bool {
        matches!(self, Self::CandidateV6)
    }

    /// V7 只在 RSI 形态与背离家族中拒绝方向相反的既有长影线。
    pub const fn includes_v7_guards(self) -> bool {
        matches!(self, Self::CandidateV7)
    }

    /// V8 及其参数同步版 V9 直接继承 V5，并保留慢均线带收复后的空单门禁。
    pub const fn includes_v8_guards(self) -> bool {
        matches!(
            self,
            Self::CandidateV8
                | Self::CandidateV9
                | Self::CandidateV10
                | Self::CandidateV11
                | Self::CandidateV12
                | Self::CandidateV13
                | Self::CandidateV14
                | Self::CandidateV18
                | Self::CandidateV19
                | Self::CandidateV20
        )
    }

    /// V10 只在 V9 参数基线上启用五类预注册入场质量门禁。
    pub const fn includes_v10_guards(self) -> bool {
        matches!(
            self,
            Self::CandidateV10
                | Self::CandidateV11
                | Self::CandidateV12
                | Self::CandidateV13
                | Self::CandidateV14
                | Self::CandidateV18
                | Self::CandidateV19
                | Self::CandidateV20
        )
    }

    /// V11 只在 V10 通过的结构门禁上追加残差确认，不改写冻结 V10。
    pub const fn includes_v11_guards(self) -> bool {
        matches!(
            self,
            Self::CandidateV11
                | Self::CandidateV13
                | Self::CandidateV14
                | Self::CandidateV18
                | Self::CandidateV19
                | Self::CandidateV20
        )
    }

    /// V12 只替换五类过度耦合入场的确认时序，不改写 V10/V11 冻结结果。
    pub const fn includes_v12_guards(self) -> bool {
        matches!(self, Self::CandidateV12)
    }

    /// V13 仅为压缩扩张启用分阶段接受；其他家族通过 V11 门禁保持冻结。
    pub const fn includes_v13_guards(self) -> bool {
        matches!(self, Self::CandidateV13)
    }

    /// V14 只替换 EMA 压缩 setup 的方向归属；其他家族通过 V11 门禁保持冻结。
    pub const fn includes_v14_guards(self) -> bool {
        matches!(self, Self::CandidateV14)
    }

    /// V15 是独立策略家族，不能误继承 V11 的组合信号或 V14 的 EMA setup。
    pub const fn includes_v15_range_squeeze(self) -> bool {
        matches!(
            self,
            Self::CandidateV15
                | Self::CandidateV16
                | Self::CandidateV17
                | Self::CandidateV18
                | Self::CandidateV19
                | Self::CandidateV20
        )
    }

    /// V16/V17 都把 V15 接受棒改为有限窗口右侧 stop entry。
    pub const fn uses_right_side_trigger(self) -> bool {
        matches!(
            self,
            Self::CandidateV16
                | Self::CandidateV17
                | Self::CandidateV18
                | Self::CandidateV19
                | Self::CandidateV20
        )
    }

    /// 只有 V16 在 stop entry 上叠加最小风险、触发后 RR 与成本占 R 门禁。
    pub const fn uses_v16_economic_gates(self) -> bool {
        matches!(self, Self::CandidateV16)
    }

    /// V18 同时运行 V11 主策略与 V17 补充状态机，并固定 V11 同棒优先。
    pub const fn is_v18_composite(self) -> bool {
        matches!(self, Self::CandidateV18)
    }

    /// V19/V20 沿用 V18 组合次序；V20 在同一 V11 主分支增加独立早期信号。
    pub const fn is_v19_composite(self) -> bool {
        matches!(self, Self::CandidateV19 | Self::CandidateV20)
    }

    /// V19 起启用长下影门禁，V20 必须继续继承而不能回退。
    pub const fn rejects_false_breakout_short_on_long_lower_wick(self) -> bool {
        matches!(self, Self::CandidateV19 | Self::CandidateV20)
    }

    /// V20 只在冻结放量突破后的前两根完成棒识别扫高失败接受。
    pub const fn enables_upthrust_failed_acceptance(self) -> bool {
        matches!(self, Self::CandidateV20)
    }

    /// 保留旧调用方名称；其语义固定为“包含 V2 新增家族”，不再表示最新版本。
    pub const fn includes_current_additions(self) -> bool {
        self.includes_v2_additions()
    }
}

/// 一根按时间升序排列、且只包含已确认数据的 15 分钟 K 线。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Candle {
    pub timestamp_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    /// 与当前 Pine 约定一致：直接使用 OKX `vol_ccy`，不再乘以收盘价。
    pub volume: f64,
}

impl Candle {
    /// 拒绝不能安全参与指标与形态计算的 OHLCV。
    pub fn is_valid(self) -> bool {
        self.open > 0.0
            && self.high >= self.open.max(self.close)
            && self.low <= self.open.min(self.close)
            // Pine 会保留零振幅已完成 K 线；依赖振幅的形态门禁自行失败，
            // 但 EMA、RSI、ATR 与时间连续性不能因此丢掉这一根。
            && self.high >= self.low
            && self.volume >= 0.0
    }

    /// 返回实体绝对长度。
    pub fn body(self) -> f64 {
        (self.close - self.open).abs()
    }

    /// 返回整根 K 线的价格范围。
    pub fn range(self) -> f64 {
        self.high - self.low
    }
}

/// Pine 指标序列在一根已完成 K 线上的快照。
#[derive(Debug, Clone, Default)]
pub struct IndicatorPoint {
    pub filtered_volume_ratio: Option<f64>,
    pub volume_event: bool,
    /// 仅供 EMA 趋势多 Research 梯度使用；冻结 V19 的 `volume_event` 仍严格使用 P90。
    pub weekly_volume_p80: Option<f64>,
    pub weekly_volume_p90: Option<f64>,
    pub weekly_volume_ready: bool,
    pub rsi14: Option<f64>,
    pub ema12: Option<f64>,
    pub ema144: Option<f64>,
    pub ema596: Option<f64>,
    pub ema696: Option<f64>,
    pub atr14: Option<f64>,
    pub bollinger_middle: Option<f64>,
    pub bollinger_upper: Option<f64>,
    pub bollinger_lower: Option<f64>,
    pub macd_histogram: Option<f64>,
}

/// 完整指标序列；索引与输入 K 线严格一一对应。
#[derive(Debug, Clone)]
pub struct IndicatorSeries {
    pub points: Vec<IndicatorPoint>,
}

impl IndicatorSeries {
    /// 读取指定索引，越界时返回 `None`，避免把缺失历史当成零值。
    pub fn get(&self, index: usize) -> Option<&IndicatorPoint> {
        self.points.get(index)
    }
}

/// 交易方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Long,
    Short,
}

impl Direction {
    /// 固定 1 单位仓位下，把价格差转换为带方向的毛收益。
    pub fn gross_pnl(self, entry: f64, exit: f64) -> f64 {
        match self {
            Self::Long => exit - entry,
            Self::Short => entry - exit,
        }
    }
}

/// 当前 Pine 中可独立触发入场的信号家族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalFamily {
    RsiBullishDivergence,
    RsiBearishDivergence,
    RsiOversoldPattern,
    RsiOverboughtPattern,
    EmaTrendLong,
    EmaTrendShort,
    ConfirmedRangeAcceptanceLong,
    LargeHorizontalRangeBreakLong,
    StrictVisualConsolidationBreakLong,
    StrictVisualConsolidationBreakShort,
    LargeAscendingTriangleBreakLong,
    AnchorFalseBreakShort,
    AnchorUpthrustFailedAcceptanceShort,
    AnchorUpthrustFailedAcceptanceRightSideShort,
    TransitionLiquiditySweepShort,
    EmaCompressionExpansionLong,
    EmaCompressionExpansionShort,
    ThreeBarBullishEngulfingLong,
    EffortNoResultShort,
    BollingerLowerReclaimLong,
    Ema596ReclaimDepartureLong,
    RangeSqueezeBreakAcceptanceLong,
    RangeSqueezeBreakAcceptanceShort,
    RangeSqueezeRightSideTriggerLong,
    RangeSqueezeRightSideTriggerShort,
    RangeSqueezeRightSideTriggerAblationLong,
    RangeSqueezeRightSideTriggerAblationShort,
    SellClimaxBaseReclaimLong,
}

/// 一笔信号在下一根开盘执行前冻结的退出政策。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum ExitPolicy {
    /// 固定止损与固定 ATR/tick 目标。
    Fixed,
    /// 逆势交易只使用信号时已确认的横盘结构目标。
    CounterTrendStructure,
    /// V4 逆势结构交易保留冻结目标，并在实际成交后以初始风险启动单向移动保护。
    CounterTrendStructureV4,
    /// V5 达到 600 根的纯 RSI 严格逆势交易使用结构确认净保本与 2R 宽追踪。
    RsiCounterTrendAgeV5,
    /// RSI 背离在 1R 后于收盘更新近似保本，最终 1.5R。
    DivergenceRegime,
    /// 三棒反包在 1R 后于收盘更新近似保本，最终 1.5R。
    ThreeBarEngulfing,
    /// 空头延续达到原目标后，于收盘更新近似保本，最终 8 ATR。
    ShortTrendExtension,
    /// 高位放量努力无结果在 1R 后于收盘更新近似保本，最终 1.5R。
    EffortNoResult,
    /// 下破布林下轨后收回，使用冻结中轨作为绝对结构目标。
    BollingerLowerReclaim,
    /// EMA596 收复离轨，使用信号时冻结的结构止损与 2R 绝对目标。
    Ema596ReclaimDeparture,
    /// V15 的 1R 部分止盈、完成棒净保本、2R ATR 追踪与箱体回收退出。
    RangeSqueezeStaged,
}

/// V26～V30 在首次突破时冻结并透传到候选账本的父横盘证据。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct HorizontalAnchorEvidence {
    /// 父横盘首根完成 K 线时间，Unix 毫秒时间戳。
    pub start_time_ms: i64,
    /// 父横盘末根完成 K 线时间，Unix 毫秒时间戳。
    pub end_time_ms: i64,
    /// 父横盘包含的 15 分钟完成 K 线数量。
    pub length_bars: usize,
    /// 突破前冻结的稳健 P90 上沿，价格单位与交易对一致。
    pub upper: f64,
    /// 突破前冻结的稳健 P10 下沿，价格单位与交易对一致。
    pub lower: f64,
    /// 完整父横盘的收盘方向效率，范围为 0～1。
    pub direction_efficiency: f64,
    /// 首次放量收盘突破棒时间，Unix 毫秒时间戳。
    pub breakout_time_ms: i64,
    /// 首次突破棒完成时的收盘价。
    pub breakout_close: f64,
    /// 突破收盘高于冻结上沿的 tick 数，只使用突破时已知价格计算。
    pub breakout_excess_ticks: f64,
    /// V27～V30 冻结的突破棒开盘价；V26 为 `None`，避免改写旧版本账本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breakout_open: Option<f64>,
    /// V27～V30 第 1～2 根确认棒的完成收盘价；信号未形成时不会进入候选账本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_close: Option<f64>,
    /// V27～V30 确认收盘低于突破棒开盘价的 tick 数；零表示恰好完全吞回实体。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breakout_body_rejection_depth_ticks: Option<f64>,
    /// V28 实体否定深度除以父横盘高度的比例；V26/V27 为 `None`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_breakout_body_rejection_depth: Option<f64>,
    /// V29 突破收盘超出上沿的价格除以父横盘高度；V26～V28 为 `None`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_breakout_excess: Option<f64>,
    /// V30 父横盘在突破前完成的上下边界切换次数；旧版本为 `None`，保持历史报告形状。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_transition_count: Option<usize>,
}

/// 信号棒收盘后冻结、等待下一根开盘成交的订单意图。
#[derive(Debug, Clone, Serialize)]
pub struct EntryIntent {
    pub signal_index: usize,
    pub signal_time_ms: i64,
    pub direction: Direction,
    pub families: Vec<SignalFamily>,
    pub signal_close: f64,
    pub signal_atr: f64,
    /// 形态止损使用绝对价；普通分支在成交后按 `stop_ticks` 从成交价推导。
    pub stop_price: Option<f64>,
    pub stop_ticks: Option<i64>,
    pub target_price: Option<f64>,
    /// 相对入场价的目标 tick；结构目标使用 `target_price`。
    pub target_ticks: Option<i64>,
    pub activation_ticks: Option<i64>,
    pub exit_policy: ExitPolicy,
    pub counter_trend: bool,
    /// V5 纯 RSI 严格逆势信号的连续 EMA12/144/696 排列年龄，最大 600；其他分支为 `None`。
    pub signal_counter_trend_ema_age_bars_capped_600: Option<usize>,
    /// V5 在信号收盘冻结的横盘近边；非纯 RSI 严格逆势分支为 `None`。
    pub counter_trend_structure_breakout_line: Option<f64>,
    /// V21/V22 确认棒已消耗的冻结结构奖励比例；其他信号家族为 `None`。
    pub anchor_upthrust_target_consumption_ratio: Option<f64>,
    /// V26～V30 的父横盘与首次突破证据；`None` 表示其他研究分支。
    pub active_parent_horizontal_anchor: Option<HorizontalAnchorEvidence>,
    /// 严格视觉横盘突破源棒冻结的区间长度；其他信号家族为 `None`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict_visual_range_length_bars: Option<usize>,
    /// 严格视觉横盘突破在信号时冻结的上下沿价差；退出研究不得用后续 K 线重算。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict_visual_range_height: Option<f64>,
    /// V4 在突破源棒冻结的短区间 1R 分支；V1～V3 为 `Some(false)`，其他家族为 `None`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict_visual_short_range_one_r_target: Option<bool>,
    /// `true` 表示该严格视觉候选必须使用突破棒极值止损，并在下一开盘失效时拒绝入场。
    pub strict_visual_breakout_candle_extreme_stop: bool,
    pub volume_ratio: Option<f64>,
    pub rsi: Option<f64>,
    pub breakout_line: Option<f64>,
}

/// 接受棒收盘后冻结、在有限窗口等待微结构突破的 stop entry。
#[derive(Debug, Clone, Serialize)]
pub struct StopEntryIntent {
    pub intent: EntryIntent,
    pub trigger_price: f64,
    /// 含该索引对应 K 线；完成后仍未成交即撤销。
    pub expires_at_index: usize,
}

/// 当前持仓及已经激活的保护单。
#[derive(Debug, Clone)]
pub struct Position {
    pub direction: Direction,
    pub entry_time_ms: i64,
    pub entry_price: f64,
    pub signal_time_ms: i64,
    pub families: Vec<SignalFamily>,
    pub initial_stop: f64,
    pub stop: f64,
    pub target: Option<f64>,
    pub final_target: Option<f64>,
    pub activation_price: Option<f64>,
    pub exit_policy: ExitPolicy,
    pub activated: bool,
    /// 信号时冻结的严格逆势 EMA 排列年龄；`None` 表示该仓不属于 V5 纯 RSI 逆势分支。
    pub signal_counter_trend_ema_age_bars_capped_600: Option<usize>,
    /// 信号时确认的横盘近边；成熟分支只在完成棒严格穿过后允许抬止损。
    pub counter_trend_structure_breakout_line: Option<f64>,
    /// 是否已经由完成棒收盘严格穿过冻结近边；盘中触碰不能把它置为 `true`。
    pub counter_trend_structure_confirmed: bool,
    /// 是否在结构确认后达到至少 2R MFE；未确认结构时即使达到 2R 也必须保持 `false`。
    pub counter_trend_two_r_trailing_activated: bool,
    /// 从实际成交开始累计的最高价，供 V4/V5 完成棒移动保护使用。
    pub highest_high_since_entry: f64,
    /// 从实际成交开始累计的最低价，供 V4/V5 完成棒移动保护使用。
    pub lowest_low_since_entry: f64,
    /// V15 冻结的突破边界；完成棒收回箱体时，剩余仓位下一开盘退出。
    pub range_boundary: Option<f64>,
    /// 固定 1 单位仓位尚未退出的数量。
    pub remaining_quantity: f64,
    /// 已分批退出数量，用于最终聚合为一笔可审计交易。
    pub realized_quantity: f64,
    /// 已分批退出的带方向毛收益。
    pub realized_gross_pnl: f64,
    /// 已分批退出的成交价乘数量，最终计算加权退出价。
    pub realized_exit_notional: f64,
    /// 已分批退出产生的单边成本。
    pub realized_exit_cost: f64,
    /// 已分批退出的成本后收益；部分成交时立即计入回放权益。
    pub realized_net_pnl: f64,
    /// V15 基于实际下一开盘和结构止损得到的 1R 价。
    pub range_one_r_price: Option<f64>,
    /// V15 基于实际下一开盘和结构止损得到的 2R 价。
    pub range_two_r_price: Option<f64>,
    /// V15 是否已经执行 33% 的 1R 部分止盈。
    pub range_partial_one_r_taken: bool,
    /// V15 是否已有完成棒收盘达到 1R，从而允许净保本。
    pub range_one_r_close_confirmed: bool,
    /// V15 是否已有完成棒收盘达到 2R，从而允许 ATR 追踪。
    pub range_two_r_trailing_activated: bool,
    /// 入场信号冻结的目标消耗比例；`None` 表示并非 V21/V22 扫高失败确认单。
    pub anchor_upthrust_target_consumption_ratio: Option<f64>,
    pub volume_ratio: Option<f64>,
    pub rsi: Option<f64>,
}

/// 退出原因，命名与 Pine 的订单注释保持可对照。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitReason {
    StopLoss,
    CounterTrendTrailingStop,
    TakeProfit,
    StructureTakeProfit,
    /// V5 成熟纯 RSI 逆势仓触及冻结远边结构目标。
    RsiCounterTrendStructureTakeProfit,
    /// V5 结构确认后触及按单边 8bps 成本计算的净保本保护价。
    RsiCounterTrendNetBreakEven,
    /// V5 结构确认且 MFE 达到 2R 后触及距最佳价 2R 的单向追踪位。
    RsiCounterTrendTwoRTrailingStop,
    DivergenceTakeProfit,
    DivergenceBreakEven,
    EngulfingTakeProfit,
    EngulfingBreakEven,
    TrendExtensionTakeProfit,
    TrendExtensionBreakEven,
    EffortNoResultTakeProfit,
    EffortNoResultBreakEven,
    BollingerLowerReclaimTakeProfit,
    Ema596ReclaimDepartureTakeProfit,
    RangeSqueezeTakeProfit,
    RangeSqueezeNetBreakEven,
    RangeSqueezeAtrTrailingStop,
    RangeSqueezeBoxReentry,
    ReverseAtNextOpen,
    EndOfSample,
}

/// 已完成交易的逐笔审计记录。
#[derive(Debug, Clone, Serialize)]
pub struct Trade {
    pub direction: Direction,
    pub families: Vec<SignalFamily>,
    /// 记录实际持仓采用的退出政策，使原始止损也能归属到逆势结构家族。
    pub exit_policy: ExitPolicy,
    /// 入场信号冻结的严格逆势 EMA 排列年龄；最大 600，非 V5 纯 RSI 分支为 `None`。
    pub signal_counter_trend_ema_age_bars_capped_600: Option<usize>,
    /// 信号时冻结的横盘近边；`None` 表示该交易没有 V5 结构确认门槛。
    pub counter_trend_structure_breakout_line: Option<f64>,
    /// 平仓前是否出现完成棒严格穿过冻结近边，用于审计净保本是否有合法前提。
    pub counter_trend_structure_confirmed: bool,
    /// 平仓前是否已在结构确认后启动 2R 宽追踪，用于区分净保本和追踪退出。
    pub counter_trend_two_r_trailing_activated: bool,
    /// V15 是否曾在 1R 退出 33%，用于区分“先盈利后回撤”和纯止损。
    pub range_partial_one_r_taken: bool,
    /// V15 是否曾在完成棒 2R 后启用 ATR 追踪。
    pub range_two_r_trailing_activated: bool,
    pub signal_time_ms: i64,
    pub entry_time_ms: i64,
    pub exit_time_ms: i64,
    pub entry_price: f64,
    pub exit_price: f64,
    pub initial_stop: f64,
    pub exit_reason: ExitReason,
    pub gross_pnl: f64,
    pub net_pnl: f64,
    pub initial_risk: f64,
    pub net_r: f64,
    /// 确认棒相对 setup 收盘已消耗的冻结结构奖励比例；其他交易不输出该值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_upthrust_target_consumption_ratio: Option<f64>,
    pub volume_ratio: Option<f64>,
    pub rsi: Option<f64>,
}

/// 一次回放中被显式阻断或冲突取消的候选，用于解释“为什么没开仓”。
#[derive(Debug, Clone, Serialize)]
pub struct BlockedSignal {
    pub signal_time_ms: i64,
    pub direction: Option<Direction>,
    pub reason: String,
}

/// 单次回放的汇总指标。
#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub trades: usize,
    pub wins: usize,
    pub losses: usize,
    pub net_pnl: f64,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub profit_factor: Option<f64>,
    pub win_rate_percent: f64,
    pub average_net_r: f64,
    /// 只在交易平仓时更新的传统权益回撤，用于与旧报告保持可解释性。
    pub closed_equity_max_drawdown: f64,
    /// TradingView 口径：入场前已平仓权益峰值加持仓期间可达的不利价格偏移。
    pub max_drawdown: f64,
}

/// Rust Research 回放结果。
#[derive(Debug, Clone, Serialize)]
pub struct ReplayReport {
    pub strategy_version: &'static str,
    pub pine_source_fnv1a32: &'static str,
    pub symbol: String,
    pub tick_size: f64,
    pub evaluation_start_ms: i64,
    pub evaluation_end_ms: i64,
    pub fee_bps_per_side: f64,
    pub slippage_bps_per_side: f64,
    pub metrics: Metrics,
    /// 信号收盘时冻结的全部可执行意图；后续候选账本只能从这里读取当时可见特征。
    pub entry_candidates: Vec<EntryIntent>,
    pub trades: Vec<Trade>,
    pub blocked_signals: Vec<BlockedSignal>,
    /// 评价窗口结束时仍持有的仓位；TradingView closed-trade 指标不强制结算它。
    pub open_position_at_end: bool,
    /// 最后一根信号已确认但评价窗口内尚无下一根开盘时保留的待成交意图。
    pub pending_entry_at_end: bool,
}

/// 只影响模拟成交成本和样本范围，不改变冻结 Pine 信号规则。
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    pub symbol: String,
    pub tick_size: f64,
    pub evaluation_start_ms: i64,
    pub evaluation_end_ms: i64,
    pub fee_bps_per_side: f64,
    pub slippage_bps_per_side: f64,
    pub rule_version: ParityRuleVersion,
}

impl ReplayConfig {
    /// 历史 TradingView V1 对照基线：固定 1 单位、零手续费、零滑点。
    pub fn tradingview_baseline(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::Frozen66d3937e,
        }
    }

    /// 历史兼容入口固定指向 V2，禁止主 Pine 升级后让既有调用方静默切换规则。
    pub fn current_pine(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self::current_pine_v2(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
    }

    /// 冻结 Pine V2 对照基线：固定 1 单位、零手续费、零滑点。
    pub fn current_pine_v2(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::Current3cbbc9d8,
        }
    }

    /// 当前 Pine V3 候选基线：必须显式选择，固定 1 单位、零手续费、零滑点。
    pub fn current_pine_v3(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV3,
        }
    }

    /// 当前 Pine V4 候选基线：必须显式选择，固定 1 单位、零手续费、零滑点。
    pub fn current_pine_v4(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV4,
        }
    }

    /// 当前 Pine V5 候选基线：必须显式选择，固定 1 单位、零手续费、零滑点。
    pub fn current_pine_v5(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV5,
        }
    }

    /// 当前 Pine V6 候选基线：EMA 趋势多等待突破接受，其他规则继承 V5。
    pub fn current_pine_v6(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV6,
        }
    }

    /// 当前 Pine V7 候选基线：继承 V5，只增加 RSI 反向长影门禁。
    pub fn current_pine_v7(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV7,
        }
    }

    /// 当前 Pine V8 候选基线：继承 V5，只增加慢均线带收复后的定向空单门禁。
    pub fn current_pine_v8(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV8,
        }
    }

    /// 当前 Pine V9 候选基线：继承 V8，仅同步 EMA 576/676 与布林 20/2.5 默认值。
    pub fn current_pine_v9(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV9,
        }
    }

    /// 当前 Pine V10 候选基线：继承 V9 参数，只启用预注册的五类入场质量门禁。
    pub fn current_pine_v10(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV10,
        }
    }

    /// 当前 Pine V11 候选：继承 V10，仅追加预注册的四项残差门禁。
    pub fn current_pine_v11(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV11,
        }
    }

    /// 当前 Pine V12 候选：继承 V11 退出与保护，只替换五类入场确认时序。
    pub fn current_pine_v12(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV12,
        }
    }

    /// 当前 Pine V13 候选：冻结 V11 其他家族，只替换压缩扩张接受时序。
    pub fn current_pine_v13(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV13,
        }
    }

    /// 当前 Pine V14 候选：冻结 V11 其他家族，只使用无方向压缩制度状态机。
    pub fn current_pine_v14(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV14,
        }
    }

    /// V15 独立真实箱体突破接受 Research；不包含 V11 的其他信号家族。
    pub fn current_pine_v15(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV15,
        }
    }

    /// V16 独立右侧触发 Research；V15 退出合同保持不变。
    pub fn current_pine_v16(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV16,
        }
    }

    /// V17 纯右侧触发消融 Research；只移除 V16 新增的三项经济门禁。
    pub fn current_pine_v17(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV17,
        }
    }

    /// V18 组合 Research；V11 同棒优先，V17 只补充未被占用的机会。
    pub fn current_pine_v18(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV18,
        }
    }

    /// V19 组合 Research；冻结 V18，只拒绝长下影假突破空单。
    pub fn current_pine_v19(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV19,
        }
    }

    /// V20 组合 Research；冻结 V19，并启用扫高后快速失败接受空单。
    pub fn current_pine_v20(
        symbol: impl Into<String>,
        tick_size: f64,
        evaluation_start_ms: i64,
        evaluation_end_ms: i64,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
            fee_bps_per_side: 0.0,
            slippage_bps_per_side: 0.0,
            rule_version: ParityRuleVersion::CandidateV20,
        }
    }
}
