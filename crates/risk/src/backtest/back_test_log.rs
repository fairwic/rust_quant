

use anyhow::anyhow;
{crud, impl_insert, RBatis};
use rbs::Value;
use serde_json::json;
use std::sync::Arc;
use std::vec;
use tracing::{debug, info, warn};

use rust_quant_core::config::db;
use rust_quant_common::model::strategy::back_test_analysis::PositionStats;
use std::time::Instant;

/// table
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BackTestLog {
    pub strategy_type: String,
    pub inst_type: String,
    pub time: String,
    pub win_rate: String,
    pub final_fund: String,
    pub open_positions_num: i32,
    pub strategy_detail: Option<String>,
    pub risk_config_detail: String,
    pub profit: String,
    pub one_bar_after_win_rate: f32,
    pub two_bar_after_win_rate: f32,
    // 开仓之后第3根结束时，仓位的是盈利的数/总开仓次数的比例
    pub three_bar_after_win_rate: f32,
    pub four_bar_after_win_rate: f32,
    // 开仓之后第5根结束时，仓位的是盈利的数/总开仓次数的比例
    pub five_bar_after_win_rate: f32,
    // 开仓之后第10根结束时，仓位的是盈利的数/总开仓次数的比例
    pub ten_bar_after_win_rate: f32,
    pub kline_start_time: i64,
    pub kline_end_time: i64,
    pub kline_nums: i32,
}
// ORM迁移TODO

pub struct BackTestLogModel {
    db: &'static RBatis,
}

impl BackTestLogModel {
    pub async fn new() -> BackTestLogModel {
        Self {
            db: db::get_db_client(),
        }
    }
    pub async fn add(&self, list: &BackTestLog) -> anyhow::Result<i64> {
        let mut v1 = vec::Vec::new();
        v1.push(list.clone());

        // let data = BackTestLog::insert_batch(&self.db, &v1, 1).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        let table_name = format!("{}", "back_test_log");
        // 构建批量插入的 SQL 语句
        let mut query = format!("INSERT INTO `{}` (strategy_type, inst_type, time, win_rate, final_fund, open_positions_num, strategy_detail, risk_config_detail, profit, three_bar_after_win_rate, five_bar_after_win_rate, ten_bar_after_win_rate, kline_start_time, kline_end_time, kline_nums) VALUES ", table_name);
        let mut params = Vec::new();

        for candle in v1 {
            query.push_str("(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?),");
            params.push(candle.strategy_type.to_string().into());
            params.push(candle.inst_type.to_string().into());
            params.push(candle.time.to_string().into());
            params.push(candle.win_rate.to_string().into());
            params.push(candle.final_fund.to_string().into());
            params.push(candle.open_positions_num.to_string().into());
            params.push(
                candle
                    .strategy_detail
                    .unwrap_or_default()
                    .to_string()
                    .into(),
            );
            params.push(candle.risk_config_detail.to_string().into());
            params.push(candle.profit.to_string().into());
            params.push(candle.three_bar_after_win_rate.to_string().into());
            params.push(candle.five_bar_after_win_rate.to_string().into());
            params.push(candle.ten_bar_after_win_rate.to_string().into());
            params.push(candle.kline_start_time.to_string().into());
            params.push(candle.kline_end_time.to_string().into());
            params.push(candle.kline_nums.to_string().into());
        }

        // 移除最后一个逗号
        query.pop();

        // info!("insert_back_test_log_quey = {}", query);
        let start_time = Instant::now();
        let data = self.db.exec(&query, params).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        let duration = start_time.elapsed();
        // let res = format!("insert_back_test_log_result = 执行时间{}毫秒 影响行数{}", duration.as_millis(), data.rows_affected);
        // info!("{}", res);
        Ok(data.last_insert_id.as_i64().unwrap())
    }

    // 更新持仓统计数据
    pub async fn update_position_stats(
        &self,
        back_test_id: i64,
        stats: PositionStats,
    ) -> anyhow::Result<u64> {
        let sql = r#"
            UPDATE back_test_log 
            SET
                one_bar_after_win_rate = ?,
                two_bar_after_win_rate = ?,
                three_bar_after_win_rate = ?,
                five_bar_after_win_rate = ?,
                four_bar_after_win_rate = ?,
                ten_bar_after_win_rate = ?
            WHERE id = ?
        "#;

        let params = vec![
            stats.one_bar_after_win_rate.to_string().into(),
            stats.two_bar_after_win_rate.to_string().into(),
            stats.three_bar_after_win_rate.to_string().into(),
            stats.four_bar_after_win_rate.to_string().into(),
            stats.five_bar_after_win_rate.to_string().into(),
            stats.ten_bar_after_win_rate.to_string().into(),
            back_test_id.to_string().into(),
        ];

        let result = self.db.exec(sql, params).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        debug!(
            "更新 back_test_log id {} 的统计数据结果: 影响行数 {}",
            back_test_id, result.rows_affected
        );

        if result.rows_affected == 0 {
            warn!(
                "未能更新 back_test_id {} 的统计数据，可能ID不存在",
                back_test_id
            );
        } else {
            info!("成功更新 back_test_id {} 的统计数据: 3K胜率: {:.2}%, 5K胜率: {:.2}%, 10K胜率: {:.2}%", 
                  back_test_id, 
                  stats.three_bar_after_win_rate * 100.0,
                  stats.five_bar_after_win_rate * 100.0,
                  stats.ten_bar_after_win_rate * 100.0);
        }

        Ok(result.rows_affected)
    }
}
