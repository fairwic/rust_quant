use chrono::{DateTime, Local, NaiveDateTime, Utc};
use crate::app_config::db;
use crate::time_util;
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::model::strategy::back_test_detail::BackTestDetail;
use crate::trading::model::strategy::position_price_analysis::PositionPriceAnalysis;

#[derive(Debug, Clone)]
pub struct PositionAnalysis {
    pub back_test_id: i32,
    pub inst_id: String,
    pub time_period: String,
    pub option_type: String,
    pub open_time: DateTime<Utc>,
    pub open_price: f64,
    pub bars_after: i32,
    pub price_after: f64,
    pub price_change_percent: f64,
    pub is_profitable: bool,
}

impl PositionAnalysis {
    pub async fn analyze_positions(back_test_id: i32, candles: &[CandlesEntity]) -> Result<(), anyhow::Error> {
        println!("Starting analysis for back_test_id: {}", back_test_id);
        
        let rb = db::get_db_client();
        let sql = format!(
            "SELECT * FROM back_test_detail 
             WHERE back_test_id = {} AND option_type IN ('long', 'short')",
            back_test_id
        );
        println!("Executing SQL query: {}", sql);
        
        match rb.query_decode::<Vec<BackTestDetail>>(&sql, vec![]).await {
            Ok(positions) => {
                println!("Found {} positions to analyze", positions.len());
                let bars_to_analyze = vec![1, 2, 3, 4, 5, 10, 20, 30];
                
                for position in positions {
                    println!("Analyzing position: {:?}", position);
                    let open_price = position.open_price.parse::<f64>()?;
                    
                    match candles.iter().position(|c| {
                        let candle_time = time_util::mill_time_to_datetime_shanghai(c.ts).unwrap();
                        let formatted_position_time = position.open_position_time
                            .split("+")
                            .next()
                            .unwrap()
                            .replace("T", " ");
                        candle_time == formatted_position_time
                    }) {
                        Some(open_index) => {
                            for bars in bars_to_analyze.clone() {
                                if open_index + bars as usize >= candles.len() {
                                    continue;
                                }

                                let future_price = candles[open_index + bars as usize].c.parse::<f64>()?;
                                let price_change = match position.option_type.as_str() {
                                    "long" => (future_price - open_price) / open_price * 100.0,
                                    "short" => (open_price - future_price) / open_price * 100.0,
                                    _ => continue,
                                };

                                let sql = "INSERT INTO position_price_analysis 
                                    (back_test_id, inst_id, time_period, option_type, open_time, 
                                     open_price, bars_after, price_after, price_change_percent, is_profitable)
                                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

                                rb.exec(sql, vec![
                                    back_test_id.to_string().into(),
                                    position.inst_id.clone().into(),
                                    position.time.clone().into(),
                                    position.option_type.clone().into(),
                                    position.open_position_time.clone().into(),
                                    open_price.to_string().into(),
                                    bars.to_string().into(),
                                    future_price.to_string().into(),
                                    price_change.to_string().into(),
                                    (if price_change > 0.0 { 1 } else { 0 }).to_string().into(),
                                ]).await?;
                            }
                        }
                        None => {
                            println!("Cannot find candle for time: {}", position.open_position_time);
                            continue;
                        }
                    }
                }
                Ok(())
            }
            Err(e) => {
                println!("Error executing query: {}", e);
                Err(anyhow::anyhow!("Database query failed: {}", e))
            }
        }
    }
} 