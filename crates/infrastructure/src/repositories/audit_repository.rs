use anyhow::Result;
use async_trait::async_trait;
use sqlx::{mysql::MySqlQueryResult, MySql, Pool, QueryBuilder};

use rust_quant_domain::entities::{
    OrderDecisionLog, OrderStateLog, PortfolioSnapshot, PositionSnapshot, RiskDecisionLog,
    SignalSnapshotLog, StrategyRun,
};
use rust_quant_domain::traits::AuditLogRepository;

pub struct SqlxAuditRepository {
    pool: Pool<MySql>,
}

impl SqlxAuditRepository {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }

    fn pool(&self) -> &Pool<MySql> {
        &self.pool
    }
}

#[async_trait]
impl AuditLogRepository for SqlxAuditRepository {
    async fn insert_strategy_run(&self, run: &StrategyRun) -> Result<u64> {
        let result: MySqlQueryResult = sqlx::query(
            r#"
            INSERT INTO strategy_run (run_id, strategy_id, inst_id, period, start_at, end_at, status)
            VALUES (?, ?, ?, ?, ?, ?, ?)
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

    async fn insert_signal_snapshots(&self, snapshots: &[SignalSnapshotLog]) -> Result<u64> {
        if snapshots.is_empty() {
            return Ok(0);
        }

        let mut builder: QueryBuilder<MySql> = QueryBuilder::new(
            "INSERT INTO signal_snapshot_log (run_id, kline_ts, filtered, filter_reasons, signal_json) ",
        );
        builder.push_values(snapshots.iter(), |mut b, s| {
            b.push_bind(&s.run_id)
                .push_bind(s.kline_ts)
                .push_bind(if s.filtered { 1 } else { 0 })
                .push_bind(&s.filter_reasons)
                .push_bind(&s.signal_json);
        });

        let result = builder.build().execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    async fn insert_risk_decisions(&self, decisions: &[RiskDecisionLog]) -> Result<u64> {
        if decisions.is_empty() {
            return Ok(0);
        }

        let mut builder: QueryBuilder<MySql> = QueryBuilder::new(
            "INSERT INTO risk_decision_log (run_id, kline_ts, decision, reason, risk_json) ",
        );
        builder.push_values(decisions.iter(), |mut b, d| {
            b.push_bind(&d.run_id)
                .push_bind(d.kline_ts)
                .push_bind(&d.decision)
                .push_bind(&d.reason)
                .push_bind(&d.risk_json);
        });
        let result = builder.build().execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    async fn insert_order_decisions(&self, decisions: &[OrderDecisionLog]) -> Result<u64> {
        if decisions.is_empty() {
            return Ok(0);
        }

        let mut builder: QueryBuilder<MySql> = QueryBuilder::new(
            "INSERT INTO order_decision_log (run_id, kline_ts, side, size, price, decision_json) ",
        );
        builder.push_values(decisions.iter(), |mut b, d| {
            b.push_bind(&d.run_id)
                .push_bind(d.kline_ts)
                .push_bind(&d.side)
                .push_bind(d.size)
                .push_bind(d.price)
                .push_bind(&d.decision_json);
        });
        let result = builder.build().execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    async fn insert_order_state_logs(&self, states: &[OrderStateLog]) -> Result<u64> {
        if states.is_empty() {
            return Ok(0);
        }

        let mut builder: QueryBuilder<MySql> = QueryBuilder::new(
            "INSERT INTO order_state_log (order_id, from_state, to_state, reason, ts) ",
        );
        builder.push_values(states.iter(), |mut b, s| {
            b.push_bind(s.order_id)
                .push_bind(&s.from_state)
                .push_bind(&s.to_state)
                .push_bind(&s.reason)
                .push_bind(s.ts);
        });
        let result = builder.build().execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    async fn insert_position_snapshots(&self, positions: &[PositionSnapshot]) -> Result<u64> {
        if positions.is_empty() {
            return Ok(0);
        }

        let mut builder: QueryBuilder<MySql> = QueryBuilder::new(
            "INSERT INTO positions (run_id, strategy_id, inst_id, side, qty, avg_price, unrealized_pnl, realized_pnl, status) ",
        );
        builder.push_values(positions.iter(), |mut b, p| {
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
        Ok(result.rows_affected())
    }

    async fn insert_portfolio_snapshots(&self, snapshots: &[PortfolioSnapshot]) -> Result<u64> {
        if snapshots.is_empty() {
            return Ok(0);
        }

        let mut builder: QueryBuilder<MySql> = QueryBuilder::new(
            "INSERT INTO portfolio_snapshot_log (run_id, total_equity, available, margin, pnl, ts) ",
        );
        builder.push_values(snapshots.iter(), |mut b, s| {
            b.push_bind(&s.run_id)
                .push_bind(s.total_equity)
                .push_bind(s.available)
                .push_bind(s.margin)
                .push_bind(s.pnl)
                .push_bind(s.ts);
        });
        let result = builder.build().execute(self.pool()).await?;
        Ok(result.rows_affected())
    }
}
