use super::market_velocity_backfill::{
    configs_from_env_and_args, run_market_velocity_backfill, MarketVelocityBackfillCliArgs,
};
use super::market_velocity_kline_scanner::{
    config_from_env_and_args, run_market_velocity_kline_scanner, MarketVelocityKlineScannerCliArgs,
};
use anyhow::{anyhow, Result};
use std::time::Duration;
use tracing::{error, info};

const DEFAULT_KLINE_SCAN_INTERVAL_SECS: u64 = 60;
const DEFAULT_RECENT_REPAIR_INTERVAL_SECS: u64 = 300;
const MAX_ONLINE_REPAIR_DAYS: u64 = 2;
const MAX_ONLINE_REPAIR_SYMBOLS: usize = 200;

/// 收敛 Market 长期任务，但保留每条 lane 的固定周期与失败边界。
///
/// 历史大范围回补和 research 不进入本进程；这里只允许两天窗口的在线缺口修复。
pub async fn run_market_worker() -> Result<()> {
    let kline_scanner = run_kline_scanner_loop();
    let recent_repair = run_recent_repair_loop();
    let symbol_sync = super::bootstrap::run_exchange_symbol_sync_worker_from_env();
    let radar = super::bootstrap::run_market_velocity_radar_worker_from_env();

    tokio::pin!(kline_scanner, recent_repair, symbol_sync, radar);
    tokio::select! {
        result = &mut kline_scanner => critical_lane_result("kline-scanner", result),
        result = &mut symbol_sync => critical_lane_result("symbol-sync", result),
        result = &mut radar => critical_lane_result("market-radar", result),
        result = &mut recent_repair => critical_lane_result("recent-gap-repair", result),
        signal = shutdown_signal() => {
            info!(signal, "market-worker received shutdown signal");
            Ok(())
        }
    }
}

async fn run_kline_scanner_loop() -> Result<()> {
    let interval_secs = positive_env_u64(
        "MARKET_WORKER_KLINE_SCAN_INTERVAL_SECS",
        DEFAULT_KLINE_SCAN_INTERVAL_SECS,
    );
    let config = config_from_env_and_args(MarketVelocityKlineScannerCliArgs {
        dry_run: Some(false),
        ..MarketVelocityKlineScannerCliArgs::default()
    })?;
    loop {
        match run_market_velocity_kline_scanner(config.clone()).await {
            Ok(report) => info!(
                symbols_total = report.symbols_total,
                candidate_events = report.candidate_events,
                events_inserted = report.events_inserted,
                duplicate_events = report.duplicate_events,
                "market-worker kline scanner cycle completed"
            ),
            Err(error) => error!(%error, "market-worker kline scanner cycle failed"),
        }
        tokio::time::sleep(Duration::from_secs(interval_secs)).await;
    }
}

async fn run_recent_repair_loop() -> Result<()> {
    let interval_secs = positive_env_u64(
        "MARKET_WORKER_RECENT_REPAIR_INTERVAL_SECS",
        DEFAULT_RECENT_REPAIR_INTERVAL_SECS,
    );
    let max_symbols = positive_env_usize(
        "MARKET_WORKER_RECENT_REPAIR_MAX_SYMBOLS",
        MAX_ONLINE_REPAIR_SYMBOLS,
    )
    .min(MAX_ONLINE_REPAIR_SYMBOLS);
    let configs = configs_from_env_and_args(MarketVelocityBackfillCliArgs {
        days: Some(MAX_ONLINE_REPAIR_DAYS),
        timeframes: Some(vec!["1m".to_string(), "5m".to_string(), "15m".to_string()]),
        require_4h: Some(false),
        dry_run: Some(false),
        max_symbols: Some(Some(max_symbols)),
        continue_on_error: Some(true),
        ..MarketVelocityBackfillCliArgs::default()
    })?;
    loop {
        for config in &configs {
            match run_market_velocity_backfill(config.clone()).await {
                Ok(report) => info!(
                    timeframe = %config.timeframe,
                    symbols_attempted = report.symbols_attempted,
                    rows_upserted = report.rows_upserted,
                    failed_symbols = report.failed_symbols.len(),
                    "market-worker recent candle repair completed"
                ),
                Err(error) => error!(
                    timeframe = %config.timeframe,
                    %error,
                    "market-worker recent candle repair failed"
                ),
            }
        }
        tokio::time::sleep(Duration::from_secs(interval_secs)).await;
    }
}

fn positive_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn positive_env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn critical_lane_result(lane: &str, result: Result<()>) -> Result<()> {
    match result {
        Ok(()) => Err(anyhow!(
            "critical market-worker lane exited unexpectedly: {lane}"
        )),
        Err(error) => Err(error.context(format!("critical market-worker lane failed: {lane}"))),
    }
}

async fn shutdown_signal() -> &'static str {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => "SIGINT",
            _ = terminate.recv() => "SIGTERM",
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl-C handler");
        "CTRL_C"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_repair_limits_are_bounded() {
        assert_eq!(MAX_ONLINE_REPAIR_DAYS, 2);
        assert!(MAX_ONLINE_REPAIR_SYMBOLS <= 200);
        assert!(critical_lane_result("radar", Ok(())).is_err());
    }
}
