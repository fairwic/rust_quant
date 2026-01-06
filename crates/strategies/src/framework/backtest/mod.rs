pub mod adapter;
pub mod conversions;
pub mod types;
pub mod utils;
pub mod fibonacci;
pub mod indicators;
pub mod engine;
pub mod risk;
pub mod position;
pub mod signal;
pub mod recording;
pub mod trait_impl;
pub mod r_system;

// 重新导出常用类型
pub use adapter::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
pub use conversions::{convert_domain_signal, to_domain_basic_risk_config};
pub use types::{
    BackTestResult, BasicRiskStrategyConfig, MoveStopLoss, SignalResult, TradePosition,
    TradeRecord, TradingState,
};
pub use utils::{calculate_profit_loss, calculate_win_rate, parse_candle_to_data_item, parse_price};
pub use indicators::{calculate_ema, get_multi_indicator_values};
pub use engine::{run_back_test, run_back_test_generic};
pub use risk::{
    check_risk_config, check_risk_config_with_r_system, init_r_system_state, RSystemRiskConfig,
    RSystemRuntime,
};
pub use position::{close_position, finalize_trading_state, open_long_position, open_short_position};
pub use signal::deal_signal;
pub use recording::{record_trade_entry, record_trade_exit};
pub use trait_impl::BackTestAbleStrategyTrait;
pub use r_system::{
    RSystemConfig, RSystemState, StopLossLevel, TieredTakeProfitConfig, TieredTakeProfitState,
    TimeStopConfig, TimeStopAction, TakeProfitAction, update_r_system_trailing_stop,
    check_tiered_take_profit, check_time_stop, create_r_state_from_position,
    update_position_stop_from_r_state,
};

