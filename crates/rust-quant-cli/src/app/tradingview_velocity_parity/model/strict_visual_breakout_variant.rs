use super::*;

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
    /// 在 V12 基础上增加真实极差门禁：区间内真实 high-low 极差相对 P90/P10 带的膨胀
    /// 倍数超过阈值即判非横盘，用于排除被百分位边界掩盖的大波段与 U 形折返。
    V13RawRangeGuard,
    /// 在 V12 基础上仅收紧强突破棒的方向位移门槛到 0.5%（不含 V13 极差门禁），
    /// 用于验证更高涨跌幅要求在 60 币种全样本上的收益方向。
    V14DirectionalMove50Bps,
    /// 与 V14 同构，方向位移门槛继续收紧到 0.75%，用于沿门槛阶梯寻找收益拐点。
    V15DirectionalMove75Bps,
    /// 与 V14 同构，方向位移门槛收紧到 1.0%，门槛阶梯的上端观测点。
    V16DirectionalMove100Bps,
    /// 在 V14（方向位移 0.5%）基础上，只对空头追加「温和突破」越界门槛：
    /// 突破收盘越界幅度 / 源 ATR ≤ 0.39 才接受。判别诊断显示越界越浅 runner 越富集、
    /// 追突破期望 R 越高，用于验证能否把空头 net_r 提到成本线以上。
    V17SoftBreakoutShort,
    /// 在 V17 基础上去掉回踩接受：温和突破棒确认即直接入场（下一开盘），不等回踩。
    /// 修正 V17「专挑不回踩 runner 却只在回踩时进场」的逻辑矛盾，测自洽入场下的真实表现。
    V18SoftBreakoutShortDirectEntry,
    /// 在 V18（直接入场 + 越界≤0.39）基础上追加 EMA 趋势负向 gate：只接受
    /// (突破收盘 - EMA144)/ATR ≥ -4.65 的空头，排除「价格远在中期均线下」的延伸追空。
    /// discriminator 逐月验证显示该组合 gate 在 13/13 个月毛 chase R 稳定为正且优于基线，
    /// 用于验证毛质量提升能否扛过 8bps 成本。
    V19SoftBreakoutShortEmaGate,
    /// 在 V19（直接入场 + 越界≤0.39 + close_vs_ema144≥-4.65）基础上追加突破棒实体门：
    /// 只接受 body_ratio ≤ 0.915 的空头。discriminator 在已过双门的子集内做条件分位显示，
    /// body_ratio 是双门后唯一仍保有独立区分力的特征——低实体突破 chase 均值 ~0.55-0.61、
    /// runner ~33%，高实体段塌陷到 ~0.30；volume/directional 的区分力已被越界门吸收。
    /// 用于验证第三道信号门能否把毛质量进一步抬到扛过 8bps taker 成本。
    V20SoftBreakoutShortBodyGate,
    /// 独立统一合同：多空都恢复 1～5 根回踩确认，强突破回到 50% 实体与 15 bps
    /// 方向位移，不使用 1.5% 整根振幅门；空头继续保留 V20 三道信号门。
    V22UnifiedRetestExecutionCost,
    /// 保留 V22 的回踩、强度、风险与成本合同，只整体移除空头的越界、EMA144
    /// 和突破实体三道附加门禁，用于隔离检验该门禁组合的实际增益。
    V23UnifiedRetestNoShortGates,
    /// 保留 V23 完整合同，只在首次合法确认时要求收盘保留至少 75% 的原突破越界幅度。
    V23AcceptanceRetention75CostReplay,
    /// 保留 V23 的完整交易合同；弱棒收盘越界时立即消费旧横盘，不再观察下一根恢复。
    V25ImmediateWeakDepartureConsume,
    /// 保留 V23 生命周期；固定门失败时允许 45% 实体且实体达到冻结区间高度 25% 成为锚点。
    V26NarrowRangeDynamicAnchor,
    /// 保留 V23 的 75% 首次接受门；确认后再等紧邻一根完成棒守住同一 75% 接受价。
    V28AcceptanceRetention75NextBarHold,
    /// 保留 V23 全部交易合同，只把视觉横盘边界从 High P90/Low P10 改为 P96/P04。
    V29BoundaryP96P04,
    /// 保留 V23 全部交易合同，拒绝一个当前区间高度内存在未解决旧有效横盘的来源。
    V30PriorValidRangeClearance,
    /// 保留 V23 全部合同；候选或活动横盘内单根实体达到区间高度 50% 时重置结构。
    V31InternalImpulseBodyGuard,
    /// V32～V40 各自只改变一个研究维度，并分别继承 V31 的其余完整合同。
    V32PriorThreeWickDirectChase,
    /// 首次确认棒出现反方向深扫并强收回时拒绝来源。
    V33OpposingConfirmationWickGuard,
    /// 持仓后的盈利放量完成棒把止损移动到成本后保本价。
    V34VolumeProfitProtection,
    /// 实际下一开盘时要求冻结目标仍至少提供 0.50R 成本后空间。
    V35RemainingNetReward50,
    /// 视觉边界保持 P90/P10，交易突破边界使用横盘前 96 根方向外部极值。
    V36PreRangeExtremeTradeBoundary96,
    V40VisualParentExecutionBoundary,
    /// 活动父区间内部允许一个更晚确认、完整位于父视觉外壳内的 V31 子区间独立交易。
    V41NestedChildRange,
    /// 保留 V41；区间高度 50% 的大实体只有同时穿出冻结核心边界时才重置横盘。
    V42ContainedInternalBodyExemption,
    /// 保留 V42；父区间向左升级时，视觉父区间上下半侧都必须承载至少 25% 实体中心。
    V43BalancedParentUpgrade,
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
            Self::V13RawRangeGuard => "v13_raw_range_guard",
            Self::V14DirectionalMove50Bps => "v14_directional_move_50bps",
            Self::V15DirectionalMove75Bps => "v15_directional_move_75bps",
            Self::V16DirectionalMove100Bps => "v16_directional_move_100bps",
            Self::V17SoftBreakoutShort => "v17_soft_breakout_short",
            Self::V18SoftBreakoutShortDirectEntry => "v18_soft_breakout_short_direct_entry",
            Self::V19SoftBreakoutShortEmaGate => "v19_soft_breakout_short_ema_gate",
            Self::V20SoftBreakoutShortBodyGate => "v20_soft_breakout_short_body_gate",
            Self::V22UnifiedRetestExecutionCost => "v22_unified_retest_maker2_taker5_slippage1",
            Self::V23UnifiedRetestNoShortGates => {
                "v23_unified_retest_no_short_gates_maker2_taker5_slippage1"
            }
            Self::V23AcceptanceRetention75CostReplay => {
                "v23_acceptance_retention_75pct_maker2_taker5_slippage1"
            }
            Self::V25ImmediateWeakDepartureConsume => {
                "v25_immediate_weak_departure_consume_maker2_taker5_slippage1"
            }
            Self::V26NarrowRangeDynamicAnchor => {
                "v26_narrow_range_dynamic_anchor_body45_range25_maker2_taker5_slippage1"
            }
            Self::V28AcceptanceRetention75NextBarHold => {
                "v28_acceptance_retention_75pct_next_bar_hold_maker2_taker5_slippage1"
            }
            Self::V29BoundaryP96P04 => {
                "v29_boundary_p96_p04_acceptance_retention_75pct_maker2_taker5_slippage1"
            }
            Self::V30PriorValidRangeClearance => {
                "v30_prior_valid_range_clearance_1height_96bars_acceptance_retention_75pct_maker2_taker5_slippage1"
            }
            Self::V31InternalImpulseBodyGuard => {
                "v31_internal_impulse_body_50pct_range_guard_acceptance_retention_75pct_maker2_taker5_slippage1"
            }
            Self::V32PriorThreeWickDirectChase => "v32_prior_three_wick_direct_chase",
            Self::V33OpposingConfirmationWickGuard => "v33_opposing_confirmation_wick_guard",
            Self::V34VolumeProfitProtection => "v34_volume_profit_protection",
            Self::V35RemainingNetReward50 => "v35_remaining_net_reward_050r",
            Self::V36PreRangeExtremeTradeBoundary96 => {
                "v36b_pre_range_extreme_trade_boundary_96bars"
            }
            Self::V40VisualParentExecutionBoundary => "v40_visual_parent_execution_boundary",
            Self::V41NestedChildRange => "v41_nested_child_range",
            Self::V42ContainedInternalBodyExemption => {
                "v42_contained_internal_body_exemption"
            }
            Self::V43BalancedParentUpgrade => "v43_balanced_parent_upgrade",
        }
    }
    /// 五个新实验均以 V31 为唯一基线，避免在同一候选中混入其他新变量。
    const fn inherits_v31_contract(self) -> bool {
        matches!(
            self,
            Self::V32PriorThreeWickDirectChase
                | Self::V33OpposingConfirmationWickGuard
                | Self::V34VolumeProfitProtection
                | Self::V35RemainingNetReward50
                | Self::V36PreRangeExtremeTradeBoundary96
                | Self::V40VisualParentExecutionBoundary
                | Self::V41NestedChildRange
                | Self::V42ContainedInternalBodyExemption
                | Self::V43BalancedParentUpgrade
        )
    }
    /// `true` 只表示启用严格横盘 Research 家族；不会注册生产或模拟运行入口。
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Self::Baseline)
    }
    /// `true` 表示突破棒只冻结来源，必须等待后续完成棒验证价格接受。
    pub const fn requires_retest_acceptance(self) -> bool {
        self.inherits_v31_contract()
            || matches!(
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
                    | Self::V13RawRangeGuard
                    | Self::V14DirectionalMove50Bps
                    | Self::V15DirectionalMove75Bps
                    | Self::V16DirectionalMove100Bps
                    | Self::V17SoftBreakoutShort
                    | Self::V22UnifiedRetestExecutionCost
                    | Self::V23UnifiedRetestNoShortGates
                    | Self::V23AcceptanceRetention75CostReplay
                    | Self::V25ImmediateWeakDepartureConsume
                    | Self::V26NarrowRangeDynamicAnchor
                    | Self::V28AcceptanceRetention75NextBarHold
                    | Self::V29BoundaryP96P04
                    | Self::V30PriorValidRangeClearance
                    | Self::V31InternalImpulseBodyGuard
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
    /// `true` 表示突破源必须同时通过该版本冻结的实体占比与方向位移门槛。
    pub const fn requires_breakout_body_strength(self) -> bool {
        self.inherits_v31_contract()
            || matches!(
                self,
                Self::V5BreakoutBodyStrength
                    | Self::V6WeakDepartureProbation
                    | Self::V8AcceptanceMargin40Atr
                    | Self::V9ExternalStructureClearance
                    | Self::V10SymmetricRetainedBreakout
                    | Self::V11BreakoutCandleExtremeStop
                    | Self::V12ExtremeStopMinOneAtr
                    | Self::V13RawRangeGuard
                    | Self::V14DirectionalMove50Bps
                    | Self::V15DirectionalMove75Bps
                    | Self::V16DirectionalMove100Bps
                    | Self::V17SoftBreakoutShort
                    | Self::V18SoftBreakoutShortDirectEntry
                    | Self::V19SoftBreakoutShortEmaGate
                    | Self::V20SoftBreakoutShortBodyGate
                    | Self::V22UnifiedRetestExecutionCost
                    | Self::V23UnifiedRetestNoShortGates
                    | Self::V23AcceptanceRetention75CostReplay
                    | Self::V25ImmediateWeakDepartureConsume
                    | Self::V26NarrowRangeDynamicAnchor
                    | Self::V28AcceptanceRetention75NextBarHold
                    | Self::V29BoundaryP96P04
                    | Self::V30PriorValidRangeClearance
                    | Self::V31InternalImpulseBodyGuard
            )
    }

    /// `true` 表示弱离区只等待紧邻下一根完成棒决定恢复或消费旧横盘。
    pub const fn uses_weak_departure_probation(self) -> bool {
        self.inherits_v31_contract()
            || matches!(
                self,
                Self::V6WeakDepartureProbation
                    | Self::V8AcceptanceMargin40Atr
                    | Self::V9ExternalStructureClearance
                    | Self::V10SymmetricRetainedBreakout
                    | Self::V11BreakoutCandleExtremeStop
                    | Self::V12ExtremeStopMinOneAtr
                    | Self::V13RawRangeGuard
                    | Self::V14DirectionalMove50Bps
                    | Self::V15DirectionalMove75Bps
                    | Self::V16DirectionalMove100Bps
                    | Self::V17SoftBreakoutShort
                    | Self::V18SoftBreakoutShortDirectEntry
                    | Self::V19SoftBreakoutShortEmaGate
                    | Self::V20SoftBreakoutShortBodyGate
                    | Self::V22UnifiedRetestExecutionCost
                    | Self::V23UnifiedRetestNoShortGates
                    | Self::V23AcceptanceRetention75CostReplay
                    | Self::V26NarrowRangeDynamicAnchor
                    | Self::V28AcceptanceRetention75NextBarHold
                    | Self::V29BoundaryP96P04
                    | Self::V30PriorValidRangeClearance
                    | Self::V31InternalImpulseBodyGuard
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
        self.inherits_v31_contract()
            || matches!(
                self,
                Self::V10SymmetricRetainedBreakout
                    | Self::V11BreakoutCandleExtremeStop
                    | Self::V12ExtremeStopMinOneAtr
                    | Self::V13RawRangeGuard
                    | Self::V14DirectionalMove50Bps
                    | Self::V15DirectionalMove75Bps
                    | Self::V16DirectionalMove100Bps
                    | Self::V17SoftBreakoutShort
                    | Self::V18SoftBreakoutShortDirectEntry
                    | Self::V19SoftBreakoutShortEmaGate
                    | Self::V20SoftBreakoutShortBodyGate
                    | Self::V22UnifiedRetestExecutionCost
                    | Self::V23UnifiedRetestNoShortGates
                    | Self::V23AcceptanceRetention75CostReplay
                    | Self::V25ImmediateWeakDepartureConsume
                    | Self::V26NarrowRangeDynamicAnchor
                    | Self::V28AcceptanceRetention75NextBarHold
                    | Self::V29BoundaryP96P04
                    | Self::V30PriorValidRangeClearance
                    | Self::V31InternalImpulseBodyGuard
            )
    }

    /// `true` 时突破棒完成即冻结整根极值外一 tick，确认等待期间不得重算。
    pub const fn uses_breakout_candle_extreme_stop(self) -> bool {
        self.inherits_v31_contract()
            || matches!(
                self,
                Self::V11BreakoutCandleExtremeStop
                    | Self::V12ExtremeStopMinOneAtr
                    | Self::V13RawRangeGuard
                    | Self::V14DirectionalMove50Bps
                    | Self::V15DirectionalMove75Bps
                    | Self::V16DirectionalMove100Bps
                    | Self::V17SoftBreakoutShort
                    | Self::V18SoftBreakoutShortDirectEntry
                    | Self::V19SoftBreakoutShortEmaGate
                    | Self::V20SoftBreakoutShortBodyGate
                    | Self::V22UnifiedRetestExecutionCost
                    | Self::V23UnifiedRetestNoShortGates
                    | Self::V23AcceptanceRetention75CostReplay
                    | Self::V25ImmediateWeakDepartureConsume
                    | Self::V26NarrowRangeDynamicAnchor
                    | Self::V28AcceptanceRetention75NextBarHold
                    | Self::V29BoundaryP96P04
                    | Self::V30PriorValidRangeClearance
                    | Self::V31InternalImpulseBodyGuard
            )
    }

    /// `true` 时在实际开盘保证最终风险距离至少一个确认信号 ATR（V12 起引入）。
    pub const fn uses_extreme_stop_min_one_atr(self) -> bool {
        self.inherits_v31_contract()
            || matches!(
                self,
                Self::V12ExtremeStopMinOneAtr
                    | Self::V13RawRangeGuard
                    | Self::V14DirectionalMove50Bps
                    | Self::V15DirectionalMove75Bps
                    | Self::V16DirectionalMove100Bps
                    | Self::V17SoftBreakoutShort
                    | Self::V18SoftBreakoutShortDirectEntry
                    | Self::V19SoftBreakoutShortEmaGate
                    | Self::V20SoftBreakoutShortBodyGate
                    | Self::V22UnifiedRetestExecutionCost
                    | Self::V23UnifiedRetestNoShortGates
                    | Self::V23AcceptanceRetention75CostReplay
                    | Self::V25ImmediateWeakDepartureConsume
                    | Self::V26NarrowRangeDynamicAnchor
                    | Self::V28AcceptanceRetention75NextBarHold
                    | Self::V29BoundaryP96P04
                    | Self::V30PriorValidRangeClearance
                    | Self::V31InternalImpulseBodyGuard
            )
    }

    /// `true` 时启用真实极差门禁：真实 high-low 极差 / P90-P10 带 超过阈值即判非横盘。
    pub const fn uses_raw_range_guard(self) -> bool {
        matches!(self, Self::V13RawRangeGuard)
    }

    /// `true` 时固定 50%/15bps 门失败后，可用 45% 实体与 25% 冻结区间高度替代门升级锚点。
    pub const fn uses_narrow_range_dynamic_anchor(self) -> bool {
        matches!(self, Self::V26NarrowRangeDynamicAnchor)
    }

    /// 返回首次 V23 合法确认时追加的保留率门槛；`None` 表示不改变原确认合同。
    pub const fn minimum_acceptance_retention_ratio(self) -> Option<f64> {
        if self.inherits_v31_contract() {
            return Some(0.75);
        }
        match self {
            Self::V23AcceptanceRetention75CostReplay
            | Self::V28AcceptanceRetention75NextBarHold
            | Self::V29BoundaryP96P04
            | Self::V30PriorValidRangeClearance
            | Self::V31InternalImpulseBodyGuard => Some(0.75),
            _ => None,
        }
    }

    /// `true` 表示首次 75% 接受确认后还要等待紧邻一根完成棒守住同一冻结接受价。
    pub const fn requires_next_completed_bar_hold(self) -> bool {
        matches!(self, Self::V28AcceptanceRetention75NextBarHold)
    }

    /// 返回视觉区间 high/low 的百分位边界；仅 V29 使用 P96/P04。
    pub const fn boundary_percentile_ranks(self) -> (f64, f64) {
        match self {
            Self::V29BoundaryP96P04 => (96.0, 4.0),
            _ => (90.0, 10.0),
        }
    }

    /// `true` 表示突破棒必须冻结并检查左侧 96 根内一个区间高度的旧有效横盘净空。
    pub const fn requires_prior_valid_range_clearance(self) -> bool {
        matches!(self, Self::V30PriorValidRangeClearance)
    }

    /// `true` 表示候选及活动横盘都启用单根实体占区间高度 50% 的结构重置门禁。
    pub const fn uses_internal_impulse_body_guard(self) -> bool {
        self.inherits_v31_contract() || matches!(self, Self::V31InternalImpulseBodyGuard)
    }

    /// `true` 表示原 V31 父状态机旁路维护一个父内局部子区间交易源。
    pub const fn uses_nested_child_range(self) -> bool {
        matches!(
            self,
            Self::V41NestedChildRange
                | Self::V42ContainedInternalBodyExemption
                | Self::V43BalancedParentUpgrade
        )
    }

    /// `true` 表示完整位于冻结核心内的大实体仍属于横盘内部换手。
    pub const fn exempts_fully_contained_internal_body(self) -> bool {
        matches!(
            self,
            Self::V42ContainedInternalBodyExemption | Self::V43BalancedParentUpgrade
        )
    }

    /// 父区间向左升级时允许的最大单侧实体中心占比；首次确认不受该门禁影响。
    pub const fn parent_upgrade_max_dominant_half_ratio(self) -> Option<f64> {
        match self {
            Self::V43BalancedParentUpgrade => Some(0.75),
            _ => None,
        }
    }

    /// 返回强突破棒方向位移门槛覆盖值（比例，非百分比）；`None` 表示沿用变体默认门槛。
    /// V14/V15/V16 沿门槛阶梯分别收紧到 0.5% / 0.75% / 1.0%，其余 V12 行为不变。
    pub const fn directional_move_ratio_override(self) -> Option<f64> {
        match self {
            Self::V14DirectionalMove50Bps => Some(0.005),
            Self::V15DirectionalMove75Bps => Some(0.0075),
            Self::V16DirectionalMove100Bps => Some(0.010),
            // V17 / V18 继承 V14 的 0.5% 方向位移门槛，另叠加空头越界门槛。
            Self::V17SoftBreakoutShort
            | Self::V18SoftBreakoutShortDirectEntry
            | Self::V19SoftBreakoutShortEmaGate
            | Self::V20SoftBreakoutShortBodyGate => Some(0.005),
            _ => None,
        }
    }

    /// 空头「温和突破」越界上限：突破收盘越界幅度 / 源 ATR 超过该值即拒绝空头信号。
    /// `None` 表示不启用该门槛。判别诊断取 0.39（保留约 40% 样本、追突破期望 R 最优性价比）。
    pub const fn soft_breakout_short_max_excess_atr(self) -> Option<f64> {
        match self {
            Self::V17SoftBreakoutShort
            | Self::V18SoftBreakoutShortDirectEntry
            | Self::V19SoftBreakoutShortEmaGate
            | Self::V20SoftBreakoutShortBodyGate
            | Self::V22UnifiedRetestExecutionCost => Some(0.39),
            _ => None,
        }
    }

    /// 空头 EMA 趋势负向 gate 下限：(突破收盘 - EMA144)/ATR ≥ 该值才接受。
    /// V19/V20/V22 启用（-4.65），排除「价格远在中期均线下」的延伸追空；判别诊断显示该档 runner
    /// 最低、追突破期望 R 最差。其余变体返回 None 表示不施加该 gate。
    pub const fn soft_breakout_short_min_close_vs_ema144_atr(self) -> Option<f64> {
        match self {
            Self::V19SoftBreakoutShortEmaGate
            | Self::V20SoftBreakoutShortBodyGate
            | Self::V22UnifiedRetestExecutionCost => Some(-4.65),
            _ => None,
        }
    }

    /// 空头突破棒实体上限门：突破棒 body_ratio ≤ 该值才接受。
    /// V20/V22 启用（0.915）。在已过越界门+EMA144 门的子集内做条件分位，body_ratio 是双门后
    /// 唯一仍有独立区分力的特征——低实体突破 chase 均值最高、runner 占比最大，越过 ~0.915 后
    /// 追突破期望 R 明显塌陷。其余变体返回 None 表示不施加该门。
    pub const fn soft_breakout_short_max_body_ratio(self) -> Option<f64> {
        match self {
            Self::V20SoftBreakoutShortBodyGate | Self::V22UnifiedRetestExecutionCost => Some(0.915),
            _ => None,
        }
    }

    /// `true` 表示采用统一研究合同的 maker 入场、taker 出场和双边 1 bps 滑点成本。
    pub const fn uses_unified_retest_execution_cost(self) -> bool {
        self.inherits_v31_contract()
            || matches!(
                self,
                Self::V22UnifiedRetestExecutionCost
                    | Self::V23UnifiedRetestNoShortGates
                    | Self::V23AcceptanceRetention75CostReplay
                    | Self::V25ImmediateWeakDepartureConsume
                    | Self::V26NarrowRangeDynamicAnchor
                    | Self::V28AcceptanceRetention75NextBarHold
                    | Self::V29BoundaryP96P04
                    | Self::V30PriorValidRangeClearance
                    | Self::V31InternalImpulseBodyGuard
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
            Self::V13RawRangeGuard => {
                "strict_visual_consolidation_symmetric_breakout_candle_extreme_stop_min_1atr_raw_range_guard_15m_research_v1"
            }
            Self::V14DirectionalMove50Bps => {
                "strict_visual_consolidation_symmetric_breakout_candle_extreme_stop_min_1atr_directional_move_50bps_15m_research_v1"
            }
            Self::V15DirectionalMove75Bps => {
                "strict_visual_consolidation_symmetric_breakout_candle_extreme_stop_min_1atr_directional_move_75bps_15m_research_v1"
            }
            Self::V16DirectionalMove100Bps => {
                "strict_visual_consolidation_symmetric_breakout_candle_extreme_stop_min_1atr_directional_move_100bps_15m_research_v1"
            }
            Self::V17SoftBreakoutShort => {
                "strict_visual_consolidation_symmetric_breakout_soft_breakout_short_excess_0_39atr_15m_research_v1"
            }
            Self::V18SoftBreakoutShortDirectEntry => {
                "strict_visual_consolidation_symmetric_breakout_soft_breakout_short_direct_entry_excess_0_39atr_15m_research_v1"
            }
            Self::V19SoftBreakoutShortEmaGate => {
                "strict_visual_consolidation_symmetric_breakout_soft_breakout_short_direct_entry_excess_0_39atr_ema144_gate_neg4_65_15m_research_v1"
            }
            Self::V20SoftBreakoutShortBodyGate => {
                "strict_visual_consolidation_symmetric_breakout_soft_breakout_short_direct_entry_excess_0_39atr_ema144_gate_neg4_65_body_0_915_15m_research_v1"
            }
            Self::V22UnifiedRetestExecutionCost => {
                "strict_visual_consolidation_symmetric_retest_50pct_15bps_short_excess_0_39atr_ema144_neg4_65_body_0_915_maker2_taker5_slippage1_15m_research_v1"
            }
            Self::V23UnifiedRetestNoShortGates => {
                "strict_visual_consolidation_symmetric_retest_50pct_15bps_no_short_gates_maker2_taker5_slippage1_15m_research_v1"
            }
            Self::V23AcceptanceRetention75CostReplay => {
                "strict_visual_consolidation_symmetric_retest_50pct_15bps_no_short_gates_acceptance_retention_75pct_maker2_taker5_slippage1_15m_research_v1"
            }
            Self::V25ImmediateWeakDepartureConsume => {
                "strict_visual_consolidation_symmetric_retest_50pct_15bps_no_short_gates_immediate_weak_departure_consume_maker2_taker5_slippage1_15m_research_v1"
            }
            Self::V26NarrowRangeDynamicAnchor => {
                "strict_visual_consolidation_symmetric_retest_fixed50_15bps_or_dynamic45_range25_no_short_gates_maker2_taker5_slippage1_15m_research_v1"
            }
            Self::V28AcceptanceRetention75NextBarHold => {
                "strict_visual_consolidation_symmetric_retest_50pct_15bps_no_short_gates_acceptance_retention_75pct_next_bar_hold_maker2_taker5_slippage1_15m_research_v1"
            }
            Self::V29BoundaryP96P04 => {
                "strict_visual_consolidation_symmetric_retest_50pct_15bps_no_short_gates_boundary_p96_p04_acceptance_retention_75pct_maker2_taker5_slippage1_15m_research_v1"
            }
            Self::V30PriorValidRangeClearance => {
                "strict_visual_consolidation_symmetric_retest_50pct_15bps_no_short_gates_prior_valid_range_clearance_1height_96bars_acceptance_retention_75pct_maker2_taker5_slippage1_15m_research_v1"
            }
            Self::V31InternalImpulseBodyGuard => {
                "strict_visual_consolidation_symmetric_retest_50pct_15bps_no_short_gates_internal_impulse_body_50pct_range_guard_acceptance_retention_75pct_maker2_taker5_slippage1_15m_research_v1"
            }
            Self::V32PriorThreeWickDirectChase => {
                "strict_visual_consolidation_v32_prior_three_wick_direct_chase_15m_research_v1"
            }
            Self::V33OpposingConfirmationWickGuard => {
                "strict_visual_consolidation_v33_opposing_confirmation_wick_guard_15m_research_v1"
            }
            Self::V34VolumeProfitProtection => {
                "strict_visual_consolidation_v34_volume_profit_protection_15m_research_v1"
            }
            Self::V35RemainingNetReward50 => {
                "strict_visual_consolidation_v35_remaining_net_reward_050r_15m_research_v1"
            }
            Self::V36PreRangeExtremeTradeBoundary96 => {
                "strict_visual_consolidation_v36b_pre_range_extreme_trade_boundary_96bars_15m_research_v1"
            }
            Self::V40VisualParentExecutionBoundary => "strict_visual_v40_parent_execution_15m_research_v1",
            Self::V41NestedChildRange => "strict_visual_v41_nested_child_range_15m_research_v1",
            Self::V42ContainedInternalBodyExemption => {
                "strict_visual_v42_contained_internal_body_exemption_15m_research_v1"
            }
            Self::V43BalancedParentUpgrade => {
                "strict_visual_v43_balanced_parent_upgrade_15m_research_v1"
            }
        }
    }
}
