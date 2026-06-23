use anyhow::{Context, Result};
use async_trait::async_trait;
use rust_quant_domain::entities::{
    OrderDecisionLog, OrderStateLog, PortfolioSnapshot, PositionSnapshot, RiskDecisionLog,
    SignalSnapshotLog, StrategyRun,
};
use rust_quant_domain::traits::AuditLogRepository;
use serde_json::Value;
use sqlx::{postgres::PgQueryResult, PgPool, Postgres, QueryBuilder};
const AUDIT_LOG_INSERT_CHUNK_ROWS: usize = 1_000;
pub struct SqlxAuditRepository {
    /// 数据库连接池。
    pool: PgPool,
}
impl SqlxAuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    fn pool(&self) -> &PgPool {
        &self.pool
    }
    fn parse_json_value(value: &str, field_name: &str) -> Result<Value> {
        serde_json::from_str(value).with_context(|| format!("invalid {}: {}", field_name, value))
    }
    /// 解析输入参数并收敛为 配置、基础设施和运行时 可使用的结构化值。
    fn parse_optional_json_value(value: Option<&str>, field_name: &str) -> Result<Option<Value>> {
        value
            .map(|raw| Self::parse_json_value(raw, field_name))
            .transpose()
    }
}
#[async_trait]
impl AuditLogRepository for SqlxAuditRepository {
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    async fn insert_strategy_run(&self, run: &StrategyRun) -> Result<u64> {
        let result: PgQueryResult = sqlx::query(
            r#"
            INSERT INTO strategy_run (run_id, strategy_id, inst_id, period, start_at, end_at, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(&run.run_id)
        .bind(&run.strategy_id)
        .bind(&run.inst_id)
        .bind(&run.period)
        .bind(run.start_at)
        .bind(run.end_at)
        .bind(&run.status)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }
    /// 持久化 配置、基础设施和运行时 结果，保证写入路径和幂等语义集中处理。
    async fn insert_signal_snapshots(&self, snapshots: &[SignalSnapshotLog]) -> Result<u64> {
        if snapshots.is_empty() {
            return Ok(0);
        }
        let mut rows_affected = 0;
        for chunk in snapshots.chunks(AUDIT_LOG_INSERT_CHUNK_ROWS) {
            let parsed_snapshots = chunk
                .iter()
                .map(|snapshot| {
                    Ok((
                        Self::parse_optional_json_value(
                            snapshot.filter_reasons.as_deref(),
                            "signal_snapshot_log.filter_reasons",
                        )?,
                        Self::parse_json_value(
                            &snapshot.signal_json,
                            "signal_snapshot_log.signal_json",
                        )?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO signal_snapshot_log (run_id, kline_ts, filtered, filter_reasons, signal_json) ",
            );
            builder.push_values(
                chunk.iter().zip(parsed_snapshots.iter()),
                |mut b, (s, parsed)| {
                    b.push_bind(&s.run_id)
                        .push_bind(s.kline_ts)
                        .push_bind(if s.filtered { 1 } else { 0 })
                        .push_bind(&parsed.0)
                        .push_bind(&parsed.1);
                },
            );
            let result = builder.build().execute(self.pool()).await?;
            rows_affected += result.rows_affected();
        }
        Ok(rows_affected)
    }
    /// 持久化 配置、基础设施和运行时 结果，保证写入路径和幂等语义集中处理。
    async fn insert_risk_decisions(&self, decisions: &[RiskDecisionLog]) -> Result<u64> {
        if decisions.is_empty() {
            return Ok(0);
        }
        let mut rows_affected = 0;
        for chunk in decisions.chunks(AUDIT_LOG_INSERT_CHUNK_ROWS) {
            let parsed_decisions = chunk
                .iter()
                .map(|decision| {
                    Self::parse_optional_json_value(
                        decision.risk_json.as_deref(),
                        "risk_decision_log.risk_json",
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO risk_decision_log (run_id, kline_ts, decision, reason, risk_json) ",
            );
            builder.push_values(
                chunk.iter().zip(parsed_decisions.iter()),
                |mut b, (d, risk_json)| {
                    b.push_bind(&d.run_id)
                        .push_bind(d.kline_ts)
                        .push_bind(&d.decision)
                        .push_bind(&d.reason)
                        .push_bind(risk_json);
                },
            );
            let result = builder.build().execute(self.pool()).await?;
            rows_affected += result.rows_affected();
        }
        Ok(rows_affected)
    }
    /// 持久化 配置、基础设施和运行时 结果，保证写入路径和幂等语义集中处理。
    async fn insert_order_decisions(&self, decisions: &[OrderDecisionLog]) -> Result<u64> {
        if decisions.is_empty() {
            return Ok(0);
        }
        let mut rows_affected = 0;
        for chunk in decisions.chunks(AUDIT_LOG_INSERT_CHUNK_ROWS) {
            let parsed_decisions = chunk
                .iter()
                .map(|decision| {
                    Self::parse_optional_json_value(
                        decision.decision_json.as_deref(),
                        "order_decision_log.decision_json",
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO order_decision_log (run_id, kline_ts, side, size, price, decision_json) ",
            );
            builder.push_values(
                chunk.iter().zip(parsed_decisions.iter()),
                |mut b, (d, decision_json)| {
                    b.push_bind(&d.run_id)
                        .push_bind(d.kline_ts)
                        .push_bind(&d.side)
                        .push_bind(d.size)
                        .push_bind(d.price)
                        .push_bind(decision_json);
                },
            );
            let result = builder.build().execute(self.pool()).await?;
            rows_affected += result.rows_affected();
        }
        Ok(rows_affected)
    }
    /// 持久化 配置、基础设施和运行时 结果，保证写入路径和幂等语义集中处理。
    async fn insert_order_state_logs(&self, states: &[OrderStateLog]) -> Result<u64> {
        if states.is_empty() {
            return Ok(0);
        }
        let mut rows_affected = 0;
        for chunk in states.chunks(AUDIT_LOG_INSERT_CHUNK_ROWS) {
            let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO order_state_log (order_id, from_state, to_state, reason, ts) ",
            );
            builder.push_values(chunk.iter(), |mut b, s| {
                b.push_bind(s.order_id)
                    .push_bind(&s.from_state)
                    .push_bind(&s.to_state)
                    .push_bind(&s.reason)
                    .push_bind(s.ts);
            });
            let result = builder.build().execute(self.pool()).await?;
            rows_affected += result.rows_affected();
        }
        Ok(rows_affected)
    }
    /// 持久化 配置、基础设施和运行时 结果，保证写入路径和幂等语义集中处理。
    async fn insert_position_snapshots(&self, positions: &[PositionSnapshot]) -> Result<u64> {
        if positions.is_empty() {
            return Ok(0);
        }
        let mut rows_affected = 0;
        for chunk in positions.chunks(AUDIT_LOG_INSERT_CHUNK_ROWS) {
            let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO positions (run_id, strategy_id, inst_id, side, qty, avg_price, unrealized_pnl, realized_pnl, status) ",
            );
            builder.push_values(chunk.iter(), |mut b, p| {
                b.push_bind(&p.run_id)
                    .push_bind(&p.strategy_id)
                    .push_bind(&p.inst_id)
                    .push_bind(&p.side)
                    .push_bind(p.qty)
                    .push_bind(p.avg_price)
                    .push_bind(p.unrealized_pnl)
                    .push_bind(p.realized_pnl)
                    .push_bind(&p.status);
            });
            let result = builder.build().execute(self.pool()).await?;
            rows_affected += result.rows_affected();
        }
        Ok(rows_affected)
    }
    /// 持久化 配置、基础设施和运行时 结果，保证写入路径和幂等语义集中处理。
    async fn insert_portfolio_snapshots(&self, snapshots: &[PortfolioSnapshot]) -> Result<u64> {
        if snapshots.is_empty() {
            return Ok(0);
        }
        let mut rows_affected = 0;
        for chunk in snapshots.chunks(AUDIT_LOG_INSERT_CHUNK_ROWS) {
            let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO portfolio_snapshot_log (run_id, total_equity, available, margin, pnl, ts) ",
            );
            builder.push_values(chunk.iter(), |mut b, s| {
                b.push_bind(&s.run_id)
                    .push_bind(s.total_equity)
                    .push_bind(s.available)
                    .push_bind(s.margin)
                    .push_bind(s.pnl)
                    .push_bind(s.ts);
            });
            let result = builder.build().execute(self.pool()).await?;
            rows_affected += result.rows_affected();
        }
        Ok(rows_affected)
    }
}
#[cfg(test)]
mod tests {
    use super::{SqlxAuditRepository, AUDIT_LOG_INSERT_CHUNK_ROWS};
    #[test]
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    fn parses_audit_jsonb_strings_before_binding() {
        let reasons = SqlxAuditRepository::parse_optional_json_value(
            Some("[\"LOW_VOLUME_INSIDE_RANGE_BLOCK_ENTRY\"]"),
            "signal_snapshot_log.filter_reasons",
        )
        .expect("filter reasons should parse");
        let signal = SqlxAuditRepository::parse_json_value(
            "{\"should_buy\":false}",
            "signal_snapshot_log.signal_json",
        )
        .expect("signal json should parse");
        assert_eq!(
            reasons.expect("filter reasons value"),
            serde_json::json!(["LOW_VOLUME_INSIDE_RANGE_BLOCK_ENTRY"])
        );
        assert_eq!(signal, serde_json::json!({"should_buy": false}));
    }
    #[test]
    fn rejects_invalid_audit_jsonb_strings() {
        let err =
            SqlxAuditRepository::parse_json_value("not-json", "signal_snapshot_log.signal_json")
                .expect_err("invalid json should fail before insert");
        assert!(err
            .to_string()
            .contains("invalid signal_snapshot_log.signal_json"));
    }
    #[test]
    fn audit_log_insert_chunk_keeps_postgres_bind_count_below_limit() {
        const POSTGRES_BIND_PARAM_LIMIT: usize = 65_535;
        const MAX_AUDIT_INSERT_COLUMNS: usize = 9;
        assert!(AUDIT_LOG_INSERT_CHUNK_ROWS * MAX_AUDIT_INSERT_COLUMNS < POSTGRES_BIND_PARAM_LIMIT);
    }
}
