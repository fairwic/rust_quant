use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::persistent_dynamic_retest_v2::run_v4_l1_scan;
use std::path::PathBuf;

/// V4 仅接收输出路径，持续资格与所有阈值均由预注册代码冻结。
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

/// 执行 V4 Research-only L1 扫描，不读取任何成交后结果。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let output = output_path_from_args()?;
    let report = run_v4_l1_scan(&output).await?;
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
