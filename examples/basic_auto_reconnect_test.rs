use std::time::Duration;
use tokio::time::sleep;
use env_logger;
use log::{info, error, warn, debug};

use okx::websocket::auto_reconnect_client::{AutoReconnectWebsocketClient, ReconnectConfig};
use okx::websocket::channel::{Args, ChannelType};

/// 基础自动重连WebSocket客户端测试
/// 
/// 这个示例展示了包组件内部自动重连的核心功能
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志，设置为DEBUG级别
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();

    println!("🚀 启动基础自动重连WebSocket测试");
    info!("🚀 启动基础自动重连WebSocket测试");

    // 创建重连配置
    let config = ReconnectConfig {
        enabled: true,           // 启用自动重连
        interval: 5,             // 5秒重连间隔
        max_attempts: 5,         // 最多重连5次
        backoff_factor: 1.5,     // 指数退避因子
        max_backoff: 30,         // 最大退避30秒
        heartbeat_interval: 3,  // 30秒心跳检查
        message_timeout: 60,     // 60秒消息超时
    };

    // 创建自动重连客户端
    println!("📡 创建自动重连客户端...");
    let client = AutoReconnectWebsocketClient::new_with_config(None, config);

    println!("📡 启动WebSocket客户端...");
    info!("📡 启动WebSocket客户端...");

    // 启动客户端
    let mut message_receiver = match client.start().await {
        Ok(rx) => {
            println!("✅ 客户端启动成功");
            info!("✅ 客户端启动成功");
            rx
        }
        Err(e) => {
            println!("❌ 客户端启动失败: {}", e);
            error!("❌ 客户端启动失败: {}", e);
            return Err(e.into());
        }
    };

    // 订阅BTC-USDT-SWAP价格数据
    println!("📋 订阅BTC-USDT-SWAP价格数据...");
    info!("📋 订阅BTC-USDT-SWAP价格数据...");
    let args = Args::new().with_inst_id("BTC-USDT-SWAP".to_string());
    match client.subscribe(ChannelType::Tickers, args).await {
        Ok(_) => {
            println!("✅ 订阅成功");
            info!("✅ 订阅成功");
        }
        Err(e) => {
            println!("❌ 订阅失败: {}", e);
            error!("❌ 订阅失败: {}", e);
            return Err(e.into());
        }
    }

    // 启动消息处理任务
    let message_task = tokio::spawn(async move {
        let mut message_count = 0;
        println!("🎧 开始接收消息...");
        info!("🎧 开始接收消息...");

        while let Some(message) = message_receiver.recv().await {
            message_count += 1;

            // 每条消息都显示
            if message_count <= 10 {
                println!("📊 收到第 {} 条消息: {:?}", message_count, message);
                info!("📊 收到第 {} 条消息: {:?}", message_count, message);
            } else if message_count % 10 == 0 {
                println!("📊 已接收 {} 条消息", message_count);
                info!("📊 已接收 {} 条消息", message_count);
            }

            // 显示价格信息
            if let Some(data) = message.get("data") {
                if let Some(array) = data.as_array() {
                    if let Some(ticker) = array.first() {
                        if let Some(last_price) = ticker.get("last") {
                            println!("💰 BTC-USDT-SWAP 最新价格: {}", last_price);
                            info!("💰 BTC-USDT-SWAP 最新价格: {}", last_price);
                        }
                    }
                }
            }
        }

        println!("🔚 消息接收任务结束");
        info!("🔚 消息接收任务结束");
    });

    // 启动连接状态监控任务
    let status_client = client.clone();
    let status_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            
            let state = status_client.get_connection_state();
            let is_healthy = status_client.is_connection_healthy();
            let active_subs = status_client.get_active_subscriptions_count();
            
            // 根据连接状态显示不同的emoji和信息
            let status_emoji = match state {
                okx::websocket::auto_reconnect_client::ConnectionState::Connected => "🟢",
                okx::websocket::auto_reconnect_client::ConnectionState::Connecting => "🟡",
                okx::websocket::auto_reconnect_client::ConnectionState::Reconnecting => "🔄",
                okx::websocket::auto_reconnect_client::ConnectionState::Disconnected => "🔴",
            };
            
            let health_emoji = if is_healthy { "💚" } else { "💔" };
            
            info!("{} 连接状态: {:?} {} 健康: {} | 活跃订阅: {}", 
                  status_emoji, state, health_emoji, is_healthy, active_subs);
        }
    });

    // 测试说明
    info!("🧪 测试说明:");
    info!("   1. 客户端将自动连接到OKX WebSocket服务器");
    info!("   2. 开始接收BTC-USDT-SWAP的实时价格数据");
    info!("   3. 💡 **测试重连功能**: 请在运行期间断开网络连接");
    info!("   4. 🔄 **观察自动重连**: 网络恢复后，客户端会自动重连并恢复数据接收");
    info!("   5. ⏰ 测试将运行60秒");
    
    // 运行测试
    tokio::select! {
        _ = message_task => {
            info!("消息处理任务结束");
        }
        _ = status_task => {
            info!("状态监控任务结束");
        }
        _ = sleep(Duration::from_secs(200)) => {
            info!("⏰ 测试时间结束 (60秒)");
        }
    }

    // 停止客户端
    info!("🔌 停止WebSocket客户端");
    client.stop().await;
    
    info!("✅ 测试完成");
    info!("");
    info!("🎉 **核心优势总结**:");
    info!("   ✅ 应用层无需处理重连逻辑");
    info!("   ✅ 内置智能重连策略（指数退避）");
    info!("   ✅ 自动恢复订阅状态");
    info!("   ✅ 实时连接健康监控");
    info!("   ✅ 简化的API设计");
    
    Ok(())
}
