use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::time::{interval, Duration, MissedTickBehavior};
use tracing::{error, info};

#[async_trait]
pub trait MaintenanceJob: Send {
    fn name(&self) -> &'static str;

    async fn run_tick(&mut self, now: DateTime<Utc>) -> Result<()>;
}

pub struct MaintenanceScheduler {
    jobs: Vec<Box<dyn MaintenanceJob>>,
    tick_interval: Duration,
}

impl MaintenanceScheduler {
    pub fn new(tick_interval: Duration) -> Self {
        Self {
            jobs: Vec::new(),
            tick_interval,
        }
    }

    pub fn register_job<J>(&mut self, job: J)
    where
        J: MaintenanceJob + 'static,
    {
        self.jobs.push(Box::new(job));
    }

    pub fn job_count(&self) -> usize {
        self.jobs.len()
    }

    pub async fn run_forever(mut self) {
        let mut ticker = interval(self.tick_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        info!(
            "Core maintenance scheduler started: jobs={}, tick_interval_secs={}",
            self.jobs.len(),
            self.tick_interval.as_secs()
        );
        loop {
            ticker.tick().await;
            self.run_once(Utc::now()).await;
        }
    }

    async fn run_once(&mut self, now: DateTime<Utc>) {
        for job in &mut self.jobs {
            let job_name = job.name();
            if let Err(err) = job.run_tick(now).await {
                error!(
                    "Core maintenance job failed: job={}, error={:?}",
                    job_name, err
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct RecordingJob {
        calls: Arc<Mutex<Vec<DateTime<Utc>>>>,
    }

    #[async_trait]
    impl MaintenanceJob for RecordingJob {
        fn name(&self) -> &'static str {
            "recording"
        }

        async fn run_tick(&mut self, now: DateTime<Utc>) -> Result<()> {
            self.calls.lock().expect("calls lock").push(now);
            Ok(())
        }
    }

    #[tokio::test]
    async fn scheduler_runs_registered_logical_jobs() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut scheduler = MaintenanceScheduler::new(Duration::from_secs(60));
        scheduler.register_job(RecordingJob {
            calls: Arc::clone(&calls),
        });
        assert_eq!(scheduler.job_count(), 1);

        let now = DateTime::parse_from_rfc3339("2026-06-25T18:00:00Z")
            .expect("valid test timestamp")
            .with_timezone(&Utc);
        scheduler.run_once(now).await;

        assert_eq!(*calls.lock().expect("calls lock"), vec![now]);
    }
}
