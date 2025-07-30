use super::enums::SelectTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SelectCandleReqDto {
    // 合约ID
    pub inst_id: String,
    // 时间间隔
    pub time_interval: String,
    // 默认取最后的条数
    pub limit: usize,
    // 选择时间
    pub select_time: Option<SelectTime>,
    // 是否确认
    pub confirm: Option<i8>,
}
