#[macro_use]
extern crate rbatis;

use std::env;
use std::time::Duration;
use base64;
use chrono::{DateTime, Utc};
use hmac::Mac;
use serde::{Deserialize, Serialize};
use tokio::time::{interval, sleep_until, Instant};

mod trading;
mod job;
mod time_util;
mod socket;
mod config;

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use trading::okx::okx_client;
// use trading::model::biz_activity_model::BizActivityModel;
use clap::Parser;
use crate::trading::model::market::candles::CandlesModel;
use crate::trading::okx::market::Market;
use crate::trading::model::market::tickers::TicketsModel;
use crate::trading::okx::{okx_websocket_client, validate_system_time};
use crate::trading::task::asset_job;
use crate::trading::task::candles_job;
use std::{
    collections::HashMap,
    io::Error as IoError,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use anyhow::anyhow;
use dotenv::dotenv;
use fast_log::Config;

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, SinkExt, stream::TryStreamExt, StreamExt};
use futures_util::future::join_all;
use rbatis::RBatis;
use rbdc_mysql::MysqlDriver;
use tracing::{error, warn, warn_span};
use redis::streams::StreamClaimOptions;
use serde_json::json;

use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{tungstenite};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Result},
};
use tracing::{debug, info, Level, span};
use tracing_subscriber::{EnvFilter, fmt, FmtSubscriber};
use crate::config::db;
use crate::trading::okx::okx_websocket_client::ApiType;
use trading::strategy::StopLossStrategy;
use crate::job::task_scheduler::TaskScheduler;
use crate::trading::model::market::candles;
use crate::trading::okx::public_data::OkxPublicData;
use crate::trading::task::{account_job, tickets_job};

use crate::trading::model::strategy::back_test_log;
use crate::trading::okx::trade;
use crate::trading::okx::trade::{AttachAlgoOrd, OrderRequest, Side, TdMode};
use crate::trading::strategy::StrategyType;
use tracing_subscriber::prelude::*;
use crate::config::db::init_db;
use crate::config::log::setup_logging;
use crate::trading::{order, task};
use crate::trading::okx::account::Account;

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    // 设置日志
    setup_logging().await?;

    //初始化数据库连接
    init_db().await;

    // 验证当前系统时间
    if env::var("APP_ENV").unwrap() != "LOCAL" {
        validate_system_time().await;
    }

    // 定义需要交易的产品及周期
    // let inst_ids = Arc::new(vec!["BTC-USDT-SWAP"]);
    // let times = Arc::new(vec!["1D"]);

    let inst_ids = Arc::new(vec!["BTC-USDT-SWAP", "SOL-USDT-SWAP", "ETH-USDT-SWAP", "ADA-USDT-SWAP", "SUSHI-USDT-SWAP"]);
    let times = Arc::new(vec!["4H", "1h", "5m", "1D"]);

    // let inst_ids = Arc::new(vec!["BTC-USDT-SWAP", "SOL-USDT-SWAP", "ETH-USDT-SWAP"]);
    // let times = Arc::new(vec!["4H", "1h", "5m", "1D"]);


    // 初始化需要同步的数据
    if env::var("IS_RUN_SYNC_DATA_JOB").unwrap() == "true" {
        println!("111111111111");
        //初始化同步一次就行
        tickets_job::init_all_ticker(&inst_ids).await?;

        task::run_sync_data_job(&inst_ids, &times).await?;
    }


    // 获取可用账户的最大数量
    // let max_avail_size = Account::get_max_size("ETH-USDT-SWAP", TdMode::ISOLATED).await?;
    // info!("max_avail_size: {:?}", max_avail_size);

    let mut scheduler = TaskScheduler::new();
    // 本地环境下执行回测任务
    if env::var("IS_BACK_TEST").unwrap() == "true" {
        let mut tasks = Vec::new();
        for inst_id in inst_ids.iter() {
            for time in times.iter() {
                let inst_id = inst_id.to_string();
                let time = time.to_string();
                tasks.push(tokio::spawn(async move {
                    //ut_boot_strategy
                    // let res = task::ut_boot_test(&inst_id, &time).await;
                    //engulfing_strategy
                    let res = task::engulfing_test(&inst_id, &time).await;
                    if let Err(error) = res {
                        error!("run strategy error: {}", error);
                    }
                }));
            }
        }
        join_all(tasks).await;
    }

    // 添加定时任务执行策略
    {
        if env::var("IS_RUN_REAL_STRATEGY").unwrap() == "true" {
            //设置交易产品最大杠杆
            task::run_set_leverage(&inst_ids).await?;
            let inst_ids = Arc::clone(&inst_ids);
            let times = Arc::clone(&times);

            {
                let inst_ids = Arc::clone(&inst_ids);
                let times = Arc::clone(&times);
                //执行ut_boot策略
                scheduler.add_periodic_task("run_ut_boot_strategy_job".to_string(), 30000, move || {
                    let inst_ids_inner = Arc::clone(&inst_ids);
                    let times_inner = Arc::clone(&times);
                    async move {
                        let res = task::run_strategy_job(inst_ids_inner, times_inner, StrategyType::UtBoot).await;
                        if let Err(error) = res {
                            error!("run ut boot strategy error: {}", error);
                        }
                    }
                });
            }

            {
                let inst_ids = Arc::clone(&inst_ids);
                let times = Arc::clone(&times);
                //添务执行Engulfing策略
                scheduler.add_periodic_task("run_engulfing_strategy_job".to_string(), 30000, move || {
                    let inst_ids_inner = Arc::clone(&inst_ids);
                    let times_inner = Arc::clone(&times);
                    async move {
                        let res = task::run_strategy_job(inst_ids_inner, times_inner, StrategyType::Engulfing).await;
                        if let Err(error) = res {
                            error!("run engulfing strategy error: {}", error);
                        }
                    }
                });
            }
        }
    }

    // 运行WebSocket服务
    {
        if env::var("IS_OPEN_SOCKET").unwrap() == "true" {
            let inst_ids = Arc::clone(&inst_ids);
            let times = Arc::clone(&times);
            socket::run_socket(inst_ids, times).await;
        }
    }

    // 捕捉Ctrl+C信号以平滑关闭
    tokio::signal::ctrl_c().await?;
    scheduler.shutdown().await;

    Ok(())
}
