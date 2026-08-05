use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::momentum_entry_shell_v10::run_v10_l1;
use std::path::PathBuf;

/// V10 L1 只接受冻结 V6 源账本与输出路径，禁止命令行修改策略参数。
fn paths_from_args() -> Result<(PathBuf, PathBuf)> {
    let mut v6_source = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--v6-source" => {
                v6_source = Some(PathBuf::from(
                    args.next().context("--v6-source requires a file path")?,
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
        v6_source.context("--v6-source is required")?,
        output.context("--output is required")?,
    ))
}

/// 生成 Research-only V10 L1 联合候选账本，不写交易库或运行策略配置。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (v6_source, output) = paths_from_args()?;
    let report = run_v10_l1(&v6_source, &output).await?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} candidates={} symbols={} months={} targets={}/3 outcome_evaluation={}",
        report.decision.status,
        report.summary.candidate_count,
        report.summary.by_symbol.len(),
        report.summary.by_month_utc.len(),
        report
            .target_audits
            .iter()
            .filter(|audit| audit.matched)
            .count(),
        report.decision.outcome_evaluation_performed,
    );
    Ok(())
}
