// Import necessary crates and modules
use crate::trading::model::big_data::top_contract_account_ratio::TopContractAccountRatioEntity;
use crate::trading::model::big_data::top_contract_position_ratio::TopContractPositionRatioEntity;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::strategy::strategy_common::{
    run_test, run_test_top_contract, SignalResult, StrategyCommonTrait, TradeRecord,
};
use async_trait::async_trait;
use redis::Commands;
// 使用 async_trait 宏
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::warn;

// Define the TopContractData struct with corrected types
#[derive(Deserialize, Serialize, Debug,Clone)]
pub struct TopContractData {
    pub candle_list: Vec<CandlesEntity>,
    pub account_ratio: Vec<TopContractAccountRatioEntity>,
    pub position_ratio: Vec<TopContractPositionRatioEntity>, // Corrected type
}

#[derive(Deserialize, Serialize, Debug)]
pub struct TopContractSingleData {
    pub candle_list: CandlesEntity,
    pub account_ratio: TopContractAccountRatioEntity,
    pub position_ratio: TopContractPositionRatioEntity, // Corrected type
}

// Define the TopContractStrategy struct with generic parameters D and S
#[derive(Deserialize, Serialize, Debug)]
pub struct TopContractStrategy {
    pub data: TopContractData,
    pub key_value: f64,
    pub atr_period: usize,
    pub heikin_ashi: bool,
}

// Implement the StrategyCommonTrait for TopContractStrategy using async_trait
impl TopContractStrategy {
    /// Generates trade signals based on the provided data.
    pub fn get_trade_signal(
        data: &TopContractSingleData, // 使用引用类型，与特征声明一致
    ) -> SignalResult {
        let mut should_buy = false;
        let mut should_sell = false;
        let mut price = 0.00;
        let mut ts = 11111111;

        if data.account_ratio.ts != data.position_ratio.ts || data.position_ratio.ts != data.candle_list.ts {
            warn!("时间数据不匹配！");
            return SignalResult {
                should_buy,
                should_sell,
                price,
                ts,
            };
        }
        let acct_ratio = &data.account_ratio;
        //看多人数比例>1
        let mut is_more_acct = false;
        let ratio = (&acct_ratio.long_short_acct_ratio).parse::<f64>().unwrap();
        println!("acct ratio:{}", ratio);
        println!("acct ratio -1:{}", (ratio - 1.00).abs());
        if (ratio - 1.00).abs() < f64::EPSILON {
            is_more_acct = true;
            println!("a and b are approximately equal.");
        }
        let postion_ratio = &data.position_ratio;
        let mut is_more_postion = false;
        //判断仓位多空比>1
        let ratio = (&postion_ratio.long_short_pos_ratio)
            .parse::<f64>()
            .unwrap();

        println!("pos ratio:{}", ratio);
        println!("pos ratio -1:{}", (ratio - 1.00).abs());

        if (ratio - 1.00).abs() < f64::EPSILON {
            is_more_postion = true;
            println!("a and b are approximately equal.");
        }
        if is_more_postion && !is_more_acct {
            //可能的含义：资金量较大的交易者看多，散户情绪偏空。
            // 若这些“大资金”在市场上有更强的定价影响力，一般可视为“主力在看多”，但并不一定马上就会涨，
            // 需要结合市场行情、成交量、情绪面等更多信息来综合判断。
            should_buy = true;
        }
        if !is_more_postion == false && is_more_acct {
            //可能的含义：大资金或所谓的“主力”看空，而散户更多地在做多。
            // 部分交易者会视“主力”方向为参考，倾向跟随仓位规模更大的方向操作。
            // 但也并非绝对，需要结合行情阶段、波动率、成交量以及是否有消息面利好/利空等来判断。
            should_sell = true;
        }
        price = data.candle_list.c.clone().parse().unwrap();
        ts = data.candle_list.ts.clone();

        SignalResult {
            should_buy,
            should_sell,
            price,
            ts,
        }
    }

    // pub fn get_trade_signal(
    //     data: &TopContractSingleData, // 使用引用类型，与特征声明一致
    //     key_value: f64,
    //     atr_period: usize,
    //     heikin_ashi: bool,
    // ) -> SignalResult {
    //     let mut should_buy = false;
    //     let mut should_sell = false;
    //     let mut price = 0.00;
    //     let mut ts = 11111111;
    //
    //     if data.account_ratio.len() != data.position_ratio.len()
    //         || data.position_ratio.len() != data.candle_list.len()
    //     {
    //         warn!("数据长度不相等！");
    //         return SignalResult {
    //             should_buy,
    //             should_sell,
    //             price,
    //             ts,
    //         };
    //     }
    //     for (i, candle_item) in data.candle_list.iter().enumerate() {
    //         let acct_ratio = data.account_ratio.get(i).unwrap();
    //         //看多人数比例>1
    //         let mut is_more_acct = false;
    //         let ratio = (&acct_ratio.long_short_acct_ratio).parse::<f64>().unwrap();
    //         println!("acct ratio:{}", ratio);
    //         println!("acct ratio -1:{}", (ratio - 1.00).abs());
    //         if (ratio - 1.00).abs() < f64::EPSILON {
    //             is_more_acct = true;
    //             println!("a and b are approximately equal.");
    //         }
    //         let postion_ratio = data.position_ratio.get(i).unwrap();
    //         let mut is_more_postion = false;
    //         //判断仓位多空比>1
    //         let ratio = (&postion_ratio.long_short_pos_ratio)
    //             .parse::<f64>()
    //             .unwrap();
    //
    //         println!("pos ratio:{}", ratio);
    //         println!("pos ratio -1:{}", (ratio - 1.00).abs());
    //
    //         if (ratio - 1.00).abs() < f64::EPSILON {
    //             is_more_postion = true;
    //             println!("a and b are approximately equal.");
    //         }
    //         if is_more_postion && !is_more_acct {
    //             //可能的含义：资金量较大的交易者看多，散户情绪偏空。
    //             // 若这些“大资金”在市场上有更强的定价影响力，一般可视为“主力在看多”，但并不一定马上就会涨，
    //             // 需要结合市场行情、成交量、情绪面等更多信息来综合判断。
    //             should_buy = true;
    //         }
    //         if !is_more_postion == false && is_more_acct {
    //             //可能的含义：大资金或所谓的“主力”看空，而散户更多地在做多。
    //             // 部分交易者会视“主力”方向为参考，倾向跟随仓位规模更大的方向操作。
    //             // 但也并非绝对，需要结合行情阶段、波动率、成交量以及是否有消息面利好/利空等来判断。
    //             should_sell = true;
    //         }
    //         price = candle_item.c.clone().parse().unwrap();
    //         ts = candle_item.ts.clone()
    //     }
    //
    //     SignalResult {
    //         should_buy,
    //         should_sell,
    //         price,
    //         ts,
    //     }
    // }

    /// Runs the backtest asynchronously.
    pub async fn run_test(
        &self,
        fib_levels: &Vec<f64>,
        max_loss_percent: f64,
        is_need_fibonacci_profit: bool,
        is_open_long: bool,
        is_open_short: bool,
        is_jude_trade_time: bool,
    ) -> (f64, f64, usize, Vec<TradeRecord>) {
        // Determine the minimum data length required for the backtest
        let min_data_length = get_min_data_length(&self.data);

        // Execute the external run_test function with appropriate parameters
        let res = run_test_top_contract(
            |candles| {
                // Generate trade signals using the strategy
                Self::get_trade_signal(candles)
                // Pass a reference to the strategy's data)
            },
            extract_candle_data(&self.data), // Extract candle data from generic data D
            fib_levels,
            max_loss_percent,
            min_data_length,
            is_need_fibonacci_profit,
            is_open_long,
            is_open_short,
            is_jude_trade_time,
        ); // Await the asynchronous run_test function

        res // Return the result of the backtest
    }
}

/// Helper function to determine the minimum data length based on data type D.
/// You need to implement this function based on how different data types store candle data.
fn get_min_data_length<D>(_data: &D) -> usize {
    1 // Default value, replace with actual logic
}

/// Helper function to extract candle data from generic data type D.
/// You need to implement this function based on the structure of different D types.
fn extract_candle_data(data: &TopContractData) -> &TopContractData {
    data
}
