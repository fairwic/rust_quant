use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum TimeDirect {
    BEFORE,
    AFTER,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SelectTime {
    //选择开始时间
    pub start_time: i64,
    //选择结束时间
    pub end_time: Option<i64>,
    //选择方向1 正
    pub direct: TimeDirect,
}
