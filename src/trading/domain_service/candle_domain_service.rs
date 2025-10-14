use crate::app_config::env as app_env;
use crate::app_config::redis_config as app_redis;

use crate::time_util;
use crate::trading::cache::latest_candle_cache as local_cache;
use crate::trading::model::entity::candles::dto::SelectCandleReqDto;
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::trading::model::entity::candles::enums::SelectTime;
use crate::trading::model::market::candles::CandlesModel;
use crate::trading::task::basic;
use anyhow::{anyhow, Result};
use redis::AsyncCommands;
use tracing::error;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_MAX_STALENESS_MS: i64 = 1000; // 默认允许交易最大数据延迟 0.5s

// 方案1：依赖注入设计 - 最推荐的方式
pub struct CandleDomainService {
    candles_model: Arc<CandlesModel>,
}

impl CandleDomainService {
    /// 依赖注入构造函数
    pub fn new(candles_model: Arc<CandlesModel>) -> Self {
        Self { candles_model }
    }

    /// 便利构造函数
    pub async fn new_default() -> Self {
        Self {
            candles_model: Arc::new(CandlesModel::new().await),
        }
    }
    /// 获取最新的一条数据不管是否确认
    /// 读取顺序：内存缓存 -> Redis -> MySQL
    pub async fn get_candle_data_new(
        &self,
        inst_id: &str,
        period: &str,
    ) -> Result<Option<CandlesEntity>> {
        // 1) 内存/Redis
        if let Some(c) = local_cache::default_provider()
            .get_or_fetch(inst_id, period)
            .await
        {
            return Ok(Some(c));
        }
        // 2) MySQL 兜底
        let db_res = self.candles_model.get_new_data(inst_id, period).await?;
        if let Some(ref candle) = db_res {
            // 回填缓存（内存+Redis，带TTL）
            local_cache::default_provider()
                .set_both(inst_id, period, candle)
                .await;
        }
        Ok(db_res)
    }
    /// 获取确认的数据
    pub async fn get_candle_data_confirm(
        &self,
        inst_id: &str,
        period: &str,
        limit: usize,
        select_time: Option<SelectTime>,
    ) -> Result<Vec<CandlesEntity>> {
        let dto = SelectCandleReqDto {
            inst_id: inst_id.to_string(),
            time_interval: period.to_string(),
            limit,
            select_time,
            confirm: Some(1),
        };
        self.fetch_and_validate_candles(dto, period).await
    }

    /// 获取最新的一条数据（带“新鲜度”判断）。若超过阈值则返回 None
    pub async fn get_new_one_candle_fresh(
        &self,
        inst_id: &str,
        period: &str,
        _max_staleness_ms: Option<i64>,
    ) -> Result<Option<CandlesEntity>> {
        // let limit = max_staleness_ms.unwrap_or_else(|| {
        //     crate::app_config::env::candle_cache_staleness_ms(period, DEFAULT_MAX_STALENESS_MS)
        // });
        // 先查缓存（内存/Redis），不做新鲜度过滤
        if let Some(c) = local_cache::default_provider()
            .get_or_fetch(inst_id, period)
            .await
        {
                return Ok(Some(c));
         }
        // // 缓存不新鲜或未命中：查 DB
        // let db_res = self.candles_model.get_new_data(inst_id, period).await?;
        // if let Some(ref c) = db_res {
        //     //如果是当前周期的数据
        //     if time_util::ts_is_max_period(c.ts, period) {
        //         // 回填缓存（可选）
        //         local_cache::default_provider()
        //             .set_both(inst_id, period, c)
        //             .await;
        //         return Ok(Some(c.clone()));
        //     }else {
        //         error!("数据库中的最新数据周期不匹配：{:?}",c.ts)
        //     }
        // }
        Ok(None)
    }

    /// 获取最后一条数据
    pub async fn get_candle_data_last(
        &self,
        inst_id: &str,
        period: &str,
    ) -> Result<Vec<CandlesEntity>> {
        let dto = SelectCandleReqDto {
            inst_id: inst_id.to_string(),
            time_interval: period.to_string(),
            limit: 1,
            select_time: None,
            confirm: None,
        };
        self.fetch_and_validate_candles(dto, period).await
    }

    /// 通用的获取蜡烛图数据方法
    pub async fn get_candle_data(&self, dto: SelectCandleReqDto) -> Result<Vec<CandlesEntity>> {
        let period = dto.time_interval.clone();
        self.fetch_and_validate_candles(dto, &period).await
    }

    /// 提取的公共逻辑：获取数据并验证
    async fn fetch_and_validate_candles(
        &self,
        dto: SelectCandleReqDto,
        period: &str,
    ) -> Result<Vec<CandlesEntity>> {
        let list = self
            .candles_model
            .fetch_candles_from_mysql(dto)
            .await
            .map_err(|e| anyhow!("获取蜡烛图数据失败: {}", e))?;

        if list.is_empty() {
            return Err(anyhow!("蜡烛图数据为空"));
        }

        // 验证数据是否正确
        basic::valid_candles_data(&list, period)
            .map_err(|e| anyhow!("蜡烛图数据验证失败: {}", e))?;

        Ok(list)
    }
}

// 方案2：无状态设计 - 适合简单场景
pub struct CandleDomainServiceStateless;

impl CandleDomainServiceStateless {
    /// 获取确认的数据（无状态版本）
    pub async fn get_confirmed_candles(
        inst_id: &str,
        period: &str,
        limit: usize,
        select_time: Option<SelectTime>,
    ) -> Result<Vec<CandlesEntity>> {
        let dto = SelectCandleReqDto {
            inst_id: inst_id.to_string(),
            time_interval: period.to_string(),
            limit,
            select_time,
            confirm: Some(1),
        };
        Self::fetch_and_validate_candles(dto, period).await
    }

    /// 获取最后一条数据（无状态版本）
    pub async fn get_last_candle(inst_id: &str, period: &str) -> Result<Vec<CandlesEntity>> {
        let dto = SelectCandleReqDto {
            inst_id: inst_id.to_string(),
            time_interval: period.to_string(),
            limit: 1,
            select_time: None,
            confirm: None,
        };
        Self::fetch_and_validate_candles(dto, period).await
    }

    /// 通用的获取蜡烛图数据方法（无状态版本）
    pub async fn get_candle_data(dto: SelectCandleReqDto) -> Result<Vec<CandlesEntity>> {
        let period = dto.time_interval.clone();
        Self::fetch_and_validate_candles(dto, &period).await
    }

    /// 提取的公共逻辑：获取数据并验证（无状态版本）
    async fn fetch_and_validate_candles(
        dto: SelectCandleReqDto,
        period: &str,
    ) -> Result<Vec<CandlesEntity>> {
        let candles_model = CandlesModel::new().await;
        let list = candles_model
            .fetch_candles_from_mysql(dto)
            .await
            .map_err(|e| anyhow!("获取蜡烛图数据失败: {}", e))?;

        if list.is_empty() {
            return Err(anyhow!("蜡烛图数据为空"));
        }

        // 验证数据是否正确
        basic::valid_candles_data(&list, period)
            .map_err(|e| anyhow!("蜡烛图数据验证失败: {}", e))?;

        Ok(list)
    }
}

// 方案3：工厂模式 - 适合复杂初始化场景
pub struct CandleDomainServiceFactory;

impl CandleDomainServiceFactory {
    pub async fn create_service() -> CandleDomainService {
        let candles_model = Arc::new(CandlesModel::new().await);
        CandleDomainService::new(candles_model)
    }

    pub async fn create_shared_service() -> Arc<CandleDomainService> {
        Arc::new(Self::create_service().await)
    }
}
