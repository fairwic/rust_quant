//! Pipeline上下文定义
//!
//! 集中管理回测过程中的所有状态，避免状态在函数间隐式传递

use crate::framework::backtest::shadow_trading::ShadowTradeManager;
use crate::framework::backtest::types::{
    BasicRiskStrategyConfig, SignalResult, TradePosition, TradingState,
};
use crate::CandleItem;

/// 回测Pipeline上下文
///
/// 统一状态容器，各Stage通过修改Context实现状态传递
#[derive(Debug)]
pub struct BacktestContext {
    // ========================================================================
    // 输入数据
    // ========================================================================
    /// 当前K线
    pub candle: CandleItem,

    /// 当前K线索引
    pub candle_index: usize,

    /// 交易对标识
    pub inst_id: String,

    // ========================================================================
    // 策略配置
    // ========================================================================
    /// 风控配置
    pub risk_config: BasicRiskStrategyConfig,

    // ========================================================================
    // 中间状态（各Stage产生/消费）
    // ========================================================================
    /// 当前信号（SignalStage产出）
    pub signal: Option<SignalResult>,

    /// 信号是否被过滤（FilterStage设置）
    pub is_signal_filtered: bool,

    /// 过滤原因
    pub filter_reasons: Vec<String>,

    // ========================================================================
    // 持久状态
    // ========================================================================
    /// 交易状态
    pub trading_state: TradingState,

    /// 当前仓位（从trading_state同步）
    pub current_position: Option<TradePosition>,

    /// Shadow Trading 管理器（用于收集 filtered_signals 且对齐 legacy engine 行为）
    pub shadow_manager: ShadowTradeManager,

    // ========================================================================
    // 控制标志
    // ========================================================================
    /// 是否在本K线执行了开仓
    pub opened_position: bool,

    /// 是否在本K线执行了平仓
    pub closed_position: bool,

    /// 平仓原因
    pub close_reason: Option<String>,
}

impl BacktestContext {
    /// 创建新的上下文
    pub fn new(
        candle: CandleItem,
        candle_index: usize,
        inst_id: String,
        risk_config: BasicRiskStrategyConfig,
        trading_state: TradingState,
    ) -> Self {
        let current_position = trading_state.trade_position.clone();
        Self {
            candle,
            candle_index,
            inst_id,
            risk_config,
            signal: None,
            is_signal_filtered: false,
            filter_reasons: Vec::new(),
            trading_state,
            current_position,
            shadow_manager: ShadowTradeManager::new(),
            opened_position: false,
            closed_position: false,
            close_reason: None,
        }
    }

    /// 重置单K线相关状态（用于下一根K线）
    pub fn reset_for_next_candle(&mut self, candle: CandleItem, candle_index: usize) {
        self.candle = candle;
        self.candle_index = candle_index;
        self.signal = None;
        self.is_signal_filtered = false;
        self.filter_reasons.clear();
        self.opened_position = false;
        self.closed_position = false;
        self.close_reason = None;
        // 同步仓位状态
        self.current_position = self.trading_state.trade_position.clone();
    }

    /// 检查是否有持仓
    #[inline]
    pub fn has_position(&self) -> bool {
        self.current_position.is_some()
    }

    /// 检查是否有有效信号
    #[inline]
    pub fn has_signal(&self) -> bool {
        self.signal
            .as_ref()
            .map(|s| s.should_buy || s.should_sell)
            .unwrap_or(false)
    }
}
