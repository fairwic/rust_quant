use anyhow::Result;
use rust_quant_infrastructure::repositories::fund_monitoring_repository::SqlxFundFlowAlertRepository;
use rust_quant_services::market::FlowAnalyzer;
use sqlx::mysql::MySqlPoolOptions;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // 0. Install Rustls Crypto Provider (required by tokio-tungstenite 0.23+ / rustls 0.23)
    let _ = rustls::crypto::ring::default_provider().install_default();

    // 1. 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting verification of Deep Stream Manager...");

    // 加载配置
    dotenv::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // 连接数据库
    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let alert_repo = Arc::new(SqlxFundFlowAlertRepository::new(pool.clone()));

    // 2. 创建 FlowAnalyzer 和 StreamManager
    let (analyzer, manager) = FlowAnalyzer::new(alert_repo);

    // 3. 启动 analyzer (它会启动内部的 stream manager)
    let analyzer_handle = tokio::spawn(async move {
        analyzer.run().await;
    });

    // 4. 手动订阅一些热门币种 (模拟“提升关注”)
    info!("Promoting BTC, ETH, SOL...");
    manager.promote("BTC-USDT-SWAP").await?;
    sleep(Duration::from_millis(100)).await;
    manager.promote("ETH-USDT-SWAP").await?;
    sleep(Duration::from_millis(100)).await;
    manager.promote("SOL-USDT-SWAP").await?;

    // 5. 等待一段时间观察数据流
    info!("Waiting for trade data...");
    sleep(Duration::from_secs(30)).await;

    // 6. 模拟“降低关注”
    info!("Demoting ETH...");
    manager.demote("ETH-USDT-SWAP").await?;

    sleep(Duration::from_secs(10)).await;

    info!("Verification finished. Press Ctrl+C to exit.");
    tokio::signal::ctrl_c().await?;

    Ok(())
}
