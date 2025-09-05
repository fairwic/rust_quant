#![allow(dead_code)] // 允许未使用的函数/类型
#![allow(unused_variables)] // 允许未使用的变量
#![allow(unused_imports)] // 允许未使用的导入

#[macro_use]
extern crate rbatis;
// use trading::model::biz_activity_model::BizActivityModel; use clap::Parser; use crate::trading::model::market::candles::CandlesModel; use crate::trading::okx::market::Market; use crate::trading::model::market::tickers::TicketsModel;
use anyhow::anyhow;
use base64;
use chrono::{DateTime, Utc};
use dotenv::dotenv;
use fast_log::Config;
use hmac::Mac;
use rust_quant::trading::task::candles_job;
use rust_quant::trading::task::{asset_job, tickets_job};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use std::{
    collections::HashMap,
    io::Error as IoError,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::time::{interval, sleep_until, Instant};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::future::join_all;
use futures_util::{future, pin_mut, stream::TryStreamExt, SinkExt, StreamExt};
use rbatis::RBatis;
use rbdc_mysql::MysqlDriver;
use redis::streams::StreamClaimOptions;
use serde::de::Unexpected::Option;
use serde_json::json;
use tracing::{error, warn, warn_span};

use rust_quant::app_config::db;
use rust_quant::job::task_scheduler::TaskScheduler;
use rust_quant::trading::model::market::candles;
use rust_quant::trading::strategy::{strategy_common, StopLossStrategy};
use rust_quant::trading::task::account_job;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Result},
};
use tracing::{debug, info, span, Level};
use tracing_subscriber::{fmt, EnvFilter, FmtSubscriber};

use okx::dto::EnumToStrTrait;
use okx::utils::validate_system_time;
use once_cell::sync::Lazy;
use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::job::RiskBalanceWithLevelJob;
use rust_quant::trading::indicator::atr::ATR;
use rust_quant::trading::indicator::vegas_indicator::{
    self, VegasIndicatorSignalValue, VegasStrategy,
};
use rust_quant::trading::model::strategy::back_test_log;
use rust_quant::trading::model::strategy::strategy_config::StrategyConfigEntityModel;
use rust_quant::trading::strategy::arc::indicator_values::arc_vegas_indicator_values;
use rust_quant::trading::strategy::order::vagas_order::{StrategyConfig, StrategyOrder};
use rust_quant::trading::strategy::strategy_common::{
    parse_candle_to_data_item, BasicRiskStrategyConfig, SignalResult,
};
use rust_quant::trading::strategy::StrategyType;
use rust_quant::trading::{order, task};
use rust_quant::{app_init, socket, trading, ENVIRONMENT_LOCAL};
use tokio_cron_scheduler::JobScheduler;
use tracing_subscriber::prelude::*;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 环境变量工具方法
    fn env_is_true(key: &str, default: bool) -> bool {
        match std::env::var(key) {
            Ok(v) => v.eq_ignore_ascii_case("true") || v == "1",
            Err(_) => default,
        }
    }
    fn env_or_default(key: &str, default: &str) -> String {
        match std::env::var(key) {
            Ok(v) => v,
            Err(_) => default.to_string(),
        }
    }
    //初始化环境
    app_init().await?;
    // 初始化并启动调度器
    let scheduler = match rust_quant::init_scheduler().await {
        Ok(s) => s,
        Err(e) => {
            error!("初始化任务调度器失败: {}", e);
            return Err(anyhow!("初始化任务调度器失败: {}", e));
        }
    };
    if let Err(e) = scheduler.start().await {
        error!("启动任务调度器失败: {}", e);
        return Err(anyhow!("启动任务调度器失败: {}", e));
    }
    info!("全局任务调度器已启动");

    // 验证当前系统时间（非本地）
    let app_env = env_or_default("APP_ENV", ENVIRONMENT_LOCAL);
    if app_env != ENVIRONMENT_LOCAL {
        info!("校验系统时间与 OKX 时间差");
        let _time_diff = validate_system_time().await?;
    }

    let inst_ids = Some(vec!["ETH-USDT-SWAP", "BTC-USDT-SWAP"]);
    let inst_ids = Some(vec!["ETH-USDT-SWAP"]);
    // let inst_ids = Some(vec!["BTC-USDT-SWAP"]);
    let period = Some(vec!["1H", "4H", "1Dutc"]);
    let period = Some(vec!["1H", "4H"]);
    let period = Some(vec!["4H"]);
    // let period = Some(vec!["1Dutc"]);

    // 初始化需要同步的数据
    if env_is_true("IS_RUN_SYNC_DATA_JOB", false) {
        //初始化同步一次就行
        if let Err(error) = tickets_job::init_all_ticker(inst_ids.clone()).await {
            error!("init all tickers error: {}", error);
        }
        match (&inst_ids, &period) {
            (Some(ids), Some(times)) => {
                if let Err(error) = task::basic::run_sync_data_job(Some(ids.clone()), times).await {
                    error!("run sync [tickets] data job error: {}", error);
                }
            }
            _ => warn!("跳过数据同步：未设置 inst_ids 或 period"),
        }
        //同步精英交易员的交易数据
        // let res = task::big_data_job::sync_top_contract(inst_ids.clone(), period.clone()).await;
        // if let Err(error) = res {
        //     error!("run sync [top contract] data job error: {}", error);
        // }
        // info!("RUN_SYNC_DATA_JOB Ok!");
    }

    // 本地环境下执行回测任务
    if env_is_true("IS_BACK_TEST", false) {
        info!("IS_BACK_TEST 已启用");
        if let (Some(inst_id), Some(times)) = (inst_ids.clone(), period.clone()) {
            for inst_id in inst_id {
                for time in times.iter() {
                    let time = time.to_string();
                    if let Err(error) = task::basic::vegas_back_test(inst_id, &time).await {
                        error!(
                            "run strategy error: {:#?} {} {} {}",
                            error.backtrace(),
                            error,
                            inst_id,
                            time
                        );
                    }
                }
            }
        } else {
            warn!("跳过回测：未设置 inst_ids 或 period");
        }
    }
    // 先运行WebSocket服务,及时同步最新的数据
    {
        if env_is_true("IS_OPEN_SOCKET", false) {
            match (inst_ids.clone(), period.clone()) {
                (Some(inst_id), Some(times)) => {
                    socket::websocket_service::run_socket(inst_id, times).await;
                }
                _ => warn!("无法启动WebSocket：未设置 inst_ids 或 period"),
            }
        }
    }

    // 2再执行定时任务执行策略
    {
        if env_is_true("IS_RUN_REAL_STRATEGY", false) {
            info!("run real strategy job");
            if let Some(inst_ids) = inst_ids.clone() {
                //1. 执行风险控制,初始化
                let risk_job = RiskBalanceWithLevelJob::new();
                if let Err(e) = risk_job.run(&inst_ids).await {
                    error!("风险控制初始化失败: {}", e);
                }

                let inst_ids = Arc::new(inst_ids);
                let times = Arc::new(period.clone().unwrap());
                //获取指定产品的策略
                //计算出最新的指标values
                let strategy_list = StrategyConfigEntityModel::new().await.get_list().await;
                let strategy_list = match strategy_list {
                    Ok(list) => {
                        info!("获取策略配置数量{:?}", list.len());
                        list
                    }
                    Err(e) => {
                        error!("获取策略配置失败: {:?}", e);
                        return Err(anyhow!("获取策略配置失败: {:?}", e));
                    }
                };
                // 创建策略订单管理器实例（复用同一个实例避免重复启动）
                let strategy_order = StrategyOrder::new();

                //遍历配置
                for strategy in strategy_list.into_iter() {
                    //遍历产品数据，计算出当前最新的指标
                    let inst_id = strategy.inst_id;
                    let time = strategy.time;
                    //获取策略的详情
                    let strategy_type = strategy.strategy_type;
                    if &strategy_type == StrategyType::Vegas.as_str() {
                        //获取数据
                        let strategy_config: VegasStrategy = serde_json::from_str::<VegasStrategy>(
                            &*strategy.value,
                        )
                        .map_err(|e| anyhow!("Failed to parse VegasStrategy config: {}", e))?;

                        let risk_config: BasicRiskStrategyConfig =
                            serde_json::from_str::<BasicRiskStrategyConfig>(&*strategy.risk_config)
                                .map_err(|e| {
                                    anyhow!("Failed to parse BasicRiskStrategyConfig config: {}", e)
                                })?;

                        let strategy_config = StrategyConfig {
                            strategy_config: strategy_config,
                            risk_config: risk_config,
                            strategy_config_id: strategy.id,
                        };

                        // 启动策略（如果已存在会跳过）
                        if let Err(e) = strategy_order
                            .run_strategy(strategy_config, inst_id, time)
                            .await
                        {
                            error!("启动策略失败:  错误: {}", e);
                            // 继续处理其他策略，不中断整个流程
                        }
                    }
                }
            }
        }
    }

    // 捕捉Ctrl+C信号以平滑关闭
    tokio::signal::ctrl_c().await?;
    Ok(())
}
