use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::momentum_exhaustion_bollinger_wick_l1::recent_fast_ema_lead::run_recent_fast_ema_lead_l1_scan;
use std::path::PathBuf;

/// 只允许调用方指定输出路径，研究身份和阈值均由模块冻结。
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

/// 执行只读 L1 扫描并打印足够判断停止边界的最小汇总。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let output = output_path_from_args()?;
    let report = run_recent_fast_ema_lead_l1_scan(&output).await?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} base_touch_setups={} rejected_setups={} impact_pct={:.4}",
        report.decision.status,
        report.summary.base_touch_setups,
        report.summary.rejected_setups,
        report.summary.impact_pct,
    );
    Ok(())
}
