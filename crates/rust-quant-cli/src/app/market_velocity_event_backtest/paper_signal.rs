use super::{
    ConfirmedEvent, MarketVelocityEventBacktestArgs, MarketVelocityPaperStrategySignalSink,
    MarketVelocityTradeDirection,
};
use crate::app::env_parse::first_non_empty_env;
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use rust_quant_domain::entities::{MarketRankEvent, MarketRankEventType};
use rust_quant_services::market::{
    build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry,
    MarketVelocityFvgEntryMode as ServiceFvgEntryMode, MarketVelocitySelectedEntry,
    MarketVelocitySignalTradeDirection, MarketVelocityStrategySignalConfig,
    MarketVelocityStrategySignalDecision,
};
use rust_quant_services::rust_quan_web::{
    ExecutionTaskClient, ExecutionTaskConfig, StrategySignalSubmitRequest,
};
use std::{
    io::{self, Write},
    time::Duration,
};
use tokio::time::timeout;

const BREAKDOWN_SHORT_STRATEGY_SLUG: &str = "market_velocity_breakdown_short";
const DEFAULT_MARKET_VELOCITY_STRATEGY_SLUG: &str = "market_velocity";
const PAPER_STRATEGY_SIGNAL_SUBMIT_TIMEOUT: Duration = Duration::from_secs(10);
const PAPER_STRATEGY_SIGNAL_FAILURE_DETAIL_LIMIT: usize = 1_000;

/// 将 paper observation 已确认入场转换为 Web 策略信号请求，保持观察信号和执行任务解耦。
pub fn build_market_velocity_paper_strategy_signal_request(
    confirmed: &ConfirmedEvent,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<StrategySignalSubmitRequest> {
    let event = market_rank_event_from_confirmed_event(confirmed)?;
    let config = paper_strategy_signal_config_from_args(args)?;
    let selected_entry = selected_entry_from_confirmed_event(confirmed)?;
    match build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry(
        &event,
        &config,
        None,
        Some(&selected_entry),
    )? {
        MarketVelocityStrategySignalDecision::Submit(request) => Ok(request),
        MarketVelocityStrategySignalDecision::Blocked(blocker) => {
            bail!("paper strategy signal blocked: {blocker:?}")
        }
    }
}

/// 按显式 sink 提交 paper observation 策略信号；Web 返回执行任务时立即失败。
pub async fn submit_market_velocity_paper_strategy_signals(
    confirmed: &[ConfirmedEvent],
    args: &MarketVelocityEventBacktestArgs,
) -> Result<()> {
    if args.paper_strategy_signal_sink != MarketVelocityPaperStrategySignalSink::Web {
        return Ok(());
    }
    if confirmed.is_empty() {
        print_paper_strategy_signal_submission_summary(0, 0)?;
        return Ok(());
    }
    let client = ExecutionTaskClient::new(paper_strategy_signal_execution_task_config()?)?;
    let mut submitted = 0usize;
    let mut failures = Vec::new();
    for event in confirmed {
        let Some(request) =
            build_paper_strategy_signal_request_for_submission(event, args, &mut failures)
        else {
            continue;
        };
        match timeout(
            PAPER_STRATEGY_SIGNAL_SUBMIT_TIMEOUT,
            client.submit_strategy_signal(request),
        )
        .await
        {
            Ok(Ok(response)) => {
                if !response.generated_tasks.is_empty() {
                    bail!(
                        "paper strategy signal generated {} execution tasks; expected signal-only",
                        response.generated_tasks.len()
                    );
                }
                submitted += 1;
            }
            Ok(Err(error)) => {
                failures.push(format!("event_id={}: {error}", event.event.id));
            }
            Err(_) => {
                failures.push(format!(
                    "event_id={}: timed out after {}s",
                    event.event.id,
                    PAPER_STRATEGY_SIGNAL_SUBMIT_TIMEOUT.as_secs()
                ));
            }
        }
    }
    print_paper_strategy_signal_submission_summary(submitted, failures.len())?;
    if !failures.is_empty() {
        let detail = paper_strategy_signal_failure_detail(&failures);
        print_paper_strategy_signal_submission_failures(&detail)?;
        bail!("paper strategy signal submissions failed: {detail}");
    }
    Ok(())
}

/// 构建单条提交请求；构建失败写入 failures，避免单条异常吞掉整轮生产摘要。
fn build_paper_strategy_signal_request_for_submission(
    event: &ConfirmedEvent,
    args: &MarketVelocityEventBacktestArgs,
    failures: &mut Vec<String>,
) -> Option<StrategySignalSubmitRequest> {
    match build_market_velocity_paper_strategy_signal_request(event, args) {
        Ok(request) => Some(request),
        Err(error) => {
            failures.push(format!(
                "event_id={}: build request failed: {error:#}",
                event.event.id
            ));
            None
        }
    }
}

/// 输出并立即刷新 paper 策略信号提交摘要，确保生产容器日志能稳定暴露本轮结果。
fn print_paper_strategy_signal_submission_summary(
    submitted: usize,
    failed: usize,
) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    write_paper_strategy_signal_submission_summary(&mut stdout, submitted, failed)
}

/// 写入 paper 策略信号提交摘要并 flush，便于测试覆盖容器 stdout 缓冲边界。
fn write_paper_strategy_signal_submission_summary<W: Write>(
    writer: &mut W,
    submitted: usize,
    failed: usize,
) -> io::Result<()> {
    writeln!(
        writer,
        "paper_strategy_signals_submitted={submitted}\tpaper_strategy_signals_failed={failed}"
    )?;
    writer.flush()
}

/// 生成截断后的失败明细，避免生产日志过长但保留排障关键字段。
fn paper_strategy_signal_failure_detail(failures: &[String]) -> String {
    failures
        .join("; ")
        .chars()
        .take(PAPER_STRATEGY_SIGNAL_FAILURE_DETAIL_LIMIT)
        .collect()
}

/// 输出并刷新失败明细，补足 tracing 未开启时的生产排障证据。
fn print_paper_strategy_signal_submission_failures(detail: &str) -> io::Result<()> {
    let mut stderr = io::stderr().lock();
    write_paper_strategy_signal_submission_failures(&mut stderr, detail)
}

/// 写入 paper 策略信号失败明细并 flush，确保容器 stderr 可直接看到根因。
fn write_paper_strategy_signal_submission_failures<W: Write>(
    writer: &mut W,
    detail: &str,
) -> io::Result<()> {
    writeln!(writer, "paper_strategy_signal_failures={detail}")?;
    writer.flush()
}

fn paper_strategy_signal_config_from_args(
    args: &MarketVelocityEventBacktestArgs,
) -> Result<MarketVelocityStrategySignalConfig> {
    let target_r = args
        .target_rs
        .first()
        .copied()
        .ok_or_else(|| anyhow!("paper strategy signal requires at least one target R"))?;
    let trade_direction = match args.trade_direction {
        MarketVelocityTradeDirection::Long => MarketVelocitySignalTradeDirection::Long,
        MarketVelocityTradeDirection::Short => MarketVelocitySignalTradeDirection::Short,
        MarketVelocityTradeDirection::Both => {
            bail!("paper strategy signal requires a single trade direction")
        }
    };
    Ok(MarketVelocityStrategySignalConfig {
        strategy_slug: strategy_slug_for_direction(args.trade_direction).to_string(),
        strategy_preset: paper_strategy_preset(args).to_string(),
        entry_rule_version: args.paper_outcome_entry_rule_version.clone(),
        trade_direction,
        min_delta_rank: args.min_delta_rank,
        max_delta_rank: args.max_delta_rank,
        min_price_change_pct: args.min_price_change_pct,
        max_price_change_pct: args.max_price_change_pct,
        stop_loss_pct: args.stop_loss_pct,
        stop_loss_mode: args.stop_loss_mode,
        structure_stop_min_pct: args.structure_stop_min_pct,
        take_profit_r: target_r,
        runner_target_r: args.runner_target_r,
        runner_fraction: args.runner_fraction,
        runner_stop_r: args.runner_stop_r,
        max_holding_hours: 24,
        automation_mode: "signal_only".to_string(),
        live_order_allowed: false,
        paper_trade_required: true,
        require_technical_confirmation: false,
        require_entry_confirmation: false,
        trend_min_average_distance_pct: args.trend_min_average_distance_pct,
        entry_confirmation_period: args.entry_period,
        entry_confirmation_fetch_limit: 80,
        entry_max_average_distance_pct: args.entry_max_distance_pct,
        entry_min_volume_ratio: args.entry_min_volume_ratio,
        entry_min_rsi: args.entry_min_rsi,
        entry_max_rsi: args.entry_max_rsi,
        entry_min_rsi_delta: args.entry_min_rsi_delta,
        entry_rsi_delta_lookback_candles: args.entry_rsi_delta_lookback_candles,
        entry_bollinger_breakout: args.entry_bollinger_breakout,
        entry_min_bollinger_bandwidth_expansion_pct: args
            .entry_min_bollinger_bandwidth_expansion_pct,
        entry_min_recent_drawdown_pct: args.entry_min_recent_drawdown_pct,
        entry_recent_drawdown_lookback_candles: args.entry_recent_drawdown_lookback_candles,
        entry_max_signal_pullback_pct: args.entry_max_signal_pullback_pct,
        entry_retest_tolerance_pct: args.entry_retest_tolerance_pct,
        entry_retest_after_signal: args.entry_retest_after_signal,
        entry_retest_max_wait_candles: args.entry_retest_max_wait_candles,
        entry_retest_min_entry_open_gap_pct: args.entry_retest_min_entry_open_gap_pct,
        entry_retest_open_fade_min_volume_ratio: args.entry_retest_open_fade_min_volume_ratio,
        fvg_entry_mode: service_fvg_entry_mode(args),
        fvg_lookback_candles: args.fvg_lookback_candles,
        fvg_max_wait_candles: args.fvg_max_wait_candles,
        fvg_impulse_retrace_fill_pct: args.fvg_impulse_retrace_fill_pct,
        fvg_impulse_retrace_min_wait_candles: args.fvg_impulse_retrace_min_wait_candles,
        entry_trigger_allowlist: args.entry_trigger_allowlist.clone(),
        entry_trigger_blocklist: args.entry_trigger_blocklist.clone(),
        symbol_blocklist: args.symbol_blocklist.clone(),
    })
}

fn market_rank_event_from_confirmed_event(confirmed: &ConfirmedEvent) -> Result<MarketRankEvent> {
    let detected_at = parse_detected_at(&confirmed.event.detected_at)?;
    Ok(MarketRankEvent {
        id: Some(confirmed.event.id),
        exchange: confirmed.event.exchange.clone(),
        symbol: confirmed.event.symbol.clone(),
        event_type: MarketRankEventType::RankVelocity,
        timeframe: Some("15m".to_string()),
        old_rank: None,
        new_rank: Some(confirmed.event.new_rank),
        delta_rank: Some(confirmed.event.delta_rank),
        volume_24h_quote: None,
        current_price: decimal_from_f64(confirmed.event.current_price, "current_price")?,
        previous_price: None,
        price_change_pct: decimal_from_f64(confirmed.event.price_change_pct, "price_change_pct")?,
        price_direction: price_direction_for_confirmed_event(confirmed).to_string(),
        technical_snapshot_status: "paper_observation".to_string(),
        technical_snapshot: None,
        detected_at,
        source: "market_velocity_paper_observation".to_string(),
        notification_state: "pending".to_string(),
    })
}

fn selected_entry_from_confirmed_event(
    confirmed: &ConfirmedEvent,
) -> Result<MarketVelocitySelectedEntry> {
    let entry_ts = Utc
        .timestamp_millis_opt(confirmed.entry_ts)
        .single()
        .ok_or_else(|| anyhow!("invalid paper strategy signal entry_ts"))?;
    Ok(MarketVelocitySelectedEntry {
        entry_price: confirmed.entry_price,
        entry_ts,
        trigger: confirmed.trigger.clone(),
        entry_path: "paper_observation".to_string(),
        signal_pullback_pct: None,
        structure_stop_loss_price: confirmed.structure_stop_loss_price,
        structure_stop_loss_source: confirmed.structure_stop_loss_source.clone(),
    })
}

fn parse_detected_at(value: &str) -> Result<DateTime<Utc>> {
    let trimmed = value.trim();
    let parsed = match DateTime::parse_from_rfc3339(trimmed) {
        Ok(value) => value,
        Err(_) => {
            let normalized = normalize_postgres_timestamptz(trimmed)
                .ok_or_else(|| anyhow!("unsupported timestamp shape"))?;
            DateTime::parse_from_rfc3339(&normalized)?
        }
    };
    Ok(parsed.with_timezone(&Utc))
}

fn normalize_postgres_timestamptz(value: &str) -> Option<String> {
    if !value.contains(' ') {
        return None;
    }
    let mut normalized = value.replacen(' ', "T", 1);
    let time_start = normalized.find('T')? + 1;
    let offset_start = normalized[time_start..].rfind(|ch| ch == '+' || ch == '-')? + time_start;
    let offset = &normalized[offset_start..];
    if offset.len() == 3 && offset[1..].chars().all(|ch| ch.is_ascii_digit()) {
        normalized.push_str(":00");
    } else if offset.len() == 5 && offset[1..].chars().all(|ch| ch.is_ascii_digit()) {
        normalized.insert(offset_start + 3, ':');
    }
    Some(normalized)
}

fn decimal_from_f64(value: f64, label: &str) -> Result<Option<Decimal>> {
    if !value.is_finite() {
        bail!("paper strategy signal {label} must be finite");
    }
    Decimal::from_f64(value)
        .map(Some)
        .ok_or_else(|| anyhow!("paper strategy signal {label} cannot be represented as Decimal"))
}

fn price_direction_for_confirmed_event(confirmed: &ConfirmedEvent) -> &'static str {
    if confirmed.event.price_change_pct < 0.0 {
        "down"
    } else {
        "up"
    }
}

fn strategy_slug_for_direction(direction: MarketVelocityTradeDirection) -> &'static str {
    match direction {
        MarketVelocityTradeDirection::Short => BREAKDOWN_SHORT_STRATEGY_SLUG,
        MarketVelocityTradeDirection::Long | MarketVelocityTradeDirection::Both => {
            DEFAULT_MARKET_VELOCITY_STRATEGY_SLUG
        }
    }
}

fn paper_strategy_preset(args: &MarketVelocityEventBacktestArgs) -> &str {
    let preset = args.paper_strategy_preset.trim();
    if preset.is_empty() {
        args.paper_outcome_entry_rule_version.trim()
    } else {
        preset
    }
}

fn service_fvg_entry_mode(args: &MarketVelocityEventBacktestArgs) -> ServiceFvgEntryMode {
    match args.fvg_entry_mode {
        super::FvgEntryMode::M15ImpulseRetrace => ServiceFvgEntryMode::M15ImpulseRetrace,
        _ => ServiceFvgEntryMode::Off,
    }
}

fn paper_strategy_signal_execution_task_config() -> Result<ExecutionTaskConfig> {
    let base_url = first_non_empty_env(&["RUST_QUAN_WEB_BASE_URL", "QUANT_WEB_BASE_URL"])
        .context("paper strategy signal sink requires RUST_QUAN_WEB_BASE_URL/QUANT_WEB_BASE_URL")?;
    let internal_secret =
        first_non_empty_env(&["EXECUTION_EVENT_SECRET", "RUST_QUAN_WEB_INTERNAL_SECRET"])
            .unwrap_or_default();
    Ok(ExecutionTaskConfig {
        base_url,
        internal_secret,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{RadarEvent, MS_15M};
    use std::io::{self, Write};

    struct FlushTrackingWriter {
        bytes: Vec<u8>,
        flush_count: usize,
    }

    impl FlushTrackingWriter {
        fn new() -> Self {
            Self {
                bytes: Vec::new(),
                flush_count: 0,
            }
        }
    }

    impl Write for FlushTrackingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flush_count += 1;
            Ok(())
        }
    }

    #[test]
    fn submission_summary_writer_flushes_after_line() {
        let mut writer = FlushTrackingWriter::new();

        write_paper_strategy_signal_submission_summary(&mut writer, 3, 2).unwrap();

        assert_eq!(
            String::from_utf8(writer.bytes).unwrap(),
            "paper_strategy_signals_submitted=3\tpaper_strategy_signals_failed=2\n"
        );
        assert_eq!(writer.flush_count, 1);
    }

    #[test]
    fn submission_failure_writer_flushes_detail_line() {
        let mut writer = FlushTrackingWriter::new();

        write_paper_strategy_signal_submission_failures(&mut writer, "event_id=42: failed")
            .unwrap();

        assert_eq!(
            String::from_utf8(writer.bytes).unwrap(),
            "paper_strategy_signal_failures=event_id=42: failed\n"
        );
        assert_eq!(writer.flush_count, 1);
    }

    #[test]
    fn submission_request_builder_records_build_error() {
        let args = MarketVelocityEventBacktestArgs {
            trade_direction: MarketVelocityTradeDirection::Short,
            target_rs: vec![0.65],
            paper_outcome_entry_rule_version: "paper_short_v1".to_string(),
            entry_trigger_allowlist: vec!["breakdown_range_low".to_string()],
            ..MarketVelocityEventBacktestArgs::default()
        };
        let confirmed = ConfirmedEvent {
            event: RadarEvent {
                id: 42,
                exchange: "okx".to_string(),
                symbol: "ETH-USDT-SWAP".to_string(),
                ts: MS_15M,
                detected_at: "not-a-timestamp".to_string(),
                new_rank: 8,
                delta_rank: 12,
                current_price: 100.0,
                price_change_pct: -1.2,
            },
            direction: MarketVelocityTradeDirection::Short,
            entry_ts: MS_15M * 2,
            entry_price: 99.0,
            entry_idx: 2,
            trigger: "breakdown_range_low".to_string(),
            structure_stop_loss_price: Some(102.0),
            structure_stop_loss_source: Some("range_high".to_string()),
        };
        let mut failures = Vec::new();

        let request =
            build_paper_strategy_signal_request_for_submission(&confirmed, &args, &mut failures);

        assert!(request.is_none());
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("event_id=42"));
        assert!(failures[0].contains("build request failed"));
    }
}
