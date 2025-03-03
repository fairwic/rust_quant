extern crate rbatis;

use std::sync::Arc;
use std::vec;
use anyhow::anyhow;
use rbatis::{crud, impl_insert, RBatis};
use rbs::Value;
use serde_json::json;
use tracing::{debug, warn, info};

use crate::app_config::db;
use crate::trading::model::strategy::back_test_analysis::PositionStats;

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
    pub profit: String,
    // 开仓之后第3根结束时，仓位的是盈利的数/总开仓次数的比例
    pub three_bar_after_win_rate: f32,
    // 开仓之后第5根结束时，仓位的是盈利的数/总开仓次数的比例
    pub five_bar_after_win_rate: f32,
    // 开仓之后第10根结束时，仓位的是盈利的数/总开仓次数的比例
    pub ten_bar_after_win_rate: f32,
}
crud!(BackTestLog{});
impl Default for BackTestLog {
    fn default() -> Self {
        Self {
            strategy_type: "".to_string(),
            inst_type: "".to_string(),
            time: "".to_string(),
            win_rate: "".to_string(),
            final_fund: "".to_string(),
            open_positions_num: 0,
            strategy_detail: None,
            profit: "".to_string(),
            three_bar_after_win_rate: 0.0,
            five_bar_after_win_rate: 0.0,
            ten_bar_after_win_rate: 0.0,
        }
    }
}

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
        // println!("111111111 list:{:#?}", list);
        // println!("db:{:#?}", self.db);
        let mut v1 = vec::Vec::new();
        v1.push(list.clone());

        // let data = BackTestLog::insert_batch(&self.db, &v1, 1).await?;
        let table_name = format!("{}", "back_test_log");
        // 构建批量插入的 SQL 语句
        let mut query = format!("INSERT INTO `{}` (strategy_type, inst_type, time, win_rate, final_fund, open_positions_num, strategy_detail, profit, three_bar_after_win_rate, five_bar_after_win_rate, ten_bar_after_win_rate) VALUES ", table_name);
        let mut params = Vec::new();

        for candle in v1 {
            query.push_str("(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?),");
            params.push(candle.strategy_type.to_string().into());
            params.push(candle.inst_type.to_string().into());
            params.push(candle.time.to_string().into());
            params.push(candle.win_rate.to_string().into());
            params.push(candle.final_fund.to_string().into());
            params.push(candle.open_positions_num.to_string().into());
            params.push(candle.strategy_detail.unwrap_or_default().to_string().into());
            params.push(candle.profit.to_string().into());
            params.push(candle.three_bar_after_win_rate.to_string().into());
            params.push(candle.five_bar_after_win_rate.to_string().into());
            params.push(candle.ten_bar_after_win_rate.to_string().into());
        }

        // 移除最后一个逗号
        query.pop();

        debug!("insert_back_test_log_quey = {}", query);
        let data = self.db.exec(&query, params).await?;
        // Ok(res
        debug!("insert_back_test_log_result = {}", json!(data));
        Ok(data.last_insert_id.as_i64().unwrap())
    }
    
    // 更新持仓统计数据
    pub async fn update_position_stats(&self, back_test_id: i64, stats: PositionStats) -> anyhow::Result<u64> {
        debug!("更新 back_test_id {} 的统计数据: 3K胜率: {:.4}, 5K胜率: {:.4}, 10K胜率: {:.4}", 
               back_test_id, 
               stats.three_bar_after_win_rate,
               stats.five_bar_after_win_rate,
               stats.ten_bar_after_win_rate);
        
        let sql = r#"
            UPDATE back_test_log 
            SET three_bar_after_win_rate = ?,
                five_bar_after_win_rate = ?,
                ten_bar_after_win_rate = ?
            WHERE id = ?
        "#;
        
        let params = vec![
            stats.three_bar_after_win_rate.to_string().into(),
            stats.five_bar_after_win_rate.to_string().into(),
            stats.ten_bar_after_win_rate.to_string().into(),
            back_test_id.to_string().into(),
        ];
        
        let result = self.db.exec(sql, params).await?;
        debug!("更新 back_test_log id {} 的统计数据结果: 影响行数 {}", back_test_id, result.rows_affected);
        
        if result.rows_affected == 0 {
            warn!("未能更新 back_test_id {} 的统计数据，可能ID不存在", back_test_id);
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
