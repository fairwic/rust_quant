use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::target2r_v9::run_l1_geometry;
use std::path::PathBuf;

/// L1 只接受冻结 V6 源账本和机器报告输出路径，禁止命令行调参。
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

/// 生成 Research-only V9 L1 几何报告，不访问数据库或成交后行情。
fn main() -> Result<()> {
    let (source, output) = paths_from_args()?;
    let report = run_l1_geometry(&source, &output)?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} candidates={} valid_geometry={} targets={}/3 outcome_evaluation={}",
        report.decision.status,
        report.summary.source_candidates,
        report.summary.valid_geometry_candidates,
        report
            .target_audits
            .iter()
            .filter(|audit| audit.matched)
            .count(),
        report.decision.outcome_evaluation_performed,
    );
    Ok(())
}
