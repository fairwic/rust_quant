use futures_util::StreamExt;
use okx::websocket::auto_reconnect_client::AutoReconnectWebsocketClient;
use std::env;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, span, Level};

use crate::cache::default_provider;
use crate::models::TickersDataEntity;
use crate::repositories::candle_service::{CandleService, StrategyTrigger};
use crate::repositories::persist_worker::{CandlePersistWorker, PersistTask};
use crate::repositories::ticker_service::TickerService;
use okx::config::Credentials;
use okx::dto::market_dto::CandleOkxRespDto;
use okx::dto::CandleOkxWsResDto;
use okx::dto::CommonOkxWsResDto;
use okx::dto::TickerOkxResWsDto;
use okx::websocket::{Args, ChannelType};
use tracing::debug;
use tracing::error;

/// WebSocket 服务入口
///
/// # 参数
/// * `inst_ids` - 交易对列表
/// * `times` - 时间周期列表
/// * `strategy_trigger` - 可选的策略触发回调函数
///
/// # 架构说明
/// - 如果提供 strategy_trigger，则 K线确认时会自动触发策略执行
/// - 如果不提供，则仅处理 K线数据存储和缓存
pub async fn run_socket(inst_ids: &[String], times: &[String]) {
    run_socket_with_strategy_trigger(inst_ids, times, None).await;
}

/// 带策略触发的 WebSocket 服务
///
/// # 参数
/// * `inst_ids` - 交易对列表
/// * `times` - 时间周期列表
/// * `strategy_trigger` - 策略触发回调函数
pub async fn run_socket_with_strategy_trigger(
    inst_ids: &[String],
    times: &[String],
    strategy_trigger: Option<StrategyTrigger>,
) {
    let span = span!(Level::DEBUG, "socket_logic");
    let _enter = span.enter();
    // 模拟盘的请求的header里面需要添加 "x-simulated-trading: 1"。
    let api_key = env::var("OKX_API_KEY").expect("未配置OKX_API_KEY");
    let api_secret = env::var("OKX_API_SECRET").expect("未配置OKX_API_SECRET");
    let passphrase = env::var("OKX_PASSPHRASE").expect("未配置OKX_PASSPHRASE");
    let sim_trading = env::var("OKX_SIMULATED_TRADING").expect("未配置OKX_SIMULATED_TRADING");

    // 🚀 [已优化] 创建批处理Worker
    info!("🚀 初始化批处理Worker...");
    let (persist_tx, persist_rx) = mpsc::unbounded_channel::<PersistTask>();
    let worker = CandlePersistWorker::new(persist_rx)
        .with_config(100, std::time::Duration::from_millis(500));

    // 启动Worker
    tokio::spawn(async move {
        worker.run().await;
    });

    // 🚀 [已优化] 创建共享的CandleService实例（带策略触发）
    let candle_service = if let Some(trigger) = strategy_trigger {
        info!("✅ 创建 CandleService 实例（启用策略触发）");
        Arc::new(CandleService::new_with_strategy_trigger(
            default_provider(),
            Some(persist_tx),
            trigger,
        ))
    } else {
        info!("✅ 创建 CandleService 实例（未启用策略触发）");
        Arc::new(CandleService::new_with_persist_worker(
            default_provider(),
            persist_tx,
        ))
    };
    info!("✅ CandleService实例已创建并启用批处理");

    // 创建自动重连客户端
    info!("📡 创建自动重连客户端...");
    let public_client = AutoReconnectWebsocketClient::new_public();

    let mut public_receiver = match public_client.start().await {
        Ok(rx) => {
            info!("✅ okx public websocket启动成功");
            rx
        }
        Err(e) => {
            error!("❌ okx public websocket启动失败: {}", e);
            return;
        }
    };
    let credentials = Credentials::new(api_key, api_secret, passphrase, sim_trading);

    let okx_websocket_client_business = AutoReconnectWebsocketClient::new_business(credentials);

    let mut private_message_receiver = match okx_websocket_client_business.start().await {
        Ok(rx) => {
            info!("✅ okx private websocket启动成功");
            rx
        }
        Err(e) => {
            error!("❌ okx private websocket启动失败: {}", e);
            return;
        }
    };

    // 订阅多个k线频道
    for inst_id in inst_ids.iter() {
        for time in times.iter() {
            let args = Args::new()
                .with_inst_id(inst_id.to_string())
                .with_param("period".to_string(), time.to_string());
            // 用私有client订阅k线频道
            let task = okx_websocket_client_business
                .subscribe(ChannelType::Candle(time.to_string()), args.clone())
                .await;
            match task {
                Ok(_) => {
                    info!("订阅k线频道成功: {:?},{:?}", inst_id, time);
                }
                Err(e) => {
                    error!("订阅k线频道失败: {:?}", e);
                }
            }
        }
    }

    // 订阅多个tickers频道
    for inst_id in inst_ids.iter() {
        let args = Args::new().with_inst_id(inst_id.to_string());
        // 用公有client订阅tickers频道
        let task = public_client
            .subscribe(ChannelType::Tickers, args.clone())
            .await;
        match task {
            Ok(_) => {
                info!("订阅tickers频道成功: {:?}", inst_id);
            }
            Err(e) => {
                error!("订阅tickers频道失败: {:?}", e);
            }
        }
    }

    let inst_filters = Arc::new(inst_ids.to_vec());
    let ticker_service = Arc::new(TickerService::new());

    // 持续监听并处理 ticker 消息
    {
        let inst_filters = Arc::clone(&inst_filters);
        let ticker_service = Arc::clone(&ticker_service);
        tokio::spawn(async move {
            while let Some(msg) = public_receiver.recv().await {
                if let Ok(ticker) = serde_json::from_value::<TickerOkxResWsDto>(msg.clone()) {
                    let tickers = ticker
                        .data
                        .iter()
                        .map(TickersDataEntity::from_okx_ticker)
                        .collect::<Vec<_>>();
                    if let Err(e) = ticker_service
                        .upsert_tickers(tickers, inst_filters.as_ref())
                        .await
                    {
                        error!("更新ticker失败: {:?}", e);
                    }
                } else if let Ok(dto) = serde_json::from_value::<CommonOkxWsResDto>(msg) {
                    if dto.code != "0" {
                        error!("收到ticker错误消息: code={}, msg={}", dto.code, dto.msg);
                    } else {
                        debug!("收到ticker确认消息: {:?}", dto);
                    }
                }
            }
        });
    }
    // 🚀 [已优化] 复用service实例 + 消除二次序列化
    let candle_service_clone = Arc::clone(&candle_service);
    tokio::spawn(async move {
        while let Some(msg) = private_message_receiver.recv().await {
            // 🚀 [已优化] 直接从 Value 解析，避免 to_string() 序列化
            if let Ok(candle) = serde_json::from_value::<CandleOkxWsResDto>(msg.clone()) {
                debug!(
                    "收到K线数据: inst_id={}, channel={}",
                    candle.arg.inst_id, candle.arg.channel
                );

                // 提取周期：candle2h -> 2h
                let period = candle.arg.channel.replace("candle", "");

                // 🚀 [已优化] 处理全部数据（而非只取last），使用into_iter避免clone
                let candle_data: Vec<CandleOkxRespDto> = candle
                    .data
                    .into_iter()
                    .map(CandleOkxRespDto::from_vec)
                    .collect();

                // 🚀 [已优化] 使用共享实例，批量处理
                if let Err(e) = candle_service_clone
                    .update_candles_batch(candle_data, &candle.arg.inst_id, &period)
                    .await
                {
                    error!(
                        "批量更新K线失败: inst_id={}, period={}, error={:?}",
                        candle.arg.inst_id, period, e
                    );
                }
            } else if let Ok(dto) = serde_json::from_value::<CommonOkxWsResDto>(msg) {
                if dto.code != "0" {
                    error!("收到错误消息: code={}, msg={}", dto.code, dto.msg);
                } else {
                    debug!("收到确认消息: {:?}", dto);
                }
            }
        }
    });
}
