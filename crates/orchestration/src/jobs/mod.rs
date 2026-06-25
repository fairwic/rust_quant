pub mod data;
pub mod maintenance;
pub mod risk;

#[cfg(test)]
mod maintenance_contract_tests {
    use std::fs;
    use std::path::Path;

    #[test]
    fn maintenance_scheduler_registers_logical_jobs_in_one_process() {
        let scheduler_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/jobs/maintenance/scheduler.rs");
        let source = fs::read_to_string(&scheduler_path).unwrap_or_else(|error| {
            panic!("failed to read {}: {}", scheduler_path.display(), error)
        });
        assert!(
            source.contains("pub trait MaintenanceJob"),
            "maintenance scheduler should expose a common logical job contract"
        );
        assert!(
            source.contains("Vec<Box<dyn MaintenanceJob>>"),
            "maintenance scheduler should register multiple logical jobs without new containers"
        );
    }

    #[test]
    fn market_rank_snapshot_prune_is_a_maintenance_job() {
        let job_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/jobs/maintenance/market_rank_snapshot_prune_job.rs");
        let source = fs::read_to_string(&job_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {}", job_path.display(), error));
        assert!(
            source.contains("pub struct MarketRankSnapshotPruneJob"),
            "market rank snapshot pruning should be a logical maintenance job"
        );
        assert!(
            source.contains("impl MaintenanceJob for MarketRankSnapshotPruneJob"),
            "market rank snapshot pruning should be registered through the maintenance scheduler contract"
        );
    }
}
