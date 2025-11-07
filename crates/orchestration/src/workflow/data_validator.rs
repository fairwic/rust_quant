use anyhow::{anyhow, Result};
use chrono::{DateTime, Local};
use tracing::error;

use rust_quant_common::utils::time;
use rust_quant_market::models::CandlesEntity;

/// 判断最新得数据是否所在当前时间的周期
pub fn valid_newest_candle_data(mysql_candles_5m: CandlesEntity, time: &str) -> bool {
    let ts = mysql_candles_5m.ts;
    // 将毫秒时间戳转换为 DateTime<Utc>
    let datetime: DateTime<Local> = time_util::mill_time_to_local_datetime(ts);

    let date = time_util::format_to_period(time, Some(datetime));
    let current_date = time_util::format_to_period(time, None);
    // 比较时间戳的小时与当前小时
    if date != current_date {
        error!("数据库最新数据的时间ts:{} date:({}) 不等于当前最新时间 local time:({}), 跳过,candles:{:?},time:{}", ts, date, current_date, mysql_candles_5m, time);
        return false;
    }
    true
}

/// 验证蜡烛图数据是否正确
pub fn valid_candles_data(mysql_candles_5m: &Vec<CandlesEntity>, time: &str) -> Result<()> {
    //验证头尾数据正确性
    let first_timestamp = mysql_candles_5m.first().unwrap().ts;
    let last_timestamp = mysql_candles_5m.last().unwrap().ts;
    let difference = last_timestamp - first_timestamp;
    let period_milliseconds = time_util::parse_period_to_mill(time)?;
    let expected_length = difference / period_milliseconds;
    
    if expected_length != (mysql_candles_5m.len() - 1) as i64 {
        let mut discontinuities = Vec::new();
        //获取哪个数据的不连续
        for window in mysql_candles_5m.windows(2) {
            let current = &window[0];
            let next = &window[1];
            let expected_next_ts = current.ts + period_milliseconds;
            if next.ts != expected_next_ts {
                discontinuities.push(expected_next_ts);
            }
        }
        return Err(anyhow!(
            "The difference between the first and last timestamps is not consistent with the period. Expected: {}, Actual: {},discontinuities:{:?}",
            expected_length,
            mysql_candles_5m.len() - 1,
            discontinuities
        ));
    }
    Ok(())
}
