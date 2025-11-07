use anyhow::{anyhow, Result};
use futures::future::join_all;
use okx::dto::EnumToStrTrait;
use serde_json::json;
use std::env;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tracing::{error, info, warn};

use rust_quant_indicators::bollings::BollingBandsSignalConfig;
use rust_quant_indicators::signal_weight::SignalWeightsConfig;
use rust_quant_indicators::vegas_indicator::{
    EmaSignalConfig, EmaTouchTrendSignalConfig, EngulfingSignalConfig, KlineHammerConfig,
    RsiSignalConfig, VegasStrategy, VolumeSignalConfig,
};
use rust_quant_market::models::SelectCandleReqDto;
use rust_quant_market::models::CandlesEntity;
use rust_quant_market::models::SelectTime;
use rust_quant_market::models::CandlesModel;
use crate::trading::model::strategy::back_test_detail::BackTestDetail;
use crate::trading::model::strategy::back_test_log::{BackTestLog, BackTestLogModel};
use crate::trading::model::strategy::{back_test_detail, back_test_log};
use rust_quant_strategies::strategy_common::{BackTestResult, BasicRiskStrategyConfig, TradeRecord};
use rust_quant_strategies::{StrategyType, Strategy};
use rust_quant_strategies::nwe_strategy::NweStrategyConfig;
use rust_quant_strategies::nwe_strategy::NweStrategy;
use rust_quant_orchestration::workflow::data_validator;
use rust_quant_orchestration::workflow::job_param_generator::ParamMergeBuilder;
use crate::CandleItem;

/// 运行 Vegas 策略测试
pub async fn run_vegas_test(
    inst_id: &str,
    time: &str,
    mut strategy: VegasStrategy,
    risk_strategy_config: BasicRiskStrategyConfig,
    mysql_candles: Arc<Vec<CandleItem>>,
) -> Result<i64> {
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
        StrategyType::Vegas.as_str(),
    )
    .await?;

    // 返回 back_test_id
    Ok(back_test_id)
}

/// 运行 NWE 策略测试
pub async fn run_nwe_test(
    inst_id: &str,
    time: &str,
    mut strategy: NweStrategy,
    risk_strategy_config: BasicRiskStrategyConfig,
    mysql_candles: Arc<Vec<CandleItem>>,
) -> Result<i64> {
    let start_time = Instant::now();

    // 策略测试阶段
    let res = strategy.run_test(&mysql_candles, risk_strategy_config);

    // 配置描述构建阶段
    let config_desc = serde_json::to_string(&strategy.config).ok();

    // 保存测试日志并获取 back_test_id
    let back_test_id = save_log(
        inst_id,
        time,
        config_desc,
        res,
        mysql_candles,
        risk_strategy_config,
        &StrategyType::Nwe.as_str(),
    )
    .await?;

    Ok(back_test_id)
}

/// 保存测试日志
pub async fn save_log(
    inst_id: &str,
    time: &str,
    strategy_config_string: Option<String>,
    back_test_result: BackTestResult,
    mysql_candles: Arc<Vec<CandleItem>>,
    risk_strategy_config: BasicRiskStrategyConfig,
    strategy_name: &str,
) -> Result<i64> {
    // 构建日志对象阶段
    let back_test_log = BackTestLog {
        // 需要确定策略类型，这里使用参数传入或推断
        strategy_type: strategy_name.to_string(),
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

    if env::var("ENABLE_RANDOM_TEST").unwrap_or_default() != "true" {
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

/// 通用保存函数，允许指定策略类型
// 通用保存函数移除，统一使用 save_log

/// 保存测试详情
pub async fn save_test_detail(
    back_test_id: i64,
    strategy_type: StrategyType,
    inst_id: &str,
    time: &str,
    list: Vec<TradeRecord>,
) -> Result<u64> {
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

/// 获取K线数据并确认
pub async fn get_candle_data_confirm(
    inst_id: &str,
    period: &str,
    limit: usize,
    select_time: Option<SelectTime>,
) -> Result<Vec<CandlesEntity>> {
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
    let result = data_validator::valid_candles_data(&mysql_candles_5m, period);
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

/// 加载并转换K线数据的辅助函数
pub async fn load_and_convert_candle_data(
    inst_id: &str,
    time: &str,
    limit: usize,
) -> Result<Arc<Vec<CandleItem>>> {
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

/// 运行回测策略
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
        let rsi_period = param.rsi_period;
        let rsi_overbought = param.rsi_overbought;
        let rsi_oversold = param.rsi_oversold;

        let risk_strategy_config = BasicRiskStrategyConfig {
            max_loss_percent: param.max_loss_percent,
            take_profit_ratio: param.take_profit_ratio,
            is_one_k_line_diff_stop_loss: param.is_move_stop_loss,
            is_used_signal_k_line_stop_loss: param.is_used_signal_k_line_stop_loss,
        };

        let volume_signal = VolumeSignalConfig {
            volume_bar_num,
            volume_increase_ratio,
            volume_decrease_ratio,
            is_open: true,
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

/// 运行一组 NWE 策略（随机/网格参数）回测，复用与 Vegas 相同的并发调度思路
pub async fn run_nwe_random_batch(
    params_batch: Vec<(NweStrategyConfig, rust_quant_strategies::strategy_common::BasicRiskStrategyConfig)>,
    inst_id: &str,
    time: &str,
    arc_candle_item_clone: Arc<Vec<CandleItem>>,
    semaphore: Arc<Semaphore>,
) {
    let mut batch_tasks = Vec::with_capacity(params_batch.len());
    for (cfg, risk_cfg) in params_batch {
        let strategy = NweStrategy::new(cfg);
        let inst_id = inst_id.to_string();
        let time = time.to_string();
        let mysql_candles = Arc::clone(&arc_candle_item_clone);
        let permit = Arc::clone(&semaphore);

        batch_tasks.push(tokio::spawn(async move {
            let _permit: tokio::sync::SemaphorePermit<'_> = permit.acquire().await.unwrap();
            match run_nwe_test(
                &inst_id,
                &time,
                strategy,
                risk_cfg,
                mysql_candles,
            )
            .await
            {
                Ok(back_test_id) => Some(back_test_id),
                Err(e) => {
                    error!("NWE test failed: {:?}", e);
                    None
                }
            }
        }));
    }

    // 等待当前批次完成
    join_all(batch_tasks).await;
}
