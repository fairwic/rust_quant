use anyhow::{Context, Result};
use rust_quant_cli::app::vegas_bear_failed_compressed_reclaim_research::run_btc_positive_macd_research;

/// 运行冻结 V67 BTC 正 MACD + 本币失败跌破回收多头研究，不触发交易执行。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context(
            "vegas_btc_positive_macd_failed_breakdown_reclaim_research requires QUANT_CORE_DATABASE_URL",
        )?;
    run_btc_positive_macd_research(&database_url).await
}
