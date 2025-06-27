#![allow(dead_code)] // 允许未使用的函数/类型
#![allow(unused_variables)] // 允许未使用的变量
#![allow(unused_imports)] // 允许未使用的导入

#[macro_use]
extern crate rbatis;
use base64;
use chrono::{DateTime, Utc};
use hmac::Mac;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use tokio::time::{interval, sleep_until, Instant};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
// use trading::model::biz_activity_model::BizActivityModel; use clap::Parser; use crate::trading::model::market::candles::CandlesModel; use crate::trading::okx::market::Market; use crate::trading::model::market::tickers::TicketsModel;
use anyhow::anyhow;
use dotenv::dotenv;
use fast_log::Config;
use rust_quant::trading::task::candles_job;
use rust_quant::trading::task::{asset_job, tickets_job};
use std::{
    collections::HashMap,
    io::Error as IoError,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

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

use okx::utils::validate_system_time;
use once_cell::sync::Lazy;
use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::job::RiskJob;
use rust_quant::trading::indicator::atr::ATR;
use rust_quant::trading::indicator::vegas_indicator::{
    self, VegasIndicatorSignalValue, VegasStrategy,
};
use rust_quant::trading::model::strategy::back_test_log;
use rust_quant::trading::model::strategy::strategy_config::StrategyConfigEntityModel;
use rust_quant::trading::strategy::arc::indicator_values::arc_vegas_indicator_values;
use rust_quant::trading::strategy::order::vagas_order::VegasOrder;
use rust_quant::trading::strategy::strategy_common::{parse_candle_to_data_item, SignalResult};
use rust_quant::trading::strategy::StrategyType;
use rust_quant::trading::{order, task};
use rust_quant::{app_init, socket, trading};
use tokio_cron_scheduler::JobScheduler;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    // 验证当前系统时间
    if env::var("APP_ENV").unwrap() != "LOCAL" {
        println!("valid okx with local time");
        validate_system_time().await;
    }

    // 定义需要交易的产品及周期
    // let inst_ids = Some(Arc::new(vec![
    //     "BTC-USDT-SWAP",
    // ]))    // let period = Arc::new(vec!["1m", "3m", "5m", "15m"]);

    // let inst_ids =Some(vec!["SUI-USDT-SWAP","BTC-USDT-SWAP","ETH-USDT-SWAP"]);
    // let inst_ids = Some(vec!["ETH-USDT-SWAP", "SUI-USDT-SWAP","OM-USDT-SWAP"]);
    // let inst_ids =Some(vec![ "ETH-USDT-SWAP"]);
    let inst_ids = Some(vec!["BTC-USDT-SWAP"]);
    // let inst_ids = Some(vec!["OM-USDT-SWAP"]);
    // let period = Some(vec!["4H",]);
    // let period = Some(vec!["1m"]);
    let period = Some(vec!["5m", "1m", "3m", "1Dutc"]);
    // let times = Arc::new(vec!["4H", "1H", "5m", "1Dutc"]);
    // let period = Some(vec!["1H"]);

    // let inst_ids = Arc::new(vec!["BTC-USDT-SWAP", "SOL-USDT-SWAP", "ETH-USDT-SWAP"]);
    // let times = Arc::new(vec!["4H", "1h", "5m", "1D"]);

    // 初始化需要同步的数据
    if env::var("IS_RUN_SYNC_DATA_JOB").unwrap() == "true" {
        //初始化同步一次就行
        let res = tickets_job::init_all_ticker(inst_ids.clone()).await;
        if let Err(error) = res {
            println!("{:?}", error);
            error!("init all tickers error: {}", error);
        }
        let res = task::basic::run_sync_data_job(inst_ids.clone(), &period.clone().unwrap()).await;
        if let Err(error) = res {
            error!("run sync [tickets] data job error: {}", error);
        }

        // let res = task::big_data_job::sync_top_contract(inst_ids.clone(), period.clone()).await;
        // if let Err(error) = res {
        //     error!("run sync [top contract] data job error: {}", error);
        // }
        // info!("RUN_SYNC_DATA_JOB Ok!");
    }

    // 获取可用账户的最大数量
    // let max_avail_size = Account::get_max_size("ETH-USDT-SWAP", TdMode::ISOLATED).await?;
    // info!("max_avail_size: {:?}", max_avail_size);

    // 本地环境下执行回测任务
    if env::var("IS_BACK_TEST").unwrap() == "true" {
        println!("IS_BACK_TEST");
        if let Some(inst_ids) = inst_ids.clone() {
            for inst_id in inst_ids {
                for time in period.clone().unwrap().iter() {
                    let time = time.to_string();
                    //ut_boot_strategy
                    // let res = task::basic::squeeze_test(inst_id, &time).await;
                    //ut_boot_strategy
                    // let res = task::basic::top_contract_test(inst_id, &time).await;
                    //ut_boot_strategy
                    // let res = task::basic::ut_boot_test(inst_id, &time).await;
                    //vegas_strategy
                    let res = task::basic::vegas_test(inst_id, &time).await;
                    //engulfing_strategy
                    // let res = task::engulfing_test(&inst_id, &time).await;
                    if let Err(error) = res {
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
        }
    }

    // 添加定时任务执行策略
    {
        if env::var("IS_RUN_REAL_STRATEGY").unwrap_or(String::from("false")) == "true" {
            println!("run real strategy job");
            if let Some(inst_ids) = inst_ids.clone() {
                //1. 执行风险控制,初始化
                let risk_job = RiskJob::new();
                risk_job.run(&inst_ids).await.unwrap();

                let inst_ids = Arc::new(inst_ids);
                let times = Arc::new(period.clone().unwrap());
                //获取指定产品的策略
                //计算出最新的指标values
                let strategy_list = StrategyConfigEntityModel::new().await.get_list().await;
                info!("获取策略配置:{:?}", strategy_list);
                if !strategy_list.is_err() {
                    //遍历配置
                    for strategy in strategy_list.unwrap().iter() {
                        //遍历产品数据，计算出当前最新的指标
                        let inst_id = strategy.inst_id.clone();
                        let time = strategy.time.clone();
                        //获取策略的详情
                        let strategy_type = strategy.strategy_type.clone();
                        if strategy_type == StrategyType::Vegas.to_string() {
                            //获取数据
                            let strategy_config: VegasStrategy =
                                serde_json::from_str::<VegasStrategy>(&*strategy.value).map_err(
                                    |e| anyhow!("Failed to parse VegasStrategy config: {}", e),
                                )?;
                            VegasOrder::new()
                                .order(strategy_config, inst_id, time)
                                .await?;
                        }
                    }
                }
            }
        }
    }

    // 运行WebSocket服务
    {
        if env::var("IS_OPEN_SOCKET").unwrap() == "true" {
            socket::websocket_service::run_socket(inst_ids.clone().unwrap(), period.unwrap()).await;
        }
    }
    // 捕捉Ctrl+C信号以平滑关闭
    tokio::signal::ctrl_c().await?;
    Ok(())
}
