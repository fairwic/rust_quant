use super::config::AllMarketCandleVolumeMonitorConfig;
use super::warmup::{confirmed_from_okx, load_active_okx_perpetual_symbols, stream_symbol_warmups};
use crate::app::market_velocity_backfill::{build_okx_http_client, fetch_okx_history_candles};
use anyhow::{anyhow, Context, Result};
use rust_quant_market::streams::confirmed_candle_aggregator::{
    AggregatedTimeframe, CandleAggregationUpdate, CandleGap, ConfirmedCandle,
    ConfirmedCandleAggregator,
};
use rust_quant_market::streams::confirmed_candle_stream::{
    run_all_market_confirmed_1m_stream, ConfirmedOneMinuteMessage,
};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

const REPAIR_RESULT_QUEUE_CAPACITY: usize = 64;
const MAX_PENDING_CANDLES_PER_SYMBOL: usize = 64;
const RULE_VERSION: &str = "closed_candle_volume_prev20_v1";

/// 带本机接收时钟的实时确认 K 线，供延迟统计与恢复排序使用。
#[derive(Clone)]
struct LiveCandle {
    /// OKX 交易对。
    symbol: String,
    /// 已确认且归一化的 1m K 线。
    candle: ConfirmedCandle,
    /// 本机单调接收时钟，用于计算进程内延迟。
    received_at: Instant,
    /// 本机接收时间，Unix 毫秒时间戳。
    received_at_ms: i64,
    /// 是否允许输出放量观测；REST 补洞数据必须为 `false`。
    emit_observations: bool,
}

/// 单个交易对的异步缺口修复结果。
struct GapRepairResult {
    /// 发生缺口的 OKX 交易对。
    symbol: String,
    /// 按开盘时间排序的补洞 K 线，错误时终止当前运行周期以避免静默错序。
    result: Result<Vec<ConfirmedCandle>>,
}

/// 最近一分钟的本机处理延迟样本窗口，单位为微秒。
#[derive(Default)]
struct LatencyWindow {
    /// 当前统计周期内的所有处理延迟，输出分位数后立即清空。
    samples_us: Vec<u64>,
}

impl LatencyWindow {
    /// 记录一根实时确认 K 线的进程内处理延迟，单位为微秒。
    fn record(&mut self, latency_us: u64) {
        self.samples_us.push(latency_us);
    }

    /// 以 nearest-rank 计算 P50/P95/P99/最大值，并清空旧窗口避免跨周期累积。
    fn take_summary(&mut self) -> Option<(usize, u64, u64, u64, u64)> {
        if self.samples_us.is_empty() {
            return None;
        }
        self.samples_us.sort_unstable();
        let count = self.samples_us.len();
        let percentile = |numerator: usize| {
            let index = (count.saturating_sub(1) * numerator) / 100;
            self.samples_us[index]
        };
        let summary = (
            count,
            percentile(50),
            percentile(95),
            percentile(99),
            *self.samples_us.last().unwrap_or(&0),
        );
        self.samples_us.clear();
        Some(summary)
    }
}

/// 启动全市场 1m 确认流，并在单任务内同步派生 5m、15m 与 4H 收盘观测。
pub async fn run_all_market_candle_volume_monitor(
    config: AllMarketCandleVolumeMonitorConfig,
) -> Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core for all-market candle monitor")?;
    let client = build_okx_http_client(config.proxy_url.as_deref())?;
    let symbols = load_active_okx_perpetual_symbols(
        &pool,
        &client,
        &config.okx_rest_base,
        config.max_symbols,
    )
    .await?;
    let (confirmed_sender, mut confirmed_receiver) = mpsc::channel(config.confirmed_queue_capacity);
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);

    // WebSocket 先启动，预热期间的确认收盘进入有界队列，避免启动查询制造新的分钟缺口。
    let stream_symbols = symbols.clone();
    let websocket_shard_size = config.websocket_shard_size;
    let mut stream_task = tokio::spawn(async move {
        run_all_market_confirmed_1m_stream(
            &stream_symbols,
            websocket_shard_size,
            confirmed_sender,
            shutdown_receiver,
        )
        .await
    });
    let (warmup_sender, mut warmup_receiver) = mpsc::channel(64);
    let warmup_symbols = symbols.clone();
    let warmup_config = config.clone();
    let mut warmup_task = tokio::spawn(async move {
        stream_symbol_warmups(warmup_symbols, warmup_config, warmup_sender).await
    });
    let (repair_sender, mut repair_receiver) =
        mpsc::channel::<GapRepairResult>(REPAIR_RESULT_QUEUE_CAPACITY);
    let mut repairing_symbols = HashSet::new();
    let mut pending_by_symbol = HashMap::<String, Vec<LiveCandle>>::new();
    let mut aggregator = ConfirmedCandleAggregator::default();
    let mut ready_symbols = HashSet::new();
    let mut isolated_symbols = HashSet::new();
    let mut warmup_remaining = symbols.len();
    let mut warmup_failed = 0_usize;
    let mut warmup_task_finished = false;
    let mut latency_window = LatencyWindow::default();
    let mut latency_report = tokio::time::interval(Duration::from_secs(60));
    latency_report.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    latency_report.tick().await;

    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal.context("listen for shutdown signal")?;
                let _ = shutdown_sender.send(true);
                warmup_task.abort();
                let stream_result = stream_task.await.context("join all-market WebSocket task")?;
                stream_result?;
                return Ok(());
            }
            stream_result = &mut stream_task => {
                return stream_result
                    .context("join all-market WebSocket task")?
                    .context("all-market WebSocket task exited");
            }
            warmup_result = &mut warmup_task, if !warmup_task_finished => {
                warmup_result
                    .context("join all-market candle warmup task")?
                    .context("all-market candle warmup task failed")?;
                warmup_task_finished = true;
            }
            warmup = warmup_receiver.recv(), if warmup_remaining > 0 => {
                let warmup = warmup.ok_or_else(|| anyhow!(
                    "all-market warmup channel closed with {warmup_remaining} symbols remaining"
                ))?;
                warmup_remaining -= 1;
                match warmup.result {
                    Ok(histories) => {
                        seed_symbol_history(&mut aggregator, &warmup.symbol, &histories)?;
                        ready_symbols.insert(warmup.symbol.clone());
                        let pending = pending_by_symbol.remove(&warmup.symbol).unwrap_or_default();
                        replay_recovered_symbol(
                            &mut aggregator,
                            &config,
                            &mut latency_window,
                            &warmup.symbol,
                            Vec::new(),
                            pending,
                        )?;
                    }
                    Err(error) => {
                        warmup_failed += 1;
                        isolated_symbols.insert(warmup.symbol.clone());
                        pending_by_symbol.remove(&warmup.symbol);
                        warn!(
                            event = "all_market_candle_warmup_failed",
                            symbol = warmup.symbol,
                            error = %error,
                            "交易对预热失败，当前连接周期内隔离该交易对"
                        );
                    }
                }
                if warmup_remaining == 0 {
                    anyhow::ensure!(
                        !ready_symbols.is_empty(),
                        "all OKX perpetual symbols failed candle warmup"
                    );
                    info!(
                        event = "all_market_candle_monitor_ready",
                        subscribed_symbols = symbols.len(),
                        ready_symbols = ready_symbols.len(),
                        failed_symbols = warmup_failed,
                        queue_capacity = config.confirmed_queue_capacity,
                        websocket_shard_size = config.websocket_shard_size,
                        minimum_volume_ratio = %config.minimum_volume_ratio,
                        "全市场收盘 K 线成交量监听器已就绪"
                    );
                }
            }
            message = confirmed_receiver.recv() => {
                let message = message.ok_or_else(|| anyhow!("all-market confirmed candle queue closed"))?;
                let live = live_candle_from_message(message)?;
                if isolated_symbols.contains(&live.symbol) {
                    continue;
                }
                if !ready_symbols.contains(&live.symbol) {
                    push_pending(&mut pending_by_symbol, live)?;
                    continue;
                }
                if repairing_symbols.contains(&live.symbol) {
                    push_pending(&mut pending_by_symbol, live)?;
                    continue;
                }
                let gap_candidate = live.clone();
                if let Some(gap) = process_candle(
                    &mut aggregator,
                    &config,
                    &mut latency_window,
                    live,
                )? {
                    let symbol = gap.symbol.clone();
                    repairing_symbols.insert(symbol.clone());
                    push_pending(&mut pending_by_symbol, gap_candidate)?;
                    spawn_gap_repair(
                        gap,
                        client.clone(),
                        config.okx_rest_base.clone(),
                        config.rest_request_sleep_ms,
                        repair_sender.clone(),
                    );
                }
            }
            repair = repair_receiver.recv() => {
                let repair = repair.ok_or_else(|| anyhow!("gap repair result queue closed"))?;
                repairing_symbols.remove(&repair.symbol);
                let recovered = repair.result
                    .with_context(|| format!("repair 1m candle gap for {}", repair.symbol))?;
                let pending = pending_by_symbol.remove(&repair.symbol).unwrap_or_default();
                replay_recovered_symbol(
                    &mut aggregator,
                    &config,
                    &mut latency_window,
                    &repair.symbol,
                    recovered,
                    pending,
                )?;
            }
            _ = latency_report.tick() => {
                if let Some((samples, p50_us, p95_us, p99_us, max_us)) = latency_window.take_summary() {
                    info!(
                        event = "all_market_candle_local_latency",
                        samples,
                        p50_us,
                        p95_us,
                        p99_us,
                        max_us,
                        queue_depth = confirmed_receiver.len(),
                        slo_us = 1_000_000_u64,
                        slo_met = p99_us < 1_000_000,
                        "全市场确认 K 线本机处理延迟"
                    );
                }
            }
        }
    }
}

/// 将 WebSocket 消息转换为聚合输入，并保留真实接收时钟。
fn live_candle_from_message(message: ConfirmedOneMinuteMessage) -> Result<LiveCandle> {
    Ok(LiveCandle {
        symbol: message.symbol,
        candle: ConfirmedCandle::try_from_okx(&message.candle)?,
        received_at: message.received_at,
        received_at_ms: message.received_at_ms,
        emit_observations: true,
    })
}

/// 为单个交易对预热四周期成交量窗口与当前高周期部分桶。
fn seed_symbol_history(
    aggregator: &mut ConfirmedCandleAggregator,
    symbol: &str,
    histories: &[(AggregatedTimeframe, Vec<ConfirmedCandle>)],
) -> Result<()> {
    for (timeframe, candles) in histories {
        aggregator.seed_volume_history(symbol, *timeframe, candles);
    }
    let one_minute = histories
        .iter()
        .find(|(timeframe, _)| *timeframe == AggregatedTimeframe::M1)
        .map(|(_, candles)| candles.as_slice())
        .unwrap_or_default();
    aggregator
        .seed_partial_one_minute_history(symbol, one_minute)
        .with_context(|| format!("seed partial one-minute history for {symbol}"))
}

/// 同步执行热路径聚合；发现缺口时不推进状态而是返回修复范围。
fn process_candle(
    aggregator: &mut ConfirmedCandleAggregator,
    config: &AllMarketCandleVolumeMonitorConfig,
    latency_window: &mut LatencyWindow,
    live: LiveCandle,
) -> Result<Option<CandleGap>> {
    let update = match aggregator.ingest_one_minute(&live.symbol, live.candle) {
        Ok(update) => update,
        Err(gap) => return Ok(Some(gap)),
    };
    let local_latency_us = live.received_at.elapsed().as_micros().min(u64::MAX as u128) as u64;
    latency_window.record(local_latency_us);
    if live.emit_observations {
        log_volume_expansions(update, config, live.received_at_ms, local_latency_us);
    }
    Ok(None)
}

/// 仅输出达到阈值的只读观测，并明确标记其不是交易信号。
fn log_volume_expansions(
    update: CandleAggregationUpdate,
    config: &AllMarketCandleVolumeMonitorConfig,
    received_at_ms: i64,
    local_latency_us: u64,
) {
    for observation in update
        .volume_observations
        .into_iter()
        .filter(|item| item.volume_ratio >= config.minimum_volume_ratio)
    {
        let expected_close_ms = observation
            .candle
            .open_time_ms
            .saturating_add(observation.timeframe.duration_ms());
        info!(
            event = "closed_candle_volume_expansion",
            rule_version = RULE_VERSION,
            symbol = observation.symbol,
            timeframe = observation.timeframe.as_str(),
            candle_open_time_ms = observation.candle.open_time_ms,
            volume_base = %observation.candle.volume_base,
            previous_20_average_volume = %observation.previous_average_volume,
            volume_ratio = %observation.volume_ratio,
            exchange_close_arrival_ms = received_at_ms.saturating_sub(expected_close_ms),
            local_processing_latency_us = local_latency_us,
            trading_signal = false,
            "已确认收盘 K 线出现放量"
        );
    }
}

/// 在预热或补洞期间暂存实时收盘，并设置每交易对硬上限防止无界内存增长。
fn push_pending(
    pending_by_symbol: &mut HashMap<String, Vec<LiveCandle>>,
    live: LiveCandle,
) -> Result<()> {
    let symbol = live.symbol.clone();
    let pending = pending_by_symbol.entry(symbol.clone()).or_default();
    if pending.len() >= MAX_PENDING_CANDLES_PER_SYMBOL {
        return Err(anyhow!(
            "pending 1m candle recovery exceeded {MAX_PENDING_CANDLES_PER_SYMBOL} rows for {symbol}"
        ));
    }
    pending.push(live);
    Ok(())
}

/// 在独立任务中通过限速 REST 修复单币缺口，其他交易对继续走实时热路径。
fn spawn_gap_repair(
    gap: CandleGap,
    client: reqwest::Client,
    okx_rest_base: String,
    request_sleep_ms: u64,
    sender: mpsc::Sender<GapRepairResult>,
) {
    tokio::spawn(async move {
        let result = async {
            let rows = fetch_okx_history_candles(
                &client,
                &okx_rest_base,
                &gap.symbol,
                "1m",
                gap.expected_open_time_ms,
                gap.actual_open_time_ms,
                100,
                request_sleep_ms,
            )
            .await?;
            confirmed_from_okx(rows)
        }
        .await;
        let _ = sender
            .send(GapRepairResult {
                symbol: gap.symbol,
                result,
            })
            .await;
    });
}

/// 按开盘时间合并 REST 与实时数据；同时间戳优先实时消息以保留真实延迟。
fn replay_recovered_symbol(
    aggregator: &mut ConfirmedCandleAggregator,
    config: &AllMarketCandleVolumeMonitorConfig,
    latency_window: &mut LatencyWindow,
    symbol: &str,
    recovered: Vec<ConfirmedCandle>,
    pending: Vec<LiveCandle>,
) -> Result<()> {
    let recovered_count = recovered.len();
    let pending_count = pending.len();
    let mut ordered = BTreeMap::<i64, LiveCandle>::new();
    for candle in recovered {
        ordered.insert(
            candle.open_time_ms,
            LiveCandle {
                symbol: symbol.to_string(),
                candle,
                received_at: Instant::now(),
                received_at_ms: 0,
                emit_observations: false,
            },
        );
    }
    // 实时收到的消息覆盖 REST 同时间戳数据，从而保留真实接收延迟并只发一次观测。
    for live in pending {
        ordered.insert(live.candle.open_time_ms, live);
    }
    for live in ordered.into_values() {
        if let Some(gap) = process_candle(aggregator, config, latency_window, live)? {
            return Err(anyhow!(
                "REST repair for {symbol} remained incomplete: expected={}, actual={}",
                gap.expected_open_time_ms,
                gap.actual_open_time_ms
            ));
        }
    }
    if recovered_count > 0 {
        info!(
            event = "all_market_candle_gap_repaired",
            symbol, recovered_count, pending_count, "1m K 线缺口已补齐并恢复实时聚合"
        );
    } else if pending_count > 0 {
        info!(
            event = "all_market_candle_startup_replayed",
            symbol, pending_count, "交易对预热期间的实时收盘已回放"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_summary_uses_nearest_rank_without_retaining_old_window() {
        let mut window = LatencyWindow::default();
        for value in 1..=100 {
            window.record(value);
        }
        assert_eq!(window.take_summary(), Some((100, 50, 95, 99, 100)));
        assert_eq!(window.take_summary(), None);
    }
}
