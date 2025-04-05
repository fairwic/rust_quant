use crate::trading::okx::public_data::OkxPublicData;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

pub mod account;
pub mod asset;
pub mod big_data;
pub mod market;
pub mod okx_client;
pub mod okx_websocket;
pub mod okx_websocket_client;
pub mod public_data;
pub mod trade;
pub mod error;


// 验证系统时间
pub async fn validate_system_time() {
    let time_str = OkxPublicData::get_time().await;
    debug!("获取okx系统时间: {:?}", time_str);
    if let Ok(time_str) = time_str {
        let time = time_str.parse::<i64>().unwrap();
        let time = DateTime::<Utc>::from_utc(
            chrono::NaiveDateTime::from_timestamp(time / 1000, ((time % 1000) * 1_000_000) as u32),
            Utc,
        );

        let now = Utc::now().timestamp_millis();
        let okx_time = time.timestamp_millis();
        let time_diff = (now - okx_time).abs();
        if time_diff < 20000 {
            info!("时间间隔相差值: {} 毫秒", time_diff);
        } else {
            info!("时间未同步，时间间隔相差值: {} 毫秒", time_diff);
        }
    }
}
