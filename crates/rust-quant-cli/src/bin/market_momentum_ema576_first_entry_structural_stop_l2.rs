use anyhow::{bail, Context, Result};
use rust_quant_cli::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::first_breakout_pullback_hold_v1::exclusive_active_retests_v2::buffered_hold_v3::dual_active_v4::failed_retest_termination_v6::persistent_qualification_v7::close_invalidated_episode_v8::armed_close_post_cross_recross_v9::pre_cross_breakout_v10::signal_cross_timeout_v11::first_entry_only_v12::structural_stop_v13::run_v13_l2_replay;
use std::path::PathBuf;

/// V13 只接受冻结 V11 L1 与输出路径，不允许命令行修改 0.30 ATR 缓冲。
fn paths_from_args() -> Result<(PathBuf, PathBuf)> {
    let mut l1_source = None;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--l1-source" => {
                l1_source = Some(PathBuf::from(
                    args.next().context("--l1-source requires a file path")?,
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
        l1_source.context("--l1-source is required")?,
        output.context("--output is required")?,
    ))
}

/// 执行 V13 Research-only EMA144 结构止损诊断，不写数据库或注册运行态策略。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let (l1_source, output) = paths_from_args()?;
    let report = run_v13_l2_replay(&l1_source, &output).await?;
    println!("{}", output.display());
    eprintln!(
        "L2 status={} completed={} events={} net_ev={:.6} net_pf={:?}",
        report.decision.status,
        report.coverage.completed_trades,
        report.coverage.completed_effective_market_events,
        report.net.expectancy_r,
        report.net.profit_factor,
    );
    Ok(())
}
