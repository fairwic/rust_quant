use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use rust_quant_cli::app::tradingview_velocity_parity::{
    frozen_tick_size, load_okx_spot_candles, parse_timestamp, replay, verify_frozen_pine_source,
    verify_v10_pine_source, verify_v11_pine_source, verify_v12_pine_source, verify_v13_pine_source,
    verify_v14_pine_source, verify_v15_pine_source, verify_v16_pine_source, verify_v17_pine_source,
    verify_v18_pine_source, verify_v19_pine_source, verify_v20_pine_source, verify_v2_pine_source,
    verify_v3_pine_source, verify_v4_pine_source, verify_v5_pine_source, verify_v6_pine_source,
    verify_v7_pine_source, verify_v8_pine_source, verify_v9_pine_source, ParityRuleVersion,
    ReplayConfig, ReplayReport,
};

const DAY_MS: i64 = 86_400_000;
const DEFAULT_END: &str = "2026-07-26T20:45:00+08:00";
const DEFAULT_WINDOWS: [i64; 3] = [30, 60, 90];
// EMA596/696 的递归种子受图表更早历史影响；60 天预热可避免把样本起点附近
// 的暂态误当成 TradingView 信号，同时仍保持公共接口回放规模可控。
const WARMUP_DAYS: i64 = 60;
const COST_FEE_BPS_PER_SIDE: f64 = 5.0;
const COST_SLIPPAGE_BPS_PER_SIDE: f64 = 3.0;

/// BTC/ETH 同源对照 CLI 参数；结束时间使用不包含上界的 Unix 毫秒语义。
#[derive(Debug)]
struct Args {
    end_ms_exclusive: i64,
    windows_days: Vec<i64>,
    proxy_url: Option<String>,
    json: bool,
    rule_version: ParityRuleVersion,
}

/// 运行 BTC/ETH 现货同源、同规则的多窗口 Research 对照。
#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    match args.rule_version {
        ParityRuleVersion::Frozen66d3937e => verify_frozen_pine_source()?,
        ParityRuleVersion::Current3cbbc9d8 => verify_v2_pine_source()?,
        ParityRuleVersion::CandidateV3 => verify_v3_pine_source()?,
        ParityRuleVersion::CandidateV4 => verify_v4_pine_source()?,
        ParityRuleVersion::CandidateV5 => verify_v5_pine_source()?,
        ParityRuleVersion::CandidateV6 => verify_v6_pine_source()?,
        ParityRuleVersion::CandidateV7 => verify_v7_pine_source()?,
        ParityRuleVersion::CandidateV8 => verify_v8_pine_source()?,
        ParityRuleVersion::CandidateV9 => verify_v9_pine_source()?,
        ParityRuleVersion::CandidateV10 => verify_v10_pine_source()?,
        ParityRuleVersion::CandidateV11 => verify_v11_pine_source()?,
        ParityRuleVersion::CandidateV12 => verify_v12_pine_source()?,
        ParityRuleVersion::CandidateV13 => verify_v13_pine_source()?,
        ParityRuleVersion::CandidateV14 => verify_v14_pine_source()?,
        ParityRuleVersion::CandidateV15 => verify_v15_pine_source()?,
        ParityRuleVersion::CandidateV16 => verify_v16_pine_source()?,
        ParityRuleVersion::CandidateV17 => verify_v17_pine_source()?,
        ParityRuleVersion::CandidateV18 => verify_v18_pine_source()?,
        ParityRuleVersion::CandidateV19 => verify_v19_pine_source()?,
        ParityRuleVersion::CandidateV20 => verify_v20_pine_source()?,
    }
    let maximum_window = args
        .windows_days
        .iter()
        .copied()
        .max()
        .context("at least one evaluation window is required")?;
    let fetch_start = args.end_ms_exclusive - (maximum_window + WARMUP_DAYS) * DAY_MS;
    let fetch_end = args.end_ms_exclusive - 1;
    let mut reports = Vec::new();

    for symbol in ["BTC-USDT", "ETH-USDT"] {
        let candles =
            load_okx_spot_candles(symbol, fetch_start, fetch_end, args.proxy_url.as_deref())
                .await?;
        let tick_size = frozen_tick_size(symbol)?;
        for window_days in &args.windows_days {
            let evaluation_start = args.end_ms_exclusive - window_days * DAY_MS;
            let baseline = replay(
                &candles,
                replay_config(
                    args.rule_version,
                    symbol,
                    tick_size,
                    evaluation_start,
                    fetch_end,
                ),
            );
            let cost_adjusted = replay(
                &candles,
                ReplayConfig {
                    fee_bps_per_side: COST_FEE_BPS_PER_SIDE,
                    slippage_bps_per_side: COST_SLIPPAGE_BPS_PER_SIDE,
                    ..replay_config(
                        args.rule_version,
                        symbol,
                        tick_size,
                        evaluation_start,
                        fetch_end,
                    )
                },
            );
            reports.push((*window_days, baseline, cost_adjusted));
        }
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&reports)?);
    } else {
        print_report(&reports);
    }
    Ok(())
}

/// 解析冻结研究入口参数；V2～V9 必须显式选择，避免历史 V1 结果静默换规则。
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut end_ms_exclusive = parse_timestamp(DEFAULT_END)?;
    let mut windows_days = DEFAULT_WINDOWS.to_vec();
    let mut proxy_url = None;
    let mut json = false;
    // 保留既有 CLI 无参数行为；V2～V9 必须显式选择，避免旧回放静默换规则。
    let mut rule_version = ParityRuleVersion::Frozen66d3937e;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--end" => {
                let value = args.next().context("--end requires RFC3339 value")?;
                end_ms_exclusive = parse_timestamp(&value)?;
            }
            "--windows" => {
                let value = args.next().context("--windows requires comma list")?;
                windows_days = value
                    .split(',')
                    .map(|part| {
                        part.trim()
                            .parse::<i64>()
                            .with_context(|| format!("invalid window days: {part}"))
                    })
                    .collect::<Result<Vec<_>>>()?;
                if windows_days.is_empty() || windows_days.iter().any(|days| *days <= 0) {
                    bail!("--windows values must all be positive");
                }
                windows_days.sort_unstable();
                windows_days.dedup();
            }
            "--proxy-url" => {
                proxy_url = Some(args.next().context("--proxy-url requires URL")?);
            }
            "--json" => json = true,
            "--rule-version" => {
                let value = args.next().context(
                    "--rule-version requires frozen-v1, current-v2, candidate-v3 through candidate-v20",
                )?;
                rule_version = match value.as_str() {
                    "frozen-v1" => ParityRuleVersion::Frozen66d3937e,
                    "current-v2" => ParityRuleVersion::Current3cbbc9d8,
                    "candidate-v3" => ParityRuleVersion::CandidateV3,
                    "candidate-v4" => ParityRuleVersion::CandidateV4,
                    "candidate-v5" => ParityRuleVersion::CandidateV5,
                    "candidate-v6" => ParityRuleVersion::CandidateV6,
                    "candidate-v7" => ParityRuleVersion::CandidateV7,
                    "candidate-v8" => ParityRuleVersion::CandidateV8,
                    "candidate-v9" => ParityRuleVersion::CandidateV9,
                    "candidate-v10" => ParityRuleVersion::CandidateV10,
                    "candidate-v11" => ParityRuleVersion::CandidateV11,
                    "candidate-v12" => ParityRuleVersion::CandidateV12,
                    "candidate-v13" => ParityRuleVersion::CandidateV13,
                    "candidate-v14" => ParityRuleVersion::CandidateV14,
                    "candidate-v15" => ParityRuleVersion::CandidateV15,
                    "candidate-v16" => ParityRuleVersion::CandidateV16,
                    "candidate-v17" => ParityRuleVersion::CandidateV17,
                    "candidate-v18" => ParityRuleVersion::CandidateV18,
                    "candidate-v19" => ParityRuleVersion::CandidateV19,
                    "candidate-v20" => ParityRuleVersion::CandidateV20,
                    other => bail!("unsupported --rule-version: {other}"),
                };
            }
            "--help" | "-h" => {
                println!(
                    "Usage: tradingview_velocity_parity [--rule-version frozen-v1|current-v2|candidate-v3..candidate-v20] [--end RFC3339] [--windows 30,60,90] [--proxy-url URL] [--json]"
                );
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    Ok(Args {
        end_ms_exclusive,
        windows_days,
        proxy_url,
        json,
        rule_version,
    })
}

/// 依据 Pine 源码身份选择对应回放配置，不允许在同一版本内混合规则。
fn replay_config(
    version: ParityRuleVersion,
    symbol: &str,
    tick_size: f64,
    evaluation_start_ms: i64,
    evaluation_end_ms: i64,
) -> ReplayConfig {
    match version {
        ParityRuleVersion::Frozen66d3937e => ReplayConfig::tradingview_baseline(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::Current3cbbc9d8 => {
            ReplayConfig::current_pine_v2(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
        }
        ParityRuleVersion::CandidateV3 => {
            ReplayConfig::current_pine_v3(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
        }
        ParityRuleVersion::CandidateV4 => {
            ReplayConfig::current_pine_v4(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
        }
        ParityRuleVersion::CandidateV5 => {
            ReplayConfig::current_pine_v5(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
        }
        ParityRuleVersion::CandidateV6 => {
            ReplayConfig::current_pine_v6(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
        }
        ParityRuleVersion::CandidateV7 => {
            ReplayConfig::current_pine_v7(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
        }
        ParityRuleVersion::CandidateV8 => {
            ReplayConfig::current_pine_v8(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
        }
        ParityRuleVersion::CandidateV9 => {
            ReplayConfig::current_pine_v9(symbol, tick_size, evaluation_start_ms, evaluation_end_ms)
        }
        ParityRuleVersion::CandidateV10 => ReplayConfig::current_pine_v10(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV11 => ReplayConfig::current_pine_v11(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV12 => ReplayConfig::current_pine_v12(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV13 => ReplayConfig::current_pine_v13(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV14 => ReplayConfig::current_pine_v14(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV15 => ReplayConfig::current_pine_v15(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV16 => ReplayConfig::current_pine_v16(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV17 => ReplayConfig::current_pine_v17(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV18 => ReplayConfig::current_pine_v18(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV19 => ReplayConfig::current_pine_v19(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
        ParityRuleVersion::CandidateV20 => ReplayConfig::current_pine_v20(
            symbol,
            tick_size,
            evaluation_start_ms,
            evaluation_end_ms,
        ),
    }
}

/// 输出零成本与双边成本压力的并列结果，并保留零成本逐笔时间审计。
fn print_report(reports: &[(i64, ReplayReport, ReplayReport)]) {
    println!(
        "TradingView velocity parity | Pine FNV={} | end(exclusive)={}",
        reports
            .first()
            .map(|(_, report, _)| report.pine_source_fnv1a32)
            .unwrap_or("n/a"),
        reports
            .first()
            .map(|(_, report, _)| format_timestamp(report.evaluation_end_ms + 1))
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!("symbol window mode trades net_pnl PF win_rate avg_R max_DD blocked");
    for (window, baseline, cost_adjusted) in reports {
        print_summary(*window, "TV-0cost", baseline);
        print_summary(*window, "cost-5+3bp", cost_adjusted);
    }
    println!("\nClosed trades (zero-cost scenario):");
    for (window, baseline, _) in reports {
        println!("{} {}d", baseline.symbol, window);
        for trade in &baseline.trades {
            println!(
                "  signal={} entry={} {:?}@{:.4} exit={} @{:.4} {:?} pnl={:.4} R={:.3} families={:?}",
                format_timestamp(trade.signal_time_ms),
                format_timestamp(trade.entry_time_ms),
                trade.direction,
                trade.entry_price,
                format_timestamp(trade.exit_time_ms),
                trade.exit_price,
                trade.exit_reason,
                trade.net_pnl,
                trade.net_r,
                trade.families
            );
        }
    }
}

/// 打印单币单窗口 closed-trade 汇总；未结仓位不会强制计入收益。
fn print_summary(window_days: i64, mode: &str, report: &ReplayReport) {
    let pf = report
        .metrics
        .profit_factor
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "n/a".to_string());
    println!(
        "{} {}d {} {} {:.4} {} {:.2}% {:.3} {:.4} {}",
        report.symbol,
        window_days,
        mode,
        report.metrics.trades,
        report.metrics.net_pnl,
        pf,
        report.metrics.win_rate_percent,
        report.metrics.average_net_r,
        report.metrics.max_drawdown,
        report.blocked_signals.len()
    );
}

/// 把 Unix 毫秒转换成 UTC RFC3339；越界时保留原值便于诊断。
fn format_timestamp(timestamp_ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(timestamp_ms)
        .map(|timestamp| timestamp.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_else(|| timestamp_ms.to_string())
}
