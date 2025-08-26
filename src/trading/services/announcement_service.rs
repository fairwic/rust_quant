use crate::trading::model::announcement_model::AnnouncementModel;
use crate::trading::model::big_data::take_volume::{TakerVolumeEntity, TakerVolumeModel};
use crate::trading::model::big_data::take_volume_contract::{
    ModelEntity, TakerVolumeContractModel,
};
use crate::trading::model::entity::announcement_entity::Announcement;
use crate::trading::utils::common::PLATFORM;
use crate::trading::utils::function::sha256;
use anyhow::anyhow;
use chrono::Utc;
use hex::encode;
use log::info;
use okx::api::announcements::announcements_api::{AnnouncementDetail, AnnouncementPage};
use okx::api::api_trait::OkxApiTrait;
use okx::{Error, OkxBigData};
use rbatis::rbdc::DateTime;
use redis::Commands;
use serde_json::{json, Value};
use sha2::Sha256;
use std::time::Duration;
use tracing::debug;

pub struct AnnouncementService {}

impl AnnouncementService {
    pub fn new() -> Self {
        Self {}
    }
    fn get_uuid(&self, ann: &AnnouncementDetail, plate_type: &PLATFORM) -> String {
        // 使用SHA256替代MD5，因为MD5已不再安全
        let input = format!("{}{}{}", ann.p_time, plate_type.to_string(), ann.url);
        sha256(&input)
    }

    pub async fn save_announcement(
        &self,
        announcement: &AnnouncementPage,
        plate_type: PLATFORM,
    ) -> anyhow::Result<u64> {
        let announcement_model = AnnouncementModel::new().await;
        let announcement_list: Vec<Announcement> = announcement
            .details
            .iter()
            .map(|ann| {
                let time = ann.p_time.parse::<u64>().unwrap_or(0);
                Announcement {
                    id: 0, // 这里必须明确指定，因为在结构体初始化时不会应用serde属性
                    uuid: self.get_uuid(ann, &plate_type),
                    ann_type: ann.ann_type.clone(),
                    p_time: time,
                    title: ann.title.clone(),
                    url: ann.url.clone(),
                    plate_type: plate_type.to_string(),
                    created_time: DateTime::now(), // 同样需要明确指定
                }
            })
            .collect();
        let res = announcement_model.add(&announcement_list).await?;
        Ok(res.rows_affected)
    }
}
