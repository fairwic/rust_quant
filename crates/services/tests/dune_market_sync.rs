use anyhow::Result;
use async_trait::async_trait;
use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_domain::traits::ExternalMarketSnapshotRepository;
use rust_quant_infrastructure::external_data::DuneQueryPerformance;
use rust_quant_services::market::{DuneMarketSyncService, DuneSqlRunner};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct InMemoryExternalMarketSnapshotRepository {
    rows: Mutex<HashMap<(String, String, String, i64), ExternalMarketSnapshot>>,
}

#[async_trait]
impl ExternalMarketSnapshotRepository for InMemoryExternalMarketSnapshotRepository {
    async fn save(&self, snapshot: ExternalMarketSnapshot) -> Result<()> {
        let key = (
            snapshot.source.clone(),
            snapshot.symbol.clone(),
            snapshot.metric_type.clone(),
            snapshot.metric_time,
        );
        self.rows.lock().unwrap().insert(key, snapshot);
        Ok(())
    }

    async fn save_batch(&self, snapshots: Vec<ExternalMarketSnapshot>) -> Result<()> {
        for snapshot in snapshots {
            self.save(snapshot).await?;
        }
        Ok(())
    }

    async fn find_range(
        &self,
        source: &str,
        symbol: &str,
        metric_type: &str,
        start_time: i64,
        end_time: i64,
        limit: Option<i64>,
    ) -> Result<Vec<ExternalMarketSnapshot>> {
        let limit = limit.unwrap_or(i64::MAX) as usize;
        let mut rows: Vec<_> = self
            .rows
            .lock()
            .unwrap()
            .values()
            .filter(|row| {
                row.source == source
                    && row.symbol == symbol
                    && row.metric_type == metric_type
                    && row.metric_time >= start_time
                    && row.metric_time <= end_time
            })
            .cloned()
            .collect();
        rows.sort_by_key(|row| row.metric_time);
        rows.truncate(limit);
        Ok(rows)
    }
}

struct FakeDuneSqlRunner;

#[async_trait]
impl DuneSqlRunner for FakeDuneSqlRunner {
    async fn run_sql(
        &self,
        sql: &str,
        performance: DuneQueryPerformance,
    ) -> Result<Vec<serde_json::Value>> {
        assert_eq!(performance, DuneQueryPerformance::Medium);
        assert!(sql.contains("ETH"));
        assert!(sql.contains("2026-03-30T00:00:00Z"));
        assert!(sql.contains("2026-03-30T08:00:00Z"));
        assert!(sql.contains("100000"));

        Ok(vec![json!({
            "hour_bucket": "2026-03-30T04:00:00Z",
            "funding_rate": 0.0000123,
            "open_interest_usd": 456789.0,
            "premium_bps": -4.156042,
            "netflow_usd": 123456.78
        })])
    }
}

#[tokio::test]
async fn sync_dune_template_renders_params_and_saves_snapshots() {
    let repo = Arc::new(InMemoryExternalMarketSnapshotRepository::default());
    let runner = Arc::new(FakeDuneSqlRunner);
    let service = DuneMarketSyncService::with_repo_and_runner(repo.clone(), runner);

    let sql_template = r#"
        select * from some_table
        where symbol = '{{symbol}}'
          and block_time >= cast('{{start_time}}' as timestamp)
          and block_time < cast('{{end_time}}' as timestamp)
          and amount_usd >= cast('{{min_usd}}' as double)
    "#;

    let mut params = HashMap::new();
    params.insert("symbol".to_string(), "ETH".to_string());
    params.insert("start_time".to_string(), "2026-03-30T00:00:00Z".to_string());
    params.insert("end_time".to_string(), "2026-03-30T08:00:00Z".to_string());
    params.insert("min_usd".to_string(), "100000".to_string());

    let saved = service
        .sync_rendered_sql(
            "hyperliquid_basis".to_string(),
            "ETH".to_string(),
            sql_template.to_string(),
            params,
            DuneQueryPerformance::Medium,
        )
        .await
        .expect("sync should succeed");

    assert_eq!(saved, 1);

    let rows = repo
        .find_range(
            "dune",
            "ETH",
            "hyperliquid_basis",
            1774843200000_i64,
            1774843200000_i64,
            Some(10),
        )
        .await
        .expect("rows should exist");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].funding_rate, Some(0.0000123));
    assert_eq!(rows[0].open_interest, Some(456789.0));
    let premium = rows[0].premium.expect("premium should exist");
    assert!((premium - (-0.0004156042)).abs() < 1e-12);
    assert_eq!(
        rows[0].raw_payload,
        Some(json!({
            "hour_bucket": "2026-03-30T04:00:00Z",
            "funding_rate": 0.0000123,
            "open_interest_usd": 456789.0,
            "premium_bps": -4.156042,
            "netflow_usd": 123456.78
        }))
    );
}

#[tokio::test]
async fn sync_dune_template_parses_dune_utc_timestamp_format() {
    struct UtcFormatRunner;

    #[async_trait]
    impl DuneSqlRunner for UtcFormatRunner {
        async fn run_sql(
            &self,
            _sql: &str,
            _performance: DuneQueryPerformance,
        ) -> Result<Vec<serde_json::Value>> {
            Ok(vec![json!({
                "hour_bucket": "2026-02-21 20:00:00.000 UTC",
                "funding_rate": 0.0000021,
                "premium": -0.00055,
                "open_interest": 543609.68
            })])
        }
    }

    let repo = Arc::new(InMemoryExternalMarketSnapshotRepository::default());
    let runner = Arc::new(UtcFormatRunner);
    let service = DuneMarketSyncService::with_repo_and_runner(repo.clone(), runner);

    let saved = service
        .sync_rendered_sql(
            "hyperliquid_basis".to_string(),
            "ETH".to_string(),
            "select 1".to_string(),
            HashMap::new(),
            DuneQueryPerformance::Medium,
        )
        .await
        .expect("sync should parse dune UTC timestamp format");

    assert_eq!(saved, 1);

    let rows = repo
        .find_range(
            "dune",
            "ETH",
            "hyperliquid_basis",
            1771704000000_i64,
            1771704000000_i64,
            Some(10),
        )
        .await
        .expect("rows should exist");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].funding_rate, Some(0.0000021));
}
