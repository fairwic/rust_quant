//! Shadow Trading（影子交易）模块
//!
//! 用于模拟被过滤信号的理论盈亏，验证过滤策略的有效性。
//!
//! ## 功能
//! - 跟踪被过滤的信号，模拟其假设入场后的表现
//! - 计算理论最大盈亏和最终盈亏
//! - 支持Long/Short双向

use super::types::{FilteredSignal, ShadowTrade, SignalResult};
use crate::framework::types::TradeSide;
use crate::CandleItem;

// ============================================================================
// Shadow Trade Manager
// ============================================================================

/// Shadow Trading 管理器
///
/// 管理影子交易的创建、更新和结算
#[derive(Debug)]
pub struct ShadowTradeManager {
    /// 活跃的影子交易
    trades: Vec<ShadowTrade>,
    /// 过滤信号记录
    filtered_signals: Vec<FilteredSignal>,
}

impl ShadowTradeManager {
    /// 创建新的管理器
    pub fn new() -> Self {
        Self {
            trades: Vec::new(),
            filtered_signals: Vec::new(),
        }
    }

    /// 获取过滤信号记录（消费所有权）
    pub fn into_filtered_signals(self) -> Vec<FilteredSignal> {
        self.filtered_signals
    }

    /// 处理过滤信号，创建影子交易
    pub fn process_filtered_signal(
        &mut self,
        signal: &SignalResult,
        candle: &CandleItem,
        inst_id: &str,
    ) {
        use rust_quant_domain::SignalDirection;

        // 确定信号方向
        let direction = match signal.direction {
            SignalDirection::Long => Some(TradeSide::Long),
            SignalDirection::Short => Some(TradeSide::Short),
            SignalDirection::Close => None,
            SignalDirection::None => {
                if signal.should_buy {
                    Some(TradeSide::Long)
                } else if signal.should_sell {
                    Some(TradeSide::Short)
                } else {
                    None
                }
            }
        };

        let Some(direction) = direction else {
            return;
        };

        let direction_str = match direction {
            TradeSide::Long => "LONG",
            TradeSide::Short => "SHORT",
        };

        // 创建 FilteredSignal 记录
        self.filtered_signals.push(FilteredSignal {
            ts: candle.ts,
            inst_id: inst_id.to_string(),
            direction: direction_str.to_string(),
            signal_price: candle.c,
            filter_reasons: signal.filter_reasons.clone(),
            indicator_snapshot: "{}".to_string(), // TODO: 序列化指标快照
            theoretical_profit: 0.0,
            theoretical_loss: 0.0,
            final_pnl: 0.0,
            trade_result: "RUNNING".to_string(),
            signal_value: signal.single_value.clone(),
        });

        // 创建 ShadowTrade
        let signal_index = self.filtered_signals.len() - 1;
        let entry_price = candle.c;

        // 确定止损价格
        let sl_price = signal
            .atr_stop_loss_price
            .or(signal.signal_kline_stop_loss_price);

        // 确定止盈价格
        let tp_price = if direction == TradeSide::Long {
            signal
                .atr_take_profit_ratio_price
                .or(signal.long_signal_take_profit_price)
        } else {
            signal
                .atr_take_profit_ratio_price
                .or(signal.short_signal_take_profit_price)
        };

        self.trades.push(ShadowTrade {
            signal_index,
            entry_price,
            direction,
            sl_price,
            tp_price,
            entry_time: candle.ts,
            max_unrealized_profit: 0.0,
            max_unrealized_loss: 0.0,
        });
    }

    /// 更新所有活跃的影子交易
    ///
    /// 检查止盈止损条件，更新浮盈浮亏
    pub fn update_trades(&mut self, candle: &CandleItem) {
        let current_high = candle.h;
        let current_low = candle.l;

        let mut completed_indices = Vec::new();

        for (idx, trade) in self.trades.iter_mut().enumerate() {
            let exit_result =
                Self::check_trade_exit(trade, current_high, current_low, &self.filtered_signals);

            if let Some((pnl, result_str)) = exit_result {
                if let Some(signal) = self.filtered_signals.get_mut(trade.signal_index) {
                    signal.final_pnl = pnl;
                    signal.theoretical_loss = trade.max_unrealized_loss;
                    signal.theoretical_profit = trade.max_unrealized_profit;
                    signal.trade_result = result_str.to_string();
                }
                completed_indices.push(idx);
            }
        }

        // 移除已完成的影子交易（从后往前移除）
        for idx in completed_indices.iter().rev() {
            self.trades.remove(*idx);
        }
    }

    /// 检查单个交易的出场条件
    ///
    /// 返回 `Some((pnl, result_str))` 表示交易结束
    fn check_trade_exit(
        trade: &mut ShadowTrade,
        current_high: f64,
        current_low: f64,
        _filtered_signals: &[FilteredSignal],
    ) -> Option<(f64, &'static str)> {
        match trade.direction {
            TradeSide::Long => {
                // 更新浮盈浮亏
                let max_profit = (current_high - trade.entry_price) / trade.entry_price;
                let max_loss = (current_low - trade.entry_price) / trade.entry_price;
                trade.max_unrealized_profit = trade.max_unrealized_profit.max(max_profit);
                trade.max_unrealized_loss = trade.max_unrealized_loss.min(max_loss);

                // 检查止损
                if let Some(sl) = trade.sl_price {
                    if current_low <= sl {
                        let pnl = (sl - trade.entry_price) / trade.entry_price;
                        return Some((pnl, "LOSS"));
                    }
                }

                // 检查止盈
                if let Some(tp) = trade.tp_price {
                    if current_high >= tp {
                        let pnl = (tp - trade.entry_price) / trade.entry_price;
                        return Some((pnl, "WIN"));
                    }
                }
            }
            TradeSide::Short => {
                // 更新浮盈浮亏
                let max_profit = (trade.entry_price - current_low) / trade.entry_price;
                let max_loss = (trade.entry_price - current_high) / trade.entry_price;
                trade.max_unrealized_profit = trade.max_unrealized_profit.max(max_profit);
                trade.max_unrealized_loss = trade.max_unrealized_loss.min(max_loss);

                // 检查止损
                if let Some(sl) = trade.sl_price {
                    if current_high >= sl {
                        let pnl = (trade.entry_price - sl) / trade.entry_price;
                        return Some((pnl, "LOSS"));
                    }
                }

                // 检查止盈
                if let Some(tp) = trade.tp_price {
                    if current_low <= tp {
                        let pnl = (trade.entry_price - tp) / trade.entry_price;
                        return Some((pnl, "WIN"));
                    }
                }
            }
        }

        None
    }

    /// 结束所有剩余的影子交易（回测结束时调用）
    pub fn finalize(&mut self, last_candle: &CandleItem) {
        for trade in self.trades.drain(..) {
            if let Some(signal) = self.filtered_signals.get_mut(trade.signal_index) {
                let current_close = last_candle.c;

                // 计算最终盈亏
                let pnl = match trade.direction {
                    TradeSide::Long => (current_close - trade.entry_price) / trade.entry_price,
                    TradeSide::Short => (trade.entry_price - current_close) / trade.entry_price,
                };

                // 更新最大浮盈浮亏
                let (max_profit, max_loss) = match trade.direction {
                    TradeSide::Long => (
                        ((last_candle.h - trade.entry_price) / trade.entry_price)
                            .max(trade.max_unrealized_profit),
                        ((last_candle.l - trade.entry_price) / trade.entry_price)
                            .min(trade.max_unrealized_loss),
                    ),
                    TradeSide::Short => (
                        ((trade.entry_price - last_candle.l) / trade.entry_price)
                            .max(trade.max_unrealized_profit),
                        ((trade.entry_price - last_candle.h) / trade.entry_price)
                            .min(trade.max_unrealized_loss),
                    ),
                };

                signal.final_pnl = pnl;
                signal.theoretical_profit = max_profit;
                signal.theoretical_loss = max_loss;
                signal.trade_result = "END".to_string();
            }
        }
    }
}

impl Default for ShadowTradeManager {
    fn default() -> Self {
        Self::new()
    }
}
