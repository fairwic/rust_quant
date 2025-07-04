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
use crate::trading::indicator::bollings::BollingBandsSignalConfig;
use crate::trading::indicator::signal_weight::SignalWeightsConfig;
use crate::trading::model::market::candles::SelectTime;
use crate::trading::model::market::candles::{self, TimeDirect};
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
use crate::trading::{order, task};
use okx::api::account::OkxAccount;
use okx::dto::account::account_dto::SetLeverageRequest;

use super::job_param_generator::ParamMergeBuilder;
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
use crate::trading::order::swap_ordr::SwapOrder;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::get_hash_key;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::get_vegas_indicator_values;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::update_candle_items;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::update_vegas_indicator_values;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values::{
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
use okx::api::api_trait::OkxApiTrait;
use okx::api::trade::OkxTrade;
use okx::dto::trade_dto::TdModeEnum;
use okx::dto::PositionSide;
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
    let res = strategy.run_test(&mysql_candles, risk_strategy_config);

    // 构建更详细的策略配置描述
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

    // 保存日志
    let start_time = Instant::now();
    let back_test_id = back_test_log::BackTestLogModel::new()
        .await
        .add(&back_test_log)
        .await?;

    if true {
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
    pub profit_threshold: Vec<f64>,
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
            profit_threshold: vec![0.03],
            is_move_stop_loss: vec![false],
            is_used_signal_k_line_stop_loss: vec![true],
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
    info!("开始随机策略测试: inst_id={}, time={}", inst_id, time);

    // 构建参数生成器
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
        config.profit_threshold,
        config.is_move_stop_loss,
        config.is_used_signal_k_line_stop_loss,
    );

    let (_, total_count) = param_generator.progress();
    info!("总共需要处理 {} 个参数组合", total_count);

    // 获取并转换K线数据
    let arc_candle_data = load_and_convert_candle_data(inst_id, time, 20000).await?;

    // 批量处理参数组合
    let mut processed_count = 0;
    loop {
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
        info!(
            "已处理 {}/{} 个参数组合",
            processed_count.min(total_count),
            total_count
        );
    }

    info!("随机策略测试完成: 总计处理 {} 个参数组合", total_count);
    Ok(())
}

/// 加载并转换K线数据的辅助函数
async fn load_and_convert_candle_data(
    inst_id: &str,
    time: &str,
    limit: usize,
) -> Result<Arc<Vec<CandleItem>>, anyhow::Error> {
    info!(
        "加载K线数据: inst_id={}, time={}, limit={}",
        inst_id, time, limit
    );

    let mysql_candles = get_candle_data(inst_id, time, limit, None)
        .await
        .map_err(|e| anyhow!("获取K线数据失败: {}", e))?;

    if mysql_candles.is_empty() {
        return Err(anyhow!("K线数据为空"));
    }

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

    info!("成功加载 {} 条K线数据", candle_item_vec.len());
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
            enable_random_test: false,
            enable_specified_test: true,
        }
    }
}

/// 主函数，执行所有策略测试
pub async fn vegas_back_test(inst_id: &str, time: &str) -> Result<(), anyhow::Error> {
    vegas_back_test_with_config(inst_id, time, VegasBackTestConfig::default()).await
}

/// 带配置的 Vegas 策略回测
pub async fn vegas_back_test_with_config(
    inst_id: &str,
    time: &str,
    config: VegasBackTestConfig,
) -> Result<(), anyhow::Error> {
    info!(
        "开始 Vegas 策略回测: inst_id={}, time={}, config={:?}",
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
        info!("执行随机策略测试");
        if let Err(e) = test_random_strategy(inst_id, time, semaphore.clone()).await {
            error!("随机策略测试失败: {}", e);
            test_results.push(("random", false));
        } else {
            test_results.push(("random", true));
        }
    }

    if config.enable_specified_test {
        info!("执行指定策略测试");
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

    info!(
        "Vegas 策略回测完成: 成功 {}/{}, 详情: {:?}",
        success_count, total_count, test_results
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
        .profit_threshold(0.03)
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
        .profit_threshold(0.01)
        .is_move_stop_loss(false)
        .is_used_signal_k_line_stop_loss(true)];
    Ok(params_batch)
}
pub async fn test_specified_strategy(
    inst_id: &str,
    time: &str,
    semaphore: Arc<Semaphore>,
) -> Result<(), anyhow::Error> {
    info!("开始指定策略测试: inst_id={}, time={}", inst_id, time);

    // 转换策略配置为参数
    let mut params_batch = test_specified_strategy_with_config(inst_id, time).await?;
    // let mut params_batch = get_strategy_config_from_db(inst_id, time).await?;

    // 加载K线数据
    let arc_candle_data = load_and_convert_candle_data(inst_id, time, 20000).await?;

    // 执行回测
    run_back_test_strategy(params_batch, inst_id, time, arc_candle_data, semaphore).await;

    info!("指定策略测试完成");
    Ok(())
}

/// 转换策略配置为参数的辅助函数
fn convert_strategy_config_to_param(
    config: &StrategyConfigEntity,
) -> Result<ParamMergeBuilder, anyhow::Error> {
    let vegas_strategy = serde_json::from_str::<VegasStrategy>(&config.value)
        .map_err(|e| anyhow!("解析策略配置JSON失败: {}", e))?;

    let risk_config =
        serde_json::from_str::<BasicRiskStrategyConfig>(&config.risk_config).unwrap_or_default();

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
        .profit_threshold(risk_config.profit_threshold)
        .is_move_stop_loss(risk_config.is_move_stop_loss)
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
            profit_threshold: param.profit_threshold,
            is_move_stop_loss: param.is_move_stop_loss,
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
    let batch_start = Instant::now();
    join_all(batch_tasks).await;
    let batch_end = Instant::now();
    info!(
        "当前批次完成，用时：{:?}",
        batch_end.duration_since(batch_start)
    );
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
        strategy_common::get_multi_indicator_values(&mut indicator_combines, &new_candle_item);

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

    SwapOrder::new()
        .ready_to_order(StrategyType::Vegas, inst_id, period, signal_result)
        .await?;
    Ok(())
}
