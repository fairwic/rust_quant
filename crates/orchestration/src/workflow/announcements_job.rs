// 风险监控任务

use crate::trading::services::announcement_service::AnnouncementService;
use rust_quant_execution::order_manager::order_service::OrderService;
use rust_quant_risk::position::position_service::PositionService;
use crate::trading::utils::common::PLATFORM;
use anyhow::{anyhow, Context, Result};
use log::{debug, error, info};
use okx::api::announcements::announcements_api::OkxAnnouncements;
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::SetLeverageRequest;
use okx::dto::asset_dto::{AssetBalance, TransferOkxReqDto};
use okx::dto::trade_dto::TdModeEnum;
use okx::dto::PositionSide;
use okx::enums::account_enums::AccountType;
use okx::enums::language_enums::Language;
use okx::{OkxAccount, OkxAsset};
use std::str::FromStr;
use tracing::{span, Level};

/// 实时获取公告信息定时任务
pub struct AnnouncementsJob {}

impl AnnouncementsJob {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn run(&self) -> Result<()> {
        //1. 请求获取公告内容
        let announcements = OkxAnnouncements::from_env()
            .unwrap()
            .get_announcements(None, Some("3".to_string()), Some(Language::ZhCn))
            .await?;
        println!("announcements: {:?}", announcements);
        let res = AnnouncementService::new()
            .save_announcement(&announcements.get(0).unwrap(), PLATFORM::PlatformOkx)
            .await?;
        println!("res: {:?}", res);
        //落库
        Ok(())
    }
}

/// 测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_init;

    #[tokio::test]
    async fn test_risk_job() {
        // 设置日志
        env_logger::init();
        app_init().await;
        let announcements_job = AnnouncementsJob::new();
        let res = announcements_job.run().await;
        println!("res: {:?}", res);
    }
}
