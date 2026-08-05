use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::momentum_exhaustion_bollinger_wick_l1::first_retest_source_extreme_reclaim::run_first_retest_source_extreme_reclaim_l1;
use std::path::PathBuf;

/// 解析冻结来源报告和新机器报告路径；研究参数全部由模块固定。
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

/// 执行 Research-only L1 账本转换并打印最小覆盖结论。
fn main() -> Result<()> {
    let (source, output) = paths_from_args()?;
    let report = run_first_retest_source_extreme_reclaim_l1(&source, &output)?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} base={} first_retests={} confirmed={} rejected={} target_rejected={}",
        report.decision.status,
        report.summary.base_touch_setups,
        report.summary.first_retest_setups,
        report.summary.confirmed_setups,
        report.summary.rejected_source_extreme_setups,
        report.summary.target_rejected_count,
    );
    Ok(())
}
