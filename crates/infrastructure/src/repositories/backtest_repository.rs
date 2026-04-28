use anyhow::Result;
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder};

use rust_quant_domain::entities::{
    BacktestDetail, BacktestLog, BacktestPerformanceMetrics, BacktestWinRateStats, DynamicConfigLog,
};
use rust_quant_domain::traits::BacktestLogRepository;

const BACKTEST_INSERT_CHUNK_ROWS: usize = 1_000;

/// 基于 SQLx 的回测日志仓储实现
pub struct SqlxBacktestRepository {
    pool: PgPool,
}

impl SqlxBacktestRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl BacktestLogRepository for SqlxBacktestRepository {
    async fn insert_log(&self, log: &BacktestLog) -> Result<i64> {
        let inserted_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO back_test_log (
                strategy_type,
                inst_type,
                time,
                win_rate,
                final_fund,
                open_positions_num,
                strategy_detail,
                risk_config_detail,
                profit,
                one_bar_after_win_rate,
                two_bar_after_win_rate,
                three_bar_after_win_rate,
                four_bar_after_win_rate,
                five_bar_after_win_rate,
                ten_bar_after_win_rate,
                kline_start_time,
                kline_end_time,
                kline_nums
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            RETURNING id
            "#,
        )
        .bind(&log.strategy_type)
        .bind(&log.inst_id)
        .bind(&log.timeframe)
        .bind(&log.win_rate)
        .bind(&log.final_fund)
        .bind(log.open_positions_num)
        .bind(&log.strategy_detail)
        .bind(&log.risk_config_detail)
        .bind(&log.profit)
        .bind(log.one_bar_after_win_rate)
        .bind(log.two_bar_after_win_rate)
        .bind(log.three_bar_after_win_rate)
        .bind(log.four_bar_after_win_rate)
        .bind(log.five_bar_after_win_rate)
        .bind(log.ten_bar_after_win_rate)
        .bind(log.kline_start_time)
        .bind(log.kline_end_time)
        .bind(log.kline_nums)
        .fetch_one(self.pool())
        .await?;

        Ok(inserted_id)
    }

    async fn insert_details(&self, details: &[BacktestDetail]) -> Result<u64> {
        if details.is_empty() {
            return Ok(0);
        }

        let mut rows_affected = 0;
        for chunk in details.chunks(BACKTEST_INSERT_CHUNK_ROWS) {
            let mut builder: QueryBuilder<Postgres> =
                QueryBuilder::new("INSERT INTO back_test_detail (option_type, strategy_type, inst_id, time, back_test_id, open_position_time, signal_open_position_time, signal_status, close_position_time, open_price, close_price, profit_loss, quantity, full_close, close_type, win_nums, loss_nums, signal_value, signal_result, stop_loss_source, stop_loss_update_history) ");

            builder.push_values(chunk.iter(), |mut b, detail| {
                b.push_bind(&detail.option_type)
                    .push_bind(&detail.strategy_type)
                    .push_bind(&detail.inst_id)
                    .push_bind(&detail.timeframe)
                    .push_bind(detail.back_test_id)
                    .push_bind(&detail.open_position_time)
                    .push_bind(&detail.signal_open_position_time)
                    .push_bind(detail.signal_status)
                    .push_bind(&detail.close_position_time)
                    .push_bind(&detail.open_price)
                    .push_bind(&detail.close_price)
                    .push_bind(&detail.profit_loss)
                    .push_bind(&detail.quantity)
                    .push_bind(&detail.full_close)
                    .push_bind(&detail.close_type)
                    .push_bind(detail.win_nums)
                    .push_bind(detail.loss_nums)
                    .push_bind(&detail.signal_value)
                    .push_bind(&detail.signal_result)
                    .push_bind(&detail.stop_loss_source)
                    .push_bind(&detail.stop_loss_update_history);
            });

            let result = builder.build().execute(self.pool()).await?;
            rows_affected += result.rows_affected();
        }

        Ok(rows_affected)
    }

    async fn update_win_rate_stats(
        &self,
        backtest_id: i64,
        stats: &BacktestWinRateStats,
    ) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE back_test_log SET
                one_bar_after_win_rate = $1,
                two_bar_after_win_rate = $2,
                three_bar_after_win_rate = $3,
                four_bar_after_win_rate = $4,
                five_bar_after_win_rate = $5,
                ten_bar_after_win_rate = $6
            WHERE id = $7
            "#,
        )
        .bind(stats.one_bar_after_win_rate)
        .bind(stats.two_bar_after_win_rate)
        .bind(stats.three_bar_after_win_rate)
        .bind(stats.four_bar_after_win_rate)
        .bind(stats.five_bar_after_win_rate)
        .bind(stats.ten_bar_after_win_rate)
        .bind(backtest_id)
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected())
    }

    async fn update_performance_metrics(
        &self,
        backtest_id: i64,
        metrics: &BacktestPerformanceMetrics,
    ) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE back_test_log SET
                sharpe_ratio = $1,
                annual_return = $2,
                total_return = $3,
                max_drawdown = $4,
                volatility = $5
            WHERE id = $6
            "#,
        )
        .bind(metrics.sharpe_ratio)
        .bind(metrics.annual_return)
        .bind(metrics.total_return)
        .bind(metrics.max_drawdown)
        .bind(metrics.volatility)
        .bind(backtest_id)
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected())
    }

    async fn insert_filtered_signals(
        &self,
        signals: &[rust_quant_domain::entities::FilteredSignalLog],
    ) -> Result<u64> {
        if signals.is_empty() {
            tracing::info!("insert_filtered_signals being called with empty list");
            return Ok(0);
        }
        tracing::info!(
            "insert_filtered_signals inserting {} signals",
            signals.len()
        );

        // 确保表存在 (仅开发阶段便利措施，生产环境应使用 migrate)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS filtered_signal_log (
                id BIGSERIAL PRIMARY KEY,
                backtest_id BIGINT NOT NULL,
                inst_id VARCHAR(32) NOT NULL,
                period VARCHAR(10) NOT NULL,
                signal_time TIMESTAMP NOT NULL,
                direction VARCHAR(10) NOT NULL,
                filter_reasons JSONB NOT NULL,
                signal_price NUMERIC(20, 8) NOT NULL,
                indicator_snapshot JSONB,
                theoretical_profit NUMERIC(20, 8),
                theoretical_loss NUMERIC(20, 8),
                final_pnl NUMERIC(20, 8),
                trade_result VARCHAR(10),
                signal_value JSONB,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(self.pool())
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_filtered_signal_log_backtest ON filtered_signal_log (backtest_id)",
        )
        .execute(self.pool())
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_filtered_signal_log_inst_period ON filtered_signal_log (inst_id, period)",
        )
        .execute(self.pool())
        .await?;
        sqlx::query("COMMENT ON TABLE filtered_signal_log IS '被过滤策略信号日志表'")
            .execute(self.pool())
            .await?;

        let mut rows_affected = 0;
        for chunk in signals.chunks(BACKTEST_INSERT_CHUNK_ROWS) {
            let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO filtered_signal_log (backtest_id, inst_id, period, signal_time, direction, filter_reasons, signal_price, indicator_snapshot, theoretical_profit, theoretical_loss, final_pnl, trade_result, signal_value) ",
            );

            builder.push_values(chunk.iter(), |mut b, signal| {
                b.push_bind(signal.backtest_id)
                    .push_bind(&signal.inst_id)
                    .push_bind(&signal.period)
                    .push_bind(&signal.signal_time)
                    .push_bind(&signal.direction)
                    .push_bind(&signal.filter_reasons)
                    .push_bind(signal.signal_price)
                    .push_bind(&signal.indicator_snapshot)
                    .push_bind(signal.theoretical_profit)
                    .push_bind(signal.theoretical_loss)
                    .push_bind(signal.final_pnl)
                    .push_bind(&signal.trade_result)
                    .push_bind(&signal.signal_value);
            });

            let result = builder.build().execute(self.pool()).await?;
            rows_affected += result.rows_affected();
        }

        Ok(rows_affected)
    }

    async fn insert_dynamic_config_logs(&self, logs: &[DynamicConfigLog]) -> Result<u64> {
        if logs.is_empty() {
            tracing::info!("insert_dynamic_config_logs being called with empty list");
            return Ok(0);
        }

        tracing::info!("insert_dynamic_config_logs inserting {} logs", logs.len());

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS dynamic_config_log (
                id BIGSERIAL PRIMARY KEY,
                backtest_id BIGINT NOT NULL,
                inst_id VARCHAR(32) NOT NULL,
                period VARCHAR(10) NOT NULL,
                kline_time TIMESTAMP NOT NULL,
                adjustments JSONB NOT NULL,
                config_snapshot JSONB,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(self.pool())
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_dynamic_config_log_backtest ON dynamic_config_log (backtest_id)",
        )
        .execute(self.pool())
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_dynamic_config_log_inst_period ON dynamic_config_log (inst_id, period)",
        )
        .execute(self.pool())
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_dynamic_config_log_kline_time ON dynamic_config_log (kline_time)",
        )
        .execute(self.pool())
        .await?;
        sqlx::query("COMMENT ON TABLE dynamic_config_log IS '动态策略配置调整日志表'")
            .execute(self.pool())
            .await?;

        let mut rows_affected = 0;
        for chunk in logs.chunks(BACKTEST_INSERT_CHUNK_ROWS) {
            let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO dynamic_config_log (backtest_id, inst_id, period, kline_time, adjustments, config_snapshot) ",
            );

            builder.push_values(chunk.iter(), |mut b, log| {
                b.push_bind(log.backtest_id)
                    .push_bind(&log.inst_id)
                    .push_bind(&log.period)
                    .push_bind(&log.kline_time)
                    .push_bind(&log.adjustments)
                    .push_bind(&log.config_snapshot);
            });

            let result = builder.build().execute(self.pool()).await?;
            rows_affected += result.rows_affected();
        }

        Ok(rows_affected)
    }
}

#[cfg(test)]
mod tests {
    use super::BACKTEST_INSERT_CHUNK_ROWS;

    #[test]
    fn backtest_insert_chunk_keeps_postgres_bind_count_below_limit() {
        const POSTGRES_BIND_PARAM_LIMIT: usize = 65_535;
        const MAX_BACKTEST_INSERT_COLUMNS: usize = 21;

        assert!(
            BACKTEST_INSERT_CHUNK_ROWS * MAX_BACKTEST_INSERT_COLUMNS < POSTGRES_BIND_PARAM_LIMIT
        );
    }
}
