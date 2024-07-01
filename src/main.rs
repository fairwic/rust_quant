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

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use trading::okx::okx_client;
use trading::model::biz_activity_model::BizActivityModel;
use clap::Parser;
use crate::trading::model::market::candles::CandlesModel;
use crate::trading::okx::market::Market;
use crate::trading::model::market::tickers::TicketsModel;
use crate::trading::okx::okx_websocket_client;
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

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, SinkExt, stream::TryStreamExt, StreamExt};
use futures_util::future::join_all;
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
use crate::trading::model::Db;
use crate::trading::okx::okx_websocket_client::ApiType;
use trading::strategy::StopLossStrategy;
use crate::job::task_scheduler::TaskScheduler;
use crate::trading::model::market::candles;
use crate::trading::okx::public_data::OkxPublicData;
use crate::trading::task::{account_job, tickets_job};

use crate::trading::model::strategy::back_test_log;
use crate::trading::model::strategy::back_test_log::BackTestLog;
use crate::trading::okx::trade;
use crate::trading::okx::trade::{AttachAlgoOrd, OrderRequest, Side, TdMode};
use crate::trading::strategy::StrategyType;
use tracing_subscriber::prelude::*;
use crate::trading::{order, task};
use crate::trading::okx::account::Account;

async fn accept_connection(peer: SocketAddr, stream: TcpStream) {
    if let Err(e) = handle_connection(peer, stream).await {
        match e {
            tungstenite::Error::ConnectionClosed | tungstenite::Error::Protocol(_) | tungstenite::Error::Utf8 => (),
            err => error!("Error processing connection: {}", err),
        }
    }
}

async fn handle_connection(peer: SocketAddr, stream: TcpStream) -> Result<()> {
    let mut ws_stream = accept_async(stream).await.expect("Failed to accept");

    info!("New WebSocket connection: {}", peer);

    while let Some(msg) = ws_stream.next().await {
        let msg = msg?;
        info!("New Message : {}", msg);
        if msg.is_text() || msg.is_binary() {
            let response = "hhhh";
            ws_stream.send(Message::from(response)).await?;
        }
    }

    Ok(())
}

// 验证系统时间
async fn validate_system_time() {
    let time_str = OkxPublicData::get_time().await;
    debug!("获取okx系统时间: {:?}", time_str);
    if let Ok(time_str) = time_str {
        let time = time_str.parse::<i64>().unwrap();
        let time = DateTime::<Utc>::from_utc(
            chrono::NaiveDateTime::from_timestamp(time / 1000, ((time % 1000) * 1_000_000) as u32),
            Utc,
        );

        let now = Utc::now().timestamp_millis();
        let okx_time = time.timestamp_millis();
        let time_diff = (now - okx_time).abs();
        if time_diff < 20000 {
            info!("时间间隔相差值: {} 毫秒", time_diff);
        } else {
            info!("时间未同步，时间间隔相差值: {} 毫秒", time_diff);
        }
    }
}

// 设置日志
async fn setup_logging() -> anyhow::Result<()> {
    dotenv().ok();
    let app_env = env::var("APP_ENV").expect("app_env config is none");
    if app_env == "LOCAL" {
        let subscriber = FmtSubscriber::builder()
            .with_max_level(Level::INFO)
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        let info_file = RollingFileAppender::new(Rotation::DAILY, "log_files", "info.log");
        let error_file = RollingFileAppender::new(Rotation::DAILY, "log_files", "error.log");

        let (info_non_blocking, _info_guard) = tracing_appender::non_blocking(info_file);
        let (error_non_blocking, _error_guard) = tracing_appender::non_blocking(error_file);

        tracing_subscriber::registry()
            .with(fmt::layer().with_writer(info_non_blocking).with_filter(EnvFilter::new("info")))
            .with(fmt::layer().with_writer(error_non_blocking).with_filter(EnvFilter::new("error")))
            .init();
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 设置日志
    setup_logging().await?;

    // 验证当前系统时间
    if env::var("APP_ENV").unwrap() != "LOCAL" {
        validate_system_time().await;
    }

    // 定义需要交易的产品及周期
    let inst_ids = Arc::new(vec!["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP", "SUSHI-USDT-SWAP"]);
    let times = Arc::new(vec!["1H", "4H", "1D"]);


    // 初始化需要同步的数据
    if env::var("IS_RUN_SYNC_DATA_JOB").unwrap() == "true" {
        task::run_sync_data_job(&inst_ids, &times).await?;
    }


    // 获取可用账户的最大数量
    // let max_avail_size = Account::get_max_size("ETH-USDT-SWAP", TdMode::ISOLATED).await?;
    // info!("max_avail_size: {:?}", max_avail_size);

    let mut scheduler = TaskScheduler::new();
    // 本地环境下执行回测任务
    if env::var("IS_BACK_TEST").unwrap() == "LOCAL" {
        let mut tasks = Vec::new();
        for inst_id in inst_ids.iter() {
            for time in times.iter() {
                let inst_id = inst_id.to_string();
                let time = time.to_string();
                tasks.push(tokio::spawn(async move {
                    task::ut_boot_test(&inst_id, &time).await;
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
            scheduler.add_periodic_task("run_ut_boot_strategy_job".to_string(), 30000, move || {
                let inst_ids_inner = Arc::clone(&inst_ids);
                let times_inner = Arc::clone(&times);
                async move {
                    let res = task::run_ut_boot_strategy_job(inst_ids_inner, times_inner).await;
                    if let Err(error) = res {
                        error!("run strategy error: {}", error);
                    }
                }
            });
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
