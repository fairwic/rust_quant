use anyhow::anyhow;
use chrono::{DateTime, Duration, Local, TimeZone, Timelike, Utc};
use hmac::digest::generic_array::arr;
use std::cmp::PartialEq;
use std::collections::{HashMap, VecDeque};
use std::env;
use std::sync::{Arc, Mutex};
use ta::indicators::BollingerBands;
use tokio::sync::{RwLock, Semaphore};
use tokio::task::spawn;
use tracing::{error, info, warn, Level};

use crate::time_util::{self, ts_add_n_period};
use crate::trading::indicator::bollings::BollingBandsSignalConfig;
use crate::trading::indicator::signal_weight::SignalWeightsConfig;
use crate::trading::model::entity::candles::enums::SelectTime;
use crate::trading::model::entity::candles::enums::TimeDirect;
use crate::trading::model::order::swap_order::SwapOrderEntityModel;
use crate::trading::model::strategy::back_test_detail::BackTestDetail;
use crate::trading::model::strategy::strategy_config::*;
use crate::trading::model::strategy::strategy_job_signal_log::StrategyJobSignalLog;
use crate::trading::model::strategy::{back_test_detail, strategy_job_signal_log};
use crate::trading::strategy::comprehensive_strategy::ComprehensiveStrategy;
use crate::trading::strategy::strategy_common::{
    get_multi_indicator_values, parse_candle_to_data_item, BackTestResult, BasicRiskStrategyConfig,
    SignalResult, TradeRecord,
};
use crate::trading::strategy::ut_boot_strategy::UtBootStrategy;
use crate::trading::strategy::{self, strategy_common};
use crate::trading::strategy::{engulfing_strategy, Strategy};
use crate::trading::task;
use okx::api::account::OkxAccount;
use okx::dto::account::account_dto::SetLeverageRequest;

use super::job_param_generator::ParamMergeBuilder;
use crate::app_config::db;
use crate::time_util::millis_time_diff;
use crate::trading::analysis::position_analysis::PositionAnalysis;
use crate::trading::domain_service::candle_domain_service::CandleDomainService;
use crate::trading::indicator::squeeze_momentum;
use crate::trading::indicator::squeeze_momentum::calculator::SqueezeCalculator;
use crate::trading::indicator::vegas_indicator::{
    EmaSignalConfig, EmaTouchTrendSignalConfig, EngulfingSignalConfig, KlineHammerConfig,
    RsiSignalConfig, VegasIndicatorSignalValue, VegasStrategy, VolumeSignalConfig,
};
use crate::trading::model::entity::candles;
use crate::trading::model::entity::candles::dto::SelectCandleReqDto;
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::trading::model::market::candles::CandlesModel;
use crate::trading::model::strategy::back_test_log;
use crate::trading::model::strategy::back_test_log::BackTestLog;
use crate::trading::services::order_service::swap_order_service::SwapOrderService;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::get_hash_key;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::update_candle_items;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::update_vegas_indicator_values;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::{
    self, ArcVegasIndicatorValues,
};
use crate::trading::strategy::engulfing_strategy::EngulfingStrategy;
use crate::trading::strategy::macd_kdj_strategy::MacdKdjStrategy;
use crate::trading::strategy::order::strategy_config::StrategyConfig;
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
use log::debug;
use okx::api::api_trait::OkxApiTrait;
use okx::api::trade::OkxTrade;
use okx::dto::trade_dto::TdModeEnum;
use okx::dto::{EnumToStrTrait, PositionSide};
use once_cell::sync::OnceCell;
use rbatis::dark_std::err;
use rbatis::dark_std::errors::new;
use redis::AsyncCommands;
use serde_json::json;
use tokio;
use tokio::time::Instant;
use tracing::span;
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

pub async fn run_vegas_test(
    inst_id: &str,
    time: &str,
    mut strategy: VegasStrategy,
    risk_strategy_config: BasicRiskStrategyConfig,
    mysql_candles: Arc<Vec<CandleItem>>,
) -> Result<i64, anyhow::Error> {
    let start_time = Instant::now();

    // 策略测试阶段
    let res = strategy.run_test(&mysql_candles, risk_strategy_config);

    // 配置描述构建阶段
    let config_desc = json!(strategy).to_string();

    // 保存测试日志并获取 back_test_id
    let back_test_id = save_log(
        inst_id,
        time,
        Some(config_desc),
        res,
        mysql_candles,
        risk_strategy_config,
    )
    .await?;

    // 返回 back_test_id
    Ok(back_test_id)
}

pub async fn save_log(
    inst_id: &str,
    time: &str,
    strategy_config_string: Option<String>,
    back_test_result: BackTestResult,
    mysql_candles: Arc<Vec<CandleItem>>,
    risk_strategy_config: BasicRiskStrategyConfig,
) -> Result<i64> {
    // 构建日志对象阶段
    let back_test_log = BackTestLog {
        // 需要确定策略类型，这里使用参数传入或推断
        strategy_type: "vegas".to_string(),
        inst_type: inst_id.parse().unwrap(),
        time: time.parse().unwrap(),
        final_fund: back_test_result.funds.to_string(), // 确保字段名称正确
        win_rate: back_test_result.win_rate.to_string(),
        open_positions_num: back_test_result.open_trades as i32,
        strategy_detail: strategy_config_string,
        risk_config_detail: json!(risk_strategy_config).to_string(),
        profit: (back_test_result.funds - 100.00).to_string(), // 确保字段名称正确
        // 初始化为0，后续会通过分析更新
        one_bar_after_win_rate: 0.0,
        two_bar_after_win_rate: 0.0,
        three_bar_after_win_rate: 0.0,
        four_bar_after_win_rate: 0.0,
        five_bar_after_win_rate: 0.0,
        ten_bar_after_win_rate: 0.0,
        kline_start_time: mysql_candles[0].ts,
        kline_end_time: mysql_candles.last().unwrap().ts,
        kline_nums: mysql_candles.len() as i32,
    };
    // 保存日志到数据库阶段
    let back_test_id = back_test_log::BackTestLogModel::new()
        .await
        .add(&back_test_log)
        .await?;

    if false {
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
    }
    Ok(back_test_id)
}

/// 随机策略测试配置
#[derive(Debug, Clone)]
pub struct RandomStrategyConfig {
    pub bb_periods: Vec<i32>,
    pub bb_multipliers: Vec<f64>,
    pub shadow_ratios: Vec<f64>,
    pub volume_bar_nums: Vec<usize>,
    pub volume_increase_ratios: Vec<f64>,
    pub volume_decrease_ratios: Vec<f64>,
    pub breakthrough_thresholds: Vec<f64>,
    pub rsi_periods: Vec<usize>,
    pub rsi_over_buy: Vec<f64>,
    pub rsi_over_sold: Vec<f64>,
    pub batch_size: usize,
    //risk
    pub max_loss_percent: Vec<f64>,
    pub is_take_profit: Vec<bool>,
    pub is_move_stop_loss: Vec<bool>,
    pub is_used_signal_k_line_stop_loss: Vec<bool>,
}

impl Default for RandomStrategyConfig {
    fn default() -> Self {
        Self {
            bb_periods: vec![10, 11, 12, 13, 14, 15, 16],
            bb_multipliers: vec![2.0, 2.5, 3.0, 3.1, 3.2],
            shadow_ratios: vec![0.7, 0.75, 0.8, 0.85, 0.9],
            volume_bar_nums: vec![4, 5, 6],
            volume_increase_ratios: (16..=25).map(|x| x as f64 * 0.1).collect(),
            volume_decrease_ratios: (16..=25).map(|x| x as f64 * 0.1).collect(),
            breakthrough_thresholds: vec![0.003],
            rsi_periods: vec![8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20],
            rsi_over_buy: vec![85.0, 86.0, 87.0, 88.0, 89.0, 90.0],
            rsi_over_sold: vec![15.0, 16.0, 17.0, 18.0, 19.0, 20.0],
            batch_size: 100,
            //risk
            max_loss_percent: vec![0.03, 0.04, 0.05, 0.06, 0.07, 0.08, 0.09, 0.1],
            is_take_profit: vec![true, false],
            is_move_stop_loss: vec![false, true],
            is_used_signal_k_line_stop_loss: vec![true, false],
        }
    }
}

pub async fn test_random_strategy(
    inst_id: &str,
    time: &str,
    semaphore: Arc<Semaphore>,
) -> Result<(), anyhow::Error> {
    test_random_strategy_with_config(inst_id, time, semaphore, RandomStrategyConfig::default())
        .await
}

pub async fn test_random_strategy_with_config(
    inst_id: &str,
    time: &str,
    semaphore: Arc<Semaphore>,
    config: RandomStrategyConfig,
) -> Result<(), anyhow::Error> {
    let start_time = Instant::now();
    info!(
        "[性能跟踪] test_random_strategy_with_config 开始: inst_id={}, time={}",
        inst_id, time
    );

    // 构建参数生成器
    let param_gen_start = Instant::now();
    let mut param_generator = ParamGenerator::new(
        config.bb_periods,
        config.shadow_ratios,
        config.bb_multipliers,
        config.volume_bar_nums,
        config.volume_increase_ratios,
        config.volume_decrease_ratios,
        config.breakthrough_thresholds,
        config.rsi_periods,
        config.rsi_over_buy,
        config.rsi_over_sold,
        config.max_loss_percent,
        config.is_take_profit,
        config.is_move_stop_loss,
        config.is_used_signal_k_line_stop_loss,
    );

    let (_, total_count) = param_generator.progress();
    let param_gen_duration = param_gen_start.elapsed();
    info!(
        "[性能跟踪] 参数生成器创建完成 - 耗时: {}ms, 总参数组合: {}",
        param_gen_duration.as_millis(),
        total_count
    );

    // 获取并转换K线数据
    let arc_candle_data = load_and_convert_candle_data(inst_id, time, 20000).await?;

    // 批量处理参数组合
    let mut processed_count = 0;
    let batch_processing_start = Instant::now();
    loop {
        let batch_start = Instant::now();
        let params_batch = param_generator.get_next_batch(config.batch_size);
        if params_batch.is_empty() {
            break;
        }

        run_back_test_strategy(
            params_batch,
            inst_id,
            time,
            arc_candle_data.clone(),
            semaphore.clone(),
        )
        .await;

        processed_count += config.batch_size;
        let batch_duration = batch_start.elapsed();
        info!(
            "[性能跟踪] 批次处理完成 - 已处理 {}/{} 个参数组合, 本批次耗时: {}ms",
            processed_count.min(total_count),
            total_count,
            batch_duration.as_millis()
        );
    }

    let batch_processing_duration = batch_processing_start.elapsed();
    let total_duration = start_time.elapsed();
    info!(
        "[性能跟踪] test_random_strategy_with_config 完成 - 总耗时: {}ms, 参数生成: {}ms, 批量处理: {}ms, 处理组合数: {}",
        total_duration.as_millis(),
        param_gen_duration.as_millis(),
        batch_processing_duration.as_millis(),
        total_count
    );
    Ok(())
}

/// 加载并转换K线数据的辅助函数
async fn load_and_convert_candle_data(
    inst_id: &str,
    time: &str,
    limit: usize,
) -> Result<Arc<Vec<CandleItem>>, anyhow::Error> {
    let start_time = Instant::now();
    info!(
        "[性能跟踪] 开始加载K线数据: inst_id={}, time={}, limit={}",
        inst_id, time, limit
    );

    let data_fetch_start = Instant::now();
    let mysql_candles = get_candle_data_confirm(inst_id, time, limit, None)
        .await
        .map_err(|e| anyhow!("获取K线数据失败: {}", e))?;
    let data_fetch_duration = data_fetch_start.elapsed();

    if mysql_candles.is_empty() {
        return Err(anyhow!("K线数据为空"));
    }

    let data_convert_start = Instant::now();
    let candle_item_vec: Vec<CandleItem> = mysql_candles
        .iter()
        .map(|candle| {
            CandleItem::builder()
                .c(candle.c.parse::<f64>().unwrap_or(0.0))
                .o(candle.o.parse::<f64>().unwrap_or(0.0))
                .h(candle.h.parse::<f64>().unwrap_or(0.0))
                .l(candle.l.parse::<f64>().unwrap_or(0.0))
                .v(candle.vol_ccy.parse::<f64>().unwrap_or(0.0))
                .ts(candle.ts)
                .build()
                .unwrap_or_else(|e| {
                    warn!("构建CandleItem失败: {}, 跳过该条记录", e);
                    // 返回一个有效的默认CandleItem
                    CandleItem::builder()
                        .c(0.0)
                        .o(0.0)
                        .h(0.0)
                        .l(0.0)
                        .v(0.0)
                        .ts(0)
                        .build()
                        .unwrap()
                })
        })
        .collect();
    let data_convert_duration = data_convert_start.elapsed();

    let total_duration = start_time.elapsed();
    info!(
        "[性能跟踪] K线数据加载完成 - 总耗时: {}ms, 数据获取: {}ms, 数据转换: {}ms, 数据条数: {}",
        total_duration.as_millis(),
        data_fetch_duration.as_millis(),
        data_convert_duration.as_millis(),
        candle_item_vec.len()
    );
    Ok(Arc::new(candle_item_vec))
}

/// Vegas 策略回测配置
#[derive(Debug, Clone)]
pub struct VegasBackTestConfig {
    /// 最大并发数
    pub max_concurrent: usize,
    /// K线数据限制
    pub candle_limit: usize,
    /// 是否启用随机策略测试
    pub enable_random_test: bool,
    /// 是否启用指定策略测试
    pub enable_specified_test: bool,
}

impl Default for VegasBackTestConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 15,
            candle_limit: 20000,
            enable_random_test: true,
            enable_specified_test: false,
        }
    }
}

/// 主函数，执行所有策略测试
pub async fn vegas_back_test(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    let start_time = Instant::now();
    info!(
        "[性能跟踪] vegas_back_test 开始 - inst_id: {}, time: {}",
        inst_id, time
    );

    let result = vegas_back_test_with_config(inst_id, time, VegasBackTestConfig::default()).await;

    let duration = start_time.elapsed();
    info!(
        "[性能跟踪] vegas_back_test 完成 - 总耗时: {}ms",
        duration.as_millis()
    );

    result
}

/// 带配置的 Vegas 策略回测
pub async fn vegas_back_test_with_config(
    inst_id: &str,
    time: &str,
    config: VegasBackTestConfig,
) -> Result<(), anyhow::Error> {
    let start_time = Instant::now();
    info!(
        "[性能跟踪] vegas_back_test_with_config 开始 - inst_id={}, time={}, config={:?}",
        inst_id, time, config
    );

    // 验证输入参数
    if inst_id.is_empty() || time.is_empty() {
        return Err(anyhow!(
            "无效的输入参数: inst_id={}, time={}",
            inst_id,
            time
        ));
    }

    // 创建信号量限制并发数
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent));

    // 执行不同类型的测试
    let mut test_results = Vec::new();

    if config.enable_random_test {
        let random_start = Instant::now();
        info!("[性能跟踪] 开始执行随机策略测试");
        if let Err(e) = test_random_strategy(inst_id, time, semaphore.clone()).await {
            error!("随机策略测试失败: {}", e);
            test_results.push(("random", false));
        } else {
            test_results.push(("random", true));
        }
        let random_duration = random_start.elapsed();
        info!(
            "[性能跟踪] 随机策略测试完成 - 耗时: {}ms",
            random_duration.as_millis()
        );
    }

    if config.enable_specified_test {
        if let Err(e) = test_specified_strategy(inst_id, time, semaphore.clone()).await {
            error!("指定策略测试失败: {}", e);
            test_results.push(("specified", false));
        } else {
            test_results.push(("specified", true));
        }
    }

    // 汇总测试结果
    let success_count = test_results.iter().filter(|(_, success)| *success).count();
    let total_count = test_results.len();

    let total_duration = start_time.elapsed();
    info!(
        "[性能跟踪] vegas_back_test_with_config 完成 - 总耗时: {}ms, 成功 {}/{}, 详情: {:?}",
        total_duration.as_millis(),
        success_count,
        total_count,
        test_results
    );

    if success_count == 0 && total_count > 0 {
        return Err(anyhow!("所有策略测试都失败了"));
    }

    Ok(())
}
pub async fn get_strategy_config_from_db(
    inst_id: &str,
    time: &str,
) -> Result<Vec<ParamMergeBuilder>, anyhow::Error> {
    // 从数据库获取策略配置
    let strategy_configs = get_strate_config(inst_id, time)
        .await
        .map_err(|e| anyhow!("获取策略配置失败: {}", e))?;

    if strategy_configs.is_empty() {
        warn!("未找到策略配置: inst_id={}, time={}", inst_id, time);
        return Ok(vec![]);
    }
    let mut conversion_errors = 0;
    let mut params_batch = Vec::with_capacity(strategy_configs.len());

    info!("找到 {} 个策略配置", strategy_configs.len());
    for config in strategy_configs.iter() {
        match convert_strategy_config_to_param(config) {
            Ok(param) => params_batch.push(param),
            Err(e) => {
                error!("转换策略配置失败: {}, config_id: {:?}", e, config.id);
                conversion_errors += 1;
            }
        }
    }
    Ok(params_batch)
}
pub async fn test_specified_strategy_with_config(
    inst_id: &str,
    time: &str,
) -> Result<Vec<ParamMergeBuilder>, anyhow::Error> {
    //1Dutc
    let params_batch = vec![ParamMergeBuilder::build()
        .bb_multiplier(2.0)
        .bb_periods(10)
        .hammer_shadow_ratio(0.9)
        .breakthrough_threshold(0.003)
        .volume_bar_num(4)
        .volume_increase_ratio(2.0)
        .volume_decrease_ratio(2.5)
        .rsi_period(9)
        .rsi_overbought(90.0)
        .rsi_oversold(20.0)
        .max_loss_percent(0.03)
        .is_take_profit(true)
        .is_move_stop_loss(true)
        .is_used_signal_k_line_stop_loss(true)];
    //1H
    let params_batch = vec![ParamMergeBuilder::build()
        .bb_periods(13)
        .bb_multiplier(2.5)
        .hammer_shadow_ratio(0.6)
        .breakthrough_threshold(0.003)
        .volume_bar_num(6)
        .volume_increase_ratio(2.4)
        .volume_decrease_ratio(2.0)
        .rsi_period(9)
        .rsi_overbought(85.0)
        .rsi_oversold(15.0)
        .max_loss_percent(0.02)
        .is_take_profit(true)
        .is_move_stop_loss(false)
        .is_used_signal_k_line_stop_loss(true)];
    Ok(params_batch)
}

pub async fn test_specified_strategy(
    inst_id: &str,
    time: &str,
    semaphore: Arc<Semaphore>,
) -> Result<(), anyhow::Error> {
    let start_time = Instant::now();
    info!(
        "[性能跟踪] test_specified_strategy 开始: inst_id={}, time={}",
        inst_id, time
    );

    // 获取策略配置阶段
    let config_get_start = Instant::now();
    let params_batch = get_strategy_config_from_db(inst_id, time).await?;
    let config_get_duration = config_get_start.elapsed();
    info!(
        "[性能跟踪] 策略配置获取完成 - 耗时: {}ms, 配置数量: {}",
        config_get_duration.as_millis(),
        params_batch.len()
    );

    // 加载K线数据阶段
    let arc_candle_data = load_and_convert_candle_data(inst_id, time, 20000).await?;

    // 执行回测阶段
    let backtest_start = Instant::now();
    run_back_test_strategy(params_batch, inst_id, time, arc_candle_data, semaphore).await;
    let backtest_duration = backtest_start.elapsed();

    let total_duration = start_time.elapsed();
    info!(
        "[性能跟踪] test_specified_strategy 完成 - 总耗时: {}ms, 配置获取: {}ms, 回测执行: {}ms",
        total_duration.as_millis(),
        config_get_duration.as_millis(),
        backtest_duration.as_millis()
    );
    Ok(())
}

/// 转换策略配置为参数的辅助函数
fn convert_strategy_config_to_param(
    config: &StrategyConfigEntity,
) -> Result<ParamMergeBuilder, anyhow::Error> {
    let vegas_strategy = serde_json::from_str::<VegasStrategy>(&config.value)
        .map_err(|e| anyhow!("解析策略配置JSON失败: {}", e))?;

    let risk_config = serde_json::from_str::<BasicRiskStrategyConfig>(&config.risk_config)?;

    // 安全地提取配置值，避免unwrap
    let kline_hammer = vegas_strategy
        .kline_hammer_signal
        .ok_or_else(|| anyhow!("缺少kline_hammer_signal配置"))?;

    let ema_signal = vegas_strategy
        .ema_signal
        .ok_or_else(|| anyhow!("缺少ema_signal配置"))?;

    let bolling_signal = vegas_strategy
        .bolling_signal
        .as_ref()
        .ok_or_else(|| anyhow!("缺少bolling_signal配置"))?;

    let volume_signal = vegas_strategy
        .volume_signal
        .ok_or_else(|| anyhow!("缺少volume_signal配置"))?;

    let rsi_signal = vegas_strategy
        .rsi_signal
        .ok_or_else(|| anyhow!("缺少rsi_signal配置"))?;

    let param = ParamMergeBuilder::build()
        .hammer_shadow_ratio(kline_hammer.up_shadow_ratio)
        .breakthrough_threshold(ema_signal.ema_breakthrough_threshold)
        .bb_periods(bolling_signal.period as i32)
        .bb_multiplier(bolling_signal.multiplier)
        .volume_bar_num(volume_signal.volume_bar_num)
        .volume_increase_ratio(volume_signal.volume_increase_ratio)
        .volume_decrease_ratio(volume_signal.volume_decrease_ratio)
        .rsi_period(rsi_signal.rsi_length)
        .rsi_overbought(rsi_signal.rsi_overbought)
        .rsi_oversold(rsi_signal.rsi_oversold)
        .kline_start_time(config.kline_start_time)
        .kline_end_time(config.kline_end_time)
        //risk
        .max_loss_percent(risk_config.max_loss_percent)
        .is_take_profit(risk_config.is_take_profit)
        .is_move_stop_loss(risk_config.is_one_k_line_diff_stop_loss)
        .is_used_signal_k_line_stop_loss(risk_config.is_used_signal_k_line_stop_loss);

    Ok(param)
}

pub async fn run_back_test_strategy(
    params_batch: Vec<ParamMergeBuilder>,
    inst_id: &str,
    time: &str,
    arc_candle_item_clone: Arc<Vec<CandleItem>>,
    semaphore: Arc<Semaphore>,
) {
    let mut batch_tasks = Vec::with_capacity(params_batch.len());
    for param in params_batch {
        let bb_period = param.bb_period;
        let shadow_ratio = param.hammer_shadow_ratio;
        let bb_multiplier = param.bb_multiplier;
        let volume_bar_num = param.volume_bar_num;
        let volume_increase_ratio = param.volume_increase_ratio;
        let volume_decrease_ratio = param.volume_decrease_ratio;
        let breakthrough_threshold = param.breakthrough_threshold;
        let rsi_period = param.rsi_period;
        let rsi_overbought = param.rsi_overbought;
        let rsi_oversold = param.rsi_oversold;

        let risk_strategy_config = BasicRiskStrategyConfig {
            max_loss_percent: param.max_loss_percent,
            is_take_profit: param.is_take_profit,
            is_one_k_line_diff_stop_loss: param.is_move_stop_loss,
            is_used_signal_k_line_stop_loss: param.is_used_signal_k_line_stop_loss,
        };

        let volume_signal = VolumeSignalConfig {
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
        };

        let strategy = VegasStrategy {
            period: time.to_string(),
            min_k_line_num: 3600,
            engulfing_signal: Some(EngulfingSignalConfig::default()),
            ema_signal: Some(EmaSignalConfig::default()),
            signal_weights: Some(SignalWeightsConfig::default()),
            volume_signal: Some(volume_signal),
            ema_touch_trend_signal: Some(ema_touch_trend_signal),
            rsi_signal: Some(rsi_signal),
            bolling_signal: Some(BollingBandsSignalConfig {
                period: bb_period as usize,
                multiplier: bb_multiplier,
                is_open: true,
                consecutive_touch_times: 4,
            }),
            kline_hammer_signal: Some(kline_hammer_signal),
        };

        let inst_id = inst_id.to_string();
        let time = time.to_string();
        let mysql_candles = Arc::clone(&arc_candle_item_clone);
        let permit = Arc::clone(&semaphore);

        // 创建任务
        batch_tasks.push(tokio::spawn(async move {
            let _permit: tokio::sync::SemaphorePermit<'_> = permit.acquire().await.unwrap();
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
    join_all(batch_tasks).await;
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
            strategy_type: strategy_type.as_str().to_owned(),
            inst_id: inst_id.to_string(),
            time: time.to_string(),
            open_position_time: trade_record.open_position_time.to_string(),
            close_position_time: match trade_record.close_position_time {
                Some(x) => x.to_string(),
                None => "".to_string(),
            },
            open_price: trade_record.open_price.to_string(),
            close_price: if trade_record.close_price.is_some() {
                Some(trade_record.close_price.unwrap().to_string())
            } else {
                None
            },
            profit_loss: trade_record.profit_loss.to_string(),
            quantity: trade_record.quantity.to_string(),
            full_close: trade_record.full_close.to_string(),
            close_type: trade_record.close_type,
            win_nums: trade_record.win_num,
            loss_nums: trade_record.loss_num,
            signal_status: trade_record.signal_status,
            signal_open_position_time: trade_record.signal_open_position_time.clone(),
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

pub async fn get_candle_data_confirm(
    inst_id: &str,
    period: &str,
    limit: usize,
    select_time: Option<SelectTime>,
) -> Result<Vec<CandlesEntity>, anyhow::Error> {
    let start_time = Instant::now();

    let dto_build_start = Instant::now();
    let dto = SelectCandleReqDto {
        inst_id: inst_id.to_string(),
        time_interval: period.to_string(),
        limit,
        select_time,
        confirm: Some(1),
    };
    let dto_build_duration = dto_build_start.elapsed();

    let db_query_start = Instant::now();
    let mysql_candles_5m = CandlesModel::new()
        .await
        .fetch_candles_from_mysql(dto)
        .await?;
    let db_query_duration = db_query_start.elapsed();

    if mysql_candles_5m.is_empty() {
        return Err(anyhow!("mysql candles 5m is empty"));
    }

    let validation_start = Instant::now();
    let result = self::valid_candles_data(&mysql_candles_5m, period);
    let validation_duration = validation_start.elapsed();

    if result.is_err() {
        return Err(anyhow!("mysql candles is error {}", result.err().unwrap()));
    }

    let total_duration = start_time.elapsed();
    info!(
        "[性能跟踪] get_candle_data_confirm 完成 - 总耗时: {}ms, DTO构建: {}ms, 数据库查询: {}ms, 数据验证: {}ms, 数据条数: {}",
        total_duration.as_millis(),
        dto_build_duration.as_millis(),
        db_query_duration.as_millis(),
        validation_duration.as_millis(),
        mysql_candles_5m.len()
    );

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
    true
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
    strategy: &StrategyConfig,
) -> Result<(), anyhow::Error> {
    //实际执行
    info!("run_strategy_job开始: inst_id={}, time={}", inst_id, time);
    //记录执行时间
    let start_time = Instant::now();
    // 直接使用新的管理器
    let res = self::run_ready_to_order_with_manager(inst_id, time, strategy).await;

    if let Err(e) = res {
        error!(
            "run ready to order inst_id:{:?} time:{:?} error:{:?}",
            inst_id, time, e
        );
        return Err(anyhow!(e));
    }

    info!(
        "run_strategy_job完成: inst_id={}, time={}, 执行时间:{}ms",
        inst_id,
        time,
        start_time.elapsed().as_millis()
    );
    Ok(())
}

/// 运行准备好的订单函数 - 使用新的管理器
pub async fn run_ready_to_order_with_manager(
    inst_id: &str,
    period: &str,
    strategy: &StrategyConfig,
) -> anyhow::Result<()> {
    // 常量定义
    const MAX_HISTORY_SIZE: usize = 10000;
    // 1. 预处理：获取哈希键和管理器
    let strategy_type = StrategyType::Vegas.as_str().to_owned();
    let key = get_hash_key(inst_id, period, &strategy_type);
    let manager = arc_vegas_indicator_values::get_indicator_manager();

    // 2. 获取最新K线数据
    let new_candle_data = CandleDomainService::new_default()
        .await
        .get_new_one_candle_fresh(inst_id, period, None)
        .await
        .map_err(|e| anyhow!("获取最新K线数据失败: {}", e))?;
    println!("new candle :{:?}", new_candle_data);
    if new_candle_data.is_none() {
        warn!(
            "获取的最新K线数据为空,跳过本次策略执行: {:?}, {:?}",
            inst_id, period
        );
        return Ok(()); // 改为返回Ok，避免阻塞策略执行
    }
    let new_candle_data = new_candle_data.unwrap();
    let new_candle_item = parse_candle_to_data_item(&new_candle_data);
    // 3. 同键互斥，读取快照并验证
    let key_mutex = manager.acquire_key_mutex(&key).await;
    let _guard = key_mutex.lock().await;

    /// 获取缓存，快照
    let (mut last_candles_vec, mut old_indicator_combines, old_time) =
        match manager.get_snapshot_last_n(&key, MAX_HISTORY_SIZE).await {
            Some((v, indicators, ts)) => (v, indicators, ts),
            None => {
                return Err(anyhow!("没有找到对应的策略值: {}", key));
            }
        };
    // 转为 VecDeque 以保持原逻辑（并保证后续 push/pop_front 性能）
    let mut new_candle_items: VecDeque<CandleItem> = last_candles_vec.into_iter().collect();

    // 4. 验证时间戳，检查是否有新数据
    let new_time = new_candle_item.ts;

    let is_update = new_candle_item.confirm == 1;
    //如果最新数据确认了的。
    if is_update {
        if old_time == new_time {
            info!(
                "未检测到新的K线数据，等待下次更新 inst_id:{:?} period;{:?}",
                inst_id, period
            );
            return Ok(());
        }
        // 验证时间差是否为一个周期（记录警告，中断执行）
        if let Ok(period_diff) = ts_add_n_period(old_time, period, 1) {
            if period_diff != new_time {
                return Err(anyhow!(
                    "K线时间戳不连续: 上一时间戳 {}, 当前时间戳 {}, 预期时间戳 {}",
                    old_time,
                    new_time,
                    period_diff
                ));
            }
        }
    } else {
        if let Ok(period_diff) = ts_add_n_period(old_time, period, 1) {
            if new_time < period_diff {
                // 非确认状态，预期内，直接早退等待确认即可
                tracing::debug!(
                    "K线未确认，等待下一次: inst_id={}, period={}",
                    inst_id,
                    period
                );
                return Ok(());
            }
            // 只有当 new_time 超过期望时间戳（异常倒流/脏数据）才需要报错
            if new_time > period_diff {
                return Err(anyhow!(
                    "K线时间戳异常: 上一={}, 当前={}, 期望={}",
                    old_time,
                    new_time,
                    period_diff
                ));
            }
        }
    }

    // 6. 计算最新指标值
    let new_indicator_values =
        strategy_common::get_multi_indicator_values(&mut old_indicator_combines, &new_candle_item);
    if inst_id == "BTC-USDT-SWAP" {
        println!(
            "new_indicator_values{:?}",
            json!(new_indicator_values).to_string()
        );
    }
    // 5. 准备更新数据
    new_candle_items.push_back(new_candle_item.clone());

    if is_update {
        // 限制历史数据大小 - 使用VecDeque的高效操作
        if new_candle_items.len() > MAX_HISTORY_SIZE {
            let excess = new_candle_items.len() - MAX_HISTORY_SIZE;
            for _ in 0..excess {
                new_candle_items.pop_front();
            }
        }

        // 7-8. 原子更新：同时写入K线与指标，避免中间态
        if let Err(e) = manager
            .update_both(
                &key,
                new_candle_items.clone(),
                old_indicator_combines.clone(),
                new_candle_item.ts,
            )
            .await
        {
            return Err(anyhow!("原子更新指标与K线失败: {}", e));
        }
    }

    // 10. 计算交易信号
    // 将VecDeque转换为Vec,为了增加性能和部分场景需要，最后n根k线的情况，取最后N根,并保留原始排序，以供策略使用,
    let candle_vec: Vec<CandleItem> = new_candle_items
        .iter()
        .rev()
        .take(10)
        .cloned()
        .rev()
        .collect();

    // 解析策略配置
    let vegas_strategy: crate::trading::indicator::vegas_indicator::VegasStrategy = 
        serde_json::from_str(&strategy.strategy_config)?;
    let signal_result = vegas_strategy.get_trade_signal(
        &candle_vec,
        &mut new_indicator_values.clone(),
        &SignalWeightsConfig::default(),
        &serde_json::from_str::<crate::trading::strategy::strategy_common::BasicRiskStrategyConfig>(&strategy.risk_config)?,
    );
    println!("signal_result:{:?}", signal_result);

    if signal_result.should_buy || signal_result.should_sell {
        info!(
            "signal_result:{:?},ts:{}",
            signal_result, new_candle_item.ts
        );
        //异步记录日志
        save_signal_log(inst_id, period, &signal_result);
        //执行交易
        let risk_config = strategy.risk_config.clone();

        SwapOrderService::new()
            .ready_to_order(
                &StrategyType::Vegas,
                inst_id,
                period,
                &signal_result,
                &serde_json::from_str::<crate::trading::strategy::strategy_common::BasicRiskStrategyConfig>(&strategy.risk_config)?,
                strategy.strategy_config_id,
            )
            .await?;
    } else {
        debug!(
            "signal_result:{:?},ts:{}",
            signal_result,
            new_candle_items.back().unwrap().ts
        );
    }
    Ok(())
}

pub fn save_signal_log(inst_id: &str, period: &str, signal_result: &SignalResult) {
    // 异步记录日志（不阻塞下单），并移除 unwrap
    let strategy_result_str = match serde_json::to_string(&signal_result) {
        Ok(s) => s,
        Err(e) => {
            error!("序列化 signal_result 失败: {}", e);
            format!("{:?}", signal_result)
        }
    };
    let signal_record = StrategyJobSignalLog {
        inst_id: inst_id.to_string(),
        time: period.to_string(),
        strategy_type: StrategyType::Vegas.as_str().to_owned(),
        strategy_result: strategy_result_str,
    };
    //启动新线程执行（捕获所有 owned 数据，满足 'static）
    let inst_id_owned = signal_record.inst_id.clone();
    tokio::spawn(async move {
        let res = strategy_job_signal_log::StrategyJobSignalLogModel::new()
            .await
            .add(signal_record)
            .await;
        if let Err(e) = res {
            error!("写入策略信号日志失败: {}", e);
        } else {
            info!("写入策略信号日志成功: {}", inst_id_owned);
        }
    });
}
