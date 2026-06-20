use anyhow::{anyhow, Result};
use rust_quant_core::database::get_db_pool;
use rust_quant_domain::entities::ExternalMarketSnapshot;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use std::collections::HashMap;

use super::report::{render_path_impact_report, render_report};
use super::types::{
    FactorBucketReport, FactorConclusion, PathImpactQuery, PathImpactReport, PathImpactSummary,
    PathImpactTradeChange, PriceOiState, ResearchFilteredSignalSample, ResearchSampleKind,
    ResearchTradeSample, VegasFactorResearchQuery, VegasFactorResearchReport, VolatilityTier,
};

const FOUR_HOURS_MS: i64 = 4 * 60 * 60 * 1000;

#[derive(Debug, Clone)]
struct EnrichedTradeSample {
    trade: ResearchTradeSample,
    tier: VolatilityTier,
    funding_bucket: Option<String>,
    price_oi_state: Option<PriceOiState>,
    flow_bucket: Option<String>,
}

#[derive(Debug, Clone)]
struct EnrichedFilteredSignalSample {
    signal: ResearchFilteredSignalSample,
    tier: VolatilityTier,
    funding_bucket: Option<String>,
    price_oi_state: Option<PriceOiState>,
    flow_bucket: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct TradeSampleRow {
    backtest_id: i64,
    inst_id: String,
    timeframe: String,
    side: String,
    open_time: chrono::NaiveDateTime,
    close_time: Option<chrono::NaiveDateTime>,
    pnl: f64,
    close_type: Option<String>,
    stop_loss_source: Option<String>,
    signal_value: Option<String>,
    signal_result: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct SnapshotRow {
    id: i64,
    source: String,
    symbol: String,
    metric_type: String,
    metric_time: i64,
    funding_rate: Option<f64>,
    premium: Option<f64>,
    open_interest: Option<f64>,
    oracle_price: Option<f64>,
    mark_price: Option<f64>,
    long_short_ratio: Option<f64>,
    raw_payload: Option<sqlx::types::Json<serde_json::Value>>,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, FromRow)]
struct FundingRateRow {
    inst_id: String,
    funding_time: i64,
    funding_rate: Option<f64>,
    premium: Option<f64>,
}

#[derive(Debug, Clone, FromRow)]
struct FilteredSignalSampleRow {
    backtest_id: i64,
    inst_id: String,
    timeframe: String,
    direction: String,
    signal_time: chrono::NaiveDateTime,
    theoretical_pnl: Option<f64>,
    trade_result: Option<String>,
    filter_reasons: Option<String>,
    signal_value: Option<String>,
}

pub struct VegasFactorResearchService {
    pool: PgPool,
}

impl VegasFactorResearchService {
    pub fn new() -> Result<Self> {
        Ok(Self {
            pool: get_db_pool().clone(),
        })
    }

    pub fn with_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn render_report(
        trades: &[ResearchTradeSample],
        filtered_signals: &[ResearchFilteredSignalSample],
        buckets: &[FactorBucketReport],
    ) -> String {
        render_report(trades, filtered_signals, buckets)
    }

    pub async fn run_report(
        &self,
        query: VegasFactorResearchQuery,
    ) -> Result<VegasFactorResearchReport> {
        let trades = self.load_trade_samples(&query).await?;
        let filtered_signal_samples = self.load_filtered_signal_samples(&query).await?;
        let snapshots = self
            .load_snapshots(&trades, &filtered_signal_samples)
            .await?;
        let enriched = self.enrich_samples(trades.clone(), &snapshots);
        let enriched_filtered =
            self.enrich_filtered_signals(filtered_signal_samples.clone(), &snapshots);
        let factor_buckets = self.build_bucket_reports(&enriched, &enriched_filtered);
        Ok(VegasFactorResearchReport {
            trade_samples: trades,
            filtered_signal_samples,
            factor_buckets,
        })
    }

    pub async fn run_report_text(&self, query: VegasFactorResearchQuery) -> Result<String> {
        let report = self.run_report(query).await?;
        Ok(render_report(
            &report.trade_samples,
            &report.filtered_signal_samples,
            &report.factor_buckets,
        ))
    }

    pub async fn run_path_impact_report(&self, query: PathImpactQuery) -> Result<PathImpactReport> {
        if query.experiment_ids.is_empty() {
            return Err(anyhow!("experiment ids 不能为空"));
        }

        let mut ids = vec![query.baseline_id];
        ids.extend(query.experiment_ids.iter().copied());
        let samples = self
            .load_trade_samples_by_ids(&ids, &query.timeframe, query.inst_id.as_deref())
            .await?;
        let baseline: Vec<_> = samples
            .iter()
            .filter(|row| row.backtest_id == query.baseline_id)
            .cloned()
            .collect();
        let mut summaries = Vec::new();
        for experiment_id in query.experiment_ids {
            let experiment: Vec<_> = samples
                .iter()
                .filter(|row| row.backtest_id == experiment_id)
                .cloned()
                .collect();
            let mut summary = Self::summarize_path_impact(
                query.baseline_id,
                experiment_id,
                &baseline,
                &experiment,
                query.top_changed_limit,
            );
            if query.inst_id.is_some() {
                summary.inst_id = query.inst_id.clone();
            }
            summaries.push(summary);
        }

        Ok(PathImpactReport { summaries })
    }

    pub async fn run_path_impact_report_text(&self, query: PathImpactQuery) -> Result<String> {
        let report = self.run_path_impact_report(query).await?;
        Ok(render_path_impact_report(&report.summaries))
    }

    async fn load_trade_samples(
        &self,
        query: &VegasFactorResearchQuery,
    ) -> Result<Vec<ResearchTradeSample>> {
        if query.baseline_ids.is_empty() {
            return Err(anyhow!("baseline ids 不能为空"));
        }

        self.load_trade_samples_by_ids(&query.baseline_ids, &query.timeframe, None)
            .await
    }

    async fn load_trade_samples_by_ids(
        &self,
        backtest_ids: &[i64],
        timeframe: &str,
        inst_id: Option<&str>,
    ) -> Result<Vec<ResearchTradeSample>> {
        if backtest_ids.is_empty() {
            return Err(anyhow!("backtest ids 不能为空"));
        }
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT o.back_test_id as backtest_id, o.inst_id, o.time as timeframe, o.option_type as side, \
             o.open_position_time as open_time, c.close_position_time as close_time, \
             COALESCE(NULLIF(c.profit_loss, ''), NULLIF(o.profit_loss, ''))::double precision as pnl, c.close_type, c.stop_loss_source, \
             NULLIF(o.signal_value, '') as signal_value, NULLIF(o.signal_result, '') as signal_result \
             FROM back_test_detail o \
             LEFT JOIN back_test_detail c \
             ON c.back_test_id = o.back_test_id AND c.inst_id = o.inst_id \
             AND c.option_type = 'close' AND c.open_position_time = o.open_position_time \
             WHERE o.option_type IN ('long','short') AND o.time = "
        );
        builder.push_bind(timeframe);
        builder.push(" AND o.back_test_id IN (");
        let mut separated = builder.separated(", ");
        for id in backtest_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
        if let Some(inst_id) = inst_id {
            builder.push(" AND o.inst_id = ");
            builder.push_bind(inst_id);
        }

        let rows = builder
            .build_query_as::<TradeSampleRow>()
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(Self::row_to_trade_sample).collect())
    }

    async fn load_filtered_signal_samples(
        &self,
        query: &VegasFactorResearchQuery,
    ) -> Result<Vec<ResearchFilteredSignalSample>> {
        if query.baseline_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT backtest_id, inst_id, period as timeframe, direction, signal_time, \
             final_pnl::double precision as theoretical_pnl, trade_result, \
             filter_reasons::text as filter_reasons, signal_value::text as signal_value \
             FROM filtered_signal_log WHERE period = ",
        );
        builder.push_bind(&query.timeframe);
        builder.push(" AND backtest_id IN (");
        let mut separated = builder.separated(", ");
        for id in &query.baseline_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let rows = builder
            .build_query_as::<FilteredSignalSampleRow>()
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| ResearchFilteredSignalSample {
                backtest_id: row.backtest_id,
                inst_id: row.inst_id,
                timeframe: row.timeframe,
                direction: row.direction,
                signal_time_ms: row.signal_time.and_utc().timestamp_millis(),
                theoretical_pnl: row.theoretical_pnl,
                trade_result: row.trade_result,
                filter_reasons: row.filter_reasons,
                signal_value: row.signal_value,
            })
            .collect())
    }

    async fn load_snapshots(
        &self,
        trades: &[ResearchTradeSample],
        filtered_signals: &[ResearchFilteredSignalSample],
    ) -> Result<Vec<ExternalMarketSnapshot>> {
        if trades.is_empty() && filtered_signals.is_empty() {
            return Ok(Vec::new());
        }

        let symbols: Vec<_> = trades
            .iter()
            .map(|row| {
                row.inst_id
                    .split('-')
                    .next()
                    .unwrap_or(&row.inst_id)
                    .to_string()
            })
            .chain(filtered_signals.iter().map(|row| {
                row.inst_id
                    .split('-')
                    .next()
                    .unwrap_or(&row.inst_id)
                    .to_string()
            }))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let min_time = trades
            .iter()
            .map(|row| row.open_time_ms)
            .chain(filtered_signals.iter().map(|row| row.signal_time_ms))
            .min()
            .unwrap_or_default()
            - FOUR_HOURS_MS;
        let max_time = trades
            .iter()
            .map(|row| row.open_time_ms)
            .chain(filtered_signals.iter().map(|row| row.signal_time_ms))
            .max()
            .unwrap_or_default();

        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, source, symbol, metric_type, metric_time, \
             NULLIF(funding_rate, '')::double precision as funding_rate, \
             NULLIF(premium, '')::double precision as premium, \
             NULLIF(open_interest, '')::double precision as open_interest, \
             NULLIF(oracle_price, '')::double precision as oracle_price, \
             NULLIF(mark_price, '')::double precision as mark_price, \
             NULLIF(long_short_ratio, '')::double precision as long_short_ratio, \
             raw_payload, created_at, updated_at \
             FROM external_market_snapshots WHERE metric_time >= ",
        );
        builder.push_bind(min_time);
        builder.push(" AND metric_time <= ");
        builder.push_bind(max_time);
        builder.push(" AND symbol IN (");
        let mut separated = builder.separated(", ");
        for symbol in &symbols {
            separated.push_bind(symbol);
        }
        separated.push_unseparated(") ORDER BY symbol, metric_type, metric_time ASC");

        let mut snapshots: Vec<_> = builder
            .build_query_as::<SnapshotRow>()
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(Self::row_to_snapshot)
            .collect();
        snapshots.extend(
            self.load_funding_rate_snapshots(&symbols, min_time, max_time)
                .await?,
        );
        Ok(snapshots)
    }

    async fn load_funding_rate_snapshots(
        &self,
        symbols: &[String],
        min_time: i64,
        max_time: i64,
    ) -> Result<Vec<ExternalMarketSnapshot>> {
        let inst_ids: Vec<_> = symbols
            .iter()
            .map(|symbol| format!("{}-USDT-SWAP", symbol))
            .collect();
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT inst_id, funding_time, NULLIF(funding_rate, '')::double precision as funding_rate, \
             NULLIF(premium, '')::double precision as premium FROM funding_rates WHERE funding_time >= ",
        );
        builder.push_bind(min_time);
        builder.push(" AND funding_time <= ");
        builder.push_bind(max_time);
        builder.push(" AND inst_id IN (");
        let mut separated = builder.separated(", ");
        for inst_id in &inst_ids {
            separated.push_bind(inst_id);
        }
        separated.push_unseparated(")");

        let rows = builder
            .build_query_as::<FundingRateRow>()
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let mut snapshot = ExternalMarketSnapshot::new(
                    "okx".to_string(),
                    row.inst_id
                        .split('-')
                        .next()
                        .unwrap_or(&row.inst_id)
                        .to_string(),
                    "funding_rate".to_string(),
                    row.funding_time,
                );
                snapshot.funding_rate = row.funding_rate;
                snapshot.premium = row.premium;
                snapshot
            })
            .collect())
    }

    fn row_to_trade_sample(row: TradeSampleRow) -> ResearchTradeSample {
        ResearchTradeSample {
            backtest_id: row.backtest_id,
            inst_id: row.inst_id,
            timeframe: row.timeframe,
            side: row.side,
            open_time_ms: row.open_time.and_utc().timestamp_millis(),
            close_time_ms: row
                .close_time
                .map(|value| value.and_utc().timestamp_millis()),
            pnl: row.pnl,
            close_type: row.close_type,
            stop_loss_source: row.stop_loss_source,
            signal_value: row.signal_value,
            signal_result: row.signal_result,
        }
    }
}
include!("service/classification_section.rs");
include!("service/path_impact_section.rs");
include!("service/enrichment_section.rs");
include!("service/bucket_reports_section.rs");
