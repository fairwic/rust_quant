use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use rust_quant_domain::entities::{
    MarketAnomaly, MarketRankEvent, MarketRankEventType, MarketRankSnapshot,
    MarketRankTechnicalSnapshot, TickerSnapshot,
};
use rust_quant_domain::traits::fund_monitoring_repository::MarketAnomalyRepository;
use rust_quant_domain::Candle;
use rust_quant_market::scanners::okx_scanner::OkxScanner;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tracing::{error, info, warn};

use super::market_velocity_signal::dispatch_market_velocity_strategy_signal_if_enabled;
use super::CandleService;
use crate::notification::TelegramNotifier;

/// 排名快照
#[derive(Clone)]
struct RankSnapshot {
    timestamp: DateTime<Utc>,
    ranks: HashMap<String, i32>,
    prices: HashMap<String, Decimal>,
}

/// 扫描器服务
/// 负责定时扫描全市场Ticker，维护 Top 150 排名，并发送 Telegram 通知
pub struct ScannerService {
    scanner: OkxScanner,
    last_snapshots: HashMap<String, TickerSnapshot>,
    anomaly_repo: Arc<dyn MarketAnomalyRepository>,
    technical_candle_service: Option<Arc<CandleService>>,
    rank_history: VecDeque<RankSnapshot>,
    last_top_150: HashSet<String>,
    /// Telegram 通知器
    telegram: Option<TelegramNotifier>,
    /// 通知冷却期: (symbol, timeframe) -> 上次通知时间
    notification_cooldown: HashMap<String, DateTime<Utc>>,
    /// 是否为首次扫描 (跳过初始化时的 Entry 通知)
    is_first_scan: bool,
}

/// 排名剧变通知阈值
const RANK_CHANGE_THRESHOLD: i32 = 3;

/// 通知冷却期 (分钟)
/// 15分钟周期 -> 15分钟冷却
/// 4小时周期 -> 4小时冷却
/// 24小时周期 -> 24小时冷却
/// 榜单变动 -> 30分钟冷却
const COOLDOWN_MINUTES_15M: i64 = 15;
const COOLDOWN_MINUTES_4H: i64 = 120; // 2 * 60
const COOLDOWN_MINUTES_24H: i64 = 720; // 12 * 60
const COOLDOWN_MINUTES_LIST: i64 = 30;
const MARKET_RANK_TOP_BOUNDARY: i32 = 50;
const MARKET_RANK_HISTORY_RETENTION_HOURS: i64 = 25;
const MARKET_RANK_TECHNICAL_TIMEFRAME: &str = "4h";
const MARKET_RANK_TECHNICAL_PERIOD: usize = 20;
const MARKET_RANK_TECHNICAL_FETCH_LIMIT: u32 = 80;
const MARKET_RANK_TECHNICAL_TOUCH_THRESHOLD_PCT: f64 = 0.3;

impl ScannerService {
    pub fn new(anomaly_repo: Arc<dyn MarketAnomalyRepository>) -> Result<Self> {
        Self::new_with_technical_candle_service(anomaly_repo, None)
    }

    pub fn new_with_technical_candle_service(
        anomaly_repo: Arc<dyn MarketAnomalyRepository>,
        technical_candle_service: Option<Arc<CandleService>>,
    ) -> Result<Self> {
        let telegram = match TelegramNotifier::from_env() {
            Ok(notifier) => {
                info!("Telegram notifier initialized successfully");
                Some(notifier)
            }
            Err(e) => {
                warn!("Telegram notifier not configured: {:?}", e);
                None
            }
        };

        Ok(Self {
            scanner: OkxScanner::new()?,
            last_snapshots: HashMap::new(),
            anomaly_repo,
            technical_candle_service,
            rank_history: VecDeque::new(),
            last_top_150: HashSet::new(),
            telegram,
            notification_cooldown: HashMap::new(),
            is_first_scan: true,
        })
    }

    /// 初始化：从数据库恢复状态，清除过期的周期数据
    pub async fn initialize(&mut self) -> Result<()> {
        let now = Utc::now();
        let mut active_rank_fallback = VecDeque::new();

        // 获取数据库中最新的更新时间
        let latest_time = self.anomaly_repo.get_latest_update_time().await?;

        if let Some(last_update) = latest_time {
            let elapsed = now - last_update;
            let elapsed_mins = elapsed.num_minutes();

            info!(
                "Database last updated {} minutes ago at {}",
                elapsed_mins, last_update
            );

            // 根据时间差决定清除哪些周期的历史数据
            let clear_15m = elapsed_mins > 15;
            let clear_4h = elapsed_mins > 240; // 4 hours
            let clear_24h = elapsed_mins > 1440; // 24 hours

            if clear_15m || clear_4h || clear_24h {
                info!(
                    "Clearing stale period data: 15m={}, 4h={}, 24h={}",
                    clear_15m, clear_4h, clear_24h
                );
                self.anomaly_repo
                    .clear_stale_period_data(clear_15m, clear_4h, clear_24h)
                    .await?;
            }

            // 从数据库恢复 Top 150 列表
            let active_records = self.anomaly_repo.get_all_active().await?;
            for record in &active_records {
                self.last_top_150.insert(record.symbol.clone());
            }

            // 恢复最后的排名快照（用于后续的 delta 计算）
            if !active_records.is_empty() {
                let mut ranks = HashMap::new();
                for record in &active_records {
                    ranks.insert(record.symbol.clone(), record.current_rank);
                }
                active_rank_fallback.push_back(RankSnapshot {
                    timestamp: last_update,
                    ranks,
                    prices: HashMap::new(),
                });
                info!(
                    "Restored {} active records from database",
                    active_records.len()
                );
            }
        } else {
            info!("No existing data in database, starting fresh");
        }

        let snapshot_since = now - Duration::hours(MARKET_RANK_HISTORY_RETENTION_HOURS);
        match self
            .anomaly_repo
            .load_recent_rank_snapshots("okx", snapshot_since)
            .await
        {
            Ok(rows) if !rows.is_empty() => {
                self.rank_history = rank_history_from_persisted_snapshots(rows);
                info!(
                    "Restored {} market rank history snapshots with price evidence",
                    self.rank_history.len()
                );
            }
            Ok(_) => {
                self.rank_history = active_rank_fallback;
            }
            Err(err) => {
                warn!(
                    "Failed to restore market rank price snapshots, falling back to active ranks: {:?}",
                    err
                );
                self.rank_history = active_rank_fallback;
            }
        }

        Ok(())
    }

    pub async fn scan_and_analyze(&mut self) -> Result<Vec<(String, Decimal)>> {
        let mut current_snapshots = self.scanner.fetch_all_tickers().await?;
        let now = Utc::now();

        // 1. 按 Quote Volume 降序排名
        current_snapshots.sort_by(|a, b| {
            b.volume_24h_quote
                .partial_cmp(&a.volume_24h_quote)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut current_ranks: HashMap<String, i32> = HashMap::new();
        let mut current_prices: HashMap<String, Decimal> = HashMap::new();
        let mut current_volumes_24h: HashMap<String, Decimal> = HashMap::new();
        let mut current_top_150: HashSet<String> = HashSet::new();

        for (i, snapshot) in current_snapshots.iter().enumerate() {
            let rank = (i + 1) as i32;
            current_ranks.insert(snapshot.symbol.clone(), rank);
            current_prices.insert(snapshot.symbol.clone(), snapshot.price);
            current_volumes_24h.insert(snapshot.symbol.clone(), snapshot.volume_24h_quote);
            if rank <= 150 {
                current_top_150.insert(snapshot.symbol.clone());
            }
        }

        self.persist_rank_snapshots_from_scan(&current_snapshots, &current_ranks, now)
            .await;

        // 初始化
        if self.last_snapshots.is_empty() {
            for snapshot in &current_snapshots {
                self.last_snapshots
                    .insert(snapshot.symbol.clone(), snapshot.clone());
            }
            self.last_top_150 = current_top_150;
            self.rank_history.push_back(RankSnapshot {
                timestamp: now,
                ranks: current_ranks,
                prices: current_prices,
            });
            info!(
                "Initialized scanner with {} tickers",
                self.last_snapshots.len()
            );
            return Ok(vec![]);
        }

        let previous_rank_snapshot = self.rank_history.back().cloned();

        // 2. 维护历史 (保留 25 小时)
        while let Some(front) = self.rank_history.front() {
            if now - front.timestamp > Duration::hours(25) {
                self.rank_history.pop_front();
            } else {
                break;
            }
        }
        self.rank_history.push_back(RankSnapshot {
            timestamp: now,
            ranks: current_ranks.clone(),
            prices: current_prices.clone(),
        });

        // 3. 获取历史排名
        let snapshot_15m = self.get_historical_snapshot(Duration::minutes(15));
        let snapshot_4h = self.get_historical_snapshot(Duration::hours(4));
        let snapshot_24h = self.get_historical_snapshot(Duration::hours(24));

        self.persist_top50_boundary_events(
            &current_ranks,
            &current_prices,
            &current_volumes_24h,
            previous_rank_snapshot.as_ref(),
            now,
        )
        .await;

        // 4. 处理 Top 150 Entry/Exit (首次扫描时跳过通知，避免刷屏)
        for symbol in &current_top_150 {
            if !self.last_top_150.contains(symbol) {
                let rank = *current_ranks.get(symbol).unwrap_or(&0);
                if self.is_first_scan {
                    // 首次扫描只记录日志，不发送通知
                } else {
                    info!("🔔 [TOP 150 ENTRY] {}: Entered at rank {}", symbol, rank);
                    self.send_list_change_notification(symbol, true, rank, now)
                        .await;
                }
            }
        }
        //跌出Top150
        for symbol in &self.last_top_150 {
            if !current_top_150.contains(symbol) {
                info!("🔔 [TOP 150 EXIT] {}: Dropped out", symbol);
                // self.send_list_change_notification(symbol, false, rank, now).await;
                if let Err(e) = self.anomaly_repo.mark_exited(symbol).await {
                    error!("Failed to mark {} as exited: {:?}", symbol, e);
                }
            }
        }

        // 5. UPSERT Top 150 记录 + 排名剧变通知
        for snapshot in &current_snapshots {
            let rank = *current_ranks.get(&snapshot.symbol).unwrap_or(&9999);
            if rank > 150 {
                continue;
            }

            let r15m = snapshot_15m
                .as_ref()
                .and_then(|item| item.ranks.get(&snapshot.symbol).cloned());
            let r4h = snapshot_4h
                .as_ref()
                .and_then(|item| item.ranks.get(&snapshot.symbol).cloned());
            let r24h = snapshot_24h
                .as_ref()
                .and_then(|item| item.ranks.get(&snapshot.symbol).cloned());
            let p15m = snapshot_15m
                .as_ref()
                .and_then(|item| item.prices.get(&snapshot.symbol).cloned());
            let p4h = snapshot_4h
                .as_ref()
                .and_then(|item| item.prices.get(&snapshot.symbol).cloned());
            let p24h = snapshot_24h
                .as_ref()
                .and_then(|item| item.prices.get(&snapshot.symbol).cloned());

            let d15m = r15m.map(|r| r - rank);
            let d4h = r4h.map(|r| r - rank);
            let d24h = r24h.map(|r| r - rank);

            self.persist_rank_velocity_event(
                &snapshot.symbol,
                "15分钟",
                r15m,
                rank,
                d15m,
                Some(snapshot.volume_24h_quote),
                Some(snapshot.price),
                p15m,
                now,
            )
            .await;
            self.persist_rank_velocity_event(
                &snapshot.symbol,
                "4小时",
                r4h,
                rank,
                d4h,
                Some(snapshot.volume_24h_quote),
                Some(snapshot.price),
                p4h,
                now,
            )
            .await;
            self.persist_rank_velocity_event(
                &snapshot.symbol,
                "24小时",
                r24h,
                rank,
                d24h,
                Some(snapshot.volume_24h_quote),
                Some(snapshot.price),
                p24h,
                now,
            )
            .await;

            // 排名剧变通知 (任一周期上升 >= 3) - 带冷却期
            self.check_and_notify_rank_change(
                &snapshot.symbol,
                "15分钟",
                r15m,
                rank,
                d15m,
                COOLDOWN_MINUTES_15M,
                now,
            )
            .await;
            self.check_and_notify_rank_change(
                &snapshot.symbol,
                "4小时",
                r4h,
                rank,
                d4h,
                COOLDOWN_MINUTES_4H,
                now,
            )
            .await;
            self.check_and_notify_rank_change(
                &snapshot.symbol,
                "24小时",
                r24h,
                rank,
                d24h,
                COOLDOWN_MINUTES_24H,
                now,
            )
            .await;

            let anomaly = MarketAnomaly {
                id: None,
                symbol: snapshot.symbol.clone(),
                current_rank: rank,
                rank_15m_ago: r15m,
                rank_4h_ago: r4h,
                rank_24h_ago: r24h,
                delta_15m: d15m,
                delta_4h: d4h,
                delta_24h: d24h,
                volume_24h: Some(snapshot.volume_24h_quote),
                updated_at: now,
                status: "ACTIVE".to_string(),
            };

            if let Err(e) = self.anomaly_repo.save(&anomaly).await {
                error!("Failed to save anomaly for {}: {:?}", snapshot.symbol, e);
            }
        }

        // 清理过期的冷却记录 (超过24小时)
        self.notification_cooldown
            .retain(|_, &mut v| now - v < Duration::hours(25));

        // Update State
        self.last_top_150 = current_top_150;
        for snapshot in current_snapshots {
            self.last_snapshots
                .insert(snapshot.symbol.clone(), snapshot);
        }

        // 首次扫描完成后，后续扫描可以正常发送通知
        if self.is_first_scan {
            info!("First scan completed, enabling notifications for subsequent scans");
            self.is_first_scan = false;
        }

        Ok(vec![])
    }

    async fn persist_rank_snapshots_from_scan(
        &self,
        current_snapshots: &[TickerSnapshot],
        current_ranks: &HashMap<String, i32>,
        captured_at: DateTime<Utc>,
    ) {
        let snapshots =
            build_market_rank_snapshots_from_scan(current_snapshots, current_ranks, captured_at);
        if let Err(err) = self.anomaly_repo.save_rank_snapshots(&snapshots).await {
            error!("Failed to save market rank price snapshots: {:?}", err);
            return;
        }

        let retention_start = captured_at - Duration::hours(MARKET_RANK_HISTORY_RETENTION_HOURS);
        if let Err(err) = self
            .anomaly_repo
            .delete_rank_snapshots_before(retention_start)
            .await
        {
            warn!(
                "Failed to prune stale market rank price snapshots: {:?}",
                err
            );
        }
    }

    /// 检查并发送排名变化通知 (带冷却期)
    #[allow(clippy::too_many_arguments)]
    async fn check_and_notify_rank_change(
        &mut self,
        symbol: &str,
        timeframe: &str,
        old_rank: Option<i32>,
        new_rank: i32,
        delta: Option<i32>,
        cooldown_minutes: i64,
        now: DateTime<Utc>,
    ) {
        if let Some(d) = delta {
            if d >= RANK_CHANGE_THRESHOLD {
                let cooldown_key = format!("{}:{}", symbol, timeframe);

                // 检查冷却期
                if let Some(&last_notify) = self.notification_cooldown.get(&cooldown_key) {
                    if now - last_notify < Duration::minutes(cooldown_minutes) {
                        return; // 仍在冷却期，跳过
                    }
                }

                let old = old_rank.unwrap_or(0);
                info!(
                    "🚀 [RANK VELOCITY {}] {}: Rank {} -> {} (Delta +{})",
                    timeframe, symbol, old, new_rank, d
                );

                if let Some(ref telegram) = self.telegram {
                    if let Err(e) = telegram
                        .notify_rank_change(symbol, timeframe, old, new_rank, d)
                        .await
                    {
                        error!("Failed to send Telegram notification: {:?}", e);
                    } else {
                        // 更新冷却时间
                        self.notification_cooldown.insert(cooldown_key, now);
                    }
                }
            }
        }
    }

    /// 发送榜单变动通知 (带冷却期)
    async fn send_list_change_notification(
        &mut self,
        symbol: &str,
        is_entry: bool,
        rank: i32,
        now: DateTime<Utc>,
    ) {
        let cooldown_key = format!("{}:LIST", symbol);

        // 检查冷却期
        if let Some(&last_notify) = self.notification_cooldown.get(&cooldown_key) {
            if now - last_notify < Duration::minutes(COOLDOWN_MINUTES_LIST) {
                return; // 仍在冷却期，跳过
            }
        }

        if let Some(ref telegram) = self.telegram {
            if let Err(e) = telegram.notify_list_change(symbol, is_entry, rank).await {
                error!("Failed to send Telegram list change notification: {:?}", e);
            } else {
                self.notification_cooldown.insert(cooldown_key, now);
            }
        }
    }

    async fn persist_rank_velocity_event(
        &self,
        symbol: &str,
        timeframe: &str,
        old_rank: Option<i32>,
        new_rank: i32,
        delta: Option<i32>,
        volume_24h_quote: Option<Decimal>,
        current_price: Option<Decimal>,
        previous_price: Option<Decimal>,
        detected_at: DateTime<Utc>,
    ) {
        if !matches!(delta, Some(d) if d >= RANK_CHANGE_THRESHOLD) {
            return;
        }

        let technical_capture = self
            .capture_rank_event_technical_snapshot(
                symbol,
                is_top50_rank(Some(new_rank)) || is_top50_rank(old_rank),
            )
            .await;
        let mut event = build_rank_velocity_event(
            symbol,
            timeframe,
            old_rank,
            new_rank,
            delta,
            volume_24h_quote,
            current_price,
            previous_price,
            detected_at,
            technical_capture,
        );
        match self.anomaly_repo.save_rank_event(&event).await {
            Ok(id) => {
                event.id = Some(id);
                if let Err(e) = dispatch_market_velocity_strategy_signal_if_enabled(&event).await {
                    error!(
                        "Failed to dispatch rank velocity strategy signal for {}: {:?}",
                        symbol, e
                    );
                }
            }
            Err(e) => {
                error!("Failed to save rank velocity event for {}: {:?}", symbol, e);
            }
        }
    }

    async fn persist_top_list_event(
        &self,
        symbol: &str,
        is_entry: bool,
        old_rank: Option<i32>,
        new_rank: Option<i32>,
        volume_24h_quote: Option<Decimal>,
        current_price: Option<Decimal>,
        previous_price: Option<Decimal>,
        detected_at: DateTime<Utc>,
    ) {
        let technical_capture = self
            .capture_rank_event_technical_snapshot(
                symbol,
                is_top50_rank(new_rank) || is_top50_rank(old_rank),
            )
            .await;
        let mut event = build_top_list_event(
            symbol,
            is_entry,
            old_rank,
            new_rank,
            volume_24h_quote,
            current_price,
            previous_price,
            detected_at,
            technical_capture,
        );
        match self.anomaly_repo.save_rank_event(&event).await {
            Ok(id) => {
                event.id = Some(id);
                if let Err(e) = dispatch_market_velocity_strategy_signal_if_enabled(&event).await {
                    error!(
                        "Failed to dispatch top list strategy signal for {}: {:?}",
                        symbol, e
                    );
                }
            }
            Err(e) => {
                error!("Failed to save top list event for {}: {:?}", symbol, e);
            }
        }
    }

    async fn capture_rank_event_technical_snapshot(
        &self,
        symbol: &str,
        should_capture: bool,
    ) -> MarketRankTechnicalCapture {
        if !should_capture {
            return MarketRankTechnicalCapture::not_requested();
        }

        let Some(candle_service) = &self.technical_candle_service else {
            return MarketRankTechnicalCapture::new("not_configured", None);
        };

        let candles = match candle_service
            .fetch_candles_from_crypto_exc_all(
                "okx",
                symbol,
                MARKET_RANK_TECHNICAL_TIMEFRAME,
                None,
                None,
                MARKET_RANK_TECHNICAL_FETCH_LIMIT,
            )
            .await
        {
            Ok(candles) => candles,
            Err(error) => {
                warn!(
                    "Failed to fetch {} candles for market rank technical snapshot {}: {:?}",
                    MARKET_RANK_TECHNICAL_TIMEFRAME, symbol, error
                );
                return MarketRankTechnicalCapture::new("fetch_failed", None);
            }
        };

        if !candles.is_empty() {
            if let Err(error) = candle_service.save_candles(candles.clone()).await {
                warn!(
                    "Failed to persist {} candles for market rank technical snapshot {}: {:?}",
                    MARKET_RANK_TECHNICAL_TIMEFRAME, symbol, error
                );
            }
        }

        match build_market_rank_technical_snapshot_from_candles(
            MARKET_RANK_TECHNICAL_TIMEFRAME,
            MARKET_RANK_TECHNICAL_PERIOD,
            &candles,
        ) {
            Some(snapshot) => MarketRankTechnicalCapture::new("captured", Some(snapshot)),
            None => MarketRankTechnicalCapture::new("insufficient_kline", None),
        }
    }

    async fn persist_top50_boundary_events(
        &self,
        current_ranks: &HashMap<String, i32>,
        current_prices: &HashMap<String, Decimal>,
        current_volumes_24h: &HashMap<String, Decimal>,
        previous_snapshot: Option<&RankSnapshot>,
        detected_at: DateTime<Utc>,
    ) {
        let Some(previous_snapshot) = previous_snapshot else {
            return;
        };

        let mut symbols: HashSet<String> = previous_snapshot.ranks.keys().cloned().collect();
        symbols.extend(current_ranks.keys().cloned());

        for symbol in symbols {
            let old_rank = previous_snapshot.ranks.get(&symbol).copied();
            let new_rank = current_ranks.get(&symbol).copied();
            let was_top50 = is_top50_rank(old_rank);
            let is_top50 = is_top50_rank(new_rank);

            if was_top50 == is_top50 {
                continue;
            }

            self.persist_top_list_event(
                &symbol,
                is_top50,
                old_rank,
                new_rank,
                current_volumes_24h.get(&symbol).cloned(),
                current_prices.get(&symbol).cloned(),
                previous_snapshot.prices.get(&symbol).cloned(),
                detected_at,
            )
            .await;
        }
    }

    /// 获取指定时间前的排名快照
    fn get_historical_snapshot(&self, duration: Duration) -> Option<RankSnapshot> {
        let now = Utc::now();
        let target = now - duration;

        self.rank_history
            .iter()
            .rev()
            .find(|snap| snap.timestamp <= target)
            .cloned()
    }
}

fn build_market_rank_snapshots_from_scan(
    current_snapshots: &[TickerSnapshot],
    current_ranks: &HashMap<String, i32>,
    captured_at: DateTime<Utc>,
) -> Vec<MarketRankSnapshot> {
    current_snapshots
        .iter()
        .filter_map(|snapshot| {
            current_ranks
                .get(&snapshot.symbol)
                .map(|rank| MarketRankSnapshot {
                    id: None,
                    exchange: "okx".to_string(),
                    symbol: snapshot.symbol.clone(),
                    rank: *rank,
                    price: snapshot.price,
                    volume_24h_quote: snapshot.volume_24h_quote,
                    captured_at,
                    created_at: captured_at,
                })
        })
        .collect()
}

fn rank_history_from_persisted_snapshots(
    snapshots: Vec<MarketRankSnapshot>,
) -> VecDeque<RankSnapshot> {
    let mut grouped: BTreeMap<DateTime<Utc>, RankSnapshot> = BTreeMap::new();
    for snapshot in snapshots {
        let entry = grouped
            .entry(snapshot.captured_at)
            .or_insert_with(|| RankSnapshot {
                timestamp: snapshot.captured_at,
                ranks: HashMap::new(),
                prices: HashMap::new(),
            });
        entry.ranks.insert(snapshot.symbol.clone(), snapshot.rank);
        entry.prices.insert(snapshot.symbol, snapshot.price);
    }

    grouped.into_values().collect()
}

fn compute_event_price_change_pct(
    current_price: Option<Decimal>,
    previous_price: Option<Decimal>,
) -> Option<Decimal> {
    let current_price = current_price?;
    let previous_price = previous_price?;
    if previous_price <= Decimal::ZERO {
        return None;
    }
    Some((current_price - previous_price) / previous_price * Decimal::new(100, 0))
}

fn price_direction(price_change_pct: Option<Decimal>) -> String {
    match price_change_pct {
        Some(value) if value > Decimal::ZERO => "up".to_string(),
        Some(value) if value < Decimal::ZERO => "down".to_string(),
        Some(_) => "flat".to_string(),
        None => "unknown".to_string(),
    }
}

fn is_top50_rank(rank: Option<i32>) -> bool {
    rank.is_some_and(|value| value > 0 && value <= MARKET_RANK_TOP_BOUNDARY)
}

fn compute_rank_delta(old_rank: Option<i32>, new_rank: Option<i32>) -> Option<i32> {
    Some(old_rank? - new_rank?)
}

#[derive(Debug, Clone)]
struct MarketRankTechnicalCapture {
    status: String,
    snapshot: Option<MarketRankTechnicalSnapshot>,
}

impl MarketRankTechnicalCapture {
    fn new(status: impl Into<String>, snapshot: Option<MarketRankTechnicalSnapshot>) -> Self {
        Self {
            status: status.into(),
            snapshot,
        }
    }

    fn not_requested() -> Self {
        Self::new("not_requested", None)
    }
}

fn build_rank_velocity_event(
    symbol: &str,
    timeframe: &str,
    old_rank: Option<i32>,
    new_rank: i32,
    delta: Option<i32>,
    volume_24h_quote: Option<Decimal>,
    current_price: Option<Decimal>,
    previous_price: Option<Decimal>,
    detected_at: DateTime<Utc>,
    technical_capture: MarketRankTechnicalCapture,
) -> MarketRankEvent {
    let price_change_pct = compute_event_price_change_pct(current_price, previous_price);
    MarketRankEvent {
        id: None,
        exchange: "okx".to_string(),
        symbol: symbol.to_string(),
        event_type: MarketRankEventType::RankVelocity,
        timeframe: Some(timeframe.to_string()),
        old_rank,
        new_rank: Some(new_rank),
        delta_rank: delta,
        volume_24h_quote,
        current_price,
        previous_price,
        price_change_pct,
        price_direction: price_direction(price_change_pct),
        technical_snapshot_status: technical_capture.status,
        technical_snapshot: technical_capture.snapshot,
        detected_at,
        source: "scanner_service".to_string(),
        notification_state: "pending".to_string(),
    }
}

fn build_top_list_event(
    symbol: &str,
    is_entry: bool,
    old_rank: Option<i32>,
    new_rank: Option<i32>,
    volume_24h_quote: Option<Decimal>,
    current_price: Option<Decimal>,
    previous_price: Option<Decimal>,
    detected_at: DateTime<Utc>,
    technical_capture: MarketRankTechnicalCapture,
) -> MarketRankEvent {
    let price_change_pct = compute_event_price_change_pct(current_price, previous_price);
    MarketRankEvent {
        id: None,
        exchange: "okx".to_string(),
        symbol: symbol.to_string(),
        event_type: if is_entry {
            MarketRankEventType::TopEntry
        } else {
            MarketRankEventType::TopExit
        },
        timeframe: None,
        old_rank,
        new_rank,
        delta_rank: compute_rank_delta(old_rank, new_rank),
        volume_24h_quote,
        current_price,
        previous_price,
        price_change_pct,
        price_direction: price_direction(price_change_pct),
        technical_snapshot_status: technical_capture.status,
        technical_snapshot: technical_capture.snapshot,
        detected_at,
        source: "scanner_service".to_string(),
        notification_state: "pending".to_string(),
    }
}

fn build_market_rank_technical_snapshot_from_candles(
    timeframe: &str,
    period: usize,
    candles: &[Candle],
) -> Option<MarketRankTechnicalSnapshot> {
    let mut candles = candles.to_vec();
    candles.sort_by_key(|candle| candle.timestamp);

    let snapshot_at = candles.last()?.datetime;
    let closes = candles
        .iter()
        .map(|candle| candle.close.value())
        .collect::<Vec<_>>();
    build_market_rank_technical_snapshot_from_closes(timeframe, period, &closes, snapshot_at)
}

fn build_market_rank_technical_snapshot_from_closes(
    timeframe: &str,
    period: usize,
    closes: &[f64],
    snapshot_at: DateTime<Utc>,
) -> Option<MarketRankTechnicalSnapshot> {
    if period == 0 || closes.len() < period || closes.iter().any(|value| !value.is_finite()) {
        return None;
    }

    let latest_close = *closes.last()?;
    let ma_value = simple_moving_average(&closes[closes.len() - period..])?;
    let ema_value = exponential_moving_average(closes, period)?;
    let previous_close = closes.get(closes.len().checked_sub(2)?).copied();
    let previous_ma = if closes.len() > period {
        simple_moving_average(&closes[closes.len() - period - 1..closes.len() - 1])
    } else {
        None
    };
    let previous_ema = if closes.len() > period {
        exponential_moving_average(&closes[..closes.len() - 1], period)
    } else {
        None
    };

    Some(MarketRankTechnicalSnapshot {
        timeframe: timeframe.to_string(),
        period: period as i32,
        close_price: decimal_from_f64(latest_close)?,
        ma_value: decimal_from_f64(ma_value)?,
        ema_value: decimal_from_f64(ema_value)?,
        ma_distance_pct: decimal_from_f64(moving_average_distance_pct(latest_close, ma_value)?)?,
        ema_distance_pct: decimal_from_f64(moving_average_distance_pct(latest_close, ema_value)?)?,
        ma_state: moving_average_state(latest_close, ma_value, previous_close, previous_ma),
        ema_state: moving_average_state(latest_close, ema_value, previous_close, previous_ema),
        candle_count: closes.len() as i32,
        snapshot_at,
    })
}

fn simple_moving_average(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn exponential_moving_average(values: &[f64], period: usize) -> Option<f64> {
    if values.len() < period {
        return None;
    }

    let mut ema = simple_moving_average(&values[..period])?;
    let multiplier = 2.0 / (period as f64 + 1.0);
    for value in &values[period..] {
        ema = (*value - ema) * multiplier + ema;
    }
    Some(ema)
}

fn moving_average_distance_pct(close: f64, average: f64) -> Option<f64> {
    if average <= 0.0 || !average.is_finite() || !close.is_finite() {
        return None;
    }
    Some((close - average) / average * 100.0)
}

fn moving_average_state(
    close: f64,
    average: f64,
    previous_close: Option<f64>,
    previous_average: Option<f64>,
) -> String {
    if let (Some(previous_close), Some(previous_average)) = (previous_close, previous_average) {
        if close > average && previous_close <= previous_average {
            return "breakout_up".to_string();
        }
        if close < average && previous_close >= previous_average {
            return "breakdown_down".to_string();
        }
    }

    let distance_pct = moving_average_distance_pct(close, average).unwrap_or(0.0);
    if distance_pct.abs() <= MARKET_RANK_TECHNICAL_TOUCH_THRESHOLD_PCT {
        "touching".to_string()
    } else if close > average {
        "above".to_string()
    } else {
        "below".to_string()
    }
}

fn decimal_from_f64(value: f64) -> Option<Decimal> {
    Decimal::from_f64(value).map(|value| value.round_dp(12))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_domain::entities::{
        MarketRankEventType, MarketRankSnapshot, MarketRankTechnicalSnapshot,
    };

    #[test]
    fn build_rank_velocity_event_uses_scanner_product_contract() {
        let detected_at = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");

        let event = build_rank_velocity_event(
            "ETH-USDT-SWAP",
            "15分钟",
            Some(42),
            18,
            Some(24),
            None,
            Some(Decimal::new(2200, 0)),
            Some(Decimal::new(2000, 0)),
            detected_at,
            MarketRankTechnicalCapture::not_requested(),
        );

        assert_eq!(event.exchange, "okx");
        assert_eq!(event.symbol, "ETH-USDT-SWAP");
        assert_eq!(event.event_type, MarketRankEventType::RankVelocity);
        assert_eq!(event.timeframe.as_deref(), Some("15分钟"));
        assert_eq!(event.old_rank, Some(42));
        assert_eq!(event.new_rank, Some(18));
        assert_eq!(event.delta_rank, Some(24));
        assert_eq!(event.current_price, Some(Decimal::new(2200, 0)));
        assert_eq!(event.previous_price, Some(Decimal::new(2000, 0)));
        assert_eq!(event.price_change_pct, Some(Decimal::new(100, 1)));
        assert_eq!(event.price_direction, "up");
        assert_eq!(event.source, "scanner_service");
        assert_eq!(event.notification_state, "pending");
    }

    #[test]
    fn build_top_list_event_uses_entry_and_exit_contract() {
        let detected_at = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");

        let entry = build_top_list_event(
            "SOL-USDT-SWAP",
            true,
            Some(55),
            Some(40),
            None,
            Some(Decimal::new(180, 1)),
            None,
            detected_at,
            MarketRankTechnicalCapture::not_requested(),
        );
        assert_eq!(entry.exchange, "okx");
        assert_eq!(entry.event_type, MarketRankEventType::TopEntry);
        assert_eq!(entry.old_rank, Some(55));
        assert_eq!(entry.new_rank, Some(40));
        assert_eq!(entry.delta_rank, Some(15));
        assert_eq!(entry.current_price, Some(Decimal::new(180, 1)));
        assert_eq!(entry.price_direction, "unknown");
        assert_eq!(entry.source, "scanner_service");

        let exit = build_top_list_event(
            "DOGE-USDT-SWAP",
            false,
            Some(45),
            Some(62),
            None,
            Some(Decimal::new(12, 2)),
            Some(Decimal::new(15, 2)),
            detected_at,
            MarketRankTechnicalCapture::not_requested(),
        );
        assert_eq!(exit.event_type, MarketRankEventType::TopExit);
        assert_eq!(exit.symbol, "DOGE-USDT-SWAP");
        assert_eq!(exit.old_rank, Some(45));
        assert_eq!(exit.new_rank, Some(62));
        assert_eq!(exit.delta_rank, Some(-17));
        assert_eq!(exit.price_change_pct, Some(Decimal::new(-200, 1)));
        assert_eq!(exit.price_direction, "down");
        assert_eq!(exit.notification_state, "pending");
    }

    #[test]
    fn build_market_rank_technical_snapshot_detects_4h_ma_and_ema_breakout() {
        let snapshot_at = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");
        let mut closes = vec![100.0; 20];
        closes.push(120.0);

        let snapshot: MarketRankTechnicalSnapshot =
            build_market_rank_technical_snapshot_from_closes("4h", 20, &closes, snapshot_at)
                .expect("enough candles should build technical snapshot");

        assert_eq!(snapshot.timeframe, "4h");
        assert_eq!(snapshot.period, 20);
        assert_eq!(snapshot.close_price, Decimal::new(120, 0));
        assert_eq!(snapshot.ma_value, Decimal::new(101, 0));
        assert_eq!(snapshot.ma_state, "breakout_up");
        assert_eq!(snapshot.ema_state, "breakout_up");
        assert_eq!(snapshot.candle_count, 21);
        assert_eq!(snapshot.snapshot_at, snapshot_at);
        assert!(snapshot.ma_distance_pct > Decimal::ZERO);
        assert!(snapshot.ema_distance_pct > Decimal::ZERO);
    }

    #[test]
    fn build_market_rank_technical_snapshot_requires_enough_closes() {
        let snapshot_at = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");

        let snapshot =
            build_market_rank_technical_snapshot_from_closes("4h", 20, &[100.0; 19], snapshot_at);

        assert!(snapshot.is_none());
    }

    #[test]
    fn rank_history_from_persisted_snapshots_restores_prices_by_scan_time() {
        let first_scan = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");
        let second_scan = DateTime::from_timestamp(1_774_815_300, 0).expect("valid test timestamp");
        let rows = vec![
            MarketRankSnapshot {
                id: Some(1),
                exchange: "okx".to_string(),
                symbol: "XLM-USDT-SWAP".to_string(),
                rank: 107,
                price: Decimal::new(105, 3),
                volume_24h_quote: Decimal::new(42_000_000, 0),
                captured_at: first_scan,
                created_at: first_scan,
            },
            MarketRankSnapshot {
                id: Some(2),
                exchange: "okx".to_string(),
                symbol: "XLM-USDT-SWAP".to_string(),
                rank: 23,
                price: Decimal::new(126, 3),
                volume_24h_quote: Decimal::new(112_000_000, 0),
                captured_at: second_scan,
                created_at: second_scan,
            },
        ];

        let history = rank_history_from_persisted_snapshots(rows);

        assert_eq!(history.len(), 2);
        assert_eq!(
            history[0].prices.get("XLM-USDT-SWAP"),
            Some(&Decimal::new(105, 3))
        );
        assert_eq!(
            history[1].prices.get("XLM-USDT-SWAP"),
            Some(&Decimal::new(126, 3))
        );
    }
}
