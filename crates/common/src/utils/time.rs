use anyhow::anyhow;
use chrono::{
    DateTime, Datelike, FixedOffset, Local, NaiveDateTime, ParseError, TimeZone, Timelike, Utc,
};
// 移除 rbatis 依赖，使用 chrono 的 NaiveDateTime 替代 Timestamp
use tracing::warn;

pub(crate) fn is_within_business_hours(ts: i64) -> bool {
    // 获取当前UTC时间
    let now_utc: DateTime<Utc> = DateTime::from_timestamp_millis(ts).unwrap();
    // 定义美国东部时间的偏移量
    // EST（标准时间）为UTC-5，EDT（夏令时）为UTC-4
    let est_offset = FixedOffset::west_opt(3 * 3600).unwrap(); // 偏移量为-5小时
    let edt_offset = FixedOffset::west_opt(3 * 3600).unwrap(); // 偏移量为-4小时

    // 判断当前时间是否在夏令时范围内
    let now_local: DateTime<Local> = Local::now();
    let is_dst = now_local.offset().local_minus_utc() == -4 * 3600;
    // 根据是否夏令时选择正确的偏移量

    let est_or_edt_offset = if is_dst { edt_offset } else { est_offset };
    // 将UTC时间转换为美国东部时间
    let now_washington_time = now_utc.with_timezone(&est_or_edt_offset);
    // 判断转换后的时间是否在早上7点到晚上22点之间
    let hour = now_washington_time.hour();
    let in_with_hour = hour >= 7 && hour < 22;
    let day_week = now_washington_time.weekday().number_from_monday();
    let is_saturday = day_week == 5;
    if is_saturday {
        warn!(
            "time is not within business hours or in saturday hour:{},day_week:{}",
            hour, day_week
        );
    }
    !is_saturday && in_with_hour
}

/// 解析周期字符串为毫秒数
pub(crate) fn parse_period_to_mill(period: &str) -> anyhow::Result<i64> {
    let duration = match &period.to_uppercase()[..] {
        "1S" => 1,
        "1M" => 60,
        "3M" => 3 * 60,
        "5M" => 5 * 60,
        "15M" => 15 * 60,
        "1H" => 3600,
        "4H" => 4 * 3600,
        "1D" => 24 * 3600,
        "1DUTC" => 24 * 3600,
        "5D" => 5 * 24 * 3600,
        _ => return Err(anyhow!("Unsupported period format{}", period)),
    };
    Ok(duration * 1000) // 转换为毫秒
}

///当前毫秒级时间增加或减少指定周期的毫秒数
pub fn ts_reduce_n_period(ts: i64, period: &str, n: usize) -> anyhow::Result<i64> {
    let res = parse_period_to_mill(period);
    //最大条数100
    let mill = n as i64 * res.unwrap();
    Ok(ts - mill)
}

///当前毫秒级时间增加或减少指定周期的毫秒数
pub fn ts_add_n_period(ts: i64, period: &str, n: usize) -> anyhow::Result<i64> {
    let res = parse_period_to_mill(period);
    //最大条数100
    let mill = n as i64 * res.unwrap();
    Ok(ts + mill)
}

///
pub fn format_to_period_str(period: &str) -> String {
    let dt = Local::now();
    let (num, unit) = period.split_at(period.chars().take_while(|c| c.is_numeric()).count());
    // println!("333333333333333333333333");
    // println!("dt {}", dt);
    // println!("num:{},unit:{}", num, unit);
    let num: i64 = num.parse().unwrap_or(1);
    //转换成小写
    match unit.to_lowercase().as_str() {
        "h" => {
            let hours = dt.hour() / num as u32 * num as u32;
            dt.date_naive()
                .and_hms_opt(hours, 0, 0)
                .unwrap()
                .format("%Y%m%d%H0000")
                .to_string()
        }
        "min" | "m" => {
            let minutes = dt.minute() / num as u32 * num as u32;
            dt.date_naive()
                .and_hms_opt(dt.hour(), minutes, 0)
                .unwrap()
                .format("%Y%m%d%H%M00")
                .to_string()
        }
        "d" => dt
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .format("%Y%m%d000000")
            .to_string(),
        _ => dt.format("%Y-%m-%d %H:%M:%S000000").to_string(),
    }
}

// 将 DateTime 格式化为周期字符串（如 "4H", "5min" 等）
pub fn format_to_period(period: &str, mut dt: Option<DateTime<Local>>) -> String {
    if dt.is_none() {
        //当前时间
        dt = Some(Local::now());
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
            dt.date_naive()
                .and_hms_opt(hours, 0, 0)
                .unwrap()
                .format("%Y%m%d%H")
                .to_string()
        }
        "min" | "m" => {
            let minutes = dt.minute() / num as u32 * num as u32;
            dt.date_naive()
                .and_hms_opt(dt.hour(), minutes, 0)
                .unwrap()
                .format("%Y%m%d%H%M")
                .to_string()
        }
        "d" => dt
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .format("%Y%m%d")
            .to_string(),
        _ => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

// 将时间戳（秒）转换为指定格式的字符串
pub fn timestamp_to_string(timestamp: i64, format: &str) -> String {
    let naive = DateTime::from_timestamp(timestamp, 0).unwrap().naive_utc();
    let datetime: DateTime<Utc> = DateTime::from_naive_utc_and_offset(naive, Utc);
    datetime.format(format).to_string()
}

// 将时间戳（毫秒）转换为指定格式的字符串
pub fn timestamp_ms_to_string(timestamp_ms: i64, format: &str) -> String {
    let naive = DateTime::from_timestamp_millis(timestamp_ms)
        .unwrap()
        .naive_utc();
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

//获取当前毫秒级时间戳
// return "1735632797959"
pub fn now_timestamp_mills() -> String {
    // 获取当前 UTC 时间
    let now = Local::now();
    // 获取当前时间的时间戳（毫秒）
    now.timestamp_millis().to_string()
}

/// 判断指定时间戳是否是周期的开始时间戳
pub fn ts_is_match_period(ts: i64, period: &str) -> bool {
    let period_start = get_period_start_timestamp(period);
    period_start == ts
}

/// 获取指定周期的开始时间戳
/// 例如：周期为 1小时（1h），5分钟（5m），1天（1D）等
pub fn get_period_start_timestamp(period: &str) -> i64 {
    // 获取当前 UTC 时间
    let now = Local::now();
    // 获取当前的时间戳，并根据周期进行调整
    let period_start = match period {
        "1m" => now
            .with_minute(now.minute())
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap(),
        "3m" => now
            .with_minute(now.minute() / 3 * 3)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap(),
        "5m" => now
            .with_minute(now.minute() / 5 * 5)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap(),
        "1H" => now
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap(),
        "4H" => now
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_hour(now.hour() / 4 * 4)
            .unwrap()
            .with_nanosecond(0)
            .unwrap(),
        "6H" => now
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_hour(now.hour() / 6 * 6)
            .unwrap()
            .with_nanosecond(0)
            .unwrap(),
        "1D" | "1Dutc" => now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap()
            .with_nanosecond(0)
            .unwrap(),
        "4D" => now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap()
            .with_day(now.day() / 4 * 4)
            .unwrap()
            .with_nanosecond(0)
            .unwrap(),
        _ => panic!("Unsupported period: {}", period),
    };
    // 返回周期开始时间的毫秒级时间戳
    period_start.timestamp_millis()
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
    // 将毫秒级时间戳转换为秒级
    let seconds = timestamp_ms / 1000;
    // 创建 DateTime<Local> 对象
    let datetime = Local.timestamp_opt(seconds, 0).unwrap();
    Ok(datetime.format("%Y-%m-%d %H:%M:%S").to_string())
}

pub fn mill_time_to_local_datetime(timestamp_ms: i64) -> DateTime<Local> {
    // 创建 DateTime<Local> 对象
    let datetime = Local.timestamp_millis_opt(timestamp_ms);
    datetime.unwrap()
}

pub fn mill_time_to_datetime_shanghai(timestamp_ms: i64) -> Result<String, String> {
    // 将毫秒级时间戳转换为 DateTime<Utc>
    match Utc.timestamp_millis_opt(timestamp_ms) {
        chrono::LocalResult::Single(datetime) => {
            // 假设时间戳是UTC时间，转换为东八区时间
            let offset = FixedOffset::east_opt(8 * 3600).unwrap();
            let local_datetime = datetime.with_timezone(&offset);

            // 格式化时间为字符串
            let formatted_datetime = local_datetime.format("%Y-%m-%d %H:%M:%S").to_string();
            Ok(formatted_datetime)
        }
        chrono::LocalResult::None => Err("Invalid timestamp: None".to_string()),
        chrono::LocalResult::Ambiguous(_, _) => Err("Invalid timestamp: Ambiguous".to_string()),
    }
}

pub fn millis_time_diff(period: &str, timestamp1: i64, timestamp2: i64) -> i64 {
    // 定义两个时间戳（毫秒）

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
                "1H" => duration.num_hours(),
                "2H" => duration.num_hours() / 2,
                "4H" => duration.num_hours() / 4,
                "1m" => duration.num_minutes(),
                "1D" => duration.num_days(),
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
