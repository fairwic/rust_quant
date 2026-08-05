use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::structure_target_v13::run_v13_l1;
use std::path::PathBuf;

/// V13 L1 只接受冻结 V12 授权和输出路径，结构参数不能由命令行改写。
fn paths_from_args() -> Result<(PathBuf, PathBuf)> {
    let mut v12_source = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--v12-source" => {
                v12_source = Some(PathBuf::from(
                    args.next().context("--v12-source requires a file path")?,
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
        v12_source.context("--v12-source is required")?,
        output.context("--output is required")?,
    ))
}

/// 生成 Research-only V13 L1 结构目标账本，不写交易库或运行策略配置。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (v12_source, output) = paths_from_args()?;
    let report = run_v13_l1(&v12_source, &output).await?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} candidates={} coverage_pct={:.4} long={} short={} symbols={} months={} events={} targets={}/3 target_r_p50={:?} target_r_p90={:?} outcome_evaluation={}",
        report.decision.status,
        report.summary.candidate_count,
        report.summary.valid_target_coverage_pct,
        report.summary.by_direction.get("long").copied().unwrap_or_default(),
        report.summary.by_direction.get("short").copied().unwrap_or_default(),
        report.summary.by_symbol.len(),
        report.summary.by_month_utc.len(),
        report.summary.effective_market_events,
        report.target_audits.iter().filter(|audit| audit.matched).count(),
        report.summary.target_r_at_limit_p50,
        report.summary.target_r_at_limit_p90,
        report.decision.outcome_evaluation_performed,
    );
    Ok(())
}
