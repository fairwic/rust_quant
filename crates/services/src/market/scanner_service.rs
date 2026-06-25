use super::market_velocity_entry::{
    build_market_velocity_entry_confirmation_from_candles, MarketVelocityEntryConfirmation,
    MarketVelocityEntryConfirmationDecision,
};
use super::market_velocity_signal::{
    dispatch_market_velocity_strategy_signal_with_config_and_entry_confirmation,
    market_velocity_signal_dispatch_is_enabled,
    market_velocity_strategy_signal_needs_entry_confirmation, MarketVelocityStrategySignalConfig,
};
use super::CandleService;
use crate::notification::TelegramNotifier;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use rust_quant_domain::entities::{
    MarketAnomaly, MarketRankEvent, MarketRankEventType, MarketRankSnapshot,
    MarketRankTechnicalSnapshot, MarketVelocityEpisode, TickerSnapshot,
};
use rust_quant_domain::traits::fund_monitoring_repository::MarketAnomalyRepository;
use rust_quant_domain::Candle;
use rust_quant_market::scanners::okx_scanner::OkxScanner;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tracing::{error, info, warn};
/// 排名快照
#[derive(Clone)]
struct RankSnapshot {
    /// 事件时间戳。
    timestamp: DateTime<Utc>,
    /// 键值扩展数据。
    ranks: HashMap<String, i32>,
    /// 键值扩展数据。
    prices: HashMap<String, Decimal>,
}
/// 扫描器服务
/// 负责定时扫描全市场Ticker，维护 Top 150 排名，并发送 Telegram 通知
pub struct ScannerService {
    /// scanner，用于行情、K 线或市场扫描。
    scanner: OkxScanner,
    /// 键值扩展数据。
    last_snapshots: HashMap<String, TickerSnapshot>,
    /// anomalyrepo，用于行情、K 线或市场扫描。
    anomaly_repo: Arc<dyn MarketAnomalyRepository>,
    /// 技术K 线service；为空时表示该条件不启用。
    technical_candle_service: Option<Arc<CandleService>>,
    /// 配置项。
    market_velocity_signal_config: Option<MarketVelocityStrategySignalConfig>,
    /// 排名history，用于行情、K 线或市场扫描。
    rank_history: VecDeque<RankSnapshot>,
    /// 最近top150，用于行情、K 线或市场扫描。
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
const MARKET_VELOCITY_ENTRY_TIMEFRAME: &str = "15m";
fn market_velocity_episode_stale_before(now: DateTime<Utc>) -> DateTime<Utc> {
    now - Duration::hours(MARKET_RANK_HISTORY_RETENTION_HOURS)
}
impl ScannerService {
    pub fn new(anomaly_repo: Arc<dyn MarketAnomalyRepository>) -> Result<Self> {
        Self::new_with_technical_candle_service(anomaly_repo, None)
    }
    /// 提供newwithtechnicalK 线service的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn new_with_technical_candle_service(
        anomaly_repo: Arc<dyn MarketAnomalyRepository>,
        technical_candle_service: Option<Arc<CandleService>>,
    ) -> Result<Self> {
        Self::new_with_technical_candle_service_and_market_velocity_signal_config(
            anomaly_repo,
            technical_candle_service,
            None,
        )
    }
    /// 提供newwithtechnicalK 线serviceand市场动量信号配置的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn new_with_technical_candle_service_and_market_velocity_signal_config(
        anomaly_repo: Arc<dyn MarketAnomalyRepository>,
        technical_candle_service: Option<Arc<CandleService>>,
        market_velocity_signal_config: Option<MarketVelocityStrategySignalConfig>,
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
            market_velocity_signal_config,
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
        let restore_targets = market_rank_history_restore_targets(now);
        match self
            .anomaly_repo
            .load_rank_snapshots_for_restore("okx", &restore_targets)
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
    /// 提供scanandanalyze的集中实现，避免行情数据调用方重复处理相同细节。
    pub async fn scan_and_analyze(&mut self) -> Result<Vec<(String, Decimal)>> {
        let mut current_snapshots = self.scanner.fetch_all_tickers().await?;
        let now = Utc::now();
        self.close_stale_market_velocity_episodes(now).await;
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
    /// 持久化 行情与市场数据 结果，保证写入路径和幂等语义集中处理。
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
    /// 持久化 行情与市场数据 结果，保证写入路径和幂等语义集中处理。
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
        let episode = build_market_velocity_episode_from_event(&event);
        let Some(episode_id) = self.market_velocity_episode_append_id(&episode).await else {
            return;
        };
        match self.anomaly_repo.save_rank_event(&event).await {
            Ok(id) => {
                event.id = Some(id);
                self.attach_rank_event_to_market_velocity_episode(episode_id, id, detected_at)
                    .await;
                let signal_config = self.market_velocity_signal_config_for_event(&event);
                let entry_confirmation = match signal_config.as_ref() {
                    Some(config) => {
                        self.market_velocity_entry_confirmation_if_needed(&event, config)
                            .await
                    }
                    None => None,
                };
                if let Some(config) = signal_config.as_ref() {
                    if let Err(e) =
                        dispatch_market_velocity_strategy_signal_with_config_and_entry_confirmation(
                            &event,
                            config,
                            entry_confirmation.as_ref(),
                        )
                        .await
                    {
                        error!(
                            "Failed to dispatch rank velocity strategy signal for {}: {:?}",
                            symbol, e
                        );
                    }
                }
            }
            Err(e) => {
                error!("Failed to save rank velocity event for {}: {:?}", symbol, e);
            }
        }
    }
    /// 持久化 行情与市场数据 结果，保证写入路径和幂等语义集中处理。
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
        let episode = build_market_velocity_episode_from_event(&event);
        let Some(episode_id) = self.market_velocity_episode_append_id(&episode).await else {
            return;
        };
        match self.anomaly_repo.save_rank_event(&event).await {
            Ok(id) => {
                event.id = Some(id);
                self.attach_rank_event_to_market_velocity_episode(episode_id, id, detected_at)
                    .await;
                let signal_config = self.market_velocity_signal_config_for_event(&event);
                let entry_confirmation = match signal_config.as_ref() {
                    Some(config) => {
                        self.market_velocity_entry_confirmation_if_needed(&event, config)
                            .await
                    }
                    None => None,
                };
                if let Some(config) = signal_config.as_ref() {
                    if let Err(e) =
                        dispatch_market_velocity_strategy_signal_with_config_and_entry_confirmation(
                            &event,
                            config,
                            entry_confirmation.as_ref(),
                        )
                        .await
                    {
                        error!(
                            "Failed to dispatch top list strategy signal for {}: {:?}",
                            symbol, e
                        );
                    }
                }
            }
            Err(e) => {
                error!("Failed to save top list event for {}: {:?}", symbol, e);
            }
        }
    }
    /// 提供市场动量episodeappendID的集中实现，避免行情数据调用方重复处理相同细节。
    async fn market_velocity_episode_append_id(
        &self,
        episode: &MarketVelocityEpisode,
    ) -> Option<i64> {
        match self
            .anomaly_repo
            .upsert_market_velocity_episode(episode)
            .await
        {
            Ok((id, write)) => write.should_append_rank_event().then_some(id),
            Err(error) => {
                error!(
                    "Failed to upsert market velocity episode for {}: {:?}",
                    episode.symbol, error
                );
                None
            }
        }
    }
    /// 停止 行情与市场数据 后台流程，确保退出时不留下未释放状态。
    async fn close_stale_market_velocity_episodes(&self, now: DateTime<Utc>) {
        let stale_before = market_velocity_episode_stale_before(now);
        match self
            .anomaly_repo
            .close_stale_market_velocity_episodes("okx", stale_before)
            .await
        {
            Ok(0) => {}
            Ok(count) => {
                info!("Closed {} stale market velocity episodes", count);
            }
            Err(error) => {
                warn!(
                    "Failed to close stale market velocity episodes: {:?}",
                    error
                );
            }
        }
    }
    /// 提供attachrankeventto市场动量episode的集中实现，避免行情数据调用方重复处理相同细节。
    async fn attach_rank_event_to_market_velocity_episode(
        &self,
        episode_id: i64,
        rank_event_id: i64,
        escalated_at: DateTime<Utc>,
    ) {
        if let Err(error) = self
            .anomaly_repo
            .attach_rank_event_to_market_velocity_episode(episode_id, rank_event_id, escalated_at)
            .await
        {
            warn!(
                "Failed to attach rank event {} to market velocity episode {}: {:?}",
                rank_event_id, episode_id, error
            );
        }
    }
    /// 提供capturerankeventtechnical快照的集中实现，避免行情数据调用方重复处理相同细节。
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
    /// 提供市场动量信号配置forevent的集中实现，避免行情数据调用方重复处理相同细节。
    fn market_velocity_signal_config_for_event(
        &self,
        event: &MarketRankEvent,
    ) -> Option<MarketVelocityStrategySignalConfig> {
        if !market_velocity_signal_dispatch_is_enabled() {
            return None;
        }
        if let Some(config) = self.market_velocity_signal_config.as_ref() {
            return Some(config.clone());
        }
        Some(match MarketVelocityStrategySignalConfig::from_env() {
            Ok(config) => config,
            Err(error) => {
                warn!(
                    "Market Velocity signal config invalid before event dispatch: symbol={}, event_id={:?}, error={:?}",
                    event.symbol, event.id, error
                );
                return None;
            }
        })
    }
    /// 提供市场动量入场确认ifneeded的集中实现，避免行情数据调用方重复处理相同细节。
    async fn market_velocity_entry_confirmation_if_needed(
        &self,
        event: &MarketRankEvent,
        config: &MarketVelocityStrategySignalConfig,
    ) -> Option<MarketVelocityEntryConfirmation> {
        match market_velocity_strategy_signal_needs_entry_confirmation(event, config) {
            Ok(true) => {
                self.capture_market_velocity_entry_confirmation(&event.symbol, config)
                    .await
            }
            Ok(false) => None,
            Err(error) => {
                warn!(
                    "Failed to evaluate Market Velocity entry confirmation need: symbol={}, event_id={:?}, error={:?}",
                    event.symbol, event.id, error
                );
                None
            }
        }
    }
    /// 提供capture市场动量入场确认的集中实现，避免行情数据调用方重复处理相同细节。
    async fn capture_market_velocity_entry_confirmation(
        &self,
        symbol: &str,
        config: &MarketVelocityStrategySignalConfig,
    ) -> Option<MarketVelocityEntryConfirmation> {
        let Some(candle_service) = &self.technical_candle_service else {
            warn!(
                "Market Velocity entry confirmation skipped because candle service is not configured: symbol={}",
                symbol
            );
            return None;
        };
        let candles = match candle_service
            .fetch_candles_from_crypto_exc_all(
                "okx",
                symbol,
                MARKET_VELOCITY_ENTRY_TIMEFRAME,
                None,
                None,
                config.entry_confirmation_fetch_limit,
            )
            .await
        {
            Ok(candles) => candles,
            Err(error) => {
                warn!(
                    "Failed to fetch {} candles for Market Velocity entry confirmation {}: {:?}",
                    MARKET_VELOCITY_ENTRY_TIMEFRAME, symbol, error
                );
                return None;
            }
        };
        if !candles.is_empty() {
            if let Err(error) = candle_service.save_candles(candles.clone()).await {
                warn!(
                    "Failed to persist {} candles for Market Velocity entry confirmation {}: {:?}",
                    MARKET_VELOCITY_ENTRY_TIMEFRAME, symbol, error
                );
            }
        }
        match build_market_velocity_entry_confirmation_from_candles(
            MARKET_VELOCITY_ENTRY_TIMEFRAME,
            &candles,
            &config.entry_confirmation_config(),
        ) {
            MarketVelocityEntryConfirmationDecision::Confirmed(confirmation) => Some(confirmation),
            MarketVelocityEntryConfirmationDecision::Blocked(blocker) => {
                info!(
                    "Market Velocity entry timing not confirmed: symbol={}, blocker={:?}",
                    symbol, blocker
                );
                None
            }
        }
    }
    /// 持久化 行情与市场数据 结果，保证写入路径和幂等语义集中处理。
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
include!("scanner_service/rank_history_section.rs");
include!("scanner_service/rank_event_section.rs");
include!("scanner_service/technical_snapshot_section.rs");
include!("scanner_service/tests.rs");
