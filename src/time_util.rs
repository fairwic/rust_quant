use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, ParseError, Timelike, TimeZone, Utc};

// 将 DateTime 格式化为周期字符串（如 "4H", "5min" 等）
pub fn format_to_period(period: &str, mut dt: Option<DateTime<Utc>>) -> String {
    if dt.is_none() {
        //当前时间
        dt = Some(Utc::now());
    }

    let dt = dt.unwrap();
    let (num, unit) = period.split_at(period.chars().take_while(|c| c.is_numeric()).count());
    // println!("333333333333333333333333");
    // println!("dt {}", dt);
    // println!("num:{},unit:{}", num, unit);
    let num: i64 = num.parse().unwrap_or(1);

    //转换成小写
    match unit.to_lowercase().as_str() {
        "h" => {
            let hours = dt.hour() / num as u32 * num as u32;
            dt.date_naive().and_hms_opt(hours, 0, 0)
                .unwrap()
                .format("%Y%m%d%H")
                .to_string()
        }
        "min" | "m" => {
            let minutes = dt.minute() / num as u32 * num as u32;
            dt.date_naive().and_hms_opt(dt.hour(), minutes, 0)
                .unwrap()
                .format("%Y%m%d%H%M")
                .to_string()
        }
        "d" => {
            dt.date_naive().and_hms_opt(0, 0, 0)
                .unwrap()
                .format("%Y%m%d")
                .to_string()
        }
        _ => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

// 将时间戳（秒）转换为指定格式的字符串
pub fn timestamp_to_string(timestamp: i64, format: &str) -> String {
    let naive = NaiveDateTime::from_timestamp_opt(timestamp, 0).unwrap();
    let datetime: DateTime<Utc> = DateTime::from_naive_utc_and_offset(naive, Utc);
    datetime.format(format).to_string()
}

// 将时间戳（毫秒）转换为指定格式的字符串
pub fn timestamp_ms_to_string(timestamp_ms: i64, format: &str) -> String {
    let naive = NaiveDateTime::from_timestamp_millis(timestamp_ms).unwrap();
    let datetime: DateTime<Utc> = DateTime::from_naive_utc_and_offset(naive, Utc);
    datetime.format(format).to_string()
}

// 将字符串解析为 DateTime<Utc>
pub fn string_to_datetime(date_str: &str, format: &str) -> Result<DateTime<Utc>, ParseError> {
    let naive = NaiveDateTime::parse_from_str(date_str, format)?;
    Ok(DateTime::from_naive_utc_and_offset(naive, Utc))
}

// 获取当前时间的字符串表示
pub fn now_string(format: &str) -> String {
    Local::now().format(format).to_string()
}

// 计算两个日期之间的天数差
pub fn days_between(start: DateTime<Utc>, end: DateTime<Utc>) -> i64 {
    (end.date_naive() - start.date_naive()).num_days()
}

// 将 DateTime<Utc> 转换为指定时区的字符串
// pub fn datetime_to_timezone_string(dt: DateTime<Utc>, timezone: chrono_tz::Tz, format: &str) -> String {
//     dt.with_timezone(&timezone).format(format).to_string()
// }

// 将 DateTime<Utc> 转换为时间戳（秒）
pub fn datetime_to_timestamp(dt: DateTime<Utc>) -> i64 {
    dt.timestamp()
}

// 将 DateTime<Utc> 转换为时间戳（毫秒）
pub fn datetime_to_timestamp_ms(dt: DateTime<Utc>) -> i64 {
    dt.timestamp_millis()
}

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

pub fn mill_time_to_datetime_shanghai(timestamp_ms: i64) -> Result<String, String> {
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

pub fn millis_time_diff(period: &str) -> i64 {
    // 定义两个时间戳（毫秒）
    let timestamp1: i64 = 1622512800000; // 示例时间戳1
    let timestamp2: i64 = 1622599200000; // 示例时间戳2

    // 尝试将时间戳转换为 DateTime 对象
    let datetime1 = Utc.timestamp_millis_opt(timestamp1).single();
    let datetime2 = Utc.timestamp_millis_opt(timestamp2).single();

    // 检查转换是否成功
    match (datetime1, datetime2) {
        (Some(dt1), Some(dt2)) => {
            // 计算时间差
            let duration = dt2.signed_duration_since(dt1);
            // 根据 period 参数返回相应的时间差
            let diff = match period {
                "H" => duration.num_hours(),
                "m" => duration.num_minutes(),
                "D" => duration.num_days(),
                _ => {
                    panic!("无效的 period 参数");
                }
            };
            println!("时间差（{}）：{}", period, diff);
            diff
        }
        _ => {
            panic!("一个或两个时间戳无效");
        }
    }
}
