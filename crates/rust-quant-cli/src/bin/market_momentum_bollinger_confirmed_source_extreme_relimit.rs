use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::momentum_exhaustion_bollinger_wick_l1::confirmed_source_extreme_relimit::run_confirmed_source_extreme_relimit;
use std::path::PathBuf;

/// 解析冻结来源 L1 和本批唯一机器报告路径；不接受研究参数覆盖。
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

/// 先执行 L1 无结果成交覆盖，仅在通过时继续一个主候选的 L2 配对回放。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (source, output) = paths_from_args()?;
    let report = run_confirmed_source_extreme_relimit(&source, &output).await?;
    println!("{}", output.display());
    if let Some(l2) = report.l2 {
        eprintln!(
            "L1 status={} fills={} retention={:.2}% | L2 status={} complete={} baseline_ev={:.6} candidate_ev={:.6} delta_r={:.6}",
            report.l1.decision.status,
            report.l1.summary.relimit_filled_setups,
            report.l1.summary.fill_retention_pct,
            l2.decision.status,
            l2.entry_summary.completed_pairs,
            l2.baseline_next_open.net_expectancy_r,
            l2.candidate_relimit.net_expectancy_r,
            l2.concentration.total_delta_net_r,
        );
    } else {
        eprintln!(
            "L1 status={} fills={} retention={:.2}% | L2 skipped",
            report.l1.decision.status,
            report.l1.summary.relimit_filled_setups,
            report.l1.summary.fill_retention_pct,
        );
    }
    Ok(())
}
