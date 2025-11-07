use anyhow::Result;
use chrono::NaiveDateTime;
use rust_quant_core::database::get_db_pool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, MySql, QueryBuilder};
use tracing::debug;

/// 回测分析记录
#[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
pub struct BackTestAnalysis {
    #[sqlx(default)]
    pub id: Option<i32>,
    pub back_test_id: i32,
    pub inst_id: String,
    pub time: String,
    pub option_type: String,
    pub open_position_time: Option<String>,
    pub open_price: String,
    pub bars_after: i32,
    pub price_after: String,
    pub price_change_percent: String,
    pub is_profitable: i8, // tinyint
    #[sqlx(default)]
    pub created_at: Option<NaiveDateTime>,
}

/// 持仓统计结果
#[derive(Debug, Serialize, Deserialize)]
pub struct PositionStats {
    pub one_bar_after_win_rate: f32,
    pub two_bar_after_win_rate: f32,
    pub three_bar_after_win_rate: f32,
    pub four_bar_after_win_rate: f32,
    pub five_bar_after_win_rate: f32,
    pub ten_bar_after_win_rate: f32,
}

/// 基于 sqlx 的 BackTestAnalysis Model
pub struct BackTestAnalysisModel;

impl BackTestAnalysisModel {
    /// 批量插入分析结果
    pub async fn batch_insert(&self, analyses: Vec<BackTestAnalysis>) -> Result<u64> {
        if analyses.is_empty() {
            return Ok(0);
        }

        let pool = get_db_pool();
        let mut total_affected = 0u64;

        // 分批插入，每批 100 条
        const BATCH_SIZE: usize = 100;
        for chunk in analyses.chunks(BATCH_SIZE) {
            let mut query_builder = QueryBuilder::<MySql>::new(
                r#"INSERT INTO back_test_analysis (
                    back_test_id, inst_id, time, option_type, open_position_time,
                    open_price, bars_after, price_after, price_change_percent, is_profitable
                ) "#,
            );

            query_builder.push_values(chunk, |mut b, analysis| {
                b.push_bind(&analysis.back_test_id)
                    .push_bind(&analysis.inst_id)
                    .push_bind(&analysis.time)
                    .push_bind(&analysis.option_type)
                    .push_bind(&analysis.open_position_time)
                    .push_bind(&analysis.open_price)
                    .push_bind(&analysis.bars_after)
                    .push_bind(&analysis.price_after)
                    .push_bind(&analysis.price_change_percent)
                    .push_bind(&analysis.is_profitable);
            });

            let result = query_builder.build().execute(pool).await?;
            total_affected += result.rows_affected();
        }

        debug!(
            "batch_insert_analysis_result = {}",
            json!({"total": analyses.len(), "affected": total_affected})
        );
        Ok(total_affected)
    }

    /// 查询指定回测的分析记录
    pub async fn find_by_back_test_id(&self, back_test_id: i32) -> Result<Vec<BackTestAnalysis>> {
        let pool = get_db_pool();

        let analyses = sqlx::query_as::<_, BackTestAnalysis>(
            "SELECT * FROM back_test_analysis WHERE back_test_id = ? ORDER BY open_position_time ASC",
        )
        .bind(back_test_id)
        .fetch_all(pool)
        .await?;

        Ok(analyses)
    }

    /// 计算持仓统计数据
    pub async fn calculate_position_stats(&self, back_test_id: i32) -> Result<PositionStats> {
        debug!("计算 back_test_id {} 的K线后胜率统计", back_test_id);

        // 并发计算所有胜率
        let (one, two, three, four, five, ten) = tokio::join!(
            self.calculate_win_rate_after_bars(back_test_id, 1),
            self.calculate_win_rate_after_bars(back_test_id, 2),
            self.calculate_win_rate_after_bars(back_test_id, 3),
            self.calculate_win_rate_after_bars(back_test_id, 4),
            self.calculate_win_rate_after_bars(back_test_id, 5),
            self.calculate_win_rate_after_bars(back_test_id, 10)
        );

        let stats = PositionStats {
            one_bar_after_win_rate: one?,
            two_bar_after_win_rate: two?,
            three_bar_after_win_rate: three?,
            four_bar_after_win_rate: four?,
            five_bar_after_win_rate: five?,
            ten_bar_after_win_rate: ten?,
        };

        Ok(stats)
    }

    /// 计算指定K线数后的胜率
    async fn calculate_win_rate_after_bars(&self, back_test_id: i32, bars: i32) -> Result<f32> {
        let pool = get_db_pool();

        #[derive(FromRow)]
        struct WinRateStats {
            total_positions: i64,
            profitable_positions: Option<i64>,
        }

        let stats = sqlx::query_as::<_, WinRateStats>(
            r#"
            SELECT 
                COUNT(*) as total_positions,
                SUM(is_profitable) as profitable_positions
            FROM back_test_analysis
            WHERE back_test_id = ? AND bars_after = ?
            "#,
        )
        .bind(back_test_id)
        .bind(bars)
        .fetch_one(pool)
        .await?;

        if stats.total_positions == 0 {
            debug!(
                "back_test_id {} 的{}K后无持仓数据",
                back_test_id, bars
            );
            return Ok(0.0);
        }

        let profitable_positions = stats.profitable_positions.unwrap_or(0);
        let win_rate = (profitable_positions as f32) / (stats.total_positions as f32);

        debug!(
            "back_test_id {} 的{}K后胜率: {:.4} ({}/{})",
            back_test_id, bars, win_rate, profitable_positions, stats.total_positions
        );

        Ok(win_rate)
    }

    /// 删除指定回测的分析记录
    pub async fn delete_by_back_test_id(&self, back_test_id: i32) -> Result<u64> {
        let pool = get_db_pool();

        let result = sqlx::query("DELETE FROM back_test_analysis WHERE back_test_id = ?")
            .bind(back_test_id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }
}
