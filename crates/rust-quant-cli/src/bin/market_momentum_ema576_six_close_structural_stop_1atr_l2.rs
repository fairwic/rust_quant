use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::first_breakout_pullback_hold_v1::exclusive_active_retests_v2::buffered_hold_v3::dual_active_v4::failed_retest_termination_v6::persistent_qualification_v7::close_invalidated_episode_v8::armed_close_post_cross_recross_v9::pre_cross_breakout_v10::signal_cross_timeout_v11::first_entry_only_v12::structural_stop_v13::confirmation_close_distance_v14::net_target_v15::entry_stop_cost_gate_v16::six_close_structural_stop_1atr_v26::run_v26_l2;
use std::path::PathBuf;

/// 解析 V25、V26、V14 三份冻结源和唯一配对 L2 输出路径。
fn paths_from_args() -> Result<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let mut v25_source = None;
    let mut v26_l1_source = None;
    let mut v14_source = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--v25-source" => {
                v25_source = Some(PathBuf::from(
                    args.next().context("--v25-source requires a file path")?,
                ));
            }
            "--v26-l1-source" => {
                v26_l1_source = Some(PathBuf::from(
                    args.next()
                        .context("--v26-l1-source requires a file path")?,
                ));
            }
            "--v14-source" => {
                v14_source = Some(PathBuf::from(
                    args.next().context("--v14-source requires a file path")?,
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
        v25_source.context("--v25-source is required")?,
        v26_l1_source.context("--v26-l1-source is required")?,
        v14_source.context("--v14-source is required")?,
        output.context("--output is required")?,
    ))
}

/// 启动冻结 V26 配对 L2；该入口只写本地研究报告。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (v25_source, v26_l1_source, v14_source, output) = paths_from_args()?;
    run_v26_l2(&v25_source, &v26_l1_source, &v14_source, &output).await?;
    println!("{}", output.display());
    Ok(())
}
