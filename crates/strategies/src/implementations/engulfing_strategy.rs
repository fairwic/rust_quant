use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::error;

use crate::strategy_common::{run_back_test, BackTestResult, SignalResult, TradeRecord};
use rust_quant_common::CandleItem;

use crate::framework::strategy_common::BasicRiskStrategyConfig; // ⭐ 正确路径

#[derive(Deserialize, Serialize, Debug)]
pub struct EngulfingStrategy {
    pub heikin_ashi: bool,
    pub num_bars: usize, // 指定所需的K线数量
}

impl EngulfingStrategy {
    /// 获取交易信号
    pub fn get_trade_signal(candles_5m: &[CandleItem], num_bars: usize) -> SignalResult {
        let mut should_buy = false;
        let mut should_sell = false;
        let mut price = 0.0;
        let mut ts = 0;

        // 确保有足够的K线数据
        if candles_5m.len() == num_bars + 1 {
            let current_candle = &candles_5m[candles_5m.len() - 1];

            let previous_candles =
                &candles_5m[candles_5m.len() - num_bars - 1..candles_5m.len() - 1];

            let current_open = current_candle.o;
            let current_close = current_candle.c;

            let mut all_previous_bearish = true;
            let mut all_previous_bullish = true;

            for previous_candle in previous_candles.iter() {
                let previous_open = previous_candle.o;
                let previous_close = previous_candle.c;

                // 检查之前的K线是否全部是看跌的,有一个不符合就标记为false
                if previous_close >= previous_open {
                    all_previous_bearish = false;
                }
                // 检查之前的K线是否全部是看涨的,有一个不符合就标记为false
                if previous_close <= previous_open {
                    // println!("zzzz previous_candle= {:#?}", previous_candle);
                    all_previous_bullish = false;
                }
            }
            // println!("all_previous_bearish= {:#?}", all_previous_bearish);
            // println!("all_previous_bullish= {:#?}", all_previous_bullish);
            // 牛市吞没形态条件
            if all_previous_bearish
                && current_close > previous_candles[previous_candles.len() - 1].o
            // &&current_
            {
                should_buy = true;
            }

            // 熊市吞没形态条件
            if all_previous_bullish
                && current_close < previous_candles[previous_candles.len() - 1].o
            {
                should_sell = true;
            }

            price = current_close;
            ts = current_candle.ts;
        } else {
            error!("engulfingStrategy run_test candles_5m.len() < num_bar")
        }

        let last_ts = match candles_5m.last() {
            Some(c) => c.ts,
            None => 0,
        };

        SignalResult {
            should_buy,
            should_sell,
            open_price: price,
            ts: if ts != 0 { ts } else { last_ts },
            single_value: None,
            single_result: None,
            signal_kline_stop_loss_price: None,
            atr_stop_loss_price: None,
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            move_stop_open_price_when_touch_price: None,
            counter_trend_pullback_take_profit_price: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            filter_reasons: vec![],
            direction: rust_quant_domain::SignalDirection::None,
        }
    }

    // /// 运行回测
    // pub async fn run_test(
    //     candles_5m: &Vec<CandlesEntity>,
    //     fib_levels: &Vec<f64>,
    //     max_loss_percent: f64,
    //     num_bars: usize,
    //     is_need_fibonacci_profit: bool,
    //     is_open_long: bool,
    //     is_open_short: bool,
    //     is_judge_trade_time: bool,
    // ) ->BackTestResult {
    //     let min_data_length = num_bars + 1;
    //     let res = run_test(
    //         |candles| Self::get_trade_signal(candles, num_bars),
    //         candles_5m,
    //         fib_levels,
    //         TradingStrategyConfig::default(),
    //         min_data_length,
    //         is_open_long,
    //         is_open_short,
    //         is_judge_trade_time,
    //     );
    //     // println!("res= {:#?}", json!(res));
    //     res
    // }
}
