use anyhow::Result;
use rust_quant_cli::app::market_velocity_live_handoff::run_market_velocity_live_handoff_runtime_from_env;
#[tokio::main]
/// 封装当前函数，减少行情数据调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    run_market_velocity_live_handoff_runtime_from_env().await
}
