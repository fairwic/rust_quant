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
    const SUPPORTED_FACTORS: [&'static str; 9] = [
        "exit_environment_context",
        "flow_proxy",
        "funding_direction_context",
        "funding_filter_context",
        "funding_macd_context",
        "funding_premium_divergence",
        "funding_trend_context",
        "funding_volume_context",
        "price_oi_state",
    ];

    pub fn new() -> Result<Self> {
        Ok(Self {
            pool: get_db_pool().clone(),
        })
    }

    pub fn with_pool(pool: PgPool) -> Self {
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

    pub fn classify_funding_signal_contexts(
        funding_bucket: Option<&str>,
        side: &str,
        signal_value: Option<&str>,
    ) -> Vec<(&'static str, String)> {
        let Some(funding_bucket) = funding_bucket else {
            return Vec::new();
        };
        let side = Self::normalize_side(side);
        let mut contexts = vec![(
            "funding_direction_context",
            format!("{funding_bucket}_{side}"),
        )];

        let Some(signal_json) = signal_value.and_then(Self::parse_signal_json) else {
            return contexts;
        };

        if let Some(trend_bucket) = Self::trend_bucket(&signal_json) {
            contexts.push((
                "funding_trend_context",
                format!("{funding_bucket}_{side}_{trend_bucket}"),
            ));
        }
        if let Some(macd_bucket) = Self::macd_bucket(&signal_json) {
            contexts.push((
                "funding_macd_context",
                format!("{funding_bucket}_{side}_{macd_bucket}"),
            ));
        }
        if let Some(volume_bucket) = Self::volume_bucket(&signal_json) {
            contexts.push((
                "funding_volume_context",
                format!("{funding_bucket}_{side}_{volume_bucket}"),
            ));
        }

        contexts
    }

    pub fn classify_funding_filter_contexts(
        funding_bucket: Option<&str>,
        direction: &str,
        filter_reasons: Option<&str>,
        signal_value: Option<&str>,
    ) -> Vec<(&'static str, String)> {
        let Some(funding_bucket) = funding_bucket else {
            return Vec::new();
        };
        let Some(primary_reason) = filter_reasons.and_then(Self::primary_filter_reason) else {
            return Vec::new();
        };
        let side = Self::normalize_side(direction);
        let Some(signal_json) = signal_value.and_then(Self::parse_signal_json) else {
            return vec![(
                "funding_filter_context",
                format!("{funding_bucket}_{side}_{primary_reason}"),
            )];
        };

        let distance = Self::distance_bucket(&signal_json).unwrap_or("distance_unknown");
        let leg = Self::leg_bucket(&signal_json).unwrap_or("leg_unknown");
        vec![(
            "funding_filter_context",
            format!("{funding_bucket}_{side}_{primary_reason}_{distance}_{leg}"),
        )]
    }

    pub fn classify_internal_exit_contexts(
        side: &str,
        close_type: Option<&str>,
        _stop_loss_source: Option<&str>,
        signal_value: Option<&str>,
    ) -> Vec<(&'static str, String)> {
        let Some(exit_bucket) = close_type.and_then(Self::exit_bucket) else {
            return Vec::new();
        };
        let Some(signal_json) = signal_value.and_then(Self::parse_signal_json) else {
            return Vec::new();
        };
        let Some(trend_alignment) = Self::trend_alignment_bucket(&signal_json, side) else {
            return Vec::new();
        };
        let Some(macd_alignment) = Self::macd_alignment_bucket(&signal_json, side) else {
            return Vec::new();
        };
        let distance = Self::distance_bucket(&signal_json).unwrap_or("distance_unknown");
        let volume = Self::volume_bucket(&signal_json).unwrap_or("volume_unknown");

        vec![(
            "exit_environment_context",
            format!("{exit_bucket}_{trend_alignment}_{macd_alignment}_{distance}_{volume}"),
        )]
    }

    pub fn evaluate_factor_conclusion(reports: &[FactorBucketReport]) -> FactorConclusion {
        let traded = reports
            .iter()
            .find(|row| row.sample_kind == ResearchSampleKind::Traded);
        let filtered = reports
            .iter()
            .find(|row| row.sample_kind == ResearchSampleKind::Filtered);

        match (traded, filtered) {
            (Some(traded), Some(filtered))
                if traded.volatility_tier == VolatilityTier::Eth
                    && traded.sample_count >= 5
                    && traded.avg_pnl > 0.0
                    && traded.sharpe_proxy >= 0.5
                    && traded.avg_pnl > filtered.avg_pnl
                    && traded.sharpe_proxy > filtered.sharpe_proxy + 0.3 =>
            {
                FactorConclusion::Candidate
            }
            (Some(traded), _)
                if traded.volatility_tier == VolatilityTier::Eth
                    && traded.sample_count >= 3
                    && (traded.avg_pnl > 0.0 || traded.sharpe_proxy > 0.0) =>
            {
                FactorConclusion::Observe
            }
            (Some(traded), Some(filtered))
                if traded.sample_count >= 3
                    && traded.avg_pnl > filtered.avg_pnl
                    && traded.sharpe_proxy >= filtered.sharpe_proxy =>
            {
                FactorConclusion::Observe
            }
            (None, Some(filtered))
                if filtered.volatility_tier == VolatilityTier::Eth
                    && filtered.sample_count >= 4
                    && filtered.win_rate >= 0.7
                    && filtered.avg_pnl > 0.02
                    && filtered.sharpe_proxy >= 0.5 =>
            {
                FactorConclusion::Candidate
            }
            _ => FactorConclusion::Reject,
        }
    }

    pub fn render_report(
        trades: &[ResearchTradeSample],
        filtered_signals: &[ResearchFilteredSignalSample],
        buckets: &[FactorBucketReport],
    ) -> String {
        render_report(trades, filtered_signals, buckets)
    }

    pub fn render_path_impact_report(summaries: &[PathImpactSummary]) -> String {
        render_path_impact_report(summaries)
    }

    pub fn summarize_path_impact(
        baseline_id: i64,
        experiment_id: i64,
        baseline: &[ResearchTradeSample],
        experiment: &[ResearchTradeSample],
        top_changed_limit: usize,
    ) -> PathImpactSummary {
        let baseline_map = Self::trade_map(baseline);
        let experiment_map = Self::trade_map(experiment);
        let mut missing_pnls = Vec::new();
        let mut new_pnls = Vec::new();
        let mut common_deltas = Vec::new();
        let mut changes = Vec::new();

        for (key, baseline_trade) in &baseline_map {
            if let Some(experiment_trade) = experiment_map.get(key) {
                let pnl_delta = experiment_trade.pnl - baseline_trade.pnl;
                common_deltas.push(pnl_delta);
                changes.push(PathImpactTradeChange {
                    change_type: "common_changed".to_string(),
                    inst_id: baseline_trade.inst_id.clone(),
                    side: baseline_trade.side.clone(),
                    open_time_ms: baseline_trade.open_time_ms,
                    baseline_pnl: Some(baseline_trade.pnl),
                    experiment_pnl: Some(experiment_trade.pnl),
                    pnl_delta,
                    close_type: experiment_trade.close_type.clone(),
                });
            } else {
                missing_pnls.push(baseline_trade.pnl);
                changes.push(PathImpactTradeChange {
                    change_type: "missing_from_experiment".to_string(),
                    inst_id: baseline_trade.inst_id.clone(),
                    side: baseline_trade.side.clone(),
                    open_time_ms: baseline_trade.open_time_ms,
                    baseline_pnl: Some(baseline_trade.pnl),
                    experiment_pnl: None,
                    pnl_delta: -baseline_trade.pnl,
                    close_type: baseline_trade.close_type.clone(),
                });
            }
        }

        for (key, experiment_trade) in &experiment_map {
            if !baseline_map.contains_key(key) {
                new_pnls.push(experiment_trade.pnl);
                changes.push(PathImpactTradeChange {
                    change_type: "new_in_experiment".to_string(),
                    inst_id: experiment_trade.inst_id.clone(),
                    side: experiment_trade.side.clone(),
                    open_time_ms: experiment_trade.open_time_ms,
                    baseline_pnl: None,
                    experiment_pnl: Some(experiment_trade.pnl),
                    pnl_delta: experiment_trade.pnl,
                    close_type: experiment_trade.close_type.clone(),
                });
            }
        }

        changes.sort_by(|left, right| {
            right
                .pnl_delta
                .abs()
                .total_cmp(&left.pnl_delta.abs())
                .then(left.open_time_ms.cmp(&right.open_time_ms))
        });
        changes.truncate(top_changed_limit);

        let missing_pnl = missing_pnls.iter().sum::<f64>();
        let new_pnl = new_pnls.iter().sum::<f64>();
        let common_pnl_delta = common_deltas.iter().sum::<f64>();
        let total_path_delta = new_pnl - missing_pnl + common_pnl_delta;
        let verdict = if total_path_delta > 1e-6 {
            "path_improved"
        } else if total_path_delta < -1e-6 {
            "path_degraded"
        } else {
            "neutral"
        };

        PathImpactSummary {
            baseline_id,
            experiment_id,
            inst_id: Self::unique_inst_id(baseline, experiment),
            missing_count: missing_pnls.len(),
            missing_pnl,
            missing_wins: missing_pnls.iter().filter(|pnl| **pnl > 0.0).count(),
            missing_avg_pnl: Self::avg(&missing_pnls),
            new_count: new_pnls.len(),
            new_pnl,
            new_wins: new_pnls.iter().filter(|pnl| **pnl > 0.0).count(),
            new_avg_pnl: Self::avg(&new_pnls),
            common_count: common_deltas.len(),
            common_pnl_delta,
            common_improved_count: common_deltas.iter().filter(|delta| **delta > 0.0).count(),
            total_path_delta,
            verdict: verdict.to_string(),
            top_changes: changes,
        }
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

    fn trade_map(
        trades: &[ResearchTradeSample],
    ) -> HashMap<(String, String, i64), &ResearchTradeSample> {
        trades
            .iter()
            .map(|trade| {
                (
                    (
                        trade.inst_id.clone(),
                        trade.side.to_ascii_lowercase(),
                        trade.open_time_ms,
                    ),
                    trade,
                )
            })
            .collect()
    }

    fn unique_inst_id(
        baseline: &[ResearchTradeSample],
        experiment: &[ResearchTradeSample],
    ) -> Option<String> {
        let ids: Vec<_> = baseline
            .iter()
            .chain(experiment.iter())
            .map(|row| row.inst_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if ids.len() == 1 {
            Some(ids[0].to_string())
        } else {
            None
        }
    }

    fn avg(values: &[f64]) -> f64 {
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
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
        match snapshot.map(|row| (row.funding_rate, row.premium)) {
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

    fn parse_signal_json(signal_value: &str) -> Option<serde_json::Value> {
        serde_json::from_str(signal_value).ok()
    }

    fn normalize_side(side: &str) -> &'static str {
        match side.to_ascii_lowercase().as_str() {
            "long" => "long",
            "short" => "short",
            "buy" => "long",
            "sell" => "short",
            value if value.contains("long") => "long",
            value if value.contains("short") => "short",
            _ => "unknown",
        }
    }

    fn trend_bucket(signal: &serde_json::Value) -> Option<&'static str> {
        let ema = signal.get("ema_values")?;
        match (
            ema.get("is_long_trend").and_then(|value| value.as_bool()),
            ema.get("is_short_trend").and_then(|value| value.as_bool()),
        ) {
            (Some(true), _) => Some("long_trend"),
            (_, Some(true)) => Some("short_trend"),
            (Some(false), Some(false)) => Some("mixed_trend"),
            _ => None,
        }
    }

    fn macd_bucket(signal: &serde_json::Value) -> Option<String> {
        let macd = signal.get("macd_value")?;
        let histogram = macd.get("histogram").and_then(|value| value.as_f64());
        let zone = match histogram {
            Some(value) if value >= 0.0 => "macd_above_zero",
            Some(_) => "macd_below_zero",
            None => match macd.get("above_zero").and_then(|value| value.as_bool()) {
                Some(true) => "macd_above_zero",
                Some(false) => "macd_below_zero",
                None => return None,
            },
        };
        let momentum = if macd
            .get("histogram_improving")
            .or_else(|| macd.get("histogram_increasing"))
            .and_then(|value| value.as_bool())
            == Some(true)
        {
            "hist_improving"
        } else if macd
            .get("histogram_decreasing")
            .and_then(|value| value.as_bool())
            == Some(true)
        {
            "hist_decreasing"
        } else {
            "hist_flat"
        };

        Some(format!("{zone}_{momentum}"))
    }

    fn volume_bucket(signal: &serde_json::Value) -> Option<&'static str> {
        let ratio = signal
            .get("volume_value")
            .and_then(|value| value.get("volume_ratio"))
            .and_then(|value| value.as_f64())?;
        if ratio >= 2.5 {
            Some("volume_extreme")
        } else if ratio >= 1.5 {
            Some("volume_expansion")
        } else if ratio < 0.8 {
            Some("volume_contract")
        } else {
            Some("volume_normal")
        }
    }

    fn exit_bucket(close_type: &str) -> Option<&'static str> {
        let lower = close_type.to_ascii_lowercase();
        if close_type.contains("Signal_Kline_Stop_Loss") || lower.contains("signal_kline_stop_loss")
        {
            Some("signal_stop")
        } else if close_type.contains("最大亏损止损") || lower.contains("max_loss") {
            Some("max_loss_stop")
        } else if close_type.contains("反向信号") || lower.contains("opposite") {
            Some("opposite_signal_close")
        } else if close_type.contains("止盈")
            || lower.contains("take_profit")
            || lower.contains("atr")
        {
            Some("take_profit")
        } else {
            None
        }
    }

    fn trend_alignment_bucket(signal: &serde_json::Value, side: &str) -> Option<&'static str> {
        match (Self::normalize_side(side), Self::trend_bucket(signal)?) {
            (_, "mixed_trend") => Some("mixed_trend"),
            ("long", "long_trend") | ("short", "short_trend") => Some("with_trend"),
            ("long", "short_trend") | ("short", "long_trend") => Some("counter_trend"),
            _ => Some("trend_unknown"),
        }
    }

    fn macd_alignment_bucket(signal: &serde_json::Value, side: &str) -> Option<&'static str> {
        let histogram = signal
            .get("macd_value")?
            .get("histogram")
            .and_then(|value| value.as_f64())?;
        match Self::normalize_side(side) {
            "long" if histogram >= 0.0 => Some("macd_align"),
            "long" => Some("macd_against"),
            "short" if histogram < 0.0 => Some("macd_align"),
            "short" => Some("macd_against"),
            _ => Some("macd_unknown"),
        }
    }

    fn distance_bucket(signal: &serde_json::Value) -> Option<&'static str> {
        match signal
            .get("ema_distance_filter")?
            .get("state")?
            .as_str()?
            .to_ascii_lowercase()
            .as_str()
        {
            "toofar" => Some("distance_too_far"),
            "normal" => Some("distance_normal"),
            "tangled" => Some("distance_tangled"),
            _ => Some("distance_other"),
        }
    }

    fn leg_bucket(signal: &serde_json::Value) -> Option<&'static str> {
        let leg = signal.get("leg_detection_value")?;
        match (
            leg.get("is_bullish_leg").and_then(|value| value.as_bool()),
            leg.get("is_bearish_leg").and_then(|value| value.as_bool()),
        ) {
            (Some(true), _) => Some("bullish_leg"),
            (_, Some(true)) => Some("bearish_leg"),
            (Some(false), Some(false)) => Some("mixed_leg"),
            _ => None,
        }
    }

    fn primary_filter_reason(filter_reasons: &str) -> Option<String> {
        serde_json::from_str::<Vec<String>>(filter_reasons)
            .ok()
            .and_then(|values| values.into_iter().next())
            .map(|value| value.to_ascii_lowercase())
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
        let mut grouped: HashMap<(String, String, VolatilityTier, String), Vec<f64>> =
            HashMap::new();
        for sample in traded_samples {
            let scope_label = Self::bucket_scope_label(&sample.trade.inst_id, sample.tier);
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
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(sample.trade.pnl);
            }
            for (factor_name, bucket_name) in Self::classify_funding_signal_contexts(
                sample.funding_bucket.as_deref(),
                &sample.trade.side,
                sample.trade.signal_value.as_deref(),
            ) {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Traded.label(), factor_name),
                        bucket_name,
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(sample.trade.pnl);
            }
            for (factor_name, bucket_name) in Self::classify_internal_exit_contexts(
                &sample.trade.side,
                sample.trade.close_type.as_deref(),
                sample.trade.stop_loss_source.as_deref(),
                sample.trade.signal_value.as_deref(),
            ) {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Traded.label(), factor_name),
                        bucket_name,
                        sample.tier,
                        scope_label.clone(),
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
                        scope_label.clone(),
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
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(sample.trade.pnl);
            }
        }
        for sample in filtered_samples {
            let pnl = sample.signal.theoretical_pnl.unwrap_or_default();
            let scope_label = Self::bucket_scope_label(&sample.signal.inst_id, sample.tier);
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
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(pnl);
            }
            for (factor_name, bucket_name) in Self::classify_funding_signal_contexts(
                sample.funding_bucket.as_deref(),
                &sample.signal.direction,
                sample.signal.signal_value.as_deref(),
            ) {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Filtered.label(), factor_name),
                        bucket_name,
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(pnl);
            }
            for (factor_name, bucket_name) in Self::classify_funding_filter_contexts(
                sample.funding_bucket.as_deref(),
                &sample.signal.direction,
                sample.signal.filter_reasons.as_deref(),
                sample.signal.signal_value.as_deref(),
            ) {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Filtered.label(), factor_name),
                        bucket_name,
                        sample.tier,
                        scope_label.clone(),
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
                        scope_label.clone(),
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
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(pnl);
            }
        }

        let mut rows = Vec::new();
        for ((factor_name, bucket_name, tier, scope_label), pnls) in grouped {
            rows.push(Self::build_bucket_row(
                factor_name,
                bucket_name,
                tier,
                scope_label,
                &pnls,
            ));
        }

        let mut by_bucket: HashMap<(String, String, VolatilityTier, String), Vec<usize>> =
            HashMap::new();
        for (idx, row) in rows.iter().enumerate() {
            by_bucket
                .entry((
                    row.factor_name.clone(),
                    row.bucket_name.clone(),
                    row.volatility_tier,
                    row.scope_label.clone(),
                ))
                .or_default()
                .push(idx);
        }
        for indexes in by_bucket.values() {
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
                        scope_label: tier.label().to_string(),
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
                .then(left.scope_label.cmp(&right.scope_label))
        });
        rows
    }

    fn build_bucket_row(
        factor_name: String,
        bucket_name: String,
        tier: VolatilityTier,
        scope_label: String,
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
            scope_label,
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

    fn bucket_scope_label(inst_id: &str, tier: VolatilityTier) -> String {
        match tier {
            VolatilityTier::Alt => inst_id.split('-').next().unwrap_or(inst_id).to_string(),
            _ => tier.label().to_string(),
        }
    }
}
