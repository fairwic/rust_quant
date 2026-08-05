use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::run_l1_scan;
use std::path::PathBuf;

/// 只开放报告路径，研究窗口、币池和阈值必须保持预注册身份。
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

/// 执行 Research-only L1 扫描；本入口不会回放成交或写入交易事实。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let output = output_path_from_args()?;
    let report = run_l1_scan(&output).await?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} candidates={} effective_events={} targets={}/{}",
        report.decision.status,
        report.summary.candidate_count,
        report.summary.effective_market_events,
        report
            .target_audits
            .iter()
            .filter(|audit| audit.matched)
            .count(),
        report.target_audits.len(),
    );
    Ok(())
}
