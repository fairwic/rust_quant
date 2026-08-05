use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::persistent_qualification_order_l2::run_l2_replay;
use std::path::PathBuf;

/// L2 只接受冻结 L1 来源和机器报告输出路径，禁止命令行调参。
fn paths_from_args() -> Result<(PathBuf, PathBuf)> {
    let mut l1_source = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--l1-source" => {
                l1_source = Some(PathBuf::from(
                    args.next().context("--l1-source requires a file path")?,
                ));
            }
            "--output" => {
                output = Some(PathBuf::from(
                    args.next().context("--output requires a file path")?,
                ));
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    Ok((
        l1_source.context("--l1-source is required")?,
        output.context("--output is required")?,
    ))
}

/// 执行 Research-only L2 成本回放；不写交易库或注册任何运行态策略。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (l1_source, output) = paths_from_args()?;
    let report = run_l2_replay(&l1_source, &output).await?;
    println!("{}", output.display());
    eprintln!(
        "L2 status={} completed={} gross_ev={:.6} net_ev={:.6} net_pf={}",
        report.decision.status,
        report.coverage.completed_trades,
        report.gross.expectancy_r,
        report.net.expectancy_r,
        report
            .net
            .profit_factor
            .map(|value| format!("{value:.6}"))
            .unwrap_or_else(|| "null".to_owned()),
    );
    Ok(())
}
