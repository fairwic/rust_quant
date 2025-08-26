use super::entity::announcement_entity::Announcement;
use crate::app_config::db;
use anyhow::Ok;
use hex::encode;
use rbatis::rbdc::db::ExecResult;
use rbatis::rbdc::DateTime;
use rbatis::RBatis;
use rbs::Value;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

pub struct AnnouncementModel {
    db: &'static RBatis,
}

impl AnnouncementModel {
    pub async fn new() -> Self {
        Self {
            db: db::get_db_client(),
        }
    }
    pub async fn get_by_uid(&self, uid: &str) -> anyhow::Result<Option<Announcement>> {
        let res = Announcement::select_by_uid(self.db, uid).await?;
        Ok(res)
    }
    pub async fn add(&self, announcements: &[Announcement]) -> anyhow::Result<ExecResult> {
        // 创建可变副本，以便我们可以设置uuid
        let mut announcements_with_uuid = Vec::with_capacity(announcements.len());

        for ann in announcements {
            // 检查是否已存在
            println!("uuid:{}", ann.uuid);
            let res = self.get_by_uid(&ann.uuid).await?;
            if res.is_some() {
                // 已存在，跳过插入
                continue;
            }
            // 创建带有uuid的新公告
            announcements_with_uuid.push(ann.clone());
        }

        // 如果没有新公告，返回空结果
        if announcements_with_uuid.is_empty() {
            return Ok(ExecResult {
                rows_affected: 0,
                last_insert_id: Value::Null,
            });
        }

        // 批量插入
        let res = Announcement::insert_batch(
            self.db,
            &announcements_with_uuid,
            announcements_with_uuid.len() as u64,
        )
        .await?;
        Ok(res)
    }
}
