use rust_quant_core::config::db;
use rust_quant_common::model::strategy::back_test_detail::BackTestDetail;
impl_select;
RBatis;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::debug;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackTestAnalysis {
    pub id: Option<i32>,
    pub back_test_id: i32,
    pub inst_id: String,
    pub time: String,
    pub option_type: String,
    pub open_position_time: String,
    pub open_price: String,
    pub bars_after: i32,
    pub price_after: String,
    pub price_change_percent: String,
    pub is_profitable: i32,
    pub created_at: Option<String>,
}
rbatis::// ORM迁移TODO

// 持仓统计结果
#[derive(Debug)]
pub struct PositionStats {
    pub one_bar_after_win_rate: f32,
    pub two_bar_after_win_rate: f32,
    pub three_bar_after_win_rate: f32,
    pub four_bar_after_win_rate: f32,
    pub five_bar_after_win_rate: f32,
    pub ten_bar_after_win_rate: f32,
}

// 分析模型，处理查询和插入
pub struct BackTestAnalysisModel {
    db: &'static RBatis,
}

impl BackTestAnalysisModel {
    pub async fn new() -> Self {
        Self {
            db: db::get_db_client(),
        }
    }

    // 查询指定回测的持仓记录
    pub async fn find_positions(&self, back_test_id: i32) -> anyhow::Result<Vec<BackTestDetail>> {
        let positions = BackTestDetail::select_positions(self.db, back_test_id).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        Ok(positions)
    }

    // 批量插入分析结果
    pub async fn batch_insert(&self, analyses: Vec<BackTestAnalysis>) -> anyhow::Result<u64> {
        if analyses.is_empty() {
            return Ok(0);
        }
        let table_name = "back_test_analysis";
        // 构建批量插入的 SQL 语句
        let mut query = format!(
            "INSERT INTO `{}` (back_test_id, inst_id, time, option_type, open_position_time, \
            open_price, bars_after, price_after, price_change_percent, is_profitable) VALUES ",
            table_name
        );
        let mut params = Vec::new();

        for analysis in analyses {
            query.push_str("(?, ?, ?, ?, ?, ?, ?, ?, ?, ?),");
            params.push(analysis.back_test_id.to_string().into());
            params.push(analysis.inst_id.into());
            params.push(analysis.time.into());
            params.push(analysis.option_type.into());
            params.push(analysis.open_position_time.into());
            params.push(analysis.open_price.into());
            params.push(analysis.bars_after.to_string().into());
            params.push(analysis.price_after.into());
            params.push(analysis.price_change_percent.into());
            params.push(analysis.is_profitable.to_string().into());
        }

        // 移除最后一个逗号
        query.pop();
        let result = self.db.exec(&query, params).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        debug!("batch_insert_analysis_result = {}", json!(result));
        Ok(result.rows_affected)
    }

    // 计算持仓统计数据
    pub async fn calculate_position_stats(
        &self,
        back_test_id: i32,
    ) -> anyhow::Result<PositionStats> {
        debug!("计算 back_test_id {} 的K线后胜率统计", back_test_id);
        // 计算1根K线后的胜率
        let one_bar_stats = self.calculate_win_rate_after_bars(back_test_id, 1).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        debug!(
            "back_test_id {} 的1K后胜率: {:.4}",
            back_test_id, one_bar_stats
        );

        // 计算2根K线后的胜率
        let two_bar_stats = self.calculate_win_rate_after_bars(back_test_id, 2).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        debug!(
            "back_test_id {} 的2K后胜率: {:.4}",
            back_test_id, two_bar_stats
        );

        // 计算3根K线后的胜率
        let three_bar_stats = self.calculate_win_rate_after_bars(back_test_id, 3).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        debug!(
            "back_test_id {} 的3K后胜率: {:.4}",
            back_test_id, three_bar_stats
        );

        // 计算4根K线后的胜率
        let four_bar_stats = self.calculate_win_rate_after_bars(back_test_id, 4).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        debug!(
            "back_test_id {} 的4K后胜率: {:.4}",
            back_test_id, four_bar_stats
        );

        // 计算5根K线后的胜率
        let five_bar_stats = self.calculate_win_rate_after_bars(back_test_id, 5).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        debug!(
            "back_test_id {} 的5K后胜率: {:.4}",
            back_test_id, five_bar_stats
        );

        // 计算10根K线后的胜率
        let ten_bar_stats = self.calculate_win_rate_after_bars(back_test_id, 10).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        debug!(
            "back_test_id {} 的10K后胜率: {:.4}",
            back_test_id, ten_bar_stats
        );

        let stats = PositionStats {
            one_bar_after_win_rate: one_bar_stats,
            two_bar_after_win_rate: two_bar_stats,
            three_bar_after_win_rate: three_bar_stats,
            four_bar_after_win_rate: four_bar_stats,
            five_bar_after_win_rate: five_bar_stats,
            ten_bar_after_win_rate: ten_bar_stats,
        };
        Ok(stats)
    }

    // 计算指定K线数后的胜率
    async fn calculate_win_rate_after_bars(
        &self,
        back_test_id: i32,
        bars: i32,
    ) -> anyhow::Result<f32> {
        let sql = r#"
            SELECT 
                COUNT(*) as total_positions,
                SUM(is_profitable) as profitable_positions
            FROM back_test_analysis
            WHERE back_test_id = ? AND bars_after = ?
        "#;

        let params = vec![back_test_id.to_string().into(), bars.to_string().into()];
        let result = self
            .db
            .query_decode::<Vec<serde_json::Value>>(sql, params)
            .await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;

        if result.is_empty() {
            debug!("back_test_id {} 的{}K后统计数据为空", back_test_id, bars);
            return Ok(0.0);
        }
        println!(
            "back_test_id {} 的{}K后统计: {:?}",
            back_test_id, bars, result
        );
        let row = &result[0];
        println!("row:{:?}", row);

        // 修复解析逻辑，处理字符串类型
        let total_positions = row["total_positions"].as_i64().unwrap_or_else(|| {
            row["total_positions"]
                .as_str()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0)
        });

        let profitable_positions = row["profitable_positions"].as_i64().unwrap_or_else(|| {
            row["profitable_positions"]
                .as_str()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0)
        });

        println!(
            "back_test_id {} 的{}K后统计: 总持仓 {}, 盈利持仓 {}",
            back_test_id, bars, total_positions, profitable_positions
        );

        if total_positions == 0 {
            debug!("back_test_id {} 的{}K后无持仓数据", back_test_id, bars);
            return Ok(0.0);
        }

        let win_rate = (profitable_positions as f32) / (total_positions as f32);
        debug!(
            "back_test_id {} 的{}K后胜率: {:.4} ({}/{})",
            back_test_id, bars, win_rate, profitable_positions, total_positions
        );

        Ok(win_rate)
    }
}
