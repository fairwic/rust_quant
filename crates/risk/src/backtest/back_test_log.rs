use anyhow::Result;
use chrono::NaiveDateTime;
use rust_quant_core::database::get_db_pool;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::backtest::PositionStats;

/// 回测日志表
#[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
pub struct BackTestLog {
    #[sqlx(default)]
    pub id: Option<i32>,
    pub strategy_type: String,
    pub inst_type: String,
    pub time: String,
    pub win_rate: String,
    pub final_fund: f32,
    pub open_positions_num: i32,
    pub strategy_detail: Option<String>,
    pub risk_config_detail: String,
    #[sqlx(default)]
    pub created_at: Option<NaiveDateTime>,
    pub profit: Option<f32>,
    pub one_bar_after_win_rate: Option<f32>,
    pub two_bar_after_win_rate: Option<f32>,
    pub three_bar_after_win_rate: Option<f32>,
    pub four_bar_after_win_rate: Option<f32>,
    pub five_bar_after_win_rate: Option<f32>,
    pub ten_bar_after_win_rate: Option<f32>,
    pub kline_start_time: i64,
    pub kline_end_time: i64,
    pub kline_nums: i32,
}

/// 基于 sqlx 的 BackTestLog Model
pub struct BackTestLogModel;

impl BackTestLogModel {
    /// 添加回测日志记录
    pub async fn add(&self, log: &BackTestLog) -> Result<i64> {
        let pool = get_db_pool();
        let start_time = Instant::now();

        let result = sqlx::query(
            r#"
            INSERT INTO back_test_log (
                strategy_type, inst_type, time, win_rate, final_fund, 
                open_positions_num, strategy_detail, risk_config_detail, 
                profit, one_bar_after_win_rate, two_bar_after_win_rate,
                three_bar_after_win_rate, four_bar_after_win_rate,
                five_bar_after_win_rate, ten_bar_after_win_rate,
                kline_start_time, kline_end_time, kline_nums
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&log.strategy_type)
        .bind(&log.inst_type)
        .bind(&log.time)
        .bind(&log.win_rate)
        .bind(&log.final_fund)
        .bind(&log.open_positions_num)
        .bind(&log.strategy_detail)
        .bind(&log.risk_config_detail)
        .bind(&log.profit)
        .bind(&log.one_bar_after_win_rate)
        .bind(&log.two_bar_after_win_rate)
        .bind(&log.three_bar_after_win_rate)
        .bind(&log.four_bar_after_win_rate)
        .bind(&log.five_bar_after_win_rate)
        .bind(&log.ten_bar_after_win_rate)
        .bind(&log.kline_start_time)
        .bind(&log.kline_end_time)
        .bind(&log.kline_nums)
        .execute(pool)
        .await?;

        let duration = start_time.elapsed();
        let last_id = result.last_insert_id() as i64;

        info!(
            "insert_back_test_log: id={}, 耗时={}ms",
            last_id,
            duration.as_millis()
        );

        Ok(last_id)
    }

    /// 更新持仓统计数据
    pub async fn update_position_stats(
        &self,
        back_test_id: i64,
        stats: PositionStats,
    ) -> Result<u64> {
        let pool = get_db_pool();

        let result = sqlx::query(
            r#"
            UPDATE back_test_log 
            SET
                one_bar_after_win_rate = ?,
                two_bar_after_win_rate = ?,
                three_bar_after_win_rate = ?,
                four_bar_after_win_rate = ?,
                five_bar_after_win_rate = ?,
                ten_bar_after_win_rate = ?
            WHERE id = ?
            "#,
        )
        .bind(&stats.one_bar_after_win_rate)
        .bind(&stats.two_bar_after_win_rate)
        .bind(&stats.three_bar_after_win_rate)
        .bind(&stats.four_bar_after_win_rate)
        .bind(&stats.five_bar_after_win_rate)
        .bind(&stats.ten_bar_after_win_rate)
        .bind(back_test_id)
        .execute(pool)
        .await?;

        let affected = result.rows_affected();

        if affected == 0 {
            warn!(
                "未能更新 back_test_id {} 的统计数据，可能ID不存在",
                back_test_id
            );
        } else {
            info!(
                "成功更新 back_test_id {} 的统计数据: 1K胜率: {:.2}%, 3K胜率: {:.2}%, 5K胜率: {:.2}%, 10K胜率: {:.2}%",
                back_test_id,
                stats.one_bar_after_win_rate * 100.0,
                stats.three_bar_after_win_rate * 100.0,
                stats.five_bar_after_win_rate * 100.0,
                stats.ten_bar_after_win_rate * 100.0
            );
        }

        Ok(affected)
    }

    /// 根据 ID 查询回测日志
    pub async fn find_by_id(&self, id: i64) -> Result<Option<BackTestLog>> {
        let pool = get_db_pool();

        let log = sqlx::query_as::<_, BackTestLog>("SELECT * FROM back_test_log WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;

        Ok(log)
    }

    /// 查询最近的回测日志
    pub async fn find_recent(&self, limit: i32) -> Result<Vec<BackTestLog>> {
        let pool = get_db_pool();

        let logs = sqlx::query_as::<_, BackTestLog>(
            "SELECT * FROM back_test_log ORDER BY created_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;

        Ok(logs)
    }

    /// 根据策略类型查询
    pub async fn find_by_strategy_type(
        &self,
        strategy_type: &str,
        limit: i32,
    ) -> Result<Vec<BackTestLog>> {
        let pool = get_db_pool();

        let logs = sqlx::query_as::<_, BackTestLog>(
            "SELECT * FROM back_test_log WHERE strategy_type = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(strategy_type)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        Ok(logs)
    }
}
