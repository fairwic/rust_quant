use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::first_breakout_pullback_hold_v1::exclusive_active_retests_v2::buffered_hold_v3::dual_active_v4::failed_retest_termination_v6::persistent_qualification_v7::close_invalidated_episode_v8::armed_close_post_cross_recross_v9::pre_cross_breakout_v10::signal_cross_timeout_v11::first_entry_only_v12::structural_stop_v13::confirmation_close_distance_v14::run_v14_l2_replay;
use std::path::PathBuf;

/// V14 L2 只接受冻结 V14 L1 与输出路径，不开放距离、风险或退出参数。
fn paths_from_args() -> Result<(PathBuf, PathBuf)> {
    let mut v14_l1_source = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--v14-l1-source" => {
                v14_l1_source = Some(PathBuf::from(
                    args.next()
                        .context("--v14-l1-source requires a file path")?,
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
        v14_l1_source.context("--v14-l1-source is required")?,
        output.context("--output is required")?,
    ))
}

/// 执行 V14 Research-only 主 cap 回放，并输出合并 L1/L2 机器结果。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (v14_l1_source, output) = paths_from_args()?;
    let report = run_v14_l2_replay(&v14_l1_source, &output).await?;
    println!("{}", output.display());
    eprintln!(
        "L2 status={} completed={} events={} net_ev={:.6} net_pf={:?}",
        report.l2.decision.status,
        report.l2.coverage.completed_trades,
        report.l2.coverage.completed_effective_market_events,
        report.l2.net.expectancy_r,
        report.l2.net.profit_factor,
    );
    Ok(())
}
