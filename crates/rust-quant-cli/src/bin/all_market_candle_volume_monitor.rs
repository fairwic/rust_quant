use anyhow::Result;
use rust_quant_cli::app::all_market_candle_volume_monitor::{
    run_all_market_candle_volume_monitor, AllMarketCandleVolumeMonitorConfig,
};

/// 运行只读的全市场收盘 K 线成交量监听器；不创建或执行任何交易任务。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();
    let config = AllMarketCandleVolumeMonitorConfig::from_env()?;
    run_all_market_candle_volume_monitor(config).await
}
