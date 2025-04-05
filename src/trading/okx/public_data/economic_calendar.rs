use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::trading::okx::okx_client;
use crate::trading::okx::okx_client::OkxApiResponse;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EconomicCalendarData {
    #[serde(rename = "calendarId")]
    pub calendar_id: String, // 经济日历ID
    pub date: String,     // actual字段值的预期发布时间，Unix时间戳的毫秒数格式
    pub region: String,   // 国家，地区或实体
    pub category: String, // 类别名
    pub event: String,    // 事件名
    #[serde(rename = "refDate")]
    pub ref_date: String, // 当前事件指向的日期
    pub actual: String,   // 事件实际值
    pub previous: String, // 当前事件上个周期的最新实际值，若发生数据修正，该字段存储上个周期修正后的实际值
    pub forecast: String, // 由权威经济学家共同得出的预测值
    #[serde(rename = "dateSpan")]
    pub date_span: String, // 0：事件的具体发生时间已知；1：事件的具体发生日期已知，但时间未知
    pub importance: String, // 重要性：1-低，2-中等，3-高
    #[serde(rename = "uTime")]
    pub u_time: String, // 当前事件的最新更新时间，Unix时间戳的毫秒数格式
    #[serde(rename = "prevInitial")]
    pub prev_initial: String, // 该事件上一周期的初始值，仅在修正发生时有值
    pub ccy: String,      // 事件实际值对应的货币
    pub unit: String,     // 事件实际值对应的单位
}

pub struct EconomicCalendar {}

impl EconomicCalendar {
    pub fn new(&self) -> &EconomicCalendar {
        self
    }
    ///  限速：1次/5s 限速规则：IP获取经济日历数据
    pub async fn get_economic_calendar(
        region: Option<&str>,
        importance: Option<&str>,
        before: Option<&str>,
        after: Option<&str>,
        limit: Option<i32>,
    ) -> anyhow::Result<Vec<EconomicCalendarData>, anyhow::Error> {
        let mut path = "/api/v5/public/economic-calendar?".to_string();

        if let Some(region) = region {
            path.push_str(&format!("&region={}", region));
        }

        if let Some(importance) = importance {
            path.push_str(&format!("&importance={}", importance));
        }

        if let Some(before) = before {
            path.push_str(&format!("&before={}", before));
        }

        if let Some(after) = after {
            path.push_str(&format!("&after={}", after));
        }

        if let Some(limit) = limit {
            path.push_str(&format!("&limit={}", limit));
        }

        let res: Vec<EconomicCalendarData> = okx_client::get_okx_client()
            .send_request(Method::GET, &path, "")
            .await?;
        Ok(res)
    }
}
