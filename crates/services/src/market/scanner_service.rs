use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_quant_domain::entities::{
    MarketAnomaly, MarketRankEvent, MarketRankEventType, TickerSnapshot,
};
use rust_quant_domain::traits::fund_monitoring_repository::MarketAnomalyRepository;
use rust_quant_market::scanners::okx_scanner::OkxScanner;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tracing::{error, info, warn};

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

impl ScannerService {
    pub fn new(anomaly_repo: Arc<dyn MarketAnomalyRepository>) -> Result<Self> {
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
                self.rank_history.push_back(RankSnapshot {
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

        // 初始化
        if self.last_snapshots.is_empty() {
            for snapshot in current_snapshots {
                self.last_snapshots
                    .insert(snapshot.symbol.clone(), snapshot);
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

        let event = build_rank_velocity_event(
            symbol,
            timeframe,
            old_rank,
            new_rank,
            delta,
            volume_24h_quote,
            current_price,
            previous_price,
            detected_at,
        );
        if let Err(e) = self.anomaly_repo.save_rank_event(&event).await {
            error!("Failed to save rank velocity event for {}: {:?}", symbol, e);
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
        let event = build_top_list_event(
            symbol,
            is_entry,
            old_rank,
            new_rank,
            volume_24h_quote,
            current_price,
            previous_price,
            detected_at,
        );
        if let Err(e) = self.anomaly_repo.save_rank_event(&event).await {
            error!("Failed to save top list event for {}: {:?}", symbol, e);
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
        detected_at,
        source: "scanner_service".to_string(),
        notification_state: "pending".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_domain::entities::MarketRankEventType;

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
}
