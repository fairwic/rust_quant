use crate::jobs::maintenance::scheduler::MaintenanceJob;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Timelike, Utc};
use rust_quant_domain::traits::fund_monitoring_repository::MarketAnomalyRepository;
use std::sync::Arc;
use tracing::info;

pub const MARKET_RANK_SNAPSHOT_PRODUCTION_RETENTION_DAYS: i64 = 7;
pub const MARKET_RANK_SNAPSHOT_LOCAL_RETENTION_DAYS: i64 = 90;
pub const MARKET_RANK_SNAPSHOT_PRUNE_UTC_HOUR: u32 = 18;
pub const MARKET_RANK_SNAPSHOT_PRUNE_UTC_MINUTE: u32 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketRankSnapshotPruneOutcome {
    Skipped,
    Pruned { retention_start: DateTime<Utc> },
}

pub struct MarketRankSnapshotPruneJob {
    anomaly_repo: Arc<dyn MarketAnomalyRepository>,
    exchange: String,
    last_rank_snapshot_pruned_at: Option<DateTime<Utc>>,
}

impl MarketRankSnapshotPruneJob {
    pub fn new(
        exchange: impl Into<String>,
        anomaly_repo: Arc<dyn MarketAnomalyRepository>,
    ) -> Self {
        Self {
            anomaly_repo,
            exchange: exchange.into(),
            last_rank_snapshot_pruned_at: None,
        }
    }

    pub async fn run_if_due(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<MarketRankSnapshotPruneOutcome> {
        if !market_rank_snapshot_prune_is_due(now, self.last_rank_snapshot_pruned_at) {
            return Ok(MarketRankSnapshotPruneOutcome::Skipped);
        }
        let retention_start = market_rank_snapshot_db_retention_start(now);
        self.anomaly_repo
            .delete_rank_snapshots_before(&self.exchange, retention_start)
            .await?;
        self.last_rank_snapshot_pruned_at = Some(now);
        info!(
            "Pruned stale market rank price snapshots: exchange={}, before={}",
            self.exchange, retention_start
        );
        Ok(MarketRankSnapshotPruneOutcome::Pruned { retention_start })
    }
}

#[async_trait]
impl MaintenanceJob for MarketRankSnapshotPruneJob {
    fn name(&self) -> &'static str {
        "market_rank_snapshot_prune"
    }

    async fn run_tick(&mut self, now: DateTime<Utc>) -> Result<()> {
        self.run_if_due(now).await.map(|_| ())
    }
}

fn market_rank_snapshot_db_retention_start(now: DateTime<Utc>) -> DateTime<Utc> {
    let app_env = std::env::var("APP_ENV").ok();
    market_rank_snapshot_db_retention_start_for_env(now, app_env.as_deref())
}

fn market_rank_snapshot_db_retention_start_for_env(
    now: DateTime<Utc>,
    app_env: Option<&str>,
) -> DateTime<Utc> {
    let retention_days = match app_env.map(str::trim).map(str::to_ascii_lowercase) {
        Some(env) if env == "prod" || env == "production" => {
            MARKET_RANK_SNAPSHOT_PRODUCTION_RETENTION_DAYS
        }
        _ => MARKET_RANK_SNAPSHOT_LOCAL_RETENTION_DAYS,
    };
    now - Duration::days(retention_days)
}

fn market_rank_snapshot_prune_is_due(
    now: DateTime<Utc>,
    last_pruned_at: Option<DateTime<Utc>>,
) -> bool {
    let in_daily_window = now.hour() == MARKET_RANK_SNAPSHOT_PRUNE_UTC_HOUR
        && now.minute() == MARKET_RANK_SNAPSHOT_PRUNE_UTC_MINUTE;
    if !in_daily_window {
        return false;
    }
    last_pruned_at
        .map(|pruned_at| pruned_at.date_naive() < now.date_naive())
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{bail, Result};
    use rust_quant_domain::entities::{
        MarketAnomaly, MarketRankEvent, MarketRankSnapshot, MarketVelocityEpisode,
        MarketVelocityEpisodeWrite,
    };
    use rust_quant_domain::traits::fund_monitoring_repository::MarketAnomalyRepository;
    use std::sync::Mutex;

    struct CapturingMarketAnomalyRepository {
        deletes: Mutex<Vec<(String, DateTime<Utc>)>>,
    }

    impl CapturingMarketAnomalyRepository {
        fn new() -> Self {
            Self {
                deletes: Mutex::new(Vec::new()),
            }
        }

        fn deletes(&self) -> Vec<(String, DateTime<Utc>)> {
            self.deletes.lock().expect("deletes lock").clone()
        }
    }

    #[async_trait]
    impl MarketAnomalyRepository for CapturingMarketAnomalyRepository {
        async fn save(&self, _anomaly: &MarketAnomaly) -> Result<i64> {
            bail!("unexpected save")
        }

        async fn mark_exited(&self, _symbol: &str) -> Result<()> {
            bail!("unexpected mark_exited")
        }

        async fn get_latest_update_time(&self) -> Result<Option<DateTime<Utc>>> {
            bail!("unexpected get_latest_update_time")
        }

        async fn get_all_active(&self) -> Result<Vec<MarketAnomaly>> {
            bail!("unexpected get_all_active")
        }

        async fn clear_stale_period_data(
            &self,
            _clear_15m: bool,
            _clear_4h: bool,
            _clear_24h: bool,
        ) -> Result<()> {
            bail!("unexpected clear_stale_period_data")
        }

        async fn save_rank_event(&self, _event: &MarketRankEvent) -> Result<i64> {
            bail!("unexpected save_rank_event")
        }

        async fn upsert_market_velocity_episode(
            &self,
            _episode: &MarketVelocityEpisode,
        ) -> Result<(i64, MarketVelocityEpisodeWrite)> {
            bail!("unexpected upsert_market_velocity_episode")
        }

        async fn attach_rank_event_to_market_velocity_episode(
            &self,
            _episode_id: i64,
            _rank_event_id: i64,
            _escalated_at: DateTime<Utc>,
        ) -> Result<()> {
            bail!("unexpected attach_rank_event_to_market_velocity_episode")
        }

        async fn close_stale_market_velocity_episodes(
            &self,
            _exchange: &str,
            _stale_before: DateTime<Utc>,
        ) -> Result<u64> {
            bail!("unexpected close_stale_market_velocity_episodes")
        }

        async fn save_rank_snapshots(&self, _snapshots: &[MarketRankSnapshot]) -> Result<()> {
            bail!("unexpected save_rank_snapshots")
        }

        async fn load_rank_snapshots_for_restore(
            &self,
            _exchange: &str,
            _targets: &[DateTime<Utc>],
        ) -> Result<Vec<MarketRankSnapshot>> {
            bail!("unexpected load_rank_snapshots_for_restore")
        }

        async fn delete_rank_snapshots_before(
            &self,
            exchange: &str,
            before: DateTime<Utc>,
        ) -> Result<()> {
            self.deletes
                .lock()
                .expect("deletes lock")
                .push((exchange.to_string(), before));
            Ok(())
        }
    }

    fn test_repo() -> Arc<CapturingMarketAnomalyRepository> {
        Arc::new(CapturingMarketAnomalyRepository::new())
    }

    #[test]
    fn db_retention_uses_seven_days_in_production() {
        let now = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");
        assert_eq!(
            market_rank_snapshot_db_retention_start_for_env(now, Some("prod")),
            now - Duration::days(MARKET_RANK_SNAPSHOT_PRODUCTION_RETENTION_DAYS)
        );
        assert_eq!(
            market_rank_snapshot_db_retention_start_for_env(now, Some("production")),
            now - Duration::days(MARKET_RANK_SNAPSHOT_PRODUCTION_RETENTION_DAYS)
        );
    }

    #[test]
    fn db_retention_keeps_longer_history_locally() {
        let now = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");
        assert_eq!(
            market_rank_snapshot_db_retention_start_for_env(now, Some("local")),
            now - Duration::days(MARKET_RANK_SNAPSHOT_LOCAL_RETENTION_DAYS)
        );
        assert_eq!(
            market_rank_snapshot_db_retention_start_for_env(now, None),
            now - Duration::days(MARKET_RANK_SNAPSHOT_LOCAL_RETENTION_DAYS)
        );
    }

    #[test]
    fn prune_waits_until_daily_utc_window() {
        let before_window = DateTime::parse_from_rfc3339("2026-06-25T17:59:59Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);
        let at_window = DateTime::parse_from_rfc3339("2026-06-25T18:00:00Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);
        assert!(!market_rank_snapshot_prune_is_due(before_window, None));
        assert!(market_rank_snapshot_prune_is_due(at_window, None));
    }

    #[test]
    fn prune_does_not_run_late_when_daily_window_was_missed() {
        let after_window = DateTime::parse_from_rfc3339("2026-06-25T18:01:00Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);
        assert!(!market_rank_snapshot_prune_is_due(after_window, None));
    }

    #[test]
    fn prune_runs_once_per_utc_day() {
        let now = DateTime::parse_from_rfc3339("2026-06-25T18:00:30Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);
        let same_day = DateTime::parse_from_rfc3339("2026-06-25T18:00:00Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);
        let previous_day = DateTime::parse_from_rfc3339("2026-06-24T18:00:00Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);
        assert!(!market_rank_snapshot_prune_is_due(now, Some(same_day)));
        assert!(market_rank_snapshot_prune_is_due(now, Some(previous_day)));
    }

    #[tokio::test]
    async fn job_deletes_stale_snapshots_after_daily_window() {
        let now = DateTime::parse_from_rfc3339("2026-06-25T18:00:00Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);
        let repo = test_repo();
        let mut job = MarketRankSnapshotPruneJob::new("okx", repo.clone());

        let outcome = job.run_if_due(now).await.expect("job should run");

        assert_eq!(
            outcome,
            MarketRankSnapshotPruneOutcome::Pruned {
                retention_start: now - Duration::days(MARKET_RANK_SNAPSHOT_LOCAL_RETENTION_DAYS)
            }
        );
        assert_eq!(
            repo.deletes(),
            vec![(
                "okx".to_string(),
                now - Duration::days(MARKET_RANK_SNAPSHOT_LOCAL_RETENTION_DAYS)
            )]
        );
    }

    #[tokio::test]
    async fn job_skips_before_window_and_after_same_day_success() {
        let repo = test_repo();
        let mut job = MarketRankSnapshotPruneJob::new("okx", repo.clone());
        let before_window = DateTime::parse_from_rfc3339("2026-06-25T17:59:59Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);
        let first_window = DateTime::parse_from_rfc3339("2026-06-25T18:00:00Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);
        let same_day_later = DateTime::parse_from_rfc3339("2026-06-25T23:00:00Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);

        assert_eq!(
            job.run_if_due(before_window)
                .await
                .expect("job should skip"),
            MarketRankSnapshotPruneOutcome::Skipped
        );
        assert!(matches!(
            job.run_if_due(first_window).await.expect("job should run"),
            MarketRankSnapshotPruneOutcome::Pruned { .. }
        ));
        assert_eq!(
            job.run_if_due(same_day_later)
                .await
                .expect("job should skip"),
            MarketRankSnapshotPruneOutcome::Skipped
        );
        assert_eq!(repo.deletes().len(), 1);
    }
}
