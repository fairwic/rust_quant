use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::reexpansion_volume_rank_stable_panel_v12::run_v12_l1;
use std::path::PathBuf;

/// V12 L1 只接受冻结 V6、V11 证据与输出路径，禁止命令行改写面板阈值。
fn paths_from_args() -> Result<(PathBuf, PathBuf, PathBuf)> {
    let mut v6_source = None;
    let mut v11_source = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--v6-source" => {
                v6_source = Some(PathBuf::from(
                    args.next().context("--v6-source requires a file path")?,
                ));
            }
            "--v11-source" => {
                v11_source = Some(PathBuf::from(
                    args.next().context("--v11-source requires a file path")?,
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
        v11_source.context("--v11-source is required")?,
        output.context("--output is required")?,
    ))
}

/// 生成 Research-only V12 L1 候选账本，不写交易库或运行策略配置。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (v6_source, v11_source, output) = paths_from_args()?;
    let report = run_v12_l1(&v6_source, &v11_source, &output).await?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} candidates={} reduction_pct={:.4} long={} short={} symbols={} months={} targets={}/3 panels={:?} outcome_evaluation={}",
        report.decision.status,
        report.summary.candidate_count,
        report.summary.candidate_reduction_pct,
        report.summary.by_direction.get("long").copied().unwrap_or_default(),
        report.summary.by_direction.get("short").copied().unwrap_or_default(),
        report.summary.by_symbol.len(),
        report.summary.by_month_utc.len(),
        report.target_audits.iter().filter(|audit| audit.matched).count(),
        report.summary.by_actual_panel_size,
        report.decision.outcome_evaluation_performed,
    );
    Ok(())
}
