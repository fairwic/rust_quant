use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};

pub fn mill_time_to_datetime(timestamp_ms: i64) -> Result<String, String> {
    // 将毫秒级时间戳转换为 DateTime<Utc>
    match Utc.timestamp_millis_opt(timestamp_ms) {
        chrono::LocalResult::Single(datetime) => {
            // 格式化时间为字符串
            let formatted_datetime = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
            Ok(formatted_datetime)
        }
        chrono::LocalResult::None => Err("Invalid timestamp: None".to_string()),
        chrono::LocalResult::Ambiguous(_, _) => Err("Invalid timestamp: Ambiguous".to_string()),
    }
}

pub fn mill_time_to_datetime_SHANGHAI(timestamp_ms: i64) -> Result<String, String> {
    // 将毫秒级时间戳转换为 DateTime<Utc>
    match Utc.timestamp_millis_opt(timestamp_ms) {
        chrono::LocalResult::Single(datetime) => {
            // 假设时间戳是UTC时间，转换为东八区时间
            let offset = FixedOffset::east(8 * 3600);
            let local_datetime = datetime.with_timezone(&offset);

            // 格式化时间为字符串
            let formatted_datetime = local_datetime.format("%Y-%m-%d %H:%M:%S").to_string();
            Ok(formatted_datetime)
        }
        chrono::LocalResult::None => Err("Invalid timestamp: None".to_string()),
        chrono::LocalResult::Ambiguous(_, _) => Err("Invalid timestamp: Ambiguous".to_string()),
    }
}
