use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::momentum_exhaustion_bollinger_wick_l1::middle_partial_exit_l2::run_middle_partial_exit_l2;
use std::path::PathBuf;

/// 只接受机器报告输出路径，所有研究参数均由预注册 L2 模块冻结。
fn output_path_from_args() -> Result<PathBuf> {
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--output" => {
                output = Some(PathBuf::from(
                    args.next().context("--output requires a file path")?,
                ));
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    output.context("--output is required")
}

/// 运行只读 L2 回放并打印最小门禁结论，不写数据库或注册运行态策略。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let output = output_path_from_args()?;
    let report = run_middle_partial_exit_l2(&output).await?;
    println!("{}", output.display());
    eprintln!(
        "L2 status={} completed_trades={} partial_triggers={} delta_net_r={:.6}",
        report.decision.status,
        report.entry_summary.completed_trades,
        report.entry_summary.partial_triggered_trades,
        report.concentration.total_delta_net_r,
    );
    Ok(())
}
