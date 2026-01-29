use anyhow::Result;
use rust_quant_orchestration::jobs::data::fund_monitor_job::FundMonitorJob;
use rustls;
use tracing::info;

use rust_quant_infrastructure::repositories::fund_monitoring_repository::{
    SqlxFundFlowAlertRepository, SqlxMarketAnomalyRepository,
};
use sqlx::mysql::MySqlPoolOptions;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // 0. Install Rustls Crypto Provider
    let _ = rustls::crypto::ring::default_provider().install_default();

    // 1. 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting Integrated Fund Monitor...");

    // 加载配置 (.env)
    dotenv::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // 连接数据库
    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let anomaly_repo = Arc::new(SqlxMarketAnomalyRepository::new(pool.clone()));
    let alert_repo = Arc::new(SqlxFundFlowAlertRepository::new(pool.clone()));

    // 2. 创建 MonitorJob (同时会创建 FlowAnalyzer)
    let (mut job, analyzer) = FundMonitorJob::new(10, anomaly_repo, alert_repo)?; // 10s 扫描一次

    // 3. 启动 FlowAnalyzer (后台运行)
    tokio::spawn(async move {
        analyzer.run().await;
    });

    // 4. 运行 Monitor Loop (主流程)
    job.run_loop().await;

    Ok(())
}
