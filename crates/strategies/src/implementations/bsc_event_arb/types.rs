use crate::strategy_common::SignalResult;
use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BscEventArbAction {
    Long,
    Flat,
    ForceExit,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BscEventArbThresholds {
    /// 24 小时成交额下限，单位 USD。
    pub min_volume_24h_usd: f64,
    /// 1 小时成交量相对 24 小时均值的最小倍数。
    pub min_volume_1h_vs_24h_avg: f64,
    /// 2% 深度的最小流动性金额，单位 USD。
    pub min_depth_2pct_usd: f64,
    /// 允许的最大买卖税费百分比。
    pub max_tax_pct: f64,
    /// 中心化交易所成交量占比下限。
    pub min_cex_volume_share: f64,
    /// 15 分钟涨跌幅触发下限。
    pub min_price_change_15m_pct: f64,
    /// 1 小时涨跌幅触发下限。
    pub min_price_change_1h_pct: f64,
    /// 成交量 Z-score 触发下限。
    pub min_volume_zscore: f64,
    /// 1 小时持仓量增长百分比下限。
    pub min_oi_growth_1h_pct: f64,
    /// 4 小时持仓量增长百分比下限。
    pub min_oi_growth_4h_pct: f64,
    /// 空头拥挤度评分下限。
    pub min_short_crowding_score: f64,
    /// 中心化交易所大额资金流阈值，单位 USD。
    pub large_cex_flow_usd: f64,
    /// 硬止损百分比。
    pub hard_stop_loss_pct: f64,
    /// 第一档止盈百分比。
    pub first_take_profit_pct: f64,
    /// 第二档止盈百分比。
    pub second_take_profit_pct: f64,
    /// 跟踪止损回撤百分比。
    pub trailing_stop_pct: f64,
    /// 事件驱动仓位最长持有分钟数。
    pub max_event_hold_minutes: i64,
    /// 时间止损的最短持仓分钟数。
    pub time_stop_minutes: i64,
    /// 触发时间止盈所需的最小浮盈百分比。
    pub min_time_stop_profit_pct: f64,
    /// 1 小时持仓量最大回落百分比。
    pub max_oi_drop_1h_pct: f64,
}
impl Default for BscEventArbThresholds {
    /// 提供默认参数，保证 回测与策略研究 在未显式配置时仍有稳定初始值。
    fn default() -> Self {
        Self {
            min_volume_24h_usd: 5_000_000.0,
            min_volume_1h_vs_24h_avg: 5.0,
            min_depth_2pct_usd: 50_000.0,
            max_tax_pct: 5.0,
            min_cex_volume_share: 0.40,
            min_price_change_15m_pct: 8.0,
            min_price_change_1h_pct: 20.0,
            min_volume_zscore: 3.0,
            min_oi_growth_1h_pct: 30.0,
            min_oi_growth_4h_pct: 80.0,
            min_short_crowding_score: 0.65,
            large_cex_flow_usd: 250_000.0,
            hard_stop_loss_pct: -10.0,
            first_take_profit_pct: 25.0,
            second_take_profit_pct: 60.0,
            trailing_stop_pct: 20.0,
            max_event_hold_minutes: 24 * 60,
            time_stop_minutes: 30,
            min_time_stop_profit_pct: 8.0,
            max_oi_drop_1h_pct: 25.0,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BscEventArbStrategyConfig {
    /// 策略名称。
    pub strategy_name: Option<String>,
    /// 阈值配置。
    pub thresholds: BscEventArbThresholds,
    /// 策略快照；为空时使用默认值或表示不限制。
    pub snapshot: Option<BscEventArbSignalSnapshot>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BscEventArbSignalSnapshot {
    /// 链 ID。
    pub chain_id: String,
    /// 标签列表。
    pub event_tags: Vec<String>,
    /// 标的价格，单位 USD。
    pub price_usd: f64,
    /// 24 小时成交额，单位 USD。
    pub volume_24h_usd: f64,
    /// 1 小时成交量相对 24 小时均值的倍数。
    pub volume_1h_vs_24h_avg: f64,
    /// 2% 深度流动性金额，单位 USD。
    pub depth_2pct_usd: f64,
    /// 是否仅存在 DEX 流动性。
    pub is_dex_only: bool,
    /// 卖出路径模拟是否通过。
    pub sell_simulation_passed: bool,
    /// 买入税费百分比。
    pub buy_tax_pct: f64,
    /// 卖出税费百分比。
    pub sell_tax_pct: f64,
    /// 是否存在黑名单风险。
    pub has_blacklist_risk: bool,
    /// 是否存在合约暂停风险。
    pub has_pause_risk: bool,
    /// 是否存在增发风险。
    pub has_mint_risk: bool,
    /// 中心化交易所成交量占比。
    pub cex_volume_share: f64,
    /// 15 分钟价格涨跌幅百分比。
    pub price_change_15m_pct: f64,
    /// 1 小时价格涨跌幅百分比。
    pub price_change_1h_pct: f64,
    /// 价格是否站上 15 分钟 VWAP。
    pub price_above_15m_vwap: bool,
    /// 成交量zscore5 分钟。
    pub volume_zscore_5m: f64,
    /// 成交量zscore15 分钟。
    pub volume_zscore_15m: f64,
    /// 1 小时持仓量增长百分比。
    pub oi_growth_1h_pct: f64,
    /// 4 小时持仓量增长百分比。
    pub oi_growth_4h_pct: f64,
    /// 资金费率。
    pub funding_rate: f64,
    /// 综合评分。
    pub short_crowding_score: f64,
    /// 价格上涨是否伴随持仓量增长。
    pub price_up_with_oi: bool,
    /// 中心化交易所净流入金额，单位 USD。
    pub cex_net_inflow_usd: f64,
    /// 大额流入后价格是否保持韧性。
    pub price_resilient_after_inflow: bool,
    /// 大额流入后是否出现中心化交易所流出。
    pub cex_outflow_after_inflow: bool,
    /// 现货承接强度。
    pub spot_absorption: bool,
    /// 价格是否跌破 15 分钟 VWAP。
    pub price_below_15m_vwap: bool,
    /// 1 小时持仓量回落百分比。
    pub oi_drop_1h_pct: f64,
    /// 资金费率是否转为正值。
    pub funding_flipped_positive: bool,
    /// 价格是否创出新高。
    pub price_making_new_high: bool,
    /// 头部持有人或流动性池是否出现异常流出。
    pub top_holder_or_lp_abnormal_outflow: bool,
    /// 中心化交易所是否出现提现或交易限制。
    pub cex_withdrawal_or_trading_restriction: bool,
    /// 距离入场的分钟数。
    pub minutes_since_entry: i64,
    /// 最大未实现收益百分比。
    pub max_unrealized_profit_pct: f64,
    /// 跟踪止盈回撤百分比。
    pub trailing_drawdown_pct: f64,
    /// 相对入场价的价格变化百分比。
    pub price_change_from_entry_pct: f64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BscEventArbDecision {
    /// 动作类型。
    pub action: BscEventArbAction,
    /// 列表数据。
    pub reasons: Vec<String>,
}
impl BscEventArbDecision {
    pub fn has_reason(&self, reason: &str) -> bool {
        self.reasons.iter().any(|item| item == reason)
    }
    /// 将内部模型转换为输出结构，避免 回测与策略研究 的内部字段直接外泄。
    pub fn to_signal(&self, price: f64, ts: i64) -> SignalResult {
        let mut signal = SignalResult {
            open_price: price,
            ts,
            filter_reasons: self.reasons.clone(),
            single_result: Some(self.result_payload().to_string()),
            ..Default::default()
        };
        match self.action {
            BscEventArbAction::Long => self.apply_long_signal(&mut signal, price),
            BscEventArbAction::ForceExit => self.apply_force_exit_signal(&mut signal),
            BscEventArbAction::Flat => {}
        }
        signal
    }
    /// 构建结果载荷，集中维护回测策略的字段组装规则。
    fn result_payload(&self) -> Value {
        json!({
            "strategy": "bsc_event_arb",
            "action": self.action_name(),
            "reasons": self.reasons,
        })
    }
    /// 构建动作名称，集中维护回测策略的字段组装规则。
    fn action_name(&self) -> &'static str {
        match self.action {
            BscEventArbAction::Long => "long",
            BscEventArbAction::Flat => "flat",
            BscEventArbAction::ForceExit => "force_exit",
        }
    }
    /// 执行 回测与策略研究 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    fn apply_long_signal(&self, signal: &mut SignalResult, price: f64) {
        signal.should_buy = true;
        signal.direction = SignalDirection::Long;
        signal.signal_kline_stop_loss_price = Some(price * 0.90);
        signal.long_signal_take_profit_price = Some(price * 1.25);
        signal.atr_take_profit_level_1 = Some(price * 1.25);
        signal.atr_take_profit_level_2 = Some(price * 1.60);
        signal.dynamic_adjustments = vec!["BSC_EVENT_ARB_EVENT_LONG".to_string()];
    }
    /// 执行 回测与策略研究 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    fn apply_force_exit_signal(&self, signal: &mut SignalResult) {
        signal.direction = SignalDirection::Close;
        signal.dynamic_adjustments = vec!["BSC_EVENT_ARB_FORCE_EXIT".to_string()];
    }
}
