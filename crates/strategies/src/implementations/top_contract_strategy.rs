use std::fmt::{Display, Formatter};
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use log::info;
use redis::Commands;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};

// 使用 async_trait 宏 use crate::trading;
// Import necessary crates and modules
use crate::trading::model::big_data::top_contract_account_ratio::TopContractAccountRatioEntity;
use crate::trading::model::big_data::top_contract_position_ratio::TopContractPositionRatioEntity;
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::trading::services::big_data::big_data_top_contract_service::BigDataTopContractService;
use crate::trading::services::big_data::big_data_top_position_service::BigDataTopPositionService;
use crate::trading::strategy::strategy_common::{
    run_back_test, BackTestResult, SignalResult, TradeRecord,
};
use crate::trading::task::basic;
use crate::trading::task::basic::save_log;

use super::strategy_common::BasicRiskStrategyConfig;

// Define the TopContractData struct with corrected types
#[derive(Deserialize, Serialize, Debug, Clone)]
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
    #[serde(with = "serde_arc")]
    pub data: Arc<TopContractData>,
    pub key_value: f64,
    pub atr_period: usize,
    pub heikin_ashi: bool,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct TopContractStrategyConfig {
    pub basic_ratio: f64,
}
impl Display for TopContractStrategyConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "basic_ratio:{}", self.basic_ratio)
    }
}

// 模块实现 Arc 的自定义序列化和反序列化

mod serde_arc {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<T, S>(data: &Arc<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        T::serialize(data, serializer)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Arc<T>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let data = T::deserialize(deserializer)?;
        Ok(Arc::new(data))
    }
}

impl TopContractStrategy {
    pub async fn new(inst_id: &str, time: &str) -> anyhow::Result<TopContractData> {
        //压测的时候需要跳过最新的一条数据
        //获取到精英交易员仓位和人数的比例
        let account_ratio_list =
            BigDataTopContractService::get_list_by_time(inst_id, time, Some(1), 1400, None).await?;

        let position_ratio_list =
            BigDataTopPositionService::get_list_by_time(inst_id, time, Some(1), 1400, None).await?;

        if account_ratio_list.len() != position_ratio_list.len() {
            error!("数据长度异常 acct_ratio_list,position_ratio_list");
        }
        // 获取K线数据
        let mysql_candles: Vec<CandlesEntity> =
            basic::get_candle_data_confirm(inst_id, time, account_ratio_list.len(), None).await?;

        // println!("{:#?}", mysql_candles);

        // 确保有数据
        if mysql_candles.is_empty() {
            return Err(anyhow!("未获取到k线数据"));
        }

        //判断数据准确性
        if mysql_candles.len() != account_ratio_list.len()
            || account_ratio_list.len() != position_ratio_list.len()
            || mysql_candles.get(0).unwrap().ts != account_ratio_list.get(0).unwrap().ts
            || account_ratio_list.get(0).unwrap().ts != position_ratio_list.get(0).unwrap().ts
        {
            return Err(anyhow!("数据长度或时间不匹配 {} {}", inst_id, time));
        }

        println!("position res{:?}", position_ratio_list);
        Ok(TopContractData {
            candle_list: mysql_candles,
            account_ratio: account_ratio_list,
            position_ratio: position_ratio_list,
        })
    }

    /// Generates trade signals based on the provided data.
    pub fn get_trade_signal(&self, data: &TopContractSingleData) -> SignalResult {
        let mut should_buy = false;
        let mut should_sell = false;
        let mut price = data.candle_list.c.clone().parse::<f64>().unwrap();
        let mut ts = data.candle_list.ts;

        if data.account_ratio.ts != data.position_ratio.ts
            || data.position_ratio.ts != data.candle_list.ts
        {
            error!("时间数据不匹配！");
            return SignalResult {
                should_buy,
                should_sell,
                open_price: price,
                signal_kline_stop_loss_price: None,
                best_take_profit_price: None,
                ts,
                single_value: None,
                single_result: None,
                best_open_price: None,
            };
        }
        let acct_ratio = &data.account_ratio;
        //看多人数比例>1
        let mut is_more_acct = false;
        let ratio = (&acct_ratio.long_short_acct_ratio).parse::<f64>().unwrap();
        if (ratio - self.key_value).is_sign_positive() {
            is_more_acct = true;
            // println!("acct ratio >1 :{}", ratio);
        }
        let postion_ratio = &data.position_ratio;
        let mut is_more_postion = false;
        //判断仓位多空比>1
        let ratio = (&postion_ratio.long_short_pos_ratio)
            .parse::<f64>()
            .unwrap();
        if (ratio - self.key_value).is_sign_positive() {
            is_more_postion = true;
            // println!("post ratio >1 {}", ratio);
        }

        if is_more_acct {
            //可能的含义：资金量较大的交易者看多，散户情绪偏空。
            // 若这些“大资金”在市场上有更强的定价影响力，一般可视为“主力在看多”，但并不一定马上就会涨，
            // 需要结合市场行情、成交量、情绪面等更多信息来综合判断。
            should_buy = true;
            println!("ts: {}仓位比大于1，但是人数比小于1，主力看多,散户情绪偏空{} acc_ratio:{} position_ratio:{}", crate::time_util::mill_time_to_local_datetime(ts), ratio, acct_ratio.long_short_acct_ratio, postion_ratio.long_short_pos_ratio);
        }

        if !is_more_acct {
            //可能的含义：大资金或所谓的“主力”看空，而散户更多地在做多。
            // 部分交易者会视“主力”方向为参考，倾向跟随仓位规模更大的方向操作。
            // 但也并非绝对，需要结合行情阶段、波动率、成交量以及是否有消息面利好/利空等来判断。
            should_sell = true;
            println!(
                "ts: {}仓位比<1，但是人数比>1，主力看空,散户看多{} acc_ratio:{} position_ratio:{}",
                crate::time_util::mill_time_to_local_datetime(ts),
                ratio,
                acct_ratio.long_short_acct_ratio,
                postion_ratio.long_short_pos_ratio
            );
        }

        price = data.candle_list.c.clone().parse().unwrap();
        ts = data.candle_list.ts.clone();

        SignalResult {
            should_buy,
            should_sell,
            open_price: price,
            best_take_profit_price: None,
            signal_kline_stop_loss_price: None,
            ts,
            single_value: None,
            single_result: None,
            best_open_price: None,
        }
    }
    /// Runs the backtest asynchronously.
    pub async fn run_test(
        &self,
        fib_levels: &Vec<f64>,
        max_loss_percent: f64,
        is_need_fibonacci_profit: bool,
        is_open_long: bool,
        is_open_short: bool,
        is_jude_trade_time: bool,
    ) -> BackTestResult {
        // Determine the minimum data length required for the backtest
        let min_data_length = Self::get_min_data_length(&self.data);

        // // Execute the external run_test function with appropriate parameters
        // let res = run_test(
        //     |candles| {
        //         // Generate trade signals using the strategy
        //         self.get_trade_signal(&candles)
        //     },
        //     Self::extract_candle_data(&self.data), // Extract candle data from generic data D
        //     fib_levels,
        //     TradingStrategyConfig::default(),
        //     min_data_length,
        //     is_open_long,
        //     is_open_short,
        //     is_jude_trade_time,
        // ); // Await the asynchronous run_test function

        let res = BackTestResult {
            funds: 0.0,
            win_rate: 0.0,
            open_trades: 0,
            trade_records: vec![],
        };
        res // Return the result of the backtest
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
}
