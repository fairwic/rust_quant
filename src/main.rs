#[macro_use]
extern crate rbatis;

use base64;
use chrono::{DateTime, Utc};
use hmac::Mac;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use tokio::time::{interval, sleep_until, Instant};

use rust_quant::trading::okx::okx_client;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
// use trading::model::biz_activity_model::BizActivityModel; use clap::Parser; use crate::trading::model::market::candles::CandlesModel; use crate::trading::okx::market::Market; use crate::trading::model::market::tickers::TicketsModel;
use anyhow::anyhow;
use dotenv::dotenv;
use fast_log::Config;
use rust_quant::trading::okx::{okx_websocket_client, validate_system_time};
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
use rust_quant::trading::okx::okx_websocket_client::ApiType;
use rust_quant::trading::okx::public_data::OkxPublicData;
use rust_quant::trading::strategy::StopLossStrategy;
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

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::socket;
use rust_quant::trading::indicator::atr::ATR;
use rust_quant::trading::model::strategy::back_test_log;
use rust_quant::trading::okx::account::Account;
use rust_quant::trading::okx::trade;
use rust_quant::trading::okx::trade::{AttachAlgoOrd, OrderRequest, Side, TdMode};
use rust_quant::trading::strategy::strategy_common::SignalResult;
use rust_quant::trading::strategy::StrategyType;
use rust_quant::trading::{order, task};
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    //设置env
    dotenv().ok();
    // 设置日志
    println!("init log config");
    setup_logging().await?;

    //初始化数据库连接
    init_db().await;

    // 验证当前系统时间
    if env::var("APP_ENV").unwrap() != "LOCAL" {
        println!("valid okx with local time");
        validate_system_time().await;
    }
    //测试下单
    //插入信号记录到数据库中

    // //test atr
    // let prices = [
    //     [60198.1, 59420.0, 60020.1],
    //     [60287.0, 59668.1, 59867.0],
    //     [59959.4, 59365.0, 59710.1],
    //     [59875.3, 58421.0, 58435.8],
    //     [58769.9, 58210.0, 58730.3],
    //     [58748.5, 58379.0, 58503.6]
    // ];
    //
    // let prices = [
    //     [60196.7, 59427.8, 60020.3],
    //     [60281.9, 59674.0, 59865.6],
    //     [59956.1, 59370.8, 59710.0],
    //     [59874.3, 58308.9, 58427.8],
    //     [58766.1, 58088.0, 58737.7],
    //     [58746.5, 58380.0, 58506.6],
    // ];
    // let mut atr = ATR::new(2);
    // for line in prices {
    //     let atr = atr.next(line[0], line[1], line[2]);
    //     println!("atr reuslt{}", atr)
    // }
    //

    // let signal_result = SignalResult {
    //     should_buy: true,
    //     should_sell: false,
    //     price: 59692.00,
    //     ts: 1720569600000,
    // };
    // let res = order::deal(StrategyType::Engulfing, "BTC-USDT-SWAP", "4H", signal_result).await;
    // println!("{:?}", res);
    // return Ok(());

    let inst_ids = Some(vec!["BTC-USDT-SWAP"]);

    // 定义需要交易的产品及周期
    // let inst_ids = Some(Arc::new(vec![
    //     "BTC-USDT-SWAP",
    // ]));

    // let period = Arc::new(vec!["1m", "3m", "5m", "15m"]);

    // let inst_ids = Arc::new(vec!["BTC-USDT-SWAP", "SOL-USDT-SWAP", "ETH-USDT-SWAP", "ADA-USDT-SWAP", "SUSHI-USDT-SWAP"]);
    // let inst_ids = Arc::new(vec!["BTC-USDT-SWAP", "ETH-USDT-SWAP"]);
    // let times = Arc::new(vec!["4H", "1H", "5m", "1Dutc"]);

    // let inst_ids = Arc::new(vec!["BTC-USDT-SWAP"]);
    let period = Arc::new(vec!["4H"]);

    // let inst_ids = Arc::new(vec!["BTC-USDT-SWAP", "SOL-USDT-SWAP", "ETH-USDT-SWAP"]);
    // let times = Arc::new(vec!["4H", "1h", "5m", "1D"]);

    // 初始化需要同步的数据
    if env::var("IS_RUN_SYNC_DATA_JOB").unwrap() == "true" {
        println!("IS_RUN_SYNC_DATA_JOB");
        //初始化同步一次就行
        let res = tickets_job::init_all_ticker(inst_ids.clone()).await;
        if let Err(error) = res {
            error!("init all tickers error: {}", error);
        }
        let res = task::basic::run_sync_data_job(inst_ids.clone(), &period).await;
        if let Err(error) = res {
            error!("run sync data job error: {}", error);
        }
        info!("RUN_SYNC_DATA_JOB Ok!");
    }

    // 获取可用账户的最大数量
    // let max_avail_size = Account::get_max_size("ETH-USDT-SWAP", TdMode::ISOLATED).await?;
    // info!("max_avail_size: {:?}", max_avail_size);

    let mut scheduler = TaskScheduler::new();
    // 本地环境下执行回测任务
    if env::var("IS_BACK_TEST").unwrap() == "true" {
        println!("IS_BACK_TEST");
        if inst_ids.is_some() {
            for inst_id in inst_ids.unwrap(){
                for time in period.iter() {
                    let time = time.to_string();
                        //ut_boot_strategy
                        let res = task::basic::ut_boot_test(inst_id, &time).await;
                        //engulfing_strategy
                        // let res = task::engulfing_test(&inst_id, &time).await;
                        if let Err(error) = res {
                            error!("run strategy error: {}", error);
                        }
                    };
                }
            }
        }

    // 添加定时任务执行策略
    // {
    //     if env::var("IS_RUN_REAL_STRATEGY").unwrap_or(String::from("false")) == "true" {
    //         println!("run real strategy job");
    //
    //         //设置交易产品最大杠杆
    //         let result = task::basic::run_set_leverage(None).await;
    //         if let Err(error) = result {
    //             error!("run set leverage error: {}", error);
    //         }
    //
    //         let inst_ids = Arc::clone(None);
    //         let times = Arc::clone(&period);
    //
    //         {
    //             let inst_ids = Arc::clone(&inst_ids);
    //             let times = Arc::clone(&times);
    //             //执行ut_boot策略
    //
    //             scheduler.add_periodic_task(
    //                 "run_ut_boot_strategy_job".to_string(),
    //                 30000,
    //                 move || {
    //                     let inst_ids_inner = Arc::clone(&inst_ids);
    //                     let times_inner = Arc::clone(&times);
    //                     async move {
    //                         println!("run ut boot job");
    //                         let res = task::basic::run_strategy_job(
    //                             inst_ids_inner,
    //                             times_inner,
    //                             StrategyType::UtBoot,
    //                         )
    //                         .await;
    //                         if let Err(error) = res {
    //                             error!("run ut boot strategy error: {}", error);
    //                         }
    //                     }
    //                 },
    //             );
    //         }
    //
    //         {
    //             let inst_ids = Arc::clone(&inst_ids);
    //             let times = Arc::clone(&times);
    //             //添务执行Engulfing策略
    //             scheduler.add_periodic_task(
    //                 "run_engulfing_strategy_job".to_string(),
    //                 30000,
    //                 move || {
    //                     let inst_ids_inner = Arc::clone(&inst_ids);
    //                     let times_inner = Arc::clone(&times);
    //                     async move {
    //                         println!("run engulfing job");
    //                         let res = task::basic::run_strategy_job(
    //                             inst_ids_inner,
    //                             times_inner,
    //                             StrategyType::Engulfing,
    //                         )
    //                         .await;
    //                         if let Err(error) = res {
    //                             error!("run engulfing strategy error: {}", error);
    //                         }
    //                     }
    //                 },
    //             );
    //         }
    //     }
    // }

    // 运行WebSocket服务
    // {
    //     if env::var("IS_OPEN_SOCKET").unwrap() == "true" {
    //         let inst_ids = Arc::clone(None);
    //         let times = Arc::clone(&period);
    //         socket::run_socket(inst_ids, times).await;
    //     }
    // }

    // 捕捉Ctrl+C信号以平滑关闭
    tokio::signal::ctrl_c().await?;

    scheduler.shutdown().await;

    Ok(())
}
