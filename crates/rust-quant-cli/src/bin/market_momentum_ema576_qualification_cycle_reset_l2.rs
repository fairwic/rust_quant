use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::first_breakout_pullback_hold_v1::exclusive_active_retests_v2::buffered_hold_v3::dual_active_v4::failed_retest_termination_v6::persistent_qualification_v7::close_invalidated_episode_v8::armed_close_post_cross_recross_v9::pre_cross_breakout_v10::signal_cross_timeout_v11::first_entry_only_v12::structural_stop_v13::confirmation_close_distance_v14::net_target_v15::entry_stop_cost_gate_v16::qualification_cycle_reset_v17::run_v17_l1_l2_replay;
use std::path::PathBuf;

/// V17 只接受冻结 V14、V16 报告与输出路径，不开放研究参数。
fn paths_from_args() -> Result<(PathBuf, PathBuf, PathBuf)> {
    let mut v14_source = None;
    let mut v16_source = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--v14-source" => {
                v14_source = Some(PathBuf::from(
                    args.next().context("--v14-source requires a file path")?,
                ));
            }
            "--v16-source" => {
                v16_source = Some(PathBuf::from(
                    args.next().context("--v16-source requires a file path")?,
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
        v16_source.context("--v16-source is required")?,
        output.context("--output is required")?,
    ))
}

/// 执行 V17 关系周期重置的 Research-only L1，并按门禁决定是否进入 L2。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (v14_source, v16_source, output) = paths_from_args()?;
    run_v17_l1_l2_replay(&v14_source, &v16_source, &output).await?;
    println!("{}", output.display());
    Ok(())
}
