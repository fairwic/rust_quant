use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::first_breakout_pullback_hold_v1::exclusive_active_retests_v2::buffered_hold_v3::dual_active_v4::failed_retest_termination_v6::persistent_qualification_v7::close_invalidated_episode_v8::armed_close_post_cross_recross_v9::pre_cross_breakout_v10::signal_cross_timeout_v11::first_entry_only_v12::structural_stop_v13::confirmation_close_distance_v14::net_target_v15::entry_stop_cost_gate_v16::run_v16_l1_l2_replay;
use std::path::PathBuf;

/// V16 只接受冻结 V14、V15 合并报告与输出路径，不开放研究参数。
fn paths_from_args() -> Result<(PathBuf, PathBuf, PathBuf)> {
    let mut v14_source = None;
    let mut v15_source = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--v14-source" => {
                v14_source = Some(PathBuf::from(
                    args.next().context("--v14-source requires a file path")?,
                ));
            }
            "--v15-source" => {
                v15_source = Some(PathBuf::from(
                    args.next().context("--v15-source requires a file path")?,
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
        v14_source.context("--v14-source is required")?,
        v15_source.context("--v15-source is required")?,
        output.context("--output is required")?,
    ))
}

/// 执行 V16 Research-only L1，并只在门禁通过时附带唯一一次 L2 回放。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (v14_source, v15_source, output) = paths_from_args()?;
    let report = run_v16_l1_l2_replay(&v14_source, &v15_source, &output).await?;
    println!("{}", output.display());
    if let Some(l2) = &report.l2 {
        eprintln!(
            "L1 status={} eligible={} rejected={} L2 status={} completed={} events={} net_ev={:.6} net_pf={:?}",
            report.l1.decision.status,
            report.l1.coverage.eligible_opportunity_count,
            report.l1.coverage.rejected_by_cost_gate_count,
            l2.decision.status,
            l2.coverage.completed_trades,
            l2.coverage.completed_effective_market_events,
            l2.net.expectancy_r,
            l2.net.profit_factor,
        );
    } else {
        eprintln!(
            "L1 status={} eligible={} rejected={} L2=not_run",
            report.l1.decision.status,
            report.l1.coverage.eligible_opportunity_count,
            report.l1.coverage.rejected_by_cost_gate_count,
        );
    }
    Ok(())
}
