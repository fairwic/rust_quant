use anyhow::Result;
use async_trait::async_trait;
use sqlx::{mysql::MySqlQueryResult, MySql, Pool, QueryBuilder};

use rust_quant_domain::entities::{
    BacktestDetail, BacktestLog, BacktestPerformanceMetrics, BacktestWinRateStats,
};
use rust_quant_domain::traits::BacktestLogRepository;

/// 基于 SQLx 的回测日志仓储实现
pub struct SqlxBacktestRepository {
    pool: Pool<MySql>,
}

impl SqlxBacktestRepository {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }

    fn pool(&self) -> &Pool<MySql> {
        &self.pool
    }
}

#[async_trait]
impl BacktestLogRepository for SqlxBacktestRepository {
    async fn insert_log(&self, log: &BacktestLog) -> Result<i64> {
        let result: MySqlQueryResult = sqlx::query!(
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
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            log.strategy_type,
            log.inst_id,
            log.timeframe,
            log.win_rate,
            log.final_fund,
            log.open_positions_num,
            log.strategy_detail,
            log.risk_config_detail,
            log.profit,
            log.one_bar_after_win_rate,
            log.two_bar_after_win_rate,
            log.three_bar_after_win_rate,
            log.four_bar_after_win_rate,
            log.five_bar_after_win_rate,
            log.ten_bar_after_win_rate,
            log.kline_start_time,
            log.kline_end_time,
            log.kline_nums
        )
        .execute(self.pool())
        .await?;

        Ok(result.last_insert_id() as i64)
    }

    async fn insert_details(&self, details: &[BacktestDetail]) -> Result<u64> {
        if details.is_empty() {
            return Ok(0);
        }

        let mut builder: QueryBuilder<MySql> =
            QueryBuilder::new("INSERT INTO back_test_detail (option_type, strategy_type, inst_id, time, back_test_id, open_position_time, signal_open_position_time, signal_status, close_position_time, open_price, close_price, profit_loss, quantity, full_close, close_type, win_nums, loss_nums, signal_value, signal_result, stop_loss_source) ");

        builder.push_values(details.iter(), |mut b, detail| {
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
                .push_bind(&detail.stop_loss_source);
        });

        let result = builder.build().execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    async fn update_win_rate_stats(
        &self,
        backtest_id: i64,
        stats: &BacktestWinRateStats,
    ) -> Result<u64> {
        let result = sqlx::query!(
            r#"
            UPDATE back_test_log SET
                one_bar_after_win_rate = ?,
                two_bar_after_win_rate = ?,
                three_bar_after_win_rate = ?,
                four_bar_after_win_rate = ?,
                five_bar_after_win_rate = ?,
                ten_bar_after_win_rate = ?
            WHERE id = ?
            "#,
            stats.one_bar_after_win_rate,
            stats.two_bar_after_win_rate,
            stats.three_bar_after_win_rate,
            stats.four_bar_after_win_rate,
            stats.five_bar_after_win_rate,
            stats.ten_bar_after_win_rate,
            backtest_id
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected())
    }

    async fn update_performance_metrics(
        &self,
        backtest_id: i64,
        metrics: &BacktestPerformanceMetrics,
    ) -> Result<u64> {
        let result = sqlx::query!(
            r#"
            UPDATE back_test_log SET
                sharpe_ratio = ?,
                annual_return = ?,
                total_return = ?,
                max_drawdown = ?,
                volatility = ?
            WHERE id = ?
            "#,
            metrics.sharpe_ratio,
            metrics.annual_return,
            metrics.total_return,
            metrics.max_drawdown,
            metrics.volatility,
            backtest_id
        )
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
                id BIGINT AUTO_INCREMENT PRIMARY KEY,
                backtest_id BIGINT NOT NULL,
                inst_id VARCHAR(32) NOT NULL,
                period VARCHAR(10) NOT NULL,
                signal_time DATETIME NOT NULL,
                direction VARCHAR(10) NOT NULL,
                filter_reasons JSON NOT NULL,
                signal_price DECIMAL(20, 8) NOT NULL,
                indicator_snapshot JSON,
                theoretical_profit DECIMAL(20, 8),
                theoretical_loss DECIMAL(20, 8),
                final_pnl DECIMAL(20, 8),
                trade_result VARCHAR(10),
                signal_value JSON,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                INDEX idx_backtest (backtest_id),
                INDEX idx_inst_period (inst_id, period)
            )
            "#,
        )
        .execute(self.pool())
        .await?;

        let mut builder: QueryBuilder<MySql> = QueryBuilder::new(
            "INSERT INTO filtered_signal_log (backtest_id, inst_id, period, signal_time, direction, filter_reasons, signal_price, indicator_snapshot, theoretical_profit, theoretical_loss, final_pnl, trade_result, signal_value) ",
        );

        builder.push_values(signals.iter(), |mut b, signal| {
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
        Ok(result.rows_affected())
    }
}
