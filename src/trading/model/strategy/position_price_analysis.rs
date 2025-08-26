use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionPriceAnalysis {
    pub id: Option<i32>,
    pub back_test_id: i32,
    pub inst_id: String,
    pub time_period: String,
    pub option_type: String,
    pub open_time: String,
    pub open_price: String,
    pub bars_after: i32,
    pub price_after: String,
    pub price_change_percent: String,
    pub is_profitable: i32,
    pub created_at: Option<String>,
}

// impl CRUDTable for PositionPriceAnalysis {
//     fn table_name() -> String {
//         "position_price_analysis".to_string()
//     }
// }

// impl Skip for PositionPriceAnalysis {
//     fn skip_serialize_field(field_name: &str) -> bool {
//         matches!(field_name, "id" | "created_at")
//     }
// }
