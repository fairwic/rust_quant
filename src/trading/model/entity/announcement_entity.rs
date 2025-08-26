extern crate rbatis;

use chrono::Utc;
use rbatis::impl_select;
use rbatis::rbdc::DateTime;
use rbatis::{crud, impl_insert, impl_update, RBatis};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;

/// table
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Announcement {
    #[serde(skip_deserializing, skip_serializing)]
    pub id: i64,
    pub uuid: String,
    pub ann_type: String,
    pub p_time: u64,
    pub title: String,
    pub url: String,
    pub plate_type: String,
    #[serde(skip_deserializing, default = "default_created_time")]
    pub created_time: DateTime,
}

fn default_id() -> i64 {
    0
}

fn default_created_time() -> DateTime {
    DateTime::now()
}

fn parse_datetime<'de, D>(deserializer: D) -> Result<DateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let ts = s.parse::<i64>().map_err(serde::de::Error::custom)?;
    Ok(DateTime::from_timestamp_millis(ts))
}

crud!(Announcement {});
impl_select!(Announcement{select_by_uid(uid:&str) ->Option => "`where uuid =#{uid} limit 1`"});
