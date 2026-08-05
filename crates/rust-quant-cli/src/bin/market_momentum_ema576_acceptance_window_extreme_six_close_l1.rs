use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::first_breakout_pullback_hold_v1::exclusive_active_retests_v2::buffered_hold_v3::dual_active_v4::failed_retest_termination_v6::persistent_qualification_v7::close_invalidated_episode_v8::armed_close_post_cross_recross_v9::pre_cross_breakout_v10::signal_cross_timeout_v11::first_entry_only_v12::structural_stop_v13::confirmation_close_distance_v14::net_target_v15::entry_stop_cost_gate_v16::composite_acceptance_window_extreme_2_0atr_six_close_ema576_hold_relation_reset_v25::run_v25_l1;
use std::path::PathBuf;

/// 解析冻结 V14/V16 源报告与 V25 唯一输出路径。
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

/// 启动 V25 L1；该入口不读取 outcome，也不注册运行态路径。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (v14_source, v16_source, output) = paths_from_args()?;
    run_v25_l1(&v14_source, &v16_source, &output).await?;
    println!("{}", output.display());
    Ok(())
}
