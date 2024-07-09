use std::cmp::PartialEq;
use std::env;
use std::sync::Arc;
use anyhow::anyhow;
use chrono::{DateTime, Local, Timelike, TimeZone, Utc};
use hmac::digest::generic_array::arr;
use tracing::{error, info, Level, span, warn};

use crate::{time_util, trading};
use crate::config::db;
use crate::trading::model::market::candles;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::model::order::swap_order::SwapOrderEntityModel;
use crate::trading::model::strategy::{back_test_detail, back_test_log, strategy_job_signal_log};
use crate::trading::model::strategy::back_test_detail::BackTestDetail;
use crate::trading::model::strategy::back_test_log::BackTestLog;
use crate::trading::model::strategy::strategy_config::*;
use crate::trading::model::strategy::strategy_job_signal_log::StrategyJobSignalLog;
use crate::trading::okx::account::{Account, Position, PositionResponse};
use crate::trading::okx::trade::{PosSide, TdMode};
use crate::trading::order;
use crate::trading::strategy;
use crate::trading::strategy::{engulfing_strategy, StopLossStrategy, Strategy, StrategyType};
use crate::trading::strategy::comprehensive_strategy::ComprehensiveStrategy;
use crate::trading::strategy::macd_kdj_strategy::MacdKdjStrategy;
use crate::trading::strategy::profit_stop_loss::ProfitStopLoss;
use crate::trading::strategy::strategy_common::SignalResult;
use crate::trading::strategy::ut_boot_strategy::{TradeRecord, UtBootStrategy};

pub mod tickets_job;
pub mod account_job;
pub mod asset_job;
pub(crate) mod candles_job;
pub mod trades_job;


/** 同步数据 任务**/
pub async fn run_sync_data_job(inst_ids: &Vec<&str>, tims: &Vec<&str>) -> Result<(), anyhow::Error> {
    let span = span!(Level::DEBUG, "run_sync_data_job");
    let _enter = span.enter();

    candles_job::init_create_table(Some(&inst_ids), Some(&tims)).await.expect("init create_table error");
    candles_job::init_all_candles(Some(&inst_ids), Some(&tims)).await?;
    candles_job::init_before_candles(Some(&inst_ids), Some(tims.clone())).await?;
    Ok(())
}

pub async fn run_set_leverage(inst_ids: &Vec<&str>) -> Result<(), anyhow::Error> {
    let span = span!(Level::DEBUG, "run_set_leverage");
    let _enter = span.enter();
    for inst_id in inst_ids.iter() {
        let mut level = 10;
        if inst_id == &"BTC-USDT-SWAP" {
            level = 20;
        } else if inst_id == &"ETH-USDT-SWAP" {
            level = 15;
        }

        for post_side in [PosSide::LONG, PosSide::SHORT] {
            let params = trading::okx::account::SetLeverageRequest {
                inst_id: Some(inst_id.to_string()),
                ccy: None,
                mgn_mode: TdMode::ISOLATED.to_string(),
                lever: level.to_string(),
                pos_side: Some(post_side.to_string()),
            };
            //延迟100ms
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            Account::set_leverage(params).await?;
        }
    }
    Ok(())
}

pub async fn breakout_long_test(mysql_candles_5m: Vec<CandlesEntity>, inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");

    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(db::get_db_client(), con);

    for breakout_period in 1..20 {
        for confirmation_period in 1..20 {
            let volume_threshold_range: Vec<f64> = (0..=20).map(|x| x as f64 * 0.1).collect(); // 从0.1到2.0，每步0.1
            for volume_threshold in volume_threshold_range.clone() {
                // let stopo_percent: Vec<f64> = (0..=3).map(|x| x as f64 * 0.1).collect(); //损失仓位,从0到30%
                // for stop in stopo_percent {
                let stop_loss_strategy = StopLossStrategy::Percent(0.1);
                let res = startegy.breakout_strategy(&*mysql_candles_5m, breakout_period, confirmation_period, volume_threshold, stop_loss_strategy).await;
                println!("strategy{:#?}", res);    // let ins_id = "BTC-USDT-SWAP";

                // 解包 Result 类型
                let (final_fund, win_rate, open_position_num) = res;
                //把back tests strategy结果写入数据
                let back_test_log = BackTestLog {
                    strategy_type: format!("{:?}", StrategyType::BreakoutUp),
                    inst_type: inst_id.parse()?,
                    time: time.parse()?,
                    final_fund: final_fund.to_string(),
                    win_rate: win_rate.to_string(),
                    open_positions_num: open_position_num,
                    strategy_detail: Some(format!("breakout_period:{},confirmation_period:{},volume_threshold:{},stop_loss_strategy: {:?}", breakout_period, confirmation_period, volume_threshold, stop_loss_strategy)),
                    profit: "".to_string(),
                };
                back_test_log::BackTestLogModel::new().await.add(&back_test_log).await?;
            }
        }
    }
    Ok(())
}


pub async fn macd_ema_test(mysql_candles_5m: Vec<CandlesEntity>, inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");

    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(db::get_db_client(), con);

    // let stopo_percent: Vec<f64> = (0..=3).map(|x| x as f64 * 0.1).collect(); //损失仓位,从0到30%
    let stop_percent: Vec<f64> = vec![0.1]; //失仓位,从10%
    for stop in stop_percent.clone() {
        let res = startegy.macd_ema_strategy(&*mysql_candles_5m, stop).await;
        println!("strategy{:#?}", res);    // let ins_id = "BTC-USDT-SWAP";

        // 解包 Result 类型
        let (final_fund, win_rate, open_position_num) = res;
        //把back tests strategy结果写入数据
        let back_test_log = BackTestLog {
            strategy_type: format!("{:?}", StrategyType::BreakoutUp),
            inst_type: inst_id.parse()?,
            time: time.parse()?,
            final_fund: final_fund.to_string(),
            win_rate: win_rate.to_string(),
            open_positions_num: open_position_num as i32,
            strategy_detail: Some(format!("stop_loss_percent: {:?}", stop)),
            profit: "".to_string(),
        };
        back_test_log::BackTestLogModel::new().await.add(&back_test_log).await?;
    }
    Ok(())
}


pub async fn kdj_macd_test(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    //获取candle数据
    let mysql_candles = self::get_candle_data(inst_id, time).await?;
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");

    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(db::get_db_client(), con);

    // let stopo_percent: Vec<f64> = (0..=3).map(|x| x as f64 * 0.1).collect(); //损失仓位,从0到30%

    let fib_levels = ProfitStopLoss::get_fibonacci_level(inst_id, time);
    let stop_percent: Vec<f64> = vec![0.02]; //失仓位,从10%
    for stop in stop_percent.clone() {
        for kdj_period in 2..30 {
            for signal_period in 1..10 {
                let res = MacdKdjStrategy::run_test(&mysql_candles, &fib_levels, stop, kdj_period, signal_period).await;
                println!("strategy{:#?}", res);    // let ins_id = "BTC-USDT-SWAP";
                // 解包 Result 类型
                let (final_fund, win_rate, open_position_num) = res;
                //把back tests strategy结果写入数据
                let back_test_log = BackTestLog {
                    strategy_type: format!("{:?}", StrategyType::MacdWithKdj),
                    inst_type: inst_id.parse()?,
                    time: time.parse()?,
                    final_fund: final_fund.to_string(),
                    win_rate: win_rate.to_string(),
                    open_positions_num: open_position_num as i32,
                    strategy_detail: Some(format!("stop_loss_percent: {:?},kdj_period:{},signal_period:{}", stop, kdj_period, signal_period)),
                    profit: "".to_string(),
                };
                back_test_log::BackTestLogModel::new().await.add(&back_test_log).await?;
            }
        }
    }
    Ok(())
}


use anyhow::Result;
use futures::future::join_all;
use redis::AsyncCommands;
use tokio;
use tokio::sync::Semaphore;
use crate::trading::strategy::engulfing_strategy::EngulfingStrategy;

pub async fn ut_boot_test(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    let mysql_candles = self::get_candle_data(inst_id, time).await?;
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/")?;
    let mut con = client.get_multiplexed_async_connection().await?;
    // let db = BizActivityModel::new().await;

    //灵敏度
    let key_values: Vec<f64> = (2..=20).map(|x| x as f64 * 0.1).collect(); //损失仓位,从0到30%
    let fibonacci_level = ProfitStopLoss::get_fibonacci_level(inst_id, time);
    println!("fibonacci_level:{:?}", fibonacci_level);


    for key_value in key_values {
        for atr_period in 2..20 {
            if key_value > atr_period as f64 {
                continue;
            }
            let max_loss_percent: Vec<f64> = (1..=6).map(|x| x as f64 * 0.01).collect(); //损失仓位,从0到30%
            for &is_fibonacci_profit in &[true] {
                for &max_loss_percent in &max_loss_percent {
                    //是否允许开多
                    let is_open_long = true;
                    //是否允许开空
                    let is_open_short = true;
                    //是否判断交易时间
                    let is_judge_trade_time = false;

                    let ut_boot_strategy = UtBootStrategy {
                        key_value,
                        atr_period,
                        heikin_ashi: false,
                    };

                    let mysql_candles_clone = mysql_candles.clone();
                    let fibonacci_level_clone = fibonacci_level.clone();
                    let inst_id_clone = inst_id.to_string();
                    let time_clone = time.to_string();


                    // (funds, win_rate, open_trades, trade_records)
                    let (final_fund, win_rate, open_position_num, trade_record_list) = UtBootStrategy::run_test(
                        &mysql_candles_clone,
                        &fibonacci_level_clone,
                        max_loss_percent,
                        is_fibonacci_profit,
                        is_open_long,
                        is_open_short,
                        ut_boot_strategy,
                        is_judge_trade_time,
                    ).await;

                    let strategy_detail = Some(format!(
                        "key_value: {:?},atr_period:{},is_open_fibonacci_profit:{},is_open_long:{},is_open_short:{},max_loss_percen:{},is_judege_trade_time:{}",
                        key_value, atr_period, is_fibonacci_profit, is_open_long, is_open_short, max_loss_percent, is_judge_trade_time
                    ));

                    let insert_id = save_test_log(
                        StrategyType::UtBoot,
                        &inst_id_clone,
                        &time_clone,
                        final_fund,
                        win_rate,
                        open_position_num as i32,
                        strategy_detail,
                    ).await.unwrap();

                    // 只在交易记录列表不为空时插入记录
                    if !trade_record_list.is_empty() {
                        if let Err(e) = save_test_detail(insert_id, StrategyType::UtBoot, &inst_id_clone, &time_clone, trade_record_list).await {
                            error!("Failed to save test detail: {:?}", e);
                        }
                    } else {
                        warn!("Empty trade record list, skipping save_test_detail.");
                    }
                }
            }
        }
    }

    Ok(())
}

pub async fn engulfing_test(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    let mysql_candles = self::get_candle_data(inst_id, time).await?;
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");
    // let db = BizActivityModel::new().await;

    let atr_threshold: Vec<f64> = (1..=10).map(|x| x as f64 * 1.0).collect(); //损失仓位,从0到30%

    let max_loss_percent: Vec<f64> = (1..=5).map(|x| x as f64 * 0.01).collect(); //损失仓位,从0到30%
    let fibonacci_level = ProfitStopLoss::get_fibonacci_level(inst_id, time);
    println!("fibonacci_level:{:?}", fibonacci_level);
    for num_bar in 3..8 {
        for max_loss_percent in &max_loss_percent {
            for is_fibonacci_profit in [true].iter() {
                //是否允许开多
                let is_open_long = false;
                //是否允许开空
                let is_open_short = true;
                //是否判断交易时间段
                let is_judge_trade_time = true;


                let res = engulfing_strategy::EngulfingStrategy::run_test(&mysql_candles, &fibonacci_level, max_loss_percent.clone(),
                                                                          num_bar,
                                                                          *is_fibonacci_profit,
                                                                          is_open_long,
                                                                          is_open_short,
                                                                          is_judge_trade_time,
                ).await;

                // let res = startegy.ut_bot_alert_strategy_with_shorting(&mysql_candles_5m, &fibonacci_level, key_value, atr_period, false).await;
                //save tests to log
                let (final_fund, win_rate, open_position_num, trade_record_list) = res;
                let strategy_detail = Some(format!("bum_bar:{},max_loss_percent:{},is_open_fibonacci_profit:{},is_open_long:{},is_open_short:{},is_judge_trade_time:{}",
                                                   num_bar, max_loss_percent, is_fibonacci_profit, is_open_long, is_open_short, is_judge_trade_time));
                let insert_id = save_test_log(StrategyType::Engulfing, inst_id, time, final_fund, win_rate, open_position_num as i32, strategy_detail).await?;

                let inset_id = save_test_detail(insert_id, StrategyType::Engulfing, inst_id, time, trade_record_list).await?;
            }
        }
    }

    Ok(())
}

pub async fn ut_boot_order(mysql_candles_5m: Vec<CandlesEntity>, inst_id: &str, time: &str, signal: SignalResult) -> Result<(), anyhow::Error> {
    //插入信号记录到数据库中
    // let signal_result = SignalResult {
    //     should_buy: signal.should_buy,
    //     should_sell: signal.should_sell,
    //     price: signal.price,
    //     ts: signal.ts,
    // };
    let signal_record = StrategyJobSignalLog {
        inst_id: inst_id.parse().unwrap(),
        time: time.parse().unwrap(),
        strategy_type: StrategyType::UtBoot.to_string(),
        strategy_result: serde_json::to_string(&signal).unwrap(),
    };
    strategy_job_signal_log::StrategyJobSignalLogModel::new().await.add(signal_record).await?;

    //执行下单
    order::deal(StrategyType::UtBoot, inst_id, time, signal).await?;
    Ok(())
}

pub async fn save_test_log(strategy_type: StrategyType, inst_id: &str, time: &str, final_fund: f64, win_rate: f64, open_position_num: i32, detail: Option<String>) -> Result<i64, anyhow::Error> {
    // 解包 Result 类型
    //把back tests strategy结果写入数据
    let back_test_log = BackTestLog {
        strategy_type: strategy_type.to_string(),
        inst_type: inst_id.parse().unwrap(),
        time: time.parse().unwrap(),
        final_fund: final_fund.to_string(),
        win_rate: win_rate.to_string(),
        open_positions_num: open_position_num,
        strategy_detail: detail,
        profit: (final_fund - 100.00).to_string(),
    };
    println!("back_test_log:{:#?}", back_test_log);

    let res = back_test_log::BackTestLogModel::new().await.add(&back_test_log).await?;
    Ok(res)
}

pub async fn save_test_detail(back_test_id: i64, strategy_type: StrategyType, inst_id: &str, time: &str, list: Vec<TradeRecord>) -> Result<u64, anyhow::Error> {
    // 解包 Result 类型
    //把back tests strategy结果写入数据
    let mut array = Vec::new();
    for trade_record in list {
        let back_test_log = BackTestDetail {
            back_test_id,
            option_type: trade_record.option_type,
            strategy_type: strategy_type.to_string(),
            inst_id: inst_id.to_string(),
            time: time.to_string(),
            open_position_time: trade_record.open_position_time.to_string(),
            close_position_time: match trade_record.close_position_time {
                Some(x) => x.to_string(),
                None => "".to_string(),
            },
            open_price: trade_record.open_price.to_string(),
            close_price: trade_record.close_price.to_string(),
            profit_loss: trade_record.profit_loss.to_string(),
            quantity: trade_record.quantity.to_string(),
            full_close: trade_record.full_close,
            close_type: trade_record.close_type,
            win_nums: trade_record.win_num,
            loss_nums: trade_record.loss_num,
        };
        array.push(back_test_log);
    }
    let res = back_test_detail::BackTestDetailModel::new().await.batch_add(array).await?;
    Ok(res)
}


pub async fn comprehensive_test(mysql_candles_5m: Vec<CandlesEntity>, inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");

    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(db::get_db_client(), con);

    let atr_threshold: Vec<f64> = (10..=30).map(|x| x as f64 * 0.1).collect(); //损失仓位,从0到30%
    let stop_percent: Vec<f64> = vec![0.1]; //失仓位,从10%
    for stop in stop_percent.clone() {
        for adx_period in 2..4 {
            for adx_smoothing in 2..4 {
                for sig_length in 1..15 {}
            }
        }
    }

    let adx_period = 15;
    let adx_smoothing = 15;
    let sig_length = 9;

    let strategy = ComprehensiveStrategy {
        /// 5分钟蜡烛图数据
        candles_5m: mysql_candles_5m.clone(),
        /// ADX（平均趋向指数）周期
        adx_period: adx_period,
        /// ADX（平均趋向指数）平滑周期
        adx_smoothing: adx_smoothing,

        /// Andean Oscillator 的长度
        andean_length: 50,
        /// EMA信号长度
        sig_length,
        /// 布林带标准差倍数
        bb_mult: 2.0,
        /// Keltner Channel 高倍数
        kc_mult_high: 1.0,
        /// Keltner Channel 中倍数
        kc_mult_mid: 1.5,
        /// Keltner Channel 低倍数
        kc_mult_low: 2.0,
        /// TTM Squeeze 长度
        ttm_length: 20,
        /// 止损百分比
        stop_loss_percent: 0.1,
    };

    let res = strategy.comprehensive_strategy().await;
    println!("strategy{:#?}", res);    // let ins_id = "BTC-USDT-SWAP";
    // 解包 Result 类型
    let (final_fund, win_rate, open_position_num) = res;
    //把back tests strategy结果写入数据
    let back_test_log = BackTestLog {
        strategy_type: format!("{:?}", StrategyType::BreakoutUp),
        inst_type: inst_id.parse()?,
        time: time.parse()?,
        final_fund: final_fund.to_string(),
        win_rate: win_rate.to_string(),
        open_positions_num: open_position_num as i32,
        strategy_detail: Some(format!("atr_threshold: {:?},ema_short_period:{}", atr_threshold, sig_length)),
        profit: "".to_string(),
    };
    back_test_log::BackTestLogModel::new().await.add(&back_test_log).await?;


    Ok(())
}


pub async fn get_candle_data(inst_id: &str, time: &str) -> Result<Vec<CandlesEntity>, anyhow::Error> {
    let mysql_candles_5m = candles::CandlesModel::new().await.fetch_candles_from_mysql(inst_id, time).await?;
    if mysql_candles_5m.is_empty() {
        return Err(anyhow!("mysql candles 5m is empty"));
    }
    Ok(mysql_candles_5m)
}

pub fn valid_newest_candle_data(mysql_candles_5m: CandlesEntity, time: &str) -> bool {
    let ts = mysql_candles_5m.ts;
    let local_time = Local::now();
    // 将毫秒时间戳转换为 DateTime<Utc>
    let datetime: DateTime<Local> = time_util::mill_time_to_local_datetime(ts);

    let date = time_util::format_to_period(time, Some(datetime));
    let current_date = time_util::format_to_period(time, None);
    // 比较时间戳的小时与当前小时
    if date != current_date {
        println!("数据库最新数据的时间ts:{} date:({}) 不等于当前最新时间 local time:({}), 跳过,candles:{:?},time:{}", ts, date, current_date, mysql_candles_5m, time);
        return false;
    }
    return true;
}

pub fn valid_candles_data(mysql_candles_5m: &Vec<CandlesEntity>, time: &str) -> anyhow::Result<()> {
    //验证头尾数据正确性
    let first_timestamp = mysql_candles_5m.first().unwrap().ts;
    let last_timestamp = mysql_candles_5m.last().unwrap().ts;
    let difference = last_timestamp - first_timestamp;
    let period_milliseconds = time_util::parse_period(time)?;
    let expected_length = difference / period_milliseconds;
    if expected_length != (mysql_candles_5m.len() - 1) as i64 {
        let mut discontinuities = Vec::new();

        //获取哪个数据的不连续
        for window in mysql_candles_5m.windows(2) {
            let current = &window[0];
            let next = &window[1];
            let expected_next_ts = current.ts + period_milliseconds;

            if next.ts != expected_next_ts {
                discontinuities.push(expected_next_ts);
            }
        }
        return Err(anyhow!(
            "The difference between the first and last timestamps is not consistent with the period. Expected: {}, Actual: {},discontinuities:{:?}",
            expected_length,
            mysql_candles_5m.len() - 1,discontinuities
        ));
    }
    Ok(())
}

// 定义一个枚举来封装不同的策略类型
enum RealStrategy {
    UtBoot(UtBootStrategy),
    Engulfing(EngulfingStrategy),
}


pub async fn run_ready_to_order(inst_id: &str, time: &str, strategy_type: StrategyType) -> Result<()> {
    info!("run ut_boot_run_real inst_id:{:?} time:{:?}", inst_id,time);
    //从策略配置中获取到对应的产品配置
    let strategy_config = StrategyConfigEntityModel::new().await.get_config(strategy_type.to_string().as_str(), inst_id, time).await?;
    if strategy_config.len() < 1 {
        warn!("策略配置为空strategy_type:{} inst_id:{:?} time:{:?}",strategy_type, inst_id, time);
        return Ok(());
    }

    //取出最新的一条数据，判断时间是否==当前时间的H,如果不是跳过
    let mysql_candles_5m = candles::CandlesModel::new().await.get_new_data(inst_id, time).await?;
    if mysql_candles_5m.is_none() {
        return Ok(());
    }
    //验证最新数据准确性
    let is_valid = self::valid_newest_candle_data(mysql_candles_5m.unwrap(), time);
    if !is_valid {
        return Ok(());
    }

    let mysql_candles_5m = candles::CandlesModel::new().await.fetch_candles_from_mysql(inst_id, time).await?;
    if mysql_candles_5m.is_empty() {
        return Err(anyhow!("mysql candles 5m is empty"));
    }

    //验证所有数据是否准确
    self::valid_candles_data(&mysql_candles_5m, time)?;

    // let ut_boot_strategy = UtBootStrategy {
    //     key_value: 1.2,
    //     atr_period: 3,
    //     heikin_ashi: false,
    // };
    // println!("strategy_config:{:#?}", serde_json::to_string(&ut_boot_strategy));

    let ut_boot_strategy_info = strategy_config.get(0).unwrap();
    let signal = match strategy_type {
        StrategyType::UtBoot => {
            let strategy_config = serde_json::from_str::<UtBootStrategy>(&*ut_boot_strategy_info.value)
                .map_err(|e| anyhow!("Failed to parse UtBootStrategy config: {}", e))?;

            let key_value = strategy_config.key_value;
            let atr_period = strategy_config.atr_period;
            let heikin_ashi = strategy_config.heikin_ashi;

            UtBootStrategy::get_trade_signal(&mysql_candles_5m, key_value, atr_period, heikin_ashi)
        }
        StrategyType::Engulfing => {
            let strategy_config = serde_json::from_str::<EngulfingStrategy>(&*ut_boot_strategy_info.value)
                .map_err(|e| anyhow!("Failed to parse EngulfingStrategy config: {}", e))?;

            EngulfingStrategy::get_trade_signal(&mysql_candles_5m, strategy_config.num_bars)
        }
        _ => {
            return Err(anyhow!("Unknown strategy type: {:?}", strategy_type));
        }
    };

    // ut boot atr 策略回测
    ut_boot_order(mysql_candles_5m.clone(), inst_id, time, signal).await?;
    Ok(())
}


/** 执行ut boot 策略 任务**/
pub async fn run_strategy_job(inst_ids: Arc<Vec<&str>>, times: Arc<Vec<&str>>, strategy_type: StrategyType) -> Result<(), anyhow::Error> {
    for inst_id in inst_ids.iter() {
        for time in times.iter() {
            //实际执行
            let inst_id = inst_id.to_string();
            let time = time.to_string();
            let res = self::run_ready_to_order(&inst_id, &time, strategy_type).await;
            if let Err(e) = res {
                error!("run ready to order strategy_tye:{} inst_id:{:?} time:{:?} error:{:?}", strategy_type,inst_id, time, e);
            }
            //执行回测
            // self::run_ut_boot_run_test(inst_id, time).await?;
        }
    }
    Ok(())
}


// /** 执行策略 任务**/
// pub async fn run_strategy_job() -> Result<(), anyhow::Error> {
//     // 初始化 Redis
//     // let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
//     // let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");
//     //
//     // // let db = BizActivityModel::new().await;
//     // let mut startegy = trading::strategy::Strategy::new(Db::get_db_client().await, con);
//
//     // let inst_ids = ["BTC-USDT-SWAP"];
//     // let inst_ids = ["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SUSHI-USDT-SWAP", "SOL-USDT-SWAP", "ADA-USDT-SWAP"];
//     let inst_ids = ["ETH-USDT-SWAP"];
//     let inst_ids = ["OMU-USDT-SWAP"];
//     let tims = ["1D", "4H", "1H", "5m"];
//     // let tims = ["1H"];
//     // let tims = ["1D"];
//     for inst_id in inst_ids {
//         for time in tims {
//             let mysql_candles_5m = candles::CandlesModel::new().await.fetch_candles_from_mysql(inst_id, time).await?;
//             if mysql_candles_5m.is_empty() {
//                 return Err(anyhow!("mysql candles 5m is empty"));
//             }
//             /// 突破策略
//             // self::breakout_long_test(mysql_candles_5m.clone(), inst_id, time).await?;
//
//             // ema 均线策略
//             // self::macd_ema_test(mysql_candles_5m.clone(), inst_id, time).await?;
//
//             // kdj_macd 均线策略
//             // self::kdj_macd_test(mysql_candles_5m.clone(), inst_id, time).await?;
//
//             let ut_boot_strategy = UtBootStrategy {
//                 key_value: 1.2,
//                 atr_period: 3,
//                 heikin_ashi: false,
//             };
//             // ut boot atr 策略回测
//             self::ut_boot_order(mysql_candles_5m.clone(), inst_id, time, ut_boot_strategy).await?;
//
//             // ut boot atr 策略
//             // self::ut_boot_(mysql_candles_5m.clone(), inst_id, time).await?;
//
//             // ut boot atr 策略
//             // self::comprehensive_test(mysql_candles_5m.clone(), inst_id, time).await?;
//
//
//             // let res = startegy.short_strategy(&*mysql_candles_5m, breakout_period, confirmation_period, volume_threshold, stop_loss_strategy).await;
//             // println!("strategy{:#?}", res);    // let ins_id = "BTC-USDT-SWAP";
//             //
//             // // 解包 Result 类型
//             // let (final_fund, win_rate) = res;
//             // //把back tests strategy结果写入数据
//             // let back_test_log = BackTestLog {
//             //     strategy_type: format!("{:?}", StrategyType::BreakoutDown),
//             //     inst_type: inst_id.parse()?,
//             //     time: time.parse()?,
//             //     final_fund: final_fund.to_string(),
//             //     win_rate: win_rate.to_string(),
//             //     strategy_detail: Some(format!("macd_fast_period: {}, macd_slow_period: {}, macd_signal:{},breakout_period:{},\
//             //                                   confirmation_period:{},volume_threshold:{},stop_loss_strategy:{:?}",
//             //                                   macd_fast_period, macd_slow_period, macd_signal_period, breakout_period, confirmation_period, volume_threshold, stop_loss_strategy
//             //     )),
//             // };
//             // back_test_log::BackTestLogModel::new().await.add(back_test_log).await?
//         }
//     }
//
//
//     Ok(())
// }
//
