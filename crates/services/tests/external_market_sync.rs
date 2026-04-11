use anyhow::Result;
use async_trait::async_trait;
use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_domain::traits::ExternalMarketSnapshotRepository;
use rust_quant_infrastructure::exchanges::{
    HyperliquidAssetContextSnapshot, HyperliquidFundingHistoryPoint,
};
use rust_quant_services::market::{
    normalize_external_market_symbol, ExternalMarketDataProvider, ExternalMarketSource,
    ExternalMarketSyncService,
};
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

struct FakeExternalMarketDataProvider;

#[async_trait]
impl ExternalMarketDataProvider for FakeExternalMarketDataProvider {
    async fn fetch_hyperliquid_funding_history(
        &self,
        coin: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<HyperliquidFundingHistoryPoint>> {
        assert_eq!(coin, "ETH");
        assert_eq!(start_time, 1774800000000_i64);
        assert_eq!(end_time, 1774814400062_i64);

        Ok(vec![HyperliquidFundingHistoryPoint {
            coin: "ETH".to_string(),
            funding_rate: 0.0000105495,
            premium: Some(-0.0004156042),
            time: 1774814400062_i64,
        }])
    }

    async fn fetch_hyperliquid_meta_and_asset_ctxs(
        &self,
        coin: &str,
    ) -> Result<HyperliquidAssetContextSnapshot> {
        assert_eq!(coin, "ETH");

        Ok(HyperliquidAssetContextSnapshot {
            coin: "ETH".to_string(),
            funding: Some(0.0000105495),
            open_interest: Some(98765.5),
            premium: Some(-0.0004156042),
            oracle_price: Some(1983.26),
            mark_price: Some(1985.11),
        })
    }
}

#[tokio::test]
async fn sync_hyperliquid_coin_converts_and_deduplicates_snapshots() {
    let repo = Arc::new(InMemoryExternalMarketSnapshotRepository::default());
    let provider = Arc::new(FakeExternalMarketDataProvider);
    let service = ExternalMarketSyncService::with_repo_and_provider(repo.clone(), provider);

    let saved = service
        .sync_hyperliquid_coin(
            "ETH",
            1774800000000_i64,
            1774814400062_i64,
            1774814400062_i64,
        )
        .await
        .expect("sync should succeed");
    assert_eq!(saved, 2);

    let saved_again = service
        .sync_hyperliquid_coin(
            "ETH",
            1774800000000_i64,
            1774814400062_i64,
            1774814400062_i64,
        )
        .await
        .expect("repeat sync should also succeed");
    assert_eq!(saved_again, 2);

    let funding_rows = repo
        .find_range(
            "hyperliquid",
            "ETH",
            "funding",
            1774800000000_i64,
            1774814400062_i64,
            Some(10),
        )
        .await
        .expect("funding rows should exist");
    assert_eq!(funding_rows.len(), 1);
    assert_eq!(funding_rows[0].funding_rate, Some(0.0000105495));
    assert_eq!(funding_rows[0].premium, Some(-0.0004156042));
    assert_eq!(
        funding_rows[0].raw_payload,
        Some(json!({
            "coin": "ETH",
            "funding_rate": 0.0000105495,
            "premium": -0.0004156042,
            "time": 1774814400062_i64
        }))
    );

    let meta_rows = repo
        .find_range(
            "hyperliquid",
            "ETH",
            "meta",
            1774814400062_i64,
            1774814400062_i64,
            Some(10),
        )
        .await
        .expect("meta rows should exist");
    assert_eq!(meta_rows.len(), 1);
    assert_eq!(meta_rows[0].open_interest, Some(98765.5));
    assert_eq!(meta_rows[0].mark_price, Some(1985.11));
    assert_eq!(
        meta_rows[0].raw_payload,
        Some(json!({
            "coin": "ETH",
            "funding": 0.0000105495,
            "open_interest": 98765.5,
            "premium": -0.0004156042,
            "oracle_price": 1983.26,
            "mark_price": 1985.11
        }))
    );
}

#[test]
fn normalize_external_market_symbol_extracts_base_coin() {
    assert_eq!(normalize_external_market_symbol("ETH-USDT-SWAP"), "ETH");
    assert_eq!(normalize_external_market_symbol("ethusdt"), "ETH");
    assert_eq!(normalize_external_market_symbol("BTC"), "BTC");
}

#[test]
fn external_market_source_names_are_stable() {
    assert_eq!(ExternalMarketSource::Hyperliquid.as_str(), "hyperliquid");
    assert_eq!(ExternalMarketSource::Okx.as_str(), "okx");
    assert_eq!(ExternalMarketSource::Binance.as_str(), "binance");
}
