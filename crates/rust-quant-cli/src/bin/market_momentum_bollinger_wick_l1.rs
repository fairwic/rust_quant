use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::momentum_exhaustion_bollinger_wick_l1::run_l1_scan;
use std::path::PathBuf;

/// 解析唯一允许变化的输出路径，研究参数全部由 L1 模块冻结。
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

/// 执行只读 L1 扫描并仅打印产物路径与覆盖结论。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let output = output_path_from_args()?;
    let report = run_l1_scan(&output).await?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} eligible_symbols={} source_wicks={} outer_touches={} effective_events={}",
        report.decision.status,
        report.coverage.eligible_symbol_count,
        report.summary.source_directional_wick_setups,
        report.summary.outer_band_touch_setups,
        report.summary.effective_market_events,
    );
    Ok(())
}
