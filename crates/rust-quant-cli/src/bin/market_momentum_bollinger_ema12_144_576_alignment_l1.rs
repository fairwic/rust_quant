use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::momentum_exhaustion_bollinger_wick_l1::single_bar_ema_12_144_576_alignment::run_single_bar_ema_12_144_576_alignment_l1_scan;
use std::path::PathBuf;

/// 解析唯一允许变化的输出路径，EMA12/144/576 单根排列合同由 L1 模块冻结。
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

/// 执行只读单根 EMA12/144/576 排列新形成 L1 扫描并打印最小覆盖结论。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let output = output_path_from_args()?;
    let report = run_single_bar_ema_12_144_576_alignment_l1_scan(&output).await?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} base_touch_setups={} newly_formed_opposite_setups={}",
        report.decision.status,
        report.summary.base_touch_setups,
        report.summary.newly_formed_opposite_setups,
    );
    Ok(())
}
