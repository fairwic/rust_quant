use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_quant_domain::entities::{MarketAnomaly, TickerSnapshot};
use rust_quant_domain::traits::fund_monitoring_repository::MarketAnomalyRepository;
use rust_quant_market::scanners::okx_scanner::OkxScanner;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::notification::TelegramNotifier;

/// 排名快照
struct RankSnapshot {
    timestamp: DateTime<Utc>,
    ranks: HashMap<String, i32>,
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
        let mut current_top_150: HashSet<String> = HashSet::new();

        for (i, snapshot) in current_snapshots.iter().enumerate() {
            let rank = (i + 1) as i32;
            current_ranks.insert(snapshot.symbol.clone(), rank);
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
            });
            info!(
                "Initialized scanner with {} tickers",
                self.last_snapshots.len()
            );
            return Ok(vec![]);
        }

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
        });

        // 3. 获取历史排名
        let rank_15m = self.get_historical_rank(Duration::minutes(15));
        let rank_4h = self.get_historical_rank(Duration::hours(4));
        let rank_24h = self.get_historical_rank(Duration::hours(24));

        // 4. 处理 Top 150 Entry/Exit
        for symbol in &current_top_150 {
            if !self.last_top_150.contains(symbol) {
                let rank = *current_ranks.get(symbol).unwrap_or(&0);
                info!("🔔 [TOP 150 ENTRY] {}: Entered at rank {}", symbol, rank);
                self.send_list_change_notification(symbol, true, rank, now)
                    .await;
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

            let r15m = rank_15m
                .as_ref()
                .and_then(|m| m.get(&snapshot.symbol).cloned());
            let r4h = rank_4h
                .as_ref()
                .and_then(|m| m.get(&snapshot.symbol).cloned());
            let r24h = rank_24h
                .as_ref()
                .and_then(|m| m.get(&snapshot.symbol).cloned());

            let d15m = r15m.map(|r| r - rank);
            let d4h = r4h.map(|r| r - rank);
            let d24h = r24h.map(|r| r - rank);

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

        Ok(vec![])
    }

    /// 检查并发送排名变化通知 (带冷却期)
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

    /// 获取指定时间前的排名快照
    fn get_historical_rank(&self, duration: Duration) -> Option<HashMap<String, i32>> {
        let now = Utc::now();
        let target = now - duration;

        self.rank_history
            .iter()
            .rev()
            .find(|snap| snap.timestamp <= target)
            .map(|snap| snap.ranks.clone())
    }
}
