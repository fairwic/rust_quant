use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::momentum_exhaustion_bollinger_wick_l1::source_extreme_reclaim_l2::run_source_extreme_reclaim_l2;
use std::path::PathBuf;

/// 解析冻结 L1 来源报告与 L2 输出路径；其他研究参数全部由模块冻结。
fn paths_from_args() -> Result<(PathBuf, PathBuf)> {
    let mut source = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--source" => {
                source = Some(PathBuf::from(
                    args.next().context("--source requires a file path")?,
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
        source.context("--source is required")?,
        output.context("--output is required")?,
    ))
}

/// 执行 Research-only L2 配对回放并打印最小联合门禁结果。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (source, output) = paths_from_args()?;
    let report = run_source_extreme_reclaim_l2(&source, &output).await?;
    println!("{}", output.display());
    eprintln!(
        "L2 status={} pairs={} complete={} baseline_ev={:.6} variant_ev={:.6} delta_r={:.6}",
        report.decision.status,
        report.entry_summary.executed_pairs,
        report.entry_summary.completed_pairs,
        report.baseline.net_expectancy_r,
        report.variant.net_expectancy_r,
        report.concentration.total_delta_net_r,
    );
    Ok(())
}
