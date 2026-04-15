use anyhow::{anyhow, Result};
use rust_quant_core::database::get_db_pool;
use rust_quant_domain::entities::ExternalMarketSnapshot;
use sqlx::{FromRow, MySql, Pool, QueryBuilder};
use std::collections::HashMap;

use super::report::render_report;
use super::types::{
    FactorBucketReport, FactorConclusion, PriceOiState, ResearchFilteredSignalSample,
    ResearchSampleKind, ResearchTradeSample, VegasFactorResearchQuery, VegasFactorResearchReport,
    VolatilityTier,
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
    pool: Pool<MySql>,
}

impl VegasFactorResearchService {
    const SUPPORTED_FACTORS: [&'static str; 3] =
        ["funding_premium_divergence", "price_oi_state", "flow_proxy"];

    pub fn new() -> Result<Self> {
        Ok(Self {
            pool: get_db_pool().clone(),
        })
    }

    pub fn with_pool(pool: Pool<MySql>) -> Self {
        Self { pool }
    }

    pub fn align_latest_snapshot(
        event_time: i64,
        snapshots: &[ExternalMarketSnapshot],
    ) -> Option<&ExternalMarketSnapshot> {
        snapshots
            .iter()
            .filter(|row| {
                row.metric_time <= event_time && event_time - row.metric_time <= FOUR_HOURS_MS
            })
            .max_by_key(|row| row.metric_time)
    }

    pub fn classify_price_oi_state(
        price_change: Option<f64>,
        oi_change: Option<f64>,
    ) -> PriceOiState {
        match (price_change, oi_change) {
            (Some(price), Some(oi)) if price > 0.0 && oi > 0.0 => PriceOiState::LongBuildup,
            (Some(price), Some(oi)) if price < 0.0 && oi > 0.0 => PriceOiState::ShortBuildup,
            (Some(price), Some(oi)) if price > 0.0 && oi < 0.0 => PriceOiState::ShortCovering,
            (Some(price), Some(oi)) if price < 0.0 && oi < 0.0 => PriceOiState::LongUnwinding,
            _ => PriceOiState::Flat,
        }
    }

    pub fn evaluate_factor_conclusion(reports: &[FactorBucketReport]) -> FactorConclusion {
        let eth_rows: Vec<_> = reports
            .iter()
            .filter(|row| row.volatility_tier == VolatilityTier::Eth)
            .collect();
        if eth_rows
            .iter()
            .any(|row| row.sample_count >= 3 && row.sharpe_proxy > 1.0 && row.avg_pnl > 0.0)
        {
            return FactorConclusion::Candidate;
        }
        if eth_rows
            .iter()
            .any(|row| row.sample_count >= 3 && (row.sharpe_proxy > 0.0 || row.avg_pnl > 0.0))
        {
            return FactorConclusion::Observe;
        }
        FactorConclusion::Reject
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

    async fn load_trade_samples(
        &self,
        query: &VegasFactorResearchQuery,
    ) -> Result<Vec<ResearchTradeSample>> {
        if query.baseline_ids.is_empty() {
            return Err(anyhow!("baseline ids 不能为空"));
        }

        let mut builder = QueryBuilder::<MySql>::new(
            "SELECT o.back_test_id as backtest_id, o.inst_id, o.time as timeframe, o.option_type as side, \
             o.open_position_time as open_time, c.close_position_time as close_time, \
             CAST(COALESCE(NULLIF(c.profit_loss, ''), NULLIF(o.profit_loss, '')) AS DOUBLE) as pnl, c.close_type, c.stop_loss_source, \
             NULLIF(o.signal_value, '') as signal_value, NULLIF(o.signal_result, '') as signal_result \
             FROM back_test_detail o \
             LEFT JOIN back_test_detail c \
             ON c.back_test_id = o.back_test_id AND c.inst_id = o.inst_id \
             AND c.option_type = 'close' AND c.open_position_time = o.open_position_time \
             WHERE o.option_type IN ('long','short') AND o.time = "
        );
        builder.push_bind(&query.timeframe);
        builder.push(" AND o.back_test_id IN (");
        let mut separated = builder.separated(", ");
        for id in &query.baseline_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

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

        let mut builder = QueryBuilder::<MySql>::new(
            "SELECT backtest_id, inst_id, period as timeframe, direction, signal_time, \
             CAST(final_pnl AS DOUBLE) as theoretical_pnl, trade_result, \
             CAST(filter_reasons AS CHAR) as filter_reasons, CAST(signal_value AS CHAR) as signal_value \
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

        let mut builder = QueryBuilder::<MySql>::new(
            "SELECT id, source, symbol, metric_type, metric_time, \
             CAST(NULLIF(funding_rate, '') AS DOUBLE) as funding_rate, \
             CAST(NULLIF(premium, '') AS DOUBLE) as premium, \
             CAST(NULLIF(open_interest, '') AS DOUBLE) as open_interest, \
             CAST(NULLIF(oracle_price, '') AS DOUBLE) as oracle_price, \
             CAST(NULLIF(mark_price, '') AS DOUBLE) as mark_price, \
             CAST(NULLIF(long_short_ratio, '') AS DOUBLE) as long_short_ratio, \
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
        let mut builder = QueryBuilder::<MySql>::new(
            "SELECT inst_id, funding_time, CAST(NULLIF(funding_rate, '') AS DOUBLE) as funding_rate, \
             CAST(NULLIF(premium, '') AS DOUBLE) as premium FROM funding_rates WHERE funding_time >= ",
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

    fn row_to_snapshot(row: SnapshotRow) -> ExternalMarketSnapshot {
        ExternalMarketSnapshot {
            id: Some(row.id),
            source: row.source,
            symbol: row.symbol,
            metric_type: row.metric_type,
            metric_time: row.metric_time,
            funding_rate: row.funding_rate,
            premium: row.premium,
            open_interest: row.open_interest,
            oracle_price: row.oracle_price,
            mark_price: row.mark_price,
            long_short_ratio: row.long_short_ratio,
            raw_payload: row.raw_payload.map(|value| value.0),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }

    fn enrich_samples(
        &self,
        trades: Vec<ResearchTradeSample>,
        snapshots: &[ExternalMarketSnapshot],
    ) -> Vec<EnrichedTradeSample> {
        let grouped = Self::group_snapshots(snapshots);
        trades
            .into_iter()
            .map(|trade| self.enrich_trade(trade, &grouped))
            .collect()
    }

    fn enrich_filtered_signals(
        &self,
        filtered_signals: Vec<ResearchFilteredSignalSample>,
        snapshots: &[ExternalMarketSnapshot],
    ) -> Vec<EnrichedFilteredSignalSample> {
        let grouped = Self::group_snapshots(snapshots);
        filtered_signals
            .into_iter()
            .map(|signal| self.enrich_filtered_signal(signal, &grouped))
            .collect()
    }

    fn enrich_trade(
        &self,
        trade: ResearchTradeSample,
        grouped: &HashMap<String, Vec<ExternalMarketSnapshot>>,
    ) -> EnrichedTradeSample {
        let symbol = trade
            .inst_id
            .split('-')
            .next()
            .unwrap_or(&trade.inst_id)
            .to_string();
        let funding_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(trade.open_time_ms, rows, |row| {
                row.funding_rate.is_some() || row.premium.is_some()
            })
        });
        let price_oi_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(trade.open_time_ms, rows, |row| {
                row.open_interest.is_some() && Self::snapshot_price(row).is_some()
            })
        });
        let previous_price_oi_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::previous_matching_snapshot(trade.open_time_ms, rows, |row| {
                row.open_interest.is_some() && Self::snapshot_price(row).is_some()
            })
        });
        let flow_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(trade.open_time_ms, rows, |row| {
                row.raw_payload
                    .as_ref()
                    .and_then(Self::extract_flow_value)
                    .is_some()
            })
        });
        let price_change = Self::pct_change(
            price_oi_snapshot.and_then(Self::snapshot_price),
            previous_price_oi_snapshot.and_then(Self::snapshot_price),
        );
        let oi_change = Self::pct_change(
            price_oi_snapshot.and_then(|row| row.open_interest),
            previous_price_oi_snapshot.and_then(|row| row.open_interest),
        );

        EnrichedTradeSample {
            tier: VolatilityTier::from_symbol(&trade.inst_id),
            funding_bucket: Self::funding_bucket(funding_snapshot),
            price_oi_state: match (price_change, oi_change) {
                (Some(_), Some(_)) => Some(Self::classify_price_oi_state(price_change, oi_change)),
                _ => None,
            },
            flow_bucket: Self::flow_bucket(flow_snapshot),
            trade,
        }
    }

    fn enrich_filtered_signal(
        &self,
        signal: ResearchFilteredSignalSample,
        grouped: &HashMap<String, Vec<ExternalMarketSnapshot>>,
    ) -> EnrichedFilteredSignalSample {
        let symbol = signal
            .inst_id
            .split('-')
            .next()
            .unwrap_or(&signal.inst_id)
            .to_string();
        let funding_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(signal.signal_time_ms, rows, |row| {
                row.funding_rate.is_some() || row.premium.is_some()
            })
        });
        let price_oi_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(signal.signal_time_ms, rows, |row| {
                row.open_interest.is_some() && Self::snapshot_price(row).is_some()
            })
        });
        let previous_price_oi_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::previous_matching_snapshot(signal.signal_time_ms, rows, |row| {
                row.open_interest.is_some() && Self::snapshot_price(row).is_some()
            })
        });
        let flow_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(signal.signal_time_ms, rows, |row| {
                row.raw_payload
                    .as_ref()
                    .and_then(Self::extract_flow_value)
                    .is_some()
            })
        });
        let price_change = Self::pct_change(
            price_oi_snapshot.and_then(Self::snapshot_price),
            previous_price_oi_snapshot.and_then(Self::snapshot_price),
        );
        let oi_change = Self::pct_change(
            price_oi_snapshot.and_then(|row| row.open_interest),
            previous_price_oi_snapshot.and_then(|row| row.open_interest),
        );

        EnrichedFilteredSignalSample {
            tier: VolatilityTier::from_symbol(&signal.inst_id),
            funding_bucket: Self::funding_bucket(funding_snapshot),
            price_oi_state: match (price_change, oi_change) {
                (Some(_), Some(_)) => Some(Self::classify_price_oi_state(price_change, oi_change)),
                _ => None,
            },
            flow_bucket: Self::flow_bucket(flow_snapshot),
            signal,
        }
    }

    fn group_snapshots(
        snapshots: &[ExternalMarketSnapshot],
    ) -> HashMap<String, Vec<ExternalMarketSnapshot>> {
        let mut grouped: HashMap<String, Vec<ExternalMarketSnapshot>> = HashMap::new();
        for snapshot in snapshots {
            grouped
                .entry(snapshot.symbol.clone())
                .or_default()
                .push(snapshot.clone());
        }
        grouped
    }

    fn latest_matching_snapshot<F>(
        event_time: i64,
        snapshots: &[ExternalMarketSnapshot],
        predicate: F,
    ) -> Option<&ExternalMarketSnapshot>
    where
        F: Fn(&ExternalMarketSnapshot) -> bool,
    {
        snapshots
            .iter()
            .filter(|row| predicate(row))
            .filter(|row| {
                row.metric_time <= event_time && event_time - row.metric_time <= FOUR_HOURS_MS
            })
            .max_by_key(|row| row.metric_time)
    }

    fn previous_matching_snapshot<F>(
        event_time: i64,
        snapshots: &[ExternalMarketSnapshot],
        predicate: F,
    ) -> Option<&ExternalMarketSnapshot>
    where
        F: Fn(&ExternalMarketSnapshot) -> bool,
    {
        snapshots
            .iter()
            .filter(|row| predicate(row))
            .filter(|row| {
                row.metric_time < event_time && event_time - row.metric_time <= FOUR_HOURS_MS * 2
            })
            .max_by_key(|row| row.metric_time)
    }

    fn snapshot_price(snapshot: &ExternalMarketSnapshot) -> Option<f64> {
        snapshot.mark_price.or(snapshot.oracle_price)
    }

    fn pct_change(current: Option<f64>, previous: Option<f64>) -> Option<f64> {
        match (current, previous) {
            (Some(now), Some(prev)) if prev.abs() > f64::EPSILON => Some((now - prev) / prev),
            _ => None,
        }
    }

    fn funding_bucket(snapshot: Option<&ExternalMarketSnapshot>) -> Option<String> {
        match snapshot.and_then(|row| Some((row.funding_rate, row.premium))) {
            Some((Some(funding), Some(premium))) if funding > 0.0 && premium > 0.0 => {
                Some("long_crowded".to_string())
            }
            Some((Some(funding), Some(premium))) if funding < 0.0 && premium < 0.0 => {
                Some("short_crowded".to_string())
            }
            Some((Some(funding), Some(premium))) if funding < 0.0 && premium > 0.0 => {
                Some("divergent_bull".to_string())
            }
            Some((Some(funding), Some(premium))) if funding > 0.0 && premium < 0.0 => {
                Some("divergent_bear".to_string())
            }
            Some((Some(funding), _)) if funding >= 0.0 => Some("funding_positive".to_string()),
            Some((Some(_), _)) => Some("funding_negative".to_string()),
            _ => None,
        }
    }

    fn flow_bucket(snapshot: Option<&ExternalMarketSnapshot>) -> Option<String> {
        let value = snapshot
            .and_then(|row| row.raw_payload.as_ref())
            .and_then(Self::extract_flow_value);
        match value {
            Some(v) if v > 0.0 => Some("inflow".to_string()),
            Some(v) if v < 0.0 => Some("outflow".to_string()),
            _ => None,
        }
    }

    fn extract_flow_value(payload: &serde_json::Value) -> Option<f64> {
        [
            "netflow_usd",
            "transfer_value_usd",
            "amount_usd",
            "value_usd",
        ]
        .iter()
        .find_map(|key| payload.get(*key).and_then(|value| value.as_f64()))
    }

    fn build_bucket_reports(
        &self,
        traded_samples: &[EnrichedTradeSample],
        filtered_samples: &[EnrichedFilteredSignalSample],
    ) -> Vec<FactorBucketReport> {
        let mut grouped: HashMap<(String, String, VolatilityTier), Vec<f64>> = HashMap::new();
        for sample in traded_samples {
            if let Some(bucket_name) = &sample.funding_bucket {
                grouped
                    .entry((
                        format!(
                            "{}::{}",
                            ResearchSampleKind::Traded.label(),
                            "funding_premium_divergence"
                        ),
                        bucket_name.clone(),
                        sample.tier,
                    ))
                    .or_default()
                    .push(sample.trade.pnl);
            }
            if let Some(price_oi_state) = sample.price_oi_state {
                grouped
                    .entry((
                        format!(
                            "{}::{}",
                            ResearchSampleKind::Traded.label(),
                            "price_oi_state"
                        ),
                        price_oi_state.label().to_string(),
                        sample.tier,
                    ))
                    .or_default()
                    .push(sample.trade.pnl);
            }
            if let Some(bucket_name) = &sample.flow_bucket {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Traded.label(), "flow_proxy"),
                        bucket_name.clone(),
                        sample.tier,
                    ))
                    .or_default()
                    .push(sample.trade.pnl);
            }
        }
        for sample in filtered_samples {
            let pnl = sample.signal.theoretical_pnl.unwrap_or_default();
            if let Some(bucket_name) = &sample.funding_bucket {
                grouped
                    .entry((
                        format!(
                            "{}::{}",
                            ResearchSampleKind::Filtered.label(),
                            "funding_premium_divergence"
                        ),
                        bucket_name.clone(),
                        sample.tier,
                    ))
                    .or_default()
                    .push(pnl);
            }
            if let Some(price_oi_state) = sample.price_oi_state {
                grouped
                    .entry((
                        format!(
                            "{}::{}",
                            ResearchSampleKind::Filtered.label(),
                            "price_oi_state"
                        ),
                        price_oi_state.label().to_string(),
                        sample.tier,
                    ))
                    .or_default()
                    .push(pnl);
            }
            if let Some(bucket_name) = &sample.flow_bucket {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Filtered.label(), "flow_proxy"),
                        bucket_name.clone(),
                        sample.tier,
                    ))
                    .or_default()
                    .push(pnl);
            }
        }

        let mut rows = Vec::new();
        for ((factor_name, bucket_name, tier), pnls) in grouped {
            rows.push(Self::build_bucket_row(
                factor_name,
                bucket_name,
                tier,
                &pnls,
            ));
        }

        let mut by_factor: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, row) in rows.iter().enumerate() {
            by_factor
                .entry(row.factor_name.clone())
                .or_default()
                .push(idx);
        }
        for indexes in by_factor.values() {
            let clone_rows: Vec<_> = indexes.iter().map(|idx| rows[*idx].clone()).collect();
            let conclusion = Self::evaluate_factor_conclusion(&clone_rows);
            for idx in indexes {
                rows[*idx].conclusion = conclusion;
            }
        }

        for sample_kind in [ResearchSampleKind::Traded, ResearchSampleKind::Filtered] {
            for factor_name in Self::SUPPORTED_FACTORS {
                let factor_key = format!("{}::{}", sample_kind.label(), factor_name);
                if rows
                    .iter()
                    .any(|row| row.factor_name == factor_name && row.sample_kind == sample_kind)
                {
                    continue;
                }
                for tier in [
                    VolatilityTier::Btc,
                    VolatilityTier::Eth,
                    VolatilityTier::Alt,
                ] {
                    rows.push(FactorBucketReport {
                        factor_name: factor_name.to_string(),
                        bucket_name: "no_data".to_string(),
                        sample_kind,
                        volatility_tier: tier,
                        sample_count: 0,
                        win_rate: 0.0,
                        avg_pnl: 0.0,
                        sharpe_proxy: 0.0,
                        avg_mfe: 0.0,
                        avg_mae: 0.0,
                        conclusion: FactorConclusion::Reject,
                    });
                }
            }
        }

        rows.sort_by(|left, right| {
            left.factor_name
                .cmp(&right.factor_name)
                .then(left.bucket_name.cmp(&right.bucket_name))
                .then(
                    left.volatility_tier
                        .label()
                        .cmp(right.volatility_tier.label()),
                )
        });
        rows
    }

    fn build_bucket_row(
        factor_name: String,
        bucket_name: String,
        tier: VolatilityTier,
        pnls: &[f64],
    ) -> FactorBucketReport {
        let (sample_kind, clean_factor_name) =
            if let Some((kind_label, name)) = factor_name.split_once("::") {
                let kind = if kind_label == ResearchSampleKind::Filtered.label() {
                    ResearchSampleKind::Filtered
                } else {
                    ResearchSampleKind::Traded
                };
                (kind, name.to_string())
            } else {
                (ResearchSampleKind::Traded, factor_name)
            };
        let sample_count = pnls.len();
        let avg_pnl = if sample_count == 0 {
            0.0
        } else {
            pnls.iter().sum::<f64>() / sample_count as f64
        };
        let variance = if sample_count <= 1 {
            0.0
        } else {
            pnls.iter().map(|row| (row - avg_pnl).powi(2)).sum::<f64>() / sample_count as f64
        };
        let std_dev = variance.sqrt();
        let sharpe_proxy = if std_dev > 0.0 {
            avg_pnl / std_dev
        } else if avg_pnl > 0.0 {
            avg_pnl
        } else {
            0.0
        };

        FactorBucketReport {
            factor_name: clean_factor_name,
            bucket_name,
            sample_kind,
            volatility_tier: tier,
            sample_count,
            win_rate: if sample_count == 0 {
                0.0
            } else {
                pnls.iter().filter(|row| **row > 0.0).count() as f64 / sample_count as f64
            },
            avg_pnl,
            sharpe_proxy,
            avg_mfe: if sample_count == 0 {
                0.0
            } else {
                pnls.iter().copied().filter(|row| *row > 0.0).sum::<f64>() / sample_count as f64
            },
            avg_mae: if sample_count == 0 {
                0.0
            } else {
                pnls.iter().copied().filter(|row| *row < 0.0).sum::<f64>() / sample_count as f64
            },
            conclusion: FactorConclusion::Observe,
        }
    }
}
