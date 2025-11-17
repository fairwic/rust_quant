//! 公共数据服务
//!
//! 封装交易所PublicData API调用（公告、系统信息等）
//!
//! # 架构
//! - 依赖domain::traits::ExchangePublicData接口
//! - 支持多交易所扩展

use anyhow::Result;
use rust_quant_infrastructure::ExchangeFactory;
use tracing::info;

/// 公共数据服务
///
/// 职责：封装交易所公共数据API调用（公告、系统状态等），支持多交易所
pub struct PublicDataService;

impl PublicDataService {
    pub fn new() -> Self {
        Self
    }

    /// 从交易所获取公告列表
    ///
    /// # Arguments
    /// * `ann_type` - 公告类型
    /// * `page_size` - 每页数量
    ///
    /// # Returns
    /// * 公告列表
    ///
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE）
    pub async fn fetch_announcements_from_exchange(
        &self,
        ann_type: Option<&str>,
        page_size: Option<&str>,
    ) -> Result<Vec<String>> {
        let exchange = ExchangeFactory::create_public_data("okx")?;
        let announcements = exchange.fetch_announcements(ann_type, page_size).await?;

        info!("✅ 从交易所 {} 获取了公告数据", exchange.name());
        Ok(announcements)
    }
}

impl Default for PublicDataService {
    fn default() -> Self {
        Self::new()
    }
}
