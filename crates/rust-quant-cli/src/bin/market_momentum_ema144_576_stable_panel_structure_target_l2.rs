use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::persistent_qualification_order_l2::run_structure_target_v13_l2_replay;
use std::path::PathBuf;

/// V13 L2 只接受冻结 V6 源账本、V13 授权与输出路径。
fn paths_from_args() -> Result<(PathBuf, PathBuf, PathBuf)> {
    let mut v6_source = None;
    let mut v13_authorization = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--v6-source" => {
                v6_source = Some(PathBuf::from(
                    args.next().context("--v6-source requires a file path")?,
                ));
            }
            "--v13-authorization" => {
                v13_authorization = Some(PathBuf::from(
                    args.next()
                        .context("--v13-authorization requires a file path")?,
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
        v13_authorization.context("--v13-authorization is required")?,
        output.context("--output is required")?,
    ))
}

/// 执行 V13 Research-only L2 成本诊断，不注册或写入任何运行态策略。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (v6_source, authorization, output) = paths_from_args()?;
    let report = run_structure_target_v13_l2_replay(&v6_source, &authorization, &output).await?;
    println!("{}", output.display());
    eprintln!(
        "L2 status={} trades={} gross_ev={:.6} gross_pf={:?} net_ev={:.6} net_pf={:?} long_net_ev={:?} short_net_ev={:?}",
        report.decision.status,
        report.coverage.completed_trades,
        report.gross.expectancy_r,
        report.gross.profit_factor,
        report.net.expectancy_r,
        report.net.profit_factor,
        report.net_by_direction.get("long").map(|value| value.expectancy_r),
        report.net_by_direction.get("short").map(|value| value.expectancy_r),
    );
    Ok(())
}
