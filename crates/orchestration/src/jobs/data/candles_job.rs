//! K线数据同步任务
//!
//! # 架构原则
//! - orchestration层：只做编排，不直接调用外部API或数据库
//! - services层：封装业务逻辑和外部API调用
//! - 通过service层访问所有业务功能

use anyhow::Result;
use tracing::{error, info};

use rust_quant_domain::{Candle, Price, Timeframe, Volume};
use rust_quant_infrastructure::repositories::{PostgresCandleRepository, SqlxCandleRepository};
use rust_quant_services::market::{
    should_use_quant_core_candle_source, CandleService as CandleMarketService, DataSyncService,
};
use sqlx::postgres::PgPoolOptions;
use std::str::FromStr;

/// K线数据同步任务
///
/// # Architecture
/// orchestration层的核心数据同步任务，只负责编排，不包含业务逻辑
///
/// # Responsibilities
/// 1. 编排数据同步流程
/// 2. 调用service层完成具体业务逻辑
/// 3. 处理错误和日志记录
///
/// # Example
/// ```rust,ignore
/// use rust_quant_orchestration::jobs::data::CandlesJob;
///
/// let job = CandlesJob::new();
/// job.sync_latest_candles(&inst_ids, &periods).await?;
/// ```
pub struct CandlesJob;

impl CandlesJob {
    pub fn new() -> Self {
        Self
    }

    /// 创建 CandleService 实例
    ///
    /// # Architecture
    /// 统一创建 CandleService 实例，使用依赖注入模式
    fn create_candle_service() -> Result<CandleMarketService> {
        if should_use_quant_core_candle_source()? {
            let database_url = std::env::var("QUANT_CORE_DATABASE_URL").map_err(|_| {
                anyhow::anyhow!("CANDLE_SOURCE=quant_core 时必须设置 QUANT_CORE_DATABASE_URL")
            })?;
            let pool = PgPoolOptions::new()
                .max_connections(5)
                .connect_lazy(&database_url)?;
            let repository = PostgresCandleRepository::new(pool);
            return Ok(CandleMarketService::new(Box::new(repository)));
        }

        let pool = rust_quant_core::database::get_db_pool();
        let repository = SqlxCandleRepository::new(pool.clone());
        Ok(CandleMarketService::new(Box::new(repository)))
    }

    /// 同步最新的K线数据
    ///
    /// # Arguments
    /// * `inst_ids` - 交易对列表
    /// * `periods` - 时间周期列表
    ///
    /// # Architecture
    /// orchestration层：只做编排，调用service层完成业务逻辑
    pub async fn sync_latest_candles(&self, inst_ids: &[String], periods: &[String]) -> Result<()> {
        info!(
            "📈 开始同步最新K线数据: {} 个交易对, {} 个周期",
            inst_ids.len(),
            periods.len()
        );

        let service = Self::create_candle_service()?;

        for inst_id in inst_ids {
            for period in periods {
                match self
                    .sync_single_candle_latest(&service, inst_id, period)
                    .await
                {
                    Ok(count) => info!("✅ K线同步成功: {} {} - {} 条", inst_id, period, count),
                    Err(e) => error!("❌ K线同步失败: {} {} - {}", inst_id, period, e),
                }
            }
        }

        info!("✅ 所有K线数据同步完成");
        Ok(())
    }

    /// 同步单个交易对的最新K线
    ///
    /// # Architecture
    /// orchestration层：调用service层完成业务逻辑
    async fn sync_single_candle_latest(
        &self,
        service: &CandleMarketService,
        inst_id: &str,
        period: &str,
    ) -> Result<usize> {
        // 1. 解析时间周期
        let timeframe = Timeframe::from_str(period)
            .map_err(|_| anyhow::anyhow!("无效的时间周期: {}", period))?;

        // 2. 获取数据库中最新的K线时间戳
        let latest_candle = service.get_latest_candle(inst_id, timeframe).await?;
        let after_ts = latest_candle.map(|c| c.timestamp).unwrap_or(0);

        // 3. 通过service层获取增量K线
        let after_str = if after_ts > 0 {
            Some(after_ts.to_string())
        } else {
            None
        };

        let domain_candles = if market_data_exchange() == "binance" {
            let after =
                after_ts
                    .checked_add(1)
                    .and_then(|ts| if ts > 1 { Some(ts as u64) } else { None });
            service
                .fetch_candles_from_crypto_exc_all("binance", inst_id, period, after, None, 100)
                .await?
        } else {
            let okx_candles = service
                .fetch_candles_from_exchange(
                    inst_id,
                    period,
                    after_str.as_deref(),
                    None,
                    Some("100"),
                )
                .await?;

            if okx_candles.is_empty() {
                return Ok(0);
            }

            okx_candles
                .iter()
                .map(|dto| Self::convert_okx_to_domain(dto, inst_id, timeframe))
                .collect::<Result<Vec<Candle>>>()?
        };

        if domain_candles.is_empty() {
            return Ok(0);
        }

        // 5. 批量保存到数据库
        let saved_count = service.save_candles(domain_candles).await?;

        Ok(saved_count)
    }

    /// 转换OKX DTO到Domain Candle
    ///
    /// # Architecture
    /// 数据转换逻辑，将外部DTO转换为领域实体
    fn convert_okx_to_domain(
        dto: &okx::dto::market_dto::CandleOkxRespDto,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Candle> {
        let timestamp = dto
            .ts
            .parse::<i64>()
            .map_err(|e| anyhow::anyhow!("解析时间戳失败: ts={}, err={}", dto.ts, e))?;

        let open = dto
            .o
            .parse::<f64>()
            .map_err(|e| anyhow::anyhow!("解析开盘价失败: o={}, err={}", dto.o, e))?;
        let open = Price::new(open)
            .map_err(|e| anyhow::anyhow!("创建Price失败: value={}, err={:?}", dto.o, e))?;

        let high = dto
            .h
            .parse::<f64>()
            .map_err(|e| anyhow::anyhow!("解析最高价失败: h={}, err={}", dto.h, e))?;
        let high = Price::new(high)
            .map_err(|e| anyhow::anyhow!("创建Price失败: value={}, err={:?}", dto.h, e))?;

        let low = dto
            .l
            .parse::<f64>()
            .map_err(|e| anyhow::anyhow!("解析最低价失败: l={}, err={}", dto.l, e))?;
        let low = Price::new(low)
            .map_err(|e| anyhow::anyhow!("创建Price失败: value={}, err={:?}", dto.l, e))?;

        let close = dto
            .c
            .parse::<f64>()
            .map_err(|e| anyhow::anyhow!("解析收盘价失败: c={}, err={}", dto.c, e))?;
        let close = Price::new(close)
            .map_err(|e| anyhow::anyhow!("创建Price失败: value={}, err={:?}", dto.c, e))?;

        let volume = dto
            .vol_ccy
            .parse::<f64>()
            .map_err(|e| anyhow::anyhow!("解析成交量失败: vol_ccy={}, err={}", dto.vol_ccy, e))?;
        let volume = Volume::new(volume)
            .map_err(|e| anyhow::anyhow!("创建Volume失败: value={}, err={:?}", dto.vol_ccy, e))?;

        let mut candle = Candle::new(
            symbol.to_string(),
            timeframe,
            timestamp,
            open,
            high,
            low,
            close,
            volume,
        );

        // 设置确认状态
        if dto.confirm == "1" {
            candle.confirm();
        }

        Ok(candle)
    }

    /// 全量执行数据同步（三步：建表、补历史、补增量）
    ///
    /// # Architecture
    /// orchestration层：只做编排，委托给DataSyncService完成业务逻辑
    ///
    /// # Arguments
    /// * `inst_ids` - 交易对列表
    /// * `periods` - 时间周期列表
    pub async fn sync_all_data(&self, inst_ids: &[String], periods: &[String]) -> Result<()> {
        info!(
            "📦 启动完整数据同步：inst_ids={}，periods={}",
            inst_ids.len(),
            periods.len()
        );

        let sync_service = DataSyncService::new();
        sync_service.run_sync_data_job(inst_ids, periods).await?;

        info!("✅ 完整数据同步完成");
        Ok(())
    }
}

impl Default for CandlesJob {
    fn default() -> Self {
        Self::new()
    }
}

fn market_data_exchange() -> String {
    std::env::var("MARKET_DATA_EXCHANGE")
        .or_else(|_| std::env::var("DEFAULT_EXCHANGE"))
        .unwrap_or_else(|_| "okx".to_string())
        .trim()
        .to_ascii_lowercase()
}

/// 并发同步多个交易对的K线
///
/// # Arguments
/// * `inst_ids` - 交易对列表
/// * `periods` - 时间周期列表
/// * `concurrency` - 并发数量
///
/// # Architecture
/// orchestration层：并发编排多个同步任务
pub async fn sync_candles_concurrent(
    inst_ids: &[String],
    periods: &[String],
    concurrency: usize,
) -> Result<()> {
    info!(
        "📈 开始并发同步K线: {} 个交易对, {} 个周期, 并发数: {}",
        inst_ids.len(),
        periods.len(),
        concurrency
    );

    use futures::stream::{self, StreamExt};

    // 构建任务列表
    let mut tasks = Vec::new();
    for inst_id in inst_ids {
        for period in periods {
            tasks.push((inst_id.clone(), period.clone()));
        }
    }

    // 并发执行
    let results: Vec<_> = stream::iter(tasks)
        .map(|(inst_id, period)| {
            let service = CandlesJob::create_candle_service();
            let job = CandlesJob::new();
            async move {
                let service = service?;
                job.sync_single_candle_latest(&service, &inst_id, &period)
                    .await
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let success_count = results.iter().filter(|r| r.is_ok()).count();
    let total_candles: usize = results.iter().filter_map(|r| r.as_ref().ok()).sum();

    info!(
        "✅ 并发同步完成: 成功 {}/{}, 总K线数: {}",
        success_count,
        results.len(),
        total_candles
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要OKX API和数据库配置
    async fn test_sync_latest_candles() {
        let job = CandlesJob::new();
        let inst_ids = vec!["BTC-USDT".to_string()];
        let periods = vec!["1H".to_string()];

        let result = job.sync_latest_candles(&inst_ids, &periods).await;
        assert!(result.is_ok());
    }
}
