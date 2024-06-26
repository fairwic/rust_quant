use anyhow::anyhow;
use chrono::{DateTime, Timelike, TimeZone, Utc};
use tracing::{info, Level, span};

use crate::{time_util, trading};
use crate::trading::model::Db;
use crate::trading::model::market::candles;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::model::order::swap_order::SwapOrderEntityModel;
use crate::trading::model::strategy::{back_test_log, strategy_job_signal_log};
use crate::trading::model::strategy::back_test_log::BackTestLog;
use crate::trading::model::strategy::strategy_config::*;
use crate::trading::model::strategy::strategy_job_signal_log::StrategyJobSignalLog;
use crate::trading::order;
use crate::trading::strategy;
use crate::trading::strategy::{StopLossStrategy, Strategy, StrategyType};
use crate::trading::strategy::comprehensive_strategy::ComprehensiveStrategy;
use crate::trading::strategy::ut_boot_strategy::{SignalResult, UtBootStrategy};

pub mod tickets_job;
pub mod account_job;
pub mod asset_job;
pub(crate) mod candles_job;
pub mod trades_job;


/** 同步数据 任务**/
pub async fn run_sync_data_job(inst_ids: &Vec<&str>, tims: &Vec<&str>) -> Result<(), anyhow::Error> {
    let span = span!(Level::DEBUG, "run_sync_data_job");
    let _enter = span.enter();

    candles_job::init_create_table(Some(&inst_ids), Some(&tims)).await.expect("init create_table errror");
    candles_job::init_all_candles(Some(&inst_ids), Some(&tims)).await?;
    candles_job::init_before_candles(Some(&inst_ids), Some(tims.clone())).await?;
    Ok(())
}

pub async fn breakout_long_test(mysql_candles_5m: Vec<CandlesEntity>, inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");

    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(Db::get_db_client().await, con);

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
                //把back test strategy结果写入数据
                let back_test_log = BackTestLog {
                    strategy_type: format!("{:?}", StrategyType::BreakoutUp),
                    inst_type: inst_id.parse()?,
                    time: time.parse()?,
                    final_fund: final_fund.to_string(),
                    win_rate: win_rate.to_string(),
                    open_positions_num: open_position_num,
                    strategy_detail: Some(format!("breakout_period:{},confirmation_period:{},volume_threshold:{},stop_loss_strategy: {:?}", breakout_period, confirmation_period, volume_threshold, stop_loss_strategy)),
                };
                back_test_log::BackTestLogModel::new().await.add(back_test_log).await?;
                // }
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
    let mut startegy = trading::strategy::Strategy::new(Db::get_db_client().await, con);

    // let stopo_percent: Vec<f64> = (0..=3).map(|x| x as f64 * 0.1).collect(); //损失仓位,从0到30%
    let stop_percent: Vec<f64> = vec![0.1]; //失仓位,从10%
    for stop in stop_percent.clone() {
        let res = startegy.macd_ema_strategy(&*mysql_candles_5m, stop).await;
        println!("strategy{:#?}", res);    // let ins_id = "BTC-USDT-SWAP";

        // 解包 Result 类型
        let (final_fund, win_rate, open_position_num) = res;
        //把back test strategy结果写入数据
        let back_test_log = BackTestLog {
            strategy_type: format!("{:?}", StrategyType::BreakoutUp),
            inst_type: inst_id.parse()?,
            time: time.parse()?,
            final_fund: final_fund.to_string(),
            win_rate: win_rate.to_string(),
            open_positions_num: open_position_num as i32,
            strategy_detail: Some(format!("stop_loss_percent: {:?}", stop)),
        };
        back_test_log::BackTestLogModel::new().await.add(back_test_log).await?;
    }
    Ok(())
}


pub async fn kdj_macd_test(mysql_candles_5m: Vec<CandlesEntity>, inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");

    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(Db::get_db_client().await, con);

    // let stopo_percent: Vec<f64> = (0..=3).map(|x| x as f64 * 0.1).collect(); //损失仓位,从0到30%
    let stop_percent: Vec<f64> = vec![0.1]; //失仓位,从10%
    for stop in stop_percent.clone() {
        for kdj_period in 2..30 {
            for ema_period in 1..30 {
                let res = startegy.kdj_macd_strategy(&*mysql_candles_5m, stop, kdj_period, ema_period).await;
                println!("strategy{:#?}", res);    // let ins_id = "BTC-USDT-SWAP";
                // 解包 Result 类型
                let (final_fund, win_rate, open_position_num) = res;
                //把back test strategy结果写入数据
                let back_test_log = BackTestLog {
                    strategy_type: format!("{:?}", StrategyType::BreakoutUp),
                    inst_type: inst_id.parse()?,
                    time: time.parse()?,
                    final_fund: final_fund.to_string(),
                    win_rate: win_rate.to_string(),
                    open_positions_num: open_position_num as i32,
                    strategy_detail: Some(format!("stop_loss_percent: {:?},kdj_period:{}", stop, kdj_period)),
                };
                back_test_log::BackTestLogModel::new().await.add(back_test_log).await?;
            }
        }
    }
    Ok(())
}


pub async fn ut_boot_test(mysql_candles_5m: Vec<CandlesEntity>, inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");
    // let db = BizActivityModel::new().await;

    let mut startegy = trading::strategy::Strategy::new(Db::get_db_client().await, con);
    let atr_threshold: Vec<f64> = (10..=30).map(|x| x as f64 * 0.1).collect(); //损失仓位,从0到30%
    let fibonacci_level = Strategy::get_fibonacci_level(inst_id, time);
    println!("fibonacci_level:{:?}", fibonacci_level);

    for key_value in atr_threshold {
        for atr_period in 2..20 {
            let res = startegy.ut_bot_alert_strategy(&mysql_candles_5m, &fibonacci_level, key_value, atr_period, false).await;
            // let res = startegy.ut_bot_alert_strategy_with_shorting(&mysql_candles_5m, &fibonacci_level, key_value, atr_period, false).await;
            //save test to log
            let (final_fund, win_rate, open_position_num) = res;
            let strategy_detail = Some(format!("key_value: {:?},atr_period:{}", key_value, atr_period));
            save_test_log(StrategyType::UtBootShort, inst_id, time, final_fund, win_rate, open_position_num as i32, strategy_detail).await?;
        }
    }
    Ok(())
}


pub async fn ut_boot_order(mysql_candles_5m: Vec<CandlesEntity>, inst_id: &str, time: &str, ut_boot_strategy: UtBootStrategy) -> Result<(), anyhow::Error> {
    let key_value = ut_boot_strategy.key_value;
    let atr_period = ut_boot_strategy.atr_period;
    let heikin_ashi = ut_boot_strategy.heikin_ashi;

    //获取开仓信号
    let signal = UtBootStrategy::get_trade_signal(&mysql_candles_5m, key_value, atr_period, heikin_ashi);

    //插入信号记录到数据库中
    let signal_result = SignalResult {
        should_buy: signal.should_buy,
        should_sell: signal.should_sell,
        price: signal.price,
    };
    let signal_record = StrategyJobSignalLog {
        inst_id: inst_id.parse().unwrap(),
        time: time.parse().unwrap(),
        strategy_type: StrategyType::UtBoot.to_string(),
        strategy_result: serde_json::to_string(&signal_result).unwrap(),
    };
    strategy_job_signal_log::StrategyJobSignalLogModel::new().await.add(signal_record).await?;


    //执行下单
    order::deal(StrategyType::UtBoot, inst_id, time, signal).await?;
    Ok(())
}

pub async fn save_test_log(strategy_type: StrategyType, inst_id: &str, time: &str, final_fund: f64, win_rate: f64, open_position_num: i32, detail: Option<String>) -> Result<(), anyhow::Error> {
    // 解包 Result 类型
    //把back test strategy结果写入数据
    let back_test_log = BackTestLog {
        strategy_type: format!("{:?}", strategy_type),
        inst_type: inst_id.parse().unwrap(),
        time: time.parse().unwrap(),
        final_fund: final_fund.to_string(),
        win_rate: win_rate.to_string(),
        open_positions_num: open_position_num,
        strategy_detail: detail,
    };
    back_test_log::BackTestLogModel::new().await.add(back_test_log).await?;
    Ok(())
}


pub async fn comprehensive_test(mysql_candles_5m: Vec<CandlesEntity>, inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");

    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(Db::get_db_client().await, con);

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
    //把back test strategy结果写入数据
    let back_test_log = BackTestLog {
        strategy_type: format!("{:?}", StrategyType::BreakoutUp),
        inst_type: inst_id.parse()?,
        time: time.parse()?,
        final_fund: final_fund.to_string(),
        win_rate: win_rate.to_string(),
        open_positions_num: open_position_num as i32,
        strategy_detail: Some(format!("atr_threshold: {:?},ema_short_period:{}", atr_threshold, sig_length)),
    };
    back_test_log::BackTestLogModel::new().await.add(back_test_log).await?;


    Ok(())
}


pub async fn run_ut_boot_run_test(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    // //取出最新的一条数据，判断时间是否==当前时间的H,如果不是跳过
    // let mysql_candles_5m = candles::CandlesModel::new().await.get_new_data(inst_id, time).await?;
    // if mysql_candles_5m.is_none() {
    //     return Ok(());
    // }
    // let ts = mysql_candles_5m.unwrap().ts;
    //
    // // 将毫秒时间戳转换为 DateTime<Utc>
    // let datetime: DateTime<Utc> = Utc.timestamp_millis(ts);
    // // 获取小时
    // let hour = datetime.hour();
    // // 获取当前时间的小时
    // let current_hour = Utc::now().hour();
    // // 比较时间戳的小时与当前小时
    // if hour != current_hour {
    //     println!("时间戳的小时 ({}) 不等于当前小时 ({}), 跳过", hour, current_hour);
    //     return Ok(());
    // }
    let mysql_candles_5m = candles::CandlesModel::new().await.fetch_candles_from_mysql(inst_id, time).await?;
    if mysql_candles_5m.is_empty() {
        return Err(anyhow!("mysql candles 5m is empty"));
    }
    // ut boot atr 策略回测
    ut_boot_test(mysql_candles_5m, inst_id, time).await?;
    Ok(())
}

pub fn validte_candle_data(mysql_candles_5m: CandlesEntity, time: &str) -> bool {
    let ts = mysql_candles_5m.ts;
    // 将毫秒时间戳转换为 DateTime<Utc>
    let mut datetime = Utc.timestamp_millis_opt(ts).unwrap();
    let date = time_util::format_to_period(time, Some(datetime));
    let current_date = time_util::format_to_period(time, None);

    // 比较时间戳的小时与当前小时
    if date != current_date {
        println!("数据库最新数据的时间 ({}) 不等于当前最新时间 ({}), 跳过,candles:{:?},time:{}", date, current_date, mysql_candles_5m, time);
        return false;
    }
    return true;
}

pub async fn run_ut_boot_run_real(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    info!("run_ut_boot_run_real inst_id:{:?} time:{:?}", inst_id,time);
    //取出最新的一条数据，判断时间是否==当前时间的H,如果不是跳过
    let mysql_candles_5m = candles::CandlesModel::new().await.get_new_data(inst_id, time).await?;
    if mysql_candles_5m.is_none() {
        return Ok(());
    }
    //验证数据准确性
    let is_valid = self::validte_candle_data(mysql_candles_5m.unwrap(), time);
    if !is_valid {
        return Ok(());
    }

    let mysql_candles_5m = candles::CandlesModel::new().await.fetch_candles_from_mysql(inst_id, time).await?;
    if mysql_candles_5m.is_empty() {
        return Err(anyhow!("mysql candles 5m is empty"));
    }

    //从策略配置中获取到对应的产品配置
    let strategy_config = StrategyConfigEntityModel::new().await.get_config(StrategyType::UtBoot.to_string().as_str(), inst_id, time).await?;
    if strategy_config.len() < 1 {
        return Err(anyhow!("ut boot strategy config is none"));
    }

    // let ut_boot_strategy = UtBootStrategy {
    //     key_value: 1.2,
    //     atr_period: 3,
    //     heikin_ashi: false,
    // };
    // println!("strategy_config:{:#?}", serde_json::to_string(&ut_boot_strategy));

    let ut_boot_strategy_info = strategy_config.get(0).unwrap();
    let ut_boot_strategy = serde_json::from_str::<UtBootStrategy>(&*ut_boot_strategy_info.value).unwrap();

    // ut boot atr 策略回测
    ut_boot_order(mysql_candles_5m.clone(), inst_id, time, ut_boot_strategy).await?;
    Ok(())
}


/** 执行ut boot 策略 任务**/
pub async fn run_ut_boot_strategy_job(inst_ids: Vec<&str>, times: Vec<&str>) -> Result<(), anyhow::Error> {
    for inst_id in &inst_ids {
        for time in &times {
            //实际执行
            let inst_id = inst_id.to_string();
            let time = time.to_string();
            self::run_ut_boot_run_real(&inst_id, &time).await?;
            //执行回测
            // self::run_ut_boot_run_test(inst_id, time).await?;
        }
    }
    Ok(())
}


/** 执行策略 任务**/
pub async fn run_strategy_job() -> Result<(), anyhow::Error> {
    // 初始化 Redis
    // let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    // let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");
    //
    // // let db = BizActivityModel::new().await;
    // let mut startegy = trading::strategy::Strategy::new(Db::get_db_client().await, con);

    // let inst_ids = ["BTC-USDT-SWAP"];
    // let inst_ids = ["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SUSHI-USDT-SWAP", "SOL-USDT-SWAP", "ADA-USDT-SWAP"];
    let inst_ids = ["ETH-USDT-SWAP"];
    // let tims = ["1D", "4H", "1H", "5m"];
    let tims = ["1H"];
    // let tims = ["1D"];
    for inst_id in inst_ids {
        for time in tims {
            let mysql_candles_5m = candles::CandlesModel::new().await.fetch_candles_from_mysql(inst_id, time).await?;
            if mysql_candles_5m.is_empty() {
                return Err(anyhow!("mysql candles 5m is empty"));
            }
            /// 突破策略
            // self::breakout_long_test(mysql_candles_5m.clone(), inst_id, time).await?;

            // ema 均线策略
            // self::macd_ema_test(mysql_candles_5m.clone(), inst_id, time).await?;

            // kdj_macd 均线策略
            // self::kdj_macd_test(mysql_candles_5m.clone(), inst_id, time).await?;

            let ut_boot_strategy = UtBootStrategy {
                key_value: 1.2,
                atr_period: 3,
                heikin_ashi: false,
            };
            // ut boot atr 策略回测
            self::ut_boot_order(mysql_candles_5m.clone(), inst_id, time, ut_boot_strategy).await?;

            // ut boot atr 策略
            // self::ut_boot_(mysql_candles_5m.clone(), inst_id, time).await?;

            // ut boot atr 策略
            // self::comprehensive_test(mysql_candles_5m.clone(), inst_id, time).await?;


            // let res = startegy.short_strategy(&*mysql_candles_5m, breakout_period, confirmation_period, volume_threshold, stop_loss_strategy).await;
            // println!("strategy{:#?}", res);    // let ins_id = "BTC-USDT-SWAP";
            //
            // // 解包 Result 类型
            // let (final_fund, win_rate) = res;
            // //把back test strategy结果写入数据
            // let back_test_log = BackTestLog {
            //     strategy_type: format!("{:?}", StrategyType::BreakoutDown),
            //     inst_type: inst_id.parse()?,
            //     time: time.parse()?,
            //     final_fund: final_fund.to_string(),
            //     win_rate: win_rate.to_string(),
            //     strategy_detail: Some(format!("macd_fast_period: {}, macd_slow_period: {}, macd_signal:{},breakout_period:{},\
            //                                   confirmation_period:{},volume_threshold:{},stop_loss_strategy:{:?}",
            //                                   macd_fast_period, macd_slow_period, macd_signal_period, breakout_period, confirmation_period, volume_threshold, stop_loss_strategy
            //     )),
            // };
            // back_test_log::BackTestLogModel::new().await.add(back_test_log).await?
        }
    }


    Ok(())
}

