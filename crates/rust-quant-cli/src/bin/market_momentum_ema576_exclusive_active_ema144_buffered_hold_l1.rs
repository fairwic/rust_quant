use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::first_breakout_pullback_hold_v1::exclusive_active_retests_v2::buffered_hold_v3::run_v3_l1_scan;
use std::path::PathBuf;

/// V3 只接受输出路径，禁止用命令行参数覆盖预注册守稳边界或生命周期。
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

/// 执行 V3 Research-only L1 扫描，不回放成交或写入数据库。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let output = output_path_from_args()?;
    let report = run_v3_l1_scan(&output).await?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} candidates={} effective_events={} target_gates={}/{}",
        report.decision.status,
        report.summary.candidate_count,
        report.summary.effective_market_events,
        report
            .target_audits
            .iter()
            .filter(|audit| audit.passed)
            .count(),
        report.target_audits.len(),
    );
    Ok(())
}
