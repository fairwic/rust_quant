use anyhow::anyhow;
use chrono::{DateTime, Duration, Local, TimeZone, Timelike, Utc};
use hmac::digest::generic_array::arr;
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use ta::indicators::BollingerBands;
use tokio::sync::{RwLock, Semaphore};
use tokio::task::spawn;
use tracing::{error, info, warn, Level};

use crate::time_util::{self, ts_add_n_period};
use crate::trading::indicator::bollings::BollingerBandsSignalConfig;
use crate::trading::indicator::signal_weight::SignalWeightsConfig;
use crate::trading::model::market::candles::SelectTime;
use crate::trading::model::market::candles::{self, TimeDirect};
use crate::trading::model::order::swap_order::SwapOrderEntityModel;
use crate::trading::model::strategy::back_test_detail::BackTestDetail;
use crate::trading::model::strategy::strategy_config::*;
use crate::trading::model::strategy::strategy_job_signal_log::StrategyJobSignalLog;
use crate::trading::model::strategy::{back_test_detail, strategy_job_signal_log};
use okx::api::account::OkxAccount;
use okx::dto::account::account_dto::SetLeverageRequest;
use crate::trading::strategy::comprehensive_strategy::ComprehensiveStrategy;
use crate::trading::strategy::strategy_common::{
    get_multi_indivator_values, parse_candle_to_data_item, BackTestResult, BasicRiskStrategyConfig,
    SignalResult, TradeRecord,
};
use crate::trading::strategy::ut_boot_strategy::UtBootStrategy;
use crate::trading::strategy::{self, strategy_common};
use crate::trading::strategy::{engulfing_strategy, Strategy};
use crate::trading::{order, task};

use super::job_param_generator::ParamMerge;
use crate::app_config::db;
use crate::time_util::millis_time_diff;
use crate::trading::analysis::position_analysis::PositionAnalysis;
use crate::trading::indicator::squeeze_momentum;
use crate::trading::indicator::squeeze_momentum::calculator::SqueezeCalculator;
use crate::trading::indicator::vegas_indicator::{
    EmaSignalConfig, EmaTouchTrendSignalConfig, EngulfingSignalConfig, KlineHammerConfig,
    RsiSignalConfig, VegasIndicatorSignalValue, VegasStrategy, VolumeSignalConfig,
};
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::model::strategy::back_test_log;
use crate::trading::model::strategy::back_test_log::BackTestLog;
use okx::api::trade::OkxTrade;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_vaules::get_hash_key;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_vaules::get_vegas_indicator_values;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_vaules::update_candle_items;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_vaules::update_vegas_indicator_values;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_vaules::{
    self, ArcVegasIndicatorValues,
};
use crate::trading::strategy::engulfing_strategy::EngulfingStrategy;
use crate::trading::strategy::macd_kdj_strategy::MacdKdjStrategy;
use crate::trading::strategy::profit_stop_loss::ProfitStopLoss;
use crate::trading::strategy::top_contract_strategy::{
    TopContractStrategy, TopContractStrategyConfig,
};
use crate::trading::strategy::{StopLossStrategy, StrategyType};
use crate::trading::task::candles_job;
use crate::trading::task::job_param_generator::ParamGenerator;
use crate::{trading, CandleItem};
use anyhow::Result;
use futures::future::join_all;
use hmac::digest::typenum::op;
use once_cell::sync::OnceCell;
use rbatis::dark_std::err;
use redis::AsyncCommands;
use serde_json::json;
use tokio;
use tokio::time::Instant;
use tracing::span;
use crate::trading::order::swap_ordr::SwapOrder;
use okx::dto::trade_dto::TdModeEnum;
use okx::dto::PositionSide;

/** 同步数据 任务**/
pub async fn run_sync_data_job(
    inst_ids: Option<Vec<&str>>,
    tims: &Vec<&str>,
) -> Result<(), anyhow::Error> {
    println!("run_sync_data_job start");

    let span = span!(Level::DEBUG, "run_sync_data_job");
    let _enter = span.enter();

    candles_job::init_create_table(inst_ids.clone(), Some(&tims))
        .await
        .expect("init create_table error");

    //初始化获取历史的k线路
    candles_job::init_all_candles(inst_ids.clone(), Some(&tims)).await?;

    //获取最新的k线路
    candles_job::init_before_candles(inst_ids.clone(), Some(tims.clone())).await?;
    Ok(())
}

/**
 * 设置杠杆
 */
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

        for post_side in [PositionSide::Long, PositionSide::Short] {
            let params = SetLeverageRequest {
                inst_id: Some(inst_id.to_string()),
                ccy: None,
                mgn_mode: TdModeEnum::ISOLATED.to_string(),
                lever: level.to_string(),
                pos_side: Some(post_side.to_string()),
            };
            //延迟100ms
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            OkxAccount::from_env()?.set_leverage(params).await?;
        }
    }
    Ok(())
}

pub async fn breakout_long_test(
    mysql_candles_5m: Vec<CandlesEntity>,
    inst_id: &str,
    time: &str,
) -> Result<(), anyhow::Error> {
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/")
        .expect("get redis client error");
    let mut con = client
        .get_multiplexed_async_connection()
        .await
        .expect("get multi redis connection error");

    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(db::get_db_client(), con);

    for breakout_period in 1..20 {
        for confirmation_period in 1..20 {
            let volume_threshold_range: Vec<f64> = (0..=20).map(|x| x as f64 * 0.1).collect(); // 从0.1到2.0，每步0.1
            for volume_threshold in volume_threshold_range.clone() {
                // let stopo_percent: Vec<f64> = (0..=3).map(|x| x as f64 * 0.1).collect(); //损失仓位,从0到30%
                // for stop in stopo_percent {
                let stop_loss_strategy = StopLossStrategy::Percent(0.1);
                let res = startegy
                    .breakout_strategy(
                        &*mysql_candles_5m,
                        breakout_period,
                        confirmation_period,
                        volume_threshold,
                        stop_loss_strategy,
                    )
                    .await;
                println!("strategy{:#?}", res); // let ins_id = "BTC-USDT-SWAP";

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
                    profit: (final_fund - 100.00).to_string(),
                    ..Default::default()
                };
                back_test_log::BackTestLogModel::new()
                    .await
                    .add(&back_test_log)
                    .await?;
            }
        }
    }
    Ok(())
}

pub async fn macd_ema_test(
    mysql_candles_5m: Vec<CandlesEntity>,
    inst_id: &str,
    time: &str,
) -> Result<(), anyhow::Error> {
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/")
        .expect("get redis client error");
    let mut con = client
        .get_multiplexed_async_connection()
        .await
        .expect("get multi redis connection error");

    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(db::get_db_client(), con);

    // let stopo_percent: Vec<f64> = (0..=3).map(|x| x as f64 * 0.1).collect(); //损失仓位,从0到30%
    let stop_percent: Vec<f64> = vec![0.1]; //失仓位,从10%
    for stop in stop_percent.clone() {
        let res = startegy.macd_ema_strategy(&*mysql_candles_5m, stop).await;
        println!("strategy{:#?}", res); // let ins_id = "BTC-USDT-SWAP";

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
            profit: (final_fund - 100.00).to_string(),
            ..Default::default()
        };
        back_test_log::BackTestLogModel::new()
            .await
            .add(&back_test_log)
            .await?;
    }
    Ok(())
}

pub async fn kdj_macd_test(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    //获取candle数据
    let mysql_candles = self::get_candle_data(inst_id, time, 50, None).await?;
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/")
        .expect("get redis client error");
    let mut con = client
        .get_multiplexed_async_connection()
        .await
        .expect("get multi redis connection error");

    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(db::get_db_client(), con);

    // let stopo_percent: Vec<f64> = (0..=3).map(|x| x as f64 * 0.1).collect(); //损失仓位,从0到30%

    let fib_levels = ProfitStopLoss::get_fibonacci_level(inst_id, time);
    let stop_percent: Vec<f64> = vec![0.02]; //失仓位,从10%
    for stop in stop_percent.clone() {
        for kdj_period in 2..30 {
            for signal_period in 1..10 {
                let res = MacdKdjStrategy::run_test(
                    &mysql_candles,
                    &fib_levels,
                    stop,
                    kdj_period,
                    signal_period,
                )
                .await;
                println!("strategy{:#?}", res); // let ins_id = "BTC-USDT-SWAP";
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
                    strategy_detail: Some(format!(
                        "stop_loss_percent: {:?},kdj_period:{},signal_period:{}",
                        stop, kdj_period, signal_period
                    )),
                    profit: (final_fund - 100.00).to_string(),
                    one_bar_after_win_rate: 0.0,
                    two_bar_after_win_rate: 0.0,
                    three_bar_after_win_rate: 0.0,
                    four_bar_after_win_rate: 0.0,
                    five_bar_after_win_rate: 0.0,
                    ten_bar_after_win_rate: 0.0,
                };
                back_test_log::BackTestLogModel::new()
                    .await
                    .add(&back_test_log)
                    .await?;
            }
        }
    }
    Ok(())
}

pub async fn run_vegas_test(
    inst_id: &str,
    time: &str,
    mut strategy: VegasStrategy,
    strategy_config: BasicRiskStrategyConfig,
    mysql_candles: Arc<Vec<CandleItem>>,
) -> Result<i64, anyhow::Error> {
    let res = strategy.run_test(&mysql_candles, strategy_config);

    // 构建更详细的策略配置描述
    let config_desc = json!(strategy).to_string();

    // 保存测试日志并获取 back_test_id
    let back_test_id = save_log(inst_id, time, Some(config_desc), res).await?;

    // 返回 back_test_id
    Ok(back_test_id)
}

pub async fn save_log(
    inst_id: &str,
    time: &str,
    strategy_config_string: Option<String>,
    back_test_result: BackTestResult,
) -> Result<i64> {
    // 添加调试日志
    // info!(
    //     "save_log start: {} {} {}",
    //     inst_id, time, back_test_result.open_trades
    // );
    // 解包 Result 类型
    //把back tests strategy结果写入数据
    let back_test_log = BackTestLog {
        // 需要确定策略类型，这里使用参数传入或推断
        strategy_type: strategy_config_string
            .as_ref()
            .and_then(|s| {
                if s.contains("Vegas") {
                    Some("Vegas")
                } else {
                    None
                }
            })
            .unwrap_or("Unknown")
            .to_string(),
        inst_type: inst_id.parse().unwrap(),
        time: time.parse().unwrap(),
        final_fund: back_test_result.funds.to_string(), // 确保字段名称正确
        win_rate: back_test_result.win_rate.to_string(),
        open_positions_num: back_test_result.open_trades as i32,
        strategy_detail: strategy_config_string,
        profit: (back_test_result.funds - 100.00).to_string(), // 确保字段名称正确
        // 初始化为0，后续会通过分析更新
        one_bar_after_win_rate: 0.0,
        two_bar_after_win_rate: 0.0,
        three_bar_after_win_rate: 0.0,
        four_bar_after_win_rate: 0.0,
        five_bar_after_win_rate: 0.0,
        ten_bar_after_win_rate: 0.0,
    };

    // 保存日志
    let start_time = Instant::now();
    let back_test_id = back_test_log::BackTestLogModel::new()
        .await
        .add(&back_test_log)
        .await?;

    // if false {
        // 保存详细交易记录
        if !back_test_result.trade_records.is_empty() {
            save_test_detail(
                back_test_id,
                StrategyType::Vegas, // 确保选择正确的策略类型
                inst_id,
                time,
                back_test_result.trade_records,
            )
            .await?;
        }
    // }
    Ok(back_test_id)
}

pub async fn test_random_strategy(
    inst_id: &str,
    time: &str,
    arc_candle_item_clone: Arc<Vec<CandleItem>>,
    semaphore: Arc<Semaphore>,
) {
    // 参数范围
    let bb_periods = vec![12, 13, 14, 15, 16];
    let bb_multipliers = vec![2.0, 2.5, 3.0, 3.5, 4.0];

    let shadow_ratios = vec![0.6, 0.7, 0.8, 0.9];

    let volume_bar_nums = vec![3, 4, 5, 6];
    let volume_increase_ratios: Vec<f64> = (20..=41).map(|x| x as f64 * 0.1).collect();
    let volume_decrease_ratios: Vec<f64> = (20..=41).map(|x| x as f64 * 0.1).collect();
    let breakthrough_thresholds = vec![0.003];

    let rsi_periods = vec![8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
    let rsi_overboughts = vec![85.0, 86.0, 87.0, 88.0, 89.0, 90.0];
    let rsi_oversolds = vec![15.0, 16.0, 17.0, 18.0, 19.0, 20.0];

    // 将参数组合转换为扁平的迭代器
    info!("正在生成参数组合...");

    let mut param_generator = ParamGenerator::new(
        bb_periods,
        shadow_ratios,
        bb_multipliers,
        volume_bar_nums,
        volume_increase_ratios,
        volume_decrease_ratios,
        breakthrough_thresholds,
        rsi_periods,
        rsi_overboughts,
        rsi_oversolds,
    );

    let (_, total_count) = param_generator.progress();
    info!("总共需要处理 {} 个参数组合", total_count);

    // 批量处理
    let batch_size = 100;
    let mut batch_num = 0;
    let mut params_batch = Vec::new();

    loop {
        params_batch = param_generator.get_next_batch(batch_size);
        if params_batch.is_empty() {
            break; // 所有参数处理完毕
        }
        run_test_strategy(
            params_batch,
            inst_id,
            time,
            arc_candle_item_clone.clone(),
            semaphore.clone(),
        )
        .await;
    }
}

// 主函数，执行所有策略测试
pub async fn vegas_test(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    // 获取数据
    let mysql_candles = self::get_candle_data(inst_id, time, 20000, None).await?;

    let candle_item_vec: Vec<CandleItem> = mysql_candles
        .iter()
        .map(|candle| {
            let data_item = CandleItem::builder()
                .c(candle.c.parse::<f64>().unwrap_or(0.0))
                .o(candle.o.parse::<f64>().unwrap_or(0.0))
                .h(candle.h.parse::<f64>().unwrap_or(0.0))
                .l(candle.l.parse::<f64>().unwrap_or(0.0))
                .v(candle.vol_ccy.parse::<f64>().unwrap_or(0.0))
                .ts(candle.ts)
                .build()
                .unwrap();
            data_item
        })
        .collect();

    let arc_candle_item_clone = Arc::new(candle_item_vec.clone()); // 克隆一份用于后续分析

    let fibonacci_level = ProfitStopLoss::get_fibonacci_level(inst_id, time);
    let fibonacci_level_clone = Arc::new(fibonacci_level);

    // 创建信号量限制并发数
    let semaphore = Arc::new(Semaphore::new(30)); // 控制最大并发数量为 10

    // 测试随机策略
    // test_random_strategy(inst_id, time, arc_candle_item_clone.clone(), semaphore.clone()).await;
    //测试指定策略
    test_specified_strategy(
        inst_id,
        time,
        arc_candle_item_clone.clone(),
        semaphore.clone(),
    )
    .await;

    Ok(())
}

pub async fn test_specified_strategy(
    inst_id: &str,
    time: &str,
    arc_candle_item_clone: Arc<Vec<CandleItem>>,
    semaphore: Arc<Semaphore>,
) {
    let params_batch = vec![
        //btc
        ParamMerge::build()
            .shadow_ratio(0.6)
            .breakthrough_threshold(0.003)
            //bollinger bands
            .bb_periods(8).bb_multiplier(2.0)
            //volume
            .volume_bar_num(3).volume_increase_ratio(2.2).volume_decrease_ratio(2.2)
            //rsi
            .rsi_period(18).rsi_overbought(90.0).rsi_oversold(20.0),
        //eth
        ParamMerge::build()
            .shadow_ratio(0.8)
            .breakthrough_threshold(0.003)
            //bollinger bands
            .bb_periods(16).bb_multiplier(3.0)
            //volume
            .volume_bar_num(6).volume_increase_ratio(2.9).volume_decrease_ratio(3.4)
            //rsi
            .rsi_period(12).rsi_overbought(85.0).rsi_oversold(15.0),
        //ada
        ParamMerge::build()
            .shadow_ratio(0.6)
            .breakthrough_threshold(0.003)
            //bollinger bands
            .bb_periods(12).bb_multiplier(2.0)
            //volume
            .volume_bar_num(6).volume_increase_ratio(2.6).volume_decrease_ratio(4.1)
            //rsi
            .rsi_period(8).rsi_overbought(85.0).rsi_oversold(15.0),
        //om
        ParamMerge::build()
            .shadow_ratio(0.6)
            .breakthrough_threshold(0.003)
            //bollinger bands
            .bb_periods(14).bb_multiplier(2.5)
            //volume
            .volume_bar_num(3).volume_increase_ratio(3.0).volume_decrease_ratio(4.1)
            //rsi
            .rsi_period(8).rsi_overbought(85.0).rsi_oversold(15.0),
        //sol
        ParamMerge::build()
            .shadow_ratio(0.6)
            .breakthrough_threshold(0.003)
            //bollinger bands
            .bb_periods(14).bb_multiplier(2.0)
            //volume
            .volume_bar_num(6).volume_increase_ratio(2.9).volume_decrease_ratio(2.4)
            //rsi
            .rsi_period(10).rsi_overbought(85.0).rsi_oversold(15.0),
        //xrp
        ParamMerge::build()
            .shadow_ratio(0.6)
            .breakthrough_threshold(0.003)
            //bollinger bands
            .bb_periods(16).bb_multiplier(3.0)
            //volume
            .volume_bar_num(5).volume_increase_ratio(2.4).volume_decrease_ratio(3.6)
            //rsi
            .rsi_period(13).rsi_overbought(85.0).rsi_oversold(15.0),
        //sui
        ParamMerge::build()
            .shadow_ratio(0.9)
            .breakthrough_threshold(0.003)
            //bollinger bands
            .bb_periods(14).bb_multiplier(3.0)
            //volume
            .volume_bar_num(4).volume_increase_ratio(2.2).volume_decrease_ratio(4.1)
            //rsi
            .rsi_period(8).rsi_overbought(85.0).rsi_oversold(15.0),
    ];

//测试
   let params_batch = vec![
        //btc
        ParamMerge::build()
            .shadow_ratio(0.6)
            .breakthrough_threshold(0.003)
            //bollinger bands
            .bb_periods(8).bb_multiplier(2.8)
            //volume
            .volume_bar_num(3).volume_increase_ratio(2.2).volume_decrease_ratio(2.2)
            //rsi
            .rsi_period(18).rsi_overbought(90.0).rsi_oversold(20.0),
   ];

    run_test_strategy(
        params_batch,
        inst_id,
        time,
        arc_candle_item_clone.clone(),
        semaphore.clone(),
    )
    .await;
}

pub async fn run_test_strategy(
    params_batch: Vec<ParamMerge>,
    inst_id: &str,
    time: &str,
    arc_candle_item_clone: Arc<Vec<CandleItem>>,
    semaphore: Arc<Semaphore>,
) {
    let mut batch_tasks = Vec::with_capacity(params_batch.len());
    for param in params_batch {
        let bb_period = param.bb_period;
        let shadow_ratio = param.shadow_ratio;
        let bb_multiplier = param.bb_multiplier;
        let volume_bar_num = param.volume_bar_num;
        let volume_increase_ratio = param.volume_increase_ratio;
        let volume_decrease_ratio = param.volume_decrease_ratio;
        let breakthrough_threshold = param.breakthrough_threshold;
        let rsi_period = param.rsi_period;
        let rsi_overbought = param.rsi_overbought;
        let rsi_oversold = param.rsi_oversold;

        let risk_strategy_config = BasicRiskStrategyConfig {
            use_dynamic_tp: false,
            use_fibonacci_tp: true,
            max_loss_percent: 0.02,
            profit_threshold: 0.01,
            is_move_stop_loss: true,
            is_used_signal_k_line_stop_loss: false,
        };

        let volumn_signal = VolumeSignalConfig {
            volume_bar_num,
            volume_increase_ratio,
            volume_decrease_ratio,
            is_open: true,
            is_force_dependent: false,
        };

        let rsi_signal = RsiSignalConfig {
            rsi_length: rsi_period,
            rsi_oversold,
            rsi_overbought,
            is_open: true,
        };

        let ema_touch_trend_signal = EmaTouchTrendSignalConfig {
            is_open: true,
            ..Default::default()
        };

        let kline_hammer_signal = KlineHammerConfig {
            up_shadow_ratio: shadow_ratio,
            down_shadow_ratio: shadow_ratio,
            max_other_side_shadow_ratio: 0.1,
            body_ratio: 0.7,
        };

        let strategy = VegasStrategy {
            min_k_line_num: 3600,
            engulfing_signal: Some(EngulfingSignalConfig::default()),
            ema_signal: Some(EmaSignalConfig::default()),
            signal_weights: Some(SignalWeightsConfig::default()),
            volume_signal: Some(volumn_signal),
            ema_touch_trend_signal: Some(ema_touch_trend_signal),
            rsi_signal: Some(rsi_signal),
            bollinger_signal: Some(BollingerBandsSignalConfig {
                period: bb_period as usize,
                multiplier: bb_multiplier,
                is_open: true,
            }),
            kline_hammer_signal: Some(kline_hammer_signal),
        };

        let inst_id = inst_id.to_string();
        let time = time.to_string();
        let mysql_candles = Arc::clone(&arc_candle_item_clone);
        let permit = Arc::clone(&semaphore);

        // 创建任务
        batch_tasks.push(tokio::spawn(async move {
            let _permit = permit.acquire().await.unwrap();
            match run_vegas_test(
                &inst_id,
                &time,
                strategy,
                risk_strategy_config,
                mysql_candles,
            )
            .await
            {
                Ok(back_test_id) => Some(back_test_id),
                Err(e) => {
                    error!("Vegas test failed: {:?}", e);
                    None
                }
            }
        }));
    }
    // 等待当前批次完成
    let batch_start = Instant::now();
    join_all(batch_tasks).await;
    let batch_end = Instant::now();
    info!("当前批次完成，用时：{:?}", batch_end.duration_since(batch_start));
}
// // 主函数，执行所有策略测试
// pub async fn squeeze_test(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
//     // 获取数据
//     let mysql_candles = self::get_candle_data(inst_id, time, 2200, None).await?;
//     let mysql_candles_clone = Arc::new(mysql_candles);
//     // 创建信号量限制并发数
//     let semaphore = Arc::new(Semaphore::new(100)); // 控制最大并发数量为 100

//     // 灵敏度参数
//     let bb_lengths: Vec<usize> = (26..=30).collect();
//     let bb_multi_nums: Vec<f64> = (10..=60).map(|x| x as f64 * 0.1).collect();
//     let kc_lengths: Vec<usize> = (10..=30).collect();
//     let kc_multi_nums: Vec<f64> = (10..=60).map(|x| x as f64 * 0.1).collect();
//     let max_loss_percent: Vec<f64> = (5..=5).map(|x| x as f64 * 0.01).collect();

//     // 创建任务容器
//     let mut tasks = Vec::new();
//     for bb_length in bb_lengths {
//         for bb_multi in bb_multi_nums.clone() {
//             for kc_length in kc_lengths.clone() {
//                 for kc_multi in kc_multi_nums.clone() {
//                     for &max_loss in &max_loss_percent {
//                         let arc = mysql_candles_clone.clone();
//                         let inst_id_clone = inst_id.to_string();
//                         let time_clone = time.to_string();
//                         // 获取信号量，控制并发
//                         // 创建任务
//                         let permit = Arc::clone(&semaphore);

//                         tasks.push(tokio::spawn({
//                             // 持有 permit，直到异步任务结束才释放

//                             let inst_id = inst_id_clone.clone();
//                             let time = time_clone.clone();
//                             async move {
//                                 let _permit = permit.acquire().await.unwrap();
//                                 let config = squeeze_momentum::squeeze_config::SqueezeConfig {
//                                     bb_length,
//                                     bb_multi,
//                                     kc_length,
//                                     kc_multi,
//                                 };
//                                 let fibonacci_level =
//                                     ProfitStopLoss::get_fibonacci_level(&inst_id, &time);
//                                 let mut stregety = SqueezeCalculator::new(config.clone());

//                                 let res = stregety
//                                     .run_test(
//                                         &arc,
//                                         &fibonacci_level,
//                                         10.00,
//                                         false,
//                                         true,
//                                         true,
//                                         false,
//                                     )
//                                     .await;

//                                 let result =
//                                     save_log(&inst_id, &time, Some(config.to_string()), res).await;
//                                 if result.is_err() {
//                                     error!("保存日志异常")
//                                 }
//                             }
//                         }));
//                     }
//                 }
//             }
//         }
//     }
//     // 等待所有任务完成
//     join_all(tasks).await;
//     Ok(()

// // 主函数，执行所有策略测试
// pub async fn ut_boot_test(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
//     // 获取数据
//     let mysql_candles = self::get_candle_data(inst_id, time, 5000, None).await?;
//     let mysql_candles_clone = Arc::new(mysql_candles);

//     let fibonacci_level = ProfitStopLoss::get_fibonacci_level(inst_id, time);
//     let fibonacci_level_clone = Arc::new(fibonacci_level);

//     // 创建信号量限制并发数
//     let semaphore = Arc::new(Semaphore::new(100)); // 控制最大并发数量为 100

//     // 灵敏度参数
//     let key_values: Vec<f64> = (2..=80).map(|x| x as f64 * 0.1).collect();
//     let emas: Vec<usize> = (1..=3).map(|i| i).collect();
//     let max_loss_percent: Vec<f64> = (5..6).map(|x| x as f64 * 0.01).collect();

//     // 创建任务容器
//     let mut tasks = Vec::new();

//     // 遍历所有组合并为每个组合生成一个任务
//     for key_value in key_values {
//         for atr_period in 1..=15 {
//             let ema_clone = emas.clone();
//             for ema in ema_clone.into_iter() {
//                 for &max_loss in &max_loss_percent {
//                     let inst_id_clone = inst_id.to_string();
//                     let time_clone = time.to_string();
//                     let mysql_candles_clone = Arc::clone(&mysql_candles_clone);
//                     let fibonacci_level_clone = Arc::clone(&fibonacci_level_clone);
//                     let permit = Arc::clone(&semaphore);
//                     // 创建任务
//                     tasks.push(tokio::spawn({
//                         let inst_id_clone = inst_id_clone.clone();
//                         let time_clone = time_clone.clone();
//                         async move {
//                             // 获取信号量，控制并发
//                             let _permit = permit.acquire().await.unwrap();

//                             // 执行策略测试并处理结果
//                             if let Err(e) = run_test_strategy(
//                                 &inst_id_clone,
//                                 &time_clone,
//                                 key_value,
//                                 atr_period,
//                                 ema.clone(),
//                                 max_loss,
//                                 mysql_candles_clone,
//                                 fibonacci_level_clone,
//                             )
//                             .await
//                             {
//                                 error!("Strategy test failed: {:?}", e);
//                             }
//                         }
//                     }));
//                 }
//             }
//         }
//     }

//     // 等待所有任务完成
//     join_all(tasks).await;

//     Ok(())
// }

// 主函数，执行所有策略测试
pub async fn top_contract_test(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    // 创建信号量限制并发数
    let semaphore = Arc::new(Semaphore::new(100)); // 控制最大并发数量为 100

    // 灵敏度参数
    let key_values: Vec<f64> = (100..=250).map(|x| x as f64 * 0.01).collect();

    let max_loss_percent: Vec<f64> = (5..6).map(|x| x as f64 * 0.01).collect();

    // 创建任务容器
    let mut tasks = Vec::new();

    let mut strate = TopContractStrategy::new(&inst_id, &time).await?;

    let arc_starte = Arc::new(strate);
    // 遍历所有组合并为每个组合生成一个任务
    for key_value in key_values {
        for &max_loss in &max_loss_percent {
            let inst_id_clone = inst_id.to_string();
            let time_clone = time.to_string();
            // 获取信号量，控制并发
            let _permit = semaphore.acquire().await.unwrap();
            // 创建任务
            tasks.push(tokio::spawn({
                let inst_id = inst_id_clone.clone();
                let time = time_clone.clone();
                let strate_clone = Arc::clone(&arc_starte);
                async move {
                    let stra = TopContractStrategy {
                        data: strate_clone,
                        key_value,
                        atr_period: 0,
                        heikin_ashi: false,
                    };
                    let fibonacci_level = ProfitStopLoss::get_fibonacci_level(&inst_id, &time);

                    let res = stra
                        .run_test(&fibonacci_level, 10.00, false, true, false, false)
                        .await;
                    let result = save_log(
                        &inst_id,
                        &time,
                        Some(
                            TopContractStrategyConfig {
                                basic_ratio: key_value,
                            }
                            .to_string(),
                        ),
                        res,
                    )
                    .await;
                    if result.is_err() {
                        error!("保存日志异常")
                    }
                }
            }));
        }
    }

    // 等待所有任务完成
    join_all(tasks).await;

    Ok(())
}

pub async fn save_test_log(
    strategy_type: StrategyType,
    inst_id: &str,
    time: &str,
    final_fund: f64,
    win_rate: f64,
    open_position_num: i32,
    detail: Option<String>,
) -> Result<i64, anyhow::Error> {
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
        one_bar_after_win_rate: 0.0,
        two_bar_after_win_rate: 0.00,
        three_bar_after_win_rate: 0.0,
        four_bar_after_win_rate: 0.00,
        five_bar_after_win_rate: 0.0,
        ten_bar_after_win_rate: 0.0,
    };
    // println!("back_test_log:{:?}", back_test_log);
    let res = back_test_log::BackTestLogModel::new()
        .await
        .add(&back_test_log)
        .await?;
    Ok(res)
}

pub async fn save_test_detail(
    back_test_id: i64,
    strategy_type: StrategyType,
    inst_id: &str,
    time: &str,
    list: Vec<TradeRecord>,
) -> Result<u64, anyhow::Error> {
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
            full_close: trade_record.full_close.to_string(),
            close_type: trade_record.close_type,
            win_nums: trade_record.win_num,
            loss_nums: trade_record.loss_num,
            signal_value: trade_record.signal_value.unwrap_or_else(|| "".to_string()),
            signal_result: trade_record.signal_result.unwrap_or_else(|| "".to_string()),
        };
        array.push(back_test_log);
    }
    let res = back_test_detail::BackTestDetailModel::new()
        .await
        .batch_add(array)
        .await?;
    Ok(res)
}

pub async fn get_candle_data(
    inst_id: &str,
    period: &str,
    limit: usize,
    select_time: Option<SelectTime>,
) -> Result<Vec<CandlesEntity>, anyhow::Error> {
    let mysql_candles_5m = candles::CandlesModel::new()
        .await
        .fetch_candles_from_mysql(inst_id, period, limit, select_time)
        .await?;
    if mysql_candles_5m.is_empty() {
        return Err(anyhow!("mysql candles 5m is empty"));
    }
    let result = self::valid_candles_data(&mysql_candles_5m, period);
    if result.is_err() {
        return Err(anyhow!("mysql candles is error {}", result.err().unwrap()));
    }
    Ok(mysql_candles_5m)
}

//判断最新得数据是否所在当前时间的周期
pub fn valid_newest_candle_data(mysql_candles_5m: CandlesEntity, time: &str) -> bool {
    let ts = mysql_candles_5m.ts;
    // 将毫秒时间戳转换为 DateTime<Utc>
    let datetime: DateTime<Local> = time_util::mill_time_to_local_datetime(ts);

    let date = time_util::format_to_period(time, Some(datetime));
    let current_date = time_util::format_to_period(time, None);
    // 比较时间戳的小时与当前小时
    if date != current_date {
        error!("数据库最新数据的时间ts:{} date:({}) 不等于当前最新时间 local time:({}), 跳过,candles:{:?},time:{}", ts, date, current_date, mysql_candles_5m, time);
        return false;
    }
    return true;
}

/**
 * 验证蜡烛图数据是否正确
 */
pub fn valid_candles_data(mysql_candles_5m: &Vec<CandlesEntity>, time: &str) -> Result<()> {
    //验证头尾数据正确性
    let first_timestamp = mysql_candles_5m.first().unwrap().ts;
    let last_timestamp = mysql_candles_5m.last().unwrap().ts;
    let difference = last_timestamp - first_timestamp;
    let period_milliseconds = time_util::parse_period_to_mill(time)?;
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

/**
 * 获取指定的产品策略配置
 */
pub async fn get_strate_config(inst_id: &str, time: &str) -> Result<Vec<StrategyConfigEntity>> {
    //从策略配置中获取到对应的产品配置
    let strategy_config = StrategyConfigEntityModel::new()
        .await
        .get_config(None, inst_id, time)
        .await?;
    if strategy_config.len() < 1 {
        warn!("策略配置为空inst_id:{:?} time:{:?}", inst_id, time);
        return Ok(vec![]);
    }
    Ok(strategy_config)
}

// 定义一个枚举来封装不同的策略类型
enum RealStrategy {
    UtBoot(UtBootStrategy),
    Engulfing(EngulfingStrategy),
}

/** 执行ut boot 策略 任务**/
pub async fn run_strategy_job(
    inst_id: &str,
    time: &str,
    strategy: &VegasStrategy,
    cell: &OnceCell<RwLock<HashMap<String, ArcVegasIndicatorValues>>>,
) -> Result<(), anyhow::Error> {
    //实际执行
    info!("run_strategy_job开始: inst_id={}, time={}", inst_id, time);

    // 检查cell是否已初始化
    let cell_initialized = cell.get().is_none();
    if cell_initialized {
        error!("OnceCell未初始化");
        return Err(anyhow!("OnceCell未初始化"));
    } else {
        info!("OnceCell已初始化");
    }

    let res = self::run_ready_to_order(inst_id, time, strategy, cell).await;

    if let Err(e) = res {
        error!(
            "run ready to order inst_id:{:?} time:{:?} error:{:?}",
            inst_id, time, e
        );
        return Err(anyhow!(e));
    }

    info!("run_strategy_job完成: inst_id={}, time={}", inst_id, time);
    Ok(())
}

/// 运行准备好的订单函数
pub async fn run_ready_to_order(
    inst_id: &str,
    period: &str,
    strategy: &VegasStrategy,
    arc_vegas_indicator_signal_values: &OnceCell<RwLock<HashMap<String, ArcVegasIndicatorValues>>>,
) -> anyhow::Result<()> {
    // 常量定义
    const MAX_HISTORY_SIZE: usize = 10000;

    // 1. 预处理：获取哈希键和RwLock
    let strategy_type = StrategyType::Vegas.to_string();
    let key = get_hash_key(inst_id, period, &strategy_type);
    let values_rwlock =
        arc_vegas_indicator_signal_values.get_or_init(|| RwLock::new(HashMap::new()));

    // 2. 获取最新K线数据
    let candle_list = task::basic::get_candle_data(inst_id, period, 1, None)
        .await
        .map_err(|e| anyhow!("获取最新K线数据失败: {}", e))?;

    if candle_list.is_empty() {
        return Err(anyhow!("获取的K线列表为空"));
    }

    let new_candle_item = parse_candle_to_data_item(&candle_list[0]);

    // 3. 读取现有数据并验证
    let current_data = {
        let read_guard = values_rwlock.read().await;
        match read_guard.get(&key) {
            Some(value) => value.clone(),
            None => {
                return Err(anyhow!("没有找到对应的策略值: {}", key));
            }
        }
    };

    // 4. 验证时间戳，检查是否有新数据
    let old_time = current_data.timestamp;
    let new_time = new_candle_item.ts;

    if old_time == new_time {
        info!("未检测到新的K线数据，等待下次更新");
        return Ok(());
    }

    // 验证时间差是否为一个周期（记录警告，中断执行）
    if let Ok(period_diff) = ts_add_n_period(old_time, period, 1) {
        if period_diff != new_time {
            warn!(
                "K线时间戳不连续: 上一时间戳 {}, 当前时间戳 {}, 预期时间戳 {}",
                old_time, new_time, period_diff
            );
            return Err(anyhow!(
                "K线时间戳不连续: 上一时间戳 {}, 当前时间戳 {}, 预期时间戳 {}",
                old_time,
                new_time,
                period_diff
            ));
        }
    }

    // 5. 准备更新数据
    let mut candle_items = current_data.candle_item.clone();
    candle_items.push(new_candle_item.clone());

    // 限制历史数据大小
    if candle_items.len() > MAX_HISTORY_SIZE {
        candle_items = candle_items.split_off(candle_items.len() - MAX_HISTORY_SIZE);
    }

    // 6. 更新K线数据到全局存储
    if let Err(e) = update_candle_items(&key, candle_items.clone()).await {
        return Err(anyhow!("更新K线数据失败: {}", e));
    }

    // 7. 计算最新指标值
    let mut indicator_combines = current_data.indicator_combines.clone();
    let new_indicator_values =
        strategy_common::get_multi_indivator_values(&mut indicator_combines, &new_candle_item);

    // 8. 更新指标值
    if let Err(e) = update_vegas_indicator_values(&key, indicator_combines).await {
        return Err(anyhow!("更新指标值失败: {}", e));
    }

    // 9. 获取更新后的数据进行信号计算
    let updated_data = {
        let read_guard = values_rwlock.read().await;
        match read_guard.get(&key) {
            Some(value) => value.clone(),
            None => {
                return Err(anyhow!("无法读取更新后的数据"));
            }
        }
    };

    // 10. 计算交易信号
    let signal_result = strategy.get_trade_signal(
        &updated_data.candle_item,
        &mut new_indicator_values.clone(),
        &SignalWeightsConfig::default(),
        &strategy_common::BasicRiskStrategyConfig::default(),
    );

    print!("signal_result:{:#?}", signal_result);
    //记录日志
    let signal_record = StrategyJobSignalLog {
        inst_id: inst_id.parse().unwrap(),
        time: period.parse().unwrap(),
        strategy_type: StrategyType::Vegas.to_string(),
        strategy_result: serde_json::to_string(&signal_result).unwrap(),
    };
    strategy_job_signal_log::StrategyJobSignalLogModel::new()
        .await
        .add(signal_record)
        .await?;

    SwapOrder::new().ready_to_order(StrategyType::Vegas, inst_id, period, signal_result).await?;
    Ok(())
}
