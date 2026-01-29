use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_quant_domain::entities::{MarketAnomaly, TickerSnapshot};
use rust_quant_domain::traits::fund_monitoring_repository::MarketAnomalyRepository;
use rust_quant_market::scanners::okx_scanner::OkxScanner;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tracing::{error, info};

/// 排名快照
struct RankSnapshot {
    timestamp: DateTime<Utc>,
    ranks: HashMap<String, i32>,
}

/// 扫描器服务
/// 负责定时扫描全市场Ticker，维护 Top 150 排名
pub struct ScannerService {
    scanner: OkxScanner,
    /// 上一次的 Ticker 快照
    last_snapshots: HashMap<String, TickerSnapshot>,
    anomaly_repo: Arc<dyn MarketAnomalyRepository>,

    /// 排名历史 (用于计算 15m, 6h, 24h 变化)
    rank_history: VecDeque<RankSnapshot>,

    /// 上一轮的 Top 150 集合
    last_top_150: HashSet<String>,
}

/// 排名剧变通知阈值
const RANK_CHANGE_THRESHOLD: i32 = 3;

impl ScannerService {
    pub fn new(anomaly_repo: Arc<dyn MarketAnomalyRepository>) -> Result<Self> {
        Ok(Self {
            scanner: OkxScanner::new()?,
            last_snapshots: HashMap::new(),
            anomaly_repo,
            rank_history: VecDeque::new(),
            last_top_150: HashSet::new(),
        })
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
            }
        }
        for symbol in &self.last_top_150 {
            if !current_top_150.contains(symbol) {
                info!("🔔 [TOP 150 EXIT] {}: Dropped out", symbol);
                if let Err(e) = self.anomaly_repo.mark_exited(symbol).await {
                    error!("Failed to mark {} as exited: {:?}", symbol, e);
                }
            }
        }

        // 5. UPSERT Top 150 记录
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

            // 排名剧变通知 (15分钟上升 >= 3)
            if let Some(delta) = d15m {
                if delta >= RANK_CHANGE_THRESHOLD {
                    info!(
                        "🚀 [RANK VELOCITY 15M] {}: Rank {} -> {} (Delta +{})",
                        snapshot.symbol,
                        r15m.unwrap_or(0),
                        rank,
                        delta
                    );
                }
            }

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

        // Update State
        self.last_top_150 = current_top_150;
        for snapshot in current_snapshots {
            self.last_snapshots
                .insert(snapshot.symbol.clone(), snapshot);
        }

        Ok(vec![])
    }

    /// 获取指定时间前的排名快照
    fn get_historical_rank(&self, duration: Duration) -> Option<HashMap<String, i32>> {
        let now = Utc::now();
        let target = now - duration;

        // 找最接近 target 的快照 (允许 10% 误差)
        self.rank_history
            .iter()
            .rev()
            .find(|snap| snap.timestamp <= target)
            .map(|snap| snap.ranks.clone())
    }
}
