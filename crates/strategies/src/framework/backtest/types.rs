//! # 回测框架核心类型
//!
//! 该模块定义了回测系统所需的核心数据结构，按功能分为以下几组：
//!
//! ## 结果类型
//! - [`BackTestResult`] - 回测整体结果
//! - [`TradeRecord`] - 单笔交易记录
//! - [`FilteredSignal`] - 被过滤信号记录
//!
//! ## 信号类型
//! - [`SignalResult`] - 策略信号输出
//! - [`ShadowTrade`] - 影子交易状态
//!
//! ## 仓位类型
//! - [`TradePosition`] - 持仓详情
//! - [`TradingState`] - 回测交易状态
//!
//! ## 配置类型
//! - [`BasicRiskStrategyConfig`] - 风控配置
//! - [`MoveStopLoss`] - 移动止损

use super::super::types::TradeSide;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ============================================================================
// 回测结果类型
// ============================================================================

/// 回测结果
#[derive(Debug, Deserialize, Serialize)]
pub struct BackTestResult {
    pub funds: f64,
    pub win_rate: f64,
    pub open_trades: usize,
    pub trade_records: Vec<TradeRecord>,
    pub filtered_signals: Vec<FilteredSignal>,
    pub dynamic_config_logs: Vec<DynamicConfigLog>,
}

impl Default for BackTestResult {
    fn default() -> Self {
        BackTestResult {
            funds: 0.0,
            win_rate: 0.0,
            open_trades: 0,
            trade_records: vec![],
            filtered_signals: vec![],
            dynamic_config_logs: vec![],
        }
    }
}

/// 交易记录
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TradeRecord {
    //交易类型
    pub option_type: String,
    //实际开仓时间
    pub open_position_time: String,
    //信号开仓时间
    pub signal_open_position_time: Option<String>,
    //平仓时间
    pub close_position_time: Option<String>,
    //开仓价格
    pub open_price: f64,
    //信号状态
    pub signal_status: i32,
    //平仓价格
    pub close_price: Option<f64>,
    //盈亏
    pub profit_loss: f64,
    //开仓数量
    pub quantity: f64,
    //是否全平
    pub full_close: bool,
    //平仓类型
    pub close_type: String,
    //盈利次数
    pub win_num: i64,
    //亏损次数
    pub loss_num: i64,
    //信号值
    pub signal_value: Option<String>,
    //信号结果
    pub signal_result: Option<String>,
    //止损来源（如 "Engulfing", "KlineHammer" 等）
    pub stop_loss_source: Option<String>,
    //止损更新历史(JSON序列化的Vec<StopLossUpdate>)
    pub stop_loss_update_history: Option<String>,
}

// ============================================================================
// 信号类型
// ============================================================================

/// 信号结果
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SignalResult {
    pub should_buy: bool,
    pub should_sell: bool,
    //开仓价格
    pub open_price: f64,
    //信号k线最高价或者最低价止损
    pub signal_kline_stop_loss_price: Option<f64>,
    /// 止损来源标记（如 "Engulfing", "KlineHammer" 等）
    pub stop_loss_source: Option<String>,
    pub best_open_price: Option<f64>,

    //ATR止盈价格(通常设置为信号线的价差的2倍率) 1:2 1:3 1:4 1:5
    pub atr_take_profit_ratio_price: Option<f64>,
    pub atr_stop_loss_price: Option<f64>,

    //做多指标动态止盈价格，比如当触发nwe突破信号线的时候。或者价格到达布林带的时候
    pub long_signal_take_profit_price: Option<f64>,

    //做空指标动态止盈价格，比如当触发nwe突破信号线的时候。或者价格到达布林带的时候
    pub short_signal_take_profit_price: Option<f64>,
    pub ts: i64,
    pub single_value: Option<String>,
    pub single_result: Option<String>,

    /// 是否均线空头排列（用于判断是否逆势做多）
    pub is_ema_short_trend: Option<bool>,

    /// 是否均线多头排列（用于判断是否逆势做空）
    pub is_ema_long_trend: Option<bool>,

    /// 三级止盈价格（基于ATR倍数）
    /// 第一级：1.5倍ATR，触达后移动止损到开仓价
    pub atr_take_profit_level_1: Option<f64>,
    /// 第二级：2倍ATR，触达后移动止损到第一级止盈价
    pub atr_take_profit_level_2: Option<f64>,
    /// 第三级：5倍ATR，完全平仓
    pub atr_take_profit_level_3: Option<f64>,

    /// 过滤原因（如 MACD_FALLING_KNIFE, RSI_OVERBOUGHT 等）
    pub filter_reasons: Vec<String>,

    /// 动态配置调整标签（如 RANGE_TP_ONE_TO_ONE）
    pub dynamic_adjustments: Vec<String>,
    /// 动态配置快照(JSON)
    pub dynamic_config_snapshot: Option<String>,

    /// 信号方向
    pub direction: rust_quant_domain::SignalDirection,
}

// ============================================================================
// 过滤信号与影子交易类型
// ============================================================================

/// 被过滤的信号记录（用于分析验证）
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FilteredSignal {
    /// 信号生成时间戳
    pub ts: i64,
    /// 交易对
    pub inst_id: String,
    /// 信号方向
    pub direction: String,
    /// 信号价格
    pub signal_price: f64,
    /// 过滤原因
    pub filter_reasons: Vec<String>,
    /// 指标快照 (JSON 字符串)
    pub indicator_snapshot: String,
    /// 理论最大盈利
    pub theoretical_profit: f64,
    /// 理论最大亏损
    pub theoretical_loss: f64,
    /// 最终模拟盈亏
    pub final_pnl: f64,
    /// 交易结果 (WIN, LOSS, BREAK_EVEN)
    pub trade_result: String,
    /// 信号详情 (各指标值的JSON快照)
    pub signal_value: Option<String>,
}

/// 动态配置调整记录（每根K线一次）
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DynamicConfigLog {
    /// K线时间戳
    pub ts: i64,
    /// 动态调整标签
    pub adjustments: Vec<String>,
    /// 动态配置快照(JSON)
    pub config_snapshot: Option<String>,
}

/// 影子交易状态（用于模拟被过滤信号的理论盈亏）
#[derive(Debug, Clone)]
pub struct ShadowTrade {
    /// 对应的 filtered_signals 索引
    pub signal_index: usize,
    /// 入场价格
    pub entry_price: f64,
    /// 交易方向
    pub direction: TradeSide,
    /// 止损价格
    pub sl_price: Option<f64>,
    /// 止盈价格
    pub tp_price: Option<f64>,
    /// 入场时间
    pub entry_time: i64,
    /// 最大浮盈
    pub max_unrealized_profit: f64,
    /// 最大浮亏
    pub max_unrealized_loss: f64,
}

impl Default for SignalResult {
    fn default() -> Self {
        Self {
            should_buy: false,
            should_sell: false,
            open_price: 0.0,
            signal_kline_stop_loss_price: None,
            stop_loss_source: None,
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            ts: 0,
            single_value: None,
            single_result: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::None,
        }
    }
}

// ============================================================================
// 仓位与状态类型
// ============================================================================

/// 持仓信息
#[derive(Debug, Clone, Default)]
pub struct TradePosition {
    //持仓数量
    pub position_nums: f64,
    //实际开仓价格
    pub open_price: f64,
    //实际平仓价格
    pub close_price: Option<f64>,
    //盈亏
    pub profit_loss: f64,
    //斐波那契触发价格
    pub triggered_fib_levels: HashSet<usize>,
    //交易方向
    pub trade_side: TradeSide,
    //是否使用最优开仓价格
    pub is_use_best_open_price: bool,
    //信号开仓时间
    pub signal_open_position_time: Option<String>,
    //实际开仓时间
    pub open_position_time: String,
    //固定比例止盈价格
    pub fixed_take_profit_price: Option<f64>,
    //信号线止损价格
    pub signal_kline_stop_close_price: Option<f64>,
    //atr止损价格
    pub atr_stop_loss_price: Option<f64>,
    //触发atr 盈亏比止盈
    pub atr_take_ratio_profit_price: Option<f64>,
    /// 动态止盈价格（来自策略信号）
    pub long_signal_take_profit_price: Option<f64>,
    pub short_signal_take_profit_price: Option<f64>,

    //触发K线开仓价格止损(当达到一个特定的价格位置的时候，移动止损线到开仓价格)
    pub move_stop_open_price: Option<f64>,
    //信号状态
    pub signal_status: i32,
    //信号线最高最低价差
    pub signal_high_low_diff: f64,

    /// 入场K线振幅比例 (high-low / low)
    pub entry_kline_amplitude: Option<f64>,

    /// 入场K线收盘位置 (0-1)
    pub entry_kline_close_pos: Option<f64>,

    /// 三级止盈价格
    pub atr_take_profit_level_1: Option<f64>,
    pub atr_take_profit_level_2: Option<f64>,
    pub atr_take_profit_level_3: Option<f64>,

    /// 已触达的止盈级别（用于跟踪止盈进度）
    pub reached_take_profit_level: u8,

    /// 止损来源（如 "Engulfing", "KlineHammer" 等）
    pub stop_loss_source: Option<String>,

    /// 止损更新历史
    pub stop_loss_updates: Vec<rust_quant_domain::value_objects::StopLossUpdate>,
}

/// 交易状态
#[derive(Debug, Clone)]
pub struct TradingState {
    //资金
    pub funds: f64,
    //盈利次数
    pub wins: i64,
    //亏损次数
    pub losses: i64,
    //开仓次数
    pub open_position_times: usize,
    //上一次信号结果
    pub last_signal_result: Option<SignalResult>,
    //总盈亏
    pub total_profit_loss: f64,
    //持仓记录
    pub trade_records: Vec<TradeRecord>,
    //交易持仓
    pub trade_position: Option<TradePosition>,
}

impl Default for TradingState {
    fn default() -> Self {
        Self {
            funds: 100.0,
            wins: 0,
            losses: 0,
            open_position_times: 0,
            last_signal_result: None,
            total_profit_loss: 0.0,
            trade_records: Vec::with_capacity(3000),
            trade_position: None,
        }
    }
}

// ============================================================================
// 配置类型
// ============================================================================

/// 止盈止损策略配置
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct BasicRiskStrategyConfig {
    // 最大止损百分比(避免当k线振幅过大，使用k线最低/高价止损时候，造成太大的亏损)
    pub max_loss_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    //(开仓K线止盈止损),多单时,当价格低于入场k线的最低价时,止损;空单时,价格高于入场k线的最高价时,止损
    pub is_used_signal_k_line_stop_loss: Option<bool>,
    //atr止盈比例
    #[serde(skip_serializing_if = "Option::is_none")]
    pub atr_take_profit_ratio: Option<f64>, // atr止盈比例，比如当盈利超过1.5:1时，直接止盈，适用短线策略
    //固定信号线的止盈比例
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_signal_kline_take_profit_ratio: Option<f64>, //固定信号线的止盈比例，比如当盈利超过 k线路的长度的 n 倍时，直接止盈，适用短线策略

    /// 高波动动态降损开关（原先由环境变量 DYNAMIC_MAX_LOSS 控制）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_max_loss: Option<bool>,

    /// 止盈价有效性校验（原先由环境变量 VALIDATE_SIGNAL_TP 控制）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate_signal_tp: Option<bool>,

    /// Vegas 风控收紧开关（原先由环境变量 TIGHTEN_VEGAS_RISK 控制）
    /// - `true`：强制收紧 max_loss_percent，并开启信号K线止损/单K止损/触价保本
    /// - `false/None`：不额外收紧
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tighten_vegas_risk: Option<bool>,
}

impl Default for BasicRiskStrategyConfig {
    fn default() -> Self {
        Self {
            max_loss_percent: 0.02, // 默认2%止损
            is_used_signal_k_line_stop_loss: Some(true),
            atr_take_profit_ratio: Some(0.00), // 默认1%盈利开始启用动态止盈
            fixed_signal_kline_take_profit_ratio: Some(0.00), // 默认不使用固定信号线的止盈
            dynamic_max_loss: Some(true),
            validate_signal_tp: Some(false),
            tighten_vegas_risk: Some(false),
        }
    }
}

/// 移动止损
pub struct MoveStopLoss {
    pub is_long: bool,
    pub is_short: bool,
    pub price: f64,
}

#[cfg(test)]
mod tests {
    use super::{BasicRiskStrategyConfig, SignalResult};

    #[test]
    fn signal_result_has_no_counter_trend_field() {
        let value =
            serde_json::to_value(SignalResult::default()).expect("serialize SignalResult");
        assert!(value
            .get("counter_trend_pullback_take_profit_price")
            .is_none());
    }

    #[test]
    fn risk_config_has_no_move_or_one_k_flags() {
        let value = serde_json::to_value(BasicRiskStrategyConfig::default())
            .expect("serialize BasicRiskStrategyConfig");
        assert!(value.get("is_one_k_line_diff_stop_loss").is_none());
        assert!(value
            .get("is_move_stop_open_price_when_touch_price")
            .is_none());
    }
}
