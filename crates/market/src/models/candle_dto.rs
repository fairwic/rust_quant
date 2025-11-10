use serde::{Deserialize, Serialize};

/// 时间方向
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum TimeDirect {
    BEFORE,
    AFTER,
}

/// 选择时间范围
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SelectTime {
    /// 选择开始时间
    pub start_time: i64,
    /// 选择结束时间
    pub end_time: Option<i64>,
    /// 选择方向
    pub direct: TimeDirect,
}

/// 查询 K线 请求 DTO
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SelectCandleReqDto {
    /// 合约ID
    pub inst_id: String,
    /// 时间间隔
    pub time_interval: String,
    /// 默认取最后的条数
    pub limit: usize,
    /// 选择时间
    pub select_time: Option<SelectTime>,
    /// 是否确认
    pub confirm: Option<i8>,
}
