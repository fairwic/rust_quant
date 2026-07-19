use super::report::{render_path_impact_report, render_report};
use super::types::{
    FactorBucketReport, FactorConclusion, PathImpactQuery, PathImpactReport, PathImpactSummary,
    PathImpactTradeChange, PriceOiState, ResearchFilteredSignalSample, ResearchSampleKind,
    ResearchTradeSample, VegasFactorResearchQuery, VegasFactorResearchReport, VolatilityTier,
};
use anyhow::{anyhow, Result};
use rust_quant_core::database::get_db_pool;
use rust_quant_domain::entities::ExternalMarketSnapshot;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use std::collections::HashMap;
const FOUR_HOURS_MS: i64 = 4 * 60 * 60 * 1000;
#[derive(Debug, Clone)]
struct EnrichedTradeSample {
    /// trade，用于记录交易或执行状态。
    trade: ResearchTradeSample,
    /// tier，用于记录交易或执行状态。
    tier: VolatilityTier,
    /// fundingbucket；为空时表示该条件不启用。
    funding_bucket: Option<String>,
    /// 状态值。
    price_oi_state: Option<PriceOiState>,
    /// flowbucket；为空时表示该条件不启用。
    flow_bucket: Option<String>,
}
#[derive(Debug, Clone)]
struct EnrichedFilteredSignalSample {
    /// 信号。
    signal: ResearchFilteredSignalSample,
    /// tier，用于记录新闻或情报分析结果。
    tier: VolatilityTier,
    /// fundingbucket；为空时表示该条件不启用。
    funding_bucket: Option<String>,
    /// 状态值。
    price_oi_state: Option<PriceOiState>,
    /// flowbucket；为空时表示该条件不启用。
    flow_bucket: Option<String>,
}
#[derive(Debug, Clone, FromRow)]
struct TradeSampleRow {
    /// backtest ID。
    backtest_id: i64,
    /// 交易所合约或现货交易对标识。
    inst_id: String,
    /// 周期。
    timeframe: String,
    /// 交易方向。
    side: String,
    /// 开仓时间。
    open_time: chrono::NaiveDateTime,
    /// 平仓时间。
    close_time: Option<chrono::NaiveDateTime>,
    /// 盈亏。
    pnl: f64,
    /// 类型标识。
    close_type: Option<String>,
    /// 止损来源；为空时使用默认值或表示不限制。
    stop_loss_source: Option<String>,
    /// 信号值；为空时表示该条件不启用。
    signal_value: Option<String>,
    /// 信号结果；为空时使用默认值或表示不限制。
    signal_result: Option<String>,
}
#[derive(Debug, Clone, FromRow)]
struct SnapshotRow {
    /// 唯一标识。
    id: i64,
    /// 数据来源。
    source: String,
    /// 交易对或资产符号。
    symbol: String,
    /// 类型标识。
    metric_type: String,
    /// 时间字段。
    metric_time: i64,
    /// 资金费率；为空时使用默认值或表示不限制。
    funding_rate: Option<f64>,
    /// 溢价率；为空时表示交易所未返回该指标。
    premium: Option<f64>,
    /// 未平仓量；为空时表示交易所未返回该指标。
    open_interest: Option<f64>,
    /// 价格数值。
    oracle_price: Option<f64>,
    /// 价格数值。
    mark_price: Option<f64>,
    /// longshort 比例；为空时使用默认值或表示不限制。
    long_short_ratio: Option<f64>,
    /// 原始 payload；为空时表示没有保留原始响应。
    raw_payload: Option<sqlx::types::Json<serde_json::Value>>,
    /// 创建时间；数据库中的无时区值按 UTC 解释。
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 最后更新时间；数据库中的无时区值按 UTC 解释。
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Debug, Clone, FromRow)]
struct FundingRateRow {
    /// 交易所合约或现货交易对标识。
    inst_id: String,
    /// 时间字段。
    funding_time: i64,
    /// 资金费率；为空时使用默认值或表示不限制。
    funding_rate: Option<f64>,
    /// 溢价率；为空时表示交易所未返回该指标。
    premium: Option<f64>,
}
#[derive(Debug, Clone, FromRow)]
struct FilteredSignalSampleRow {
    /// backtest ID。
    backtest_id: i64,
    /// 交易所合约或现货交易对标识。
    inst_id: String,
    /// 周期。
    timeframe: String,
    /// direction，用于展示或持久化查询结果。
    direction: String,
    /// 信号生成时间。
    signal_time: chrono::NaiveDateTime,
    /// theoretical盈亏；为空时表示该条件不启用。
    theoretical_pnl: Option<f64>,
    /// trade结果；为空时使用默认值或表示不限制。
    trade_result: Option<String>,
    /// 过滤原因列表；为空时表示没有过滤原因。
    filter_reasons: Option<String>,
    /// 信号值；为空时表示该条件不启用。
    signal_value: Option<String>,
}
pub struct VegasFactorResearchService {
    /// 数据库连接池。
    pool: PgPool,
}
impl VegasFactorResearchService {
    /// 构建 回测与策略研究 所需实例，并集中初始化依赖和默认状态。
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
    /// 执行 回测与策略研究 主流程，并把外部依赖调用、状态推进和错误返回串起来。
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
    /// 执行 回测与策略研究 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    pub async fn run_report_text(&self, query: VegasFactorResearchQuery) -> Result<String> {
        let report = self.run_report(query).await?;
        Ok(render_report(
            &report.trade_samples,
            &report.filtered_signal_samples,
            &report.factor_buckets,
        ))
    }
    /// 执行 回测与策略研究 主流程，并把外部依赖调用、状态推进和错误返回串起来。
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
    /// 执行 回测与策略研究 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    pub async fn run_path_impact_report_text(&self, query: PathImpactQuery) -> Result<String> {
        let report = self.run_path_impact_report(query).await?;
        Ok(render_path_impact_report(&report.summaries))
    }
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
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
        // 历史表使用 timestamp without time zone 保存 UTC；查询时显式补回时区，
        // 避免 sqlx 将其直接解码为 DateTime<Utc> 时发生类型不兼容。
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, source, symbol, metric_type, metric_time, \
             NULLIF(funding_rate, '')::double precision as funding_rate, \
             NULLIF(premium, '')::double precision as premium, \
             NULLIF(open_interest, '')::double precision as open_interest, \
             NULLIF(oracle_price, '')::double precision as oracle_price, \
             NULLIF(mark_price, '')::double precision as mark_price, \
             NULLIF(long_short_ratio, '')::double precision as long_short_ratio, \
             raw_payload, created_at AT TIME ZONE 'UTC' AS created_at, \
             updated_at AT TIME ZONE 'UTC' AS updated_at \
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
    /// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 提供数据行to交易sample的集中实现，避免回测策略调用方重复处理相同细节。
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
