use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::momentum_exhaustion_bollinger_wick_l1::first_retest_band_reentry::run_first_retest_band_reentry_l1_scan;
use std::path::PathBuf;

/// 只允许指定机器报告输出路径，研究身份和参数由模块冻结。
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

/// 执行 Research-only L1 扫描并打印最小覆盖结论。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let output = output_path_from_args()?;
    let report = run_first_retest_band_reentry_l1_scan(&output).await?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} base={} first_retests={} confirmed={} rejected={} target_rejected={}",
        report.decision.status,
        report.summary.base_touch_setups,
        report.summary.first_retest_setups,
        report.summary.confirmed_setups,
        report.summary.rejected_close_acceptance_setups,
        report.summary.target_rejected_count,
    );
    Ok(())
}
