use anyhow::Result;
use async_trait::async_trait;
use sqlx::{mysql::MySqlQueryResult, MySql, Pool, QueryBuilder};

use rust_quant_domain::entities::{BacktestDetail, BacktestLog, BacktestWinRateStats};
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
        let result: MySqlQueryResult = sqlx::query(
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
        .execute(self.pool())
        .await?;

        Ok(result.last_insert_id() as i64)
    }

    async fn insert_details(&self, details: &[BacktestDetail]) -> Result<u64> {
        if details.is_empty() {
            return Ok(0);
        }

        let mut builder: QueryBuilder<MySql> =
            QueryBuilder::new("INSERT INTO back_test_detail (option_type, strategy_type, inst_id, time, back_test_id, open_position_time, signal_open_position_time, signal_status, close_position_time, open_price, close_price, profit_loss, quantity, full_close, close_type, win_nums, loss_nums, signal_value, signal_result) ");

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
                .push_bind(&detail.signal_result);
        });

        let result = builder.build().execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    async fn update_win_rate_stats(
        &self,
        backtest_id: i64,
        stats: &BacktestWinRateStats,
    ) -> Result<u64> {
        let result = sqlx::query(
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
}
