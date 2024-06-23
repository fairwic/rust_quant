use anyhow::anyhow;
use chrono::{DateTime, Timelike, TimeZone, Utc};
use tracing::{info, Level, span};
use crate::trading;
use crate::trading::model::Db;
use crate::trading::model::market::candles;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::model::strategy::{back_test_log, strategy_job_signal_log};
use crate::trading::model::strategy::back_test_log::BackTestLog;
use crate::trading::model::strategy::strategy_job_signal_log::StrategyJobSignalLog;
use crate::trading::order;
use crate::trading::strategy::{StopLossStrategy, StrategyType};
use crate::trading::strategy::comprehensive_strategy::ComprehensiveStrategy;
use crate::trading::strategy::ut_boot_strategy::{SignalResult, UtBootStrategy};

pub mod tickets_job;
pub mod account_job;
pub mod asset_job;
pub(crate) mod candles_job;
pub mod trades_job;


/** 同步数据 任务**/
pub async fn run_sync_data_job(inst_ids: Vec<&str>, tims: Vec<&str>) -> Result<(), anyhow::Error> {
    let span = span!(Level::DEBUG, "run_sync_data_job");
    let _enter = span.enter();

    candles_job::init_create_table(Some(&inst_ids), Some(&tims)).await.expect("init create_table errror");
    candles_job::init_all_candles(Some(&inst_ids), Some(&tims)).await?;
    candles_job::init_before_candles(Some(&inst_ids), Some(tims)).await?;
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
    let stop_percent: Vec<f64> = vec![0.1]; //失仓位,从10%
    for key_value in atr_threshold {
        for atr_period in 1..20 {
            let res = startegy.ut_bot_alert_strategy(&*mysql_candles_5m, key_value, atr_period, false).await;
            let (final_fund, win_rate, open_position_num) = res;
            let strategy_detail = Some(format!("key_value: {:?},atr_period:{}", key_value, atr_period));
            save_test_log(inst_id, time, final_fund, win_rate, open_position_num as i32, strategy_detail).await;
        }
    }
    Ok(())
}


pub async fn ut_boot_order(mysql_candles_5m: Vec<CandlesEntity>, inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    let key_value = 1.2;
    let atr_period = 3;
    let heikin_ashi = false;
    //获取开仓信号
    let signal = UtBootStrategy::get_trade_signal(&mysql_candles_5m, key_value, atr_period, heikin_ashi);
    info!("ut_boot_strategy signal:{:?}", signal);
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
    // let signal = SignalResult {
    //     should_buy: true,
    //     should_sell: false,
    //     price: 3540.0,
    // };
    //执行下单
    order::deal(inst_id, time, signal).await?;

    Ok(())
}

pub async fn save_test_log(inst_id: &str, time: &str, final_fund: f64, win_rate: f64, open_position_num: i32, detail: Option<String>) -> Result<(), anyhow::Error> {
    // 解包 Result 类型
    //把back test strategy结果写入数据
    let back_test_log = BackTestLog {
        strategy_type: format!("{:?}", StrategyType::BreakoutUp),
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


/** 执行ut boot 策略 任务**/
pub async fn run_ut_boot_strategy_job() -> Result<(), anyhow::Error> {
    let inst_ids = ["ETH-USDT-SWAP"];
    let times = ["1H"];
    for inst_id in inst_ids {
        for time in times {
            //取出最新的一条数据，判断时间是否==当前时间的H,如果不是跳过
            let mysql_candles_5m = candles::CandlesModel::new().await.get_new_data(inst_id, time).await?;
            if mysql_candles_5m.is_none() {
                continue;
            }
            let ts = mysql_candles_5m.unwrap().ts;

            // 将毫秒时间戳转换为 DateTime<Utc>
            let datetime: DateTime<Utc> = Utc.timestamp_millis(ts);
            // 获取小时
            let hour = datetime.hour();
            // 获取当前时间的小时
            let current_hour = Utc::now().hour();
            // 比较时间戳的小时与当前小时
            if hour != current_hour {
                println!("时间戳的小时 ({}) 不等于当前小时 ({}), 跳过", hour, current_hour);
                continue;
            }

            let mysql_candles_5m = candles::CandlesModel::new().await.fetch_candles_from_mysql(inst_id, time).await?;
            if mysql_candles_5m.is_empty() {
                return Err(anyhow!("mysql candles 5m is empty"));
            }
            // ut boot atr 策略回测
            ut_boot_order(mysql_candles_5m.clone(), inst_id, time).await?;
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

            // ut boot atr 策略回测
            self::ut_boot_order(mysql_candles_5m.clone(), inst_id, time).await?;

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

