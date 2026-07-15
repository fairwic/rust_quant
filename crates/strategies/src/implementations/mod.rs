// 具体策略实现模块
// 通用执行器和辅助模块
// ✅ executor_common 已使用 trait 解耦，不再有循环依赖
pub mod executor_common;
// executor_common_lite 保留用于不需要完整功能的场景
pub mod executor_common_lite;
pub mod profit_stop_loss;
// 具体策略实现
pub mod engulfing_strategy;
// 执行器
pub mod bb_rsi_strategy;
pub mod bear_short_stack;
pub mod bsc_event_arb;
pub mod btc_eth_liquidity_scalper;
pub mod keltner_channel_scalper;
// pub mod grid_scalper;  // 临时禁用：导入问题
pub mod momentum_breakout_scalper;
pub mod nwe_executor;
pub mod range_breakout_drop;
pub mod range_reversion_scalper;
pub mod rsi_divergence_strategy;
pub mod smart_money_concepts;
pub mod supertrend_strategy;
pub mod vegas_backtest;
pub mod vegas_executor;
// NWE 策略子模块
pub mod nwe_strategy;
pub mod pa_quant_tree;
// 重新导出
pub use bb_rsi_strategy::*;
pub use bear_short_stack::*;
pub use bsc_event_arb::*;
pub use btc_eth_liquidity_scalper::*;
pub use engulfing_strategy::*;
pub use executor_common::*;
pub use executor_common_lite::ExecutionContext as LiteExecutionContext; // 避免冲突
                                                                        // pub use grid_scalper::*;  // 临时禁用
pub use keltner_channel_scalper::*;
pub use momentum_breakout_scalper::*;
pub use nwe_executor::*;
pub use pa_quant_tree::*;
pub use profit_stop_loss::*;
pub use range_breakout_drop::*;
pub use range_reversion_scalper::*;
pub use rsi_divergence_strategy::*;
pub use smart_money_concepts::*;
pub use supertrend_strategy::*;
pub use vegas_backtest::*;
pub use vegas_executor::*;
