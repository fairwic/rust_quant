use anyhow::Result;
use rust_quant_cli::app::market_velocity_event_backtest::{
    config_from_env_and_args, parse_cli_args_from, run_market_velocity_event_backtest,
};
#[tokio::main]
/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();
    let args = parse_cli_args_from(std::env::args().skip(1))?;
    let config = config_from_env_and_args(args)?;
    run_market_velocity_event_backtest(config).await
}
