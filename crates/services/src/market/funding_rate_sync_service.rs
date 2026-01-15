use anyhow::Result;
use okx::api::api_trait::OkxApiTrait;
use okx::api::public_data::OkxPublicData;
use rust_quant_core::database::get_db_pool;
use rust_quant_domain::traits::funding_rate_repository::FundingRateRepository;
use rust_quant_infrastructure::repositories::funding_rate_repository::SqlxFundingRateRepository;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

/// 资金费率数据同步服务
///
/// 负责双向同步：
/// 1. 向前（Forward）：同步最新的增量数据
/// 2. 向后（Backward）：回填历史数据
pub struct FundingRateSyncService {
    api: OkxPublicData,
    repo: Arc<dyn FundingRateRepository>,
}

impl FundingRateSyncService {
    pub fn new() -> Result<Self> {
        let api = OkxPublicData::from_env()?;
        let pool = get_db_pool().clone();
        let repo = Arc::new(SqlxFundingRateRepository::new(pool));
        Ok(Self { api, repo })
    }

    /// 执行动态同步 (增量 + 历史)
    pub async fn sync_dynamic(&self, inst_ids: &[String]) -> Result<()> {
        info!("📦 启动资金费率同步：{} 个交易对", inst_ids.len());

        for inst_id in inst_ids {
            // 1. 同步增量数据 (包含初始化)
            if let Err(e) = self.sync_incremental(inst_id).await {
                error!("❌ 增量同步失败: inst_id={}, err={}", inst_id, e);
            }

            // 2. 回填历史数据
            if let Err(e) = self.sync_historical(inst_id).await {
                error!("❌ 历史回填失败: inst_id={}, err={}", inst_id, e);
            }
        }

        info!("✅ 资金费率同步任务完成");
        Ok(())
    }

    /// 增量同步：从最新的 DB 记录通过 after 向前获取
    async fn sync_incremental(&self, inst_id: &str) -> Result<()> {
        let latest = self.repo.find_latest(inst_id).await?;

        // OKX funding-rate-history 接口参数：
        // instId, before, after, limit
        // before: 请求 fundingTime < before 的数据 (更旧)
        // after: 请求 fundingTime > after 的数据 (更新)
        // 注意：OKX 文档中 before/after 含义在不同接口可能不同，需实测 verify
        // 根据公共数据接口通常惯例：
        // 列表按时间倒序排列 (最新在前)
        // after = time, 返回 < time 的数据 (更旧) -> 向后翻页
        // before = time, 返回 > time 的数据 (更新) -> 向前翻页

        // 增量策略：
        // 如果 DB 有数据，取最新的 time，请求 > time 的数据 (before = latest_time)
        // 如果 DB 无数据，不用做增量，直接等下一次 loop 或留给 historical 初始化

        let target_ts = latest.map(|r| r.funding_time).unwrap_or(0);

        // 如果没有数据，增量部分其实就是拉取最新的几条，可以复用历史逻辑的第一次 fetch
        if target_ts == 0 {
            info!("🆕 初始化同步 (无历史记录): {}", inst_id);
            return self.fetch_and_save(inst_id, None, None).await.map(|_| ());
        }

        info!("⏩ 增量同步: {}, last_time={}", inst_id, target_ts);

        // 尝试获取比 target_ts 更新的数据
        // 使用 before 参数: 返回 > target_ts 的数据
        let limit = Some(100);
        let _has_more = true;
        let _min_ts_in_batch = 0; // 用于分页，但在向前同步中，通常不需要持续翻页，因为资金费率8小时一次，差距不会太大

        // 注意：get_funding_rate_history API 签名: before, after, limit
        // 假设 API 实现正确映射了 query param
        // before: < timestamp ? NO, check docs.
        // OKX Docs: "Pagination of data to return records newer than the requested fundingTime." (for before?)
        // Let's assume standard cursor pagination: before -> newer, after -> older.

        let rates = self
            .api
            .get_funding_rate_history(inst_id, Some(target_ts), None, limit)
            .await?;

        if !rates.is_empty() {
            info!("增量更新: 获取到 {} 条数据", rates.len());
            self.save_batch(rates).await?;
        }

        Ok(())
    }

    /// 历史回填：从最早的 DB 记录通过 after 向后获取
    async fn sync_historical(&self, inst_id: &str) -> Result<()> {
        let oldest = self.repo.find_oldest(inst_id).await?;
        let mut after_ts = oldest.map(|r| r.funding_time);

        info!("📚 历史回填: {}, start_after={:?}", inst_id, after_ts);

        loop {
            // 获取比 after_ts 更旧的数据
            tokio::time::sleep(Duration::from_millis(5000)).await;
            let rates = self
                .api
                .get_funding_rate_history(inst_id, None, after_ts, Some(100))
                .await?;

            if rates.is_empty() {
                info!("历史回填完成: {} (无更多数据)", inst_id);
                break;
            }

            let count = rates.len();
            // 更新游标为本次批次中最早的时间 (最后一条)
            let last_rate = rates.last().unwrap(); // safe because !empty
            let last_ts = last_rate.funding_time.parse::<i64>().unwrap_or(0);

            self.save_batch(rates).await?;

            info!("回填保存 {} 条, cursor updated to {}", count, last_ts);
            after_ts = Some(last_ts);
        }

        Ok(())
    }

    async fn fetch_and_save(
        &self,
        inst_id: &str,
        before: Option<i64>,
        after: Option<i64>,
    ) -> Result<usize> {
        let rates = self
            .api
            .get_funding_rate_history(inst_id, before, after, Some(100))
            .await?;
        let count = rates.len();
        if count > 0 {
            self.save_batch(rates).await?;
        }
        Ok(count)
    }

    async fn save_batch(
        &self,
        rates: Vec<okx::dto::public_data::public_data_dto::FundingRateHistoryOkxRespDto>,
    ) -> Result<()> {
        use rust_quant_domain::entities::funding_rate::FundingRate;

        for rate_dto in rates {
            let entity = FundingRate {
                id: None,
                inst_id: rate_dto.inst_id.clone(),
                funding_rate: f64::from_str(&rate_dto.funding_rate).unwrap_or(0.0),
                funding_time: rate_dto.funding_time.parse().unwrap_or(0),
                method: rate_dto.method.clone(),
                next_funding_rate: None,
                next_funding_time: None,
                min_funding_rate: None,
                max_funding_rate: None,
                sett_funding_rate: None,
                sett_state: None,
                premium: None,
                ts: 0,
                realized_rate: Some(f64::from_str(&rate_dto.realized_rate).unwrap_or(0.0)),
                interest_rate: None,
            };

            // 忽略重复键错误 (insert ignore 语义通过 save 的 on duplicate updates 实现)
            if let Err(e) = self.repo.save(entity).await {
                error!("保存资金费率失败: {}", e);
            }
        }
        Ok(())
    }
}
