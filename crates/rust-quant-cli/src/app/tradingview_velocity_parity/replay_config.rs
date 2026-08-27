use serde::Serialize;

use super::model::{BlockedSignal, EntryIntent, Metrics, ParityRuleVersion, Trade};

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
    /// `None` 表示退出沿用入场费率；统一回踩 V22/V23 用 `Some(5.0)` 表达 taker 退出。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_fee_bps_per_side: Option<f64>,
    /// `None` 表示退出沿用入场滑点；统一回踩 V22/V23 显式冻结为 1 bps。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_slippage_bps_per_side: Option<f64>,
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
    /// 可选退出费率；缺省保持历史双边同费率回放。
    pub exit_fee_bps_per_side: Option<f64>,
    /// 可选退出滑点；缺省保持历史双边同滑点回放。
    pub exit_slippage_bps_per_side: Option<f64>,
    pub rule_version: ParityRuleVersion,
}

impl ReplayConfig {
    /// 返回入场 maker/taker 费率与滑点的合计 bps。
    pub fn entry_cost_bps(&self) -> f64 {
        self.fee_bps_per_side + self.slippage_bps_per_side
    }

    /// 返回退出费率与滑点合计；没有独立退出合同时沿用入场口径。
    pub fn exit_cost_bps(&self) -> f64 {
        self.exit_fee_bps_per_side.unwrap_or(self.fee_bps_per_side)
            + self
                .exit_slippage_bps_per_side
                .unwrap_or(self.slippage_bps_per_side)
    }

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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
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
            exit_fee_bps_per_side: None,
            exit_slippage_bps_per_side: None,
            rule_version: ParityRuleVersion::CandidateV20,
        }
    }
}
