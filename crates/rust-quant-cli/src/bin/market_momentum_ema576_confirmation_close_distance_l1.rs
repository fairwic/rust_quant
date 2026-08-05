use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::first_breakout_pullback_hold_v1::exclusive_active_retests_v2::buffered_hold_v3::dual_active_v4::failed_retest_termination_v6::persistent_qualification_v7::close_invalidated_episode_v8::armed_close_post_cross_recross_v9::pre_cross_breakout_v10::signal_cross_timeout_v11::first_entry_only_v12::structural_stop_v13::confirmation_close_distance_v14::run_v14_l1_scan;
use std::path::PathBuf;

/// V14 L1 只接受冻结 V11 账本与输出路径，不开放 outcome 或阈值参数。
fn paths_from_args() -> Result<(PathBuf, PathBuf)> {
    let mut v11_l1_source = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--v11-l1-source" => {
                v11_l1_source = Some(PathBuf::from(
                    args.next()
                        .context("--v11-l1-source requires a file path")?,
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
        v11_l1_source.context("--v11-l1-source is required")?,
        output.context("--output is required")?,
    ))
}

/// 执行 V14 Research-only 无 outcome 距离扫描，不连接数据库或运行成交回放。
fn main() -> Result<()> {
    let (v11_l1_source, output) = paths_from_args()?;
    let report = run_v14_l1_scan(&v11_l1_source, &output)?;
    println!("{}", output.display());
    eprintln!(
        "L1 status={} selected_cap_atr={:.2} candidates={} layer_target_rejected={}",
        report.decision.status,
        report.selected_cap_atr,
        report.candidates.len(),
        report.layer_audit.target_rejected,
    );
    Ok(())
}
