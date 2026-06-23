use anyhow::{anyhow, Result};
use chrono::Utc;
use okx::dto::market_dto::CandleOkxRespDto;
use rust_quant_common::utils::time::ts_add_n_period;
use rust_quant_infrastructure::ExchangeFactory;
use rust_quant_market::models::candles::CandlesModel;
use rust_quant_market::models::tickers::TicketsModel;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};
/// 市场数据同步服务
///
/// 负责历史/增量K线的批量回填与校准，复刻 legacy `run_sync_data_job`
pub struct DataSyncService;
impl DataSyncService {
    pub fn new() -> Self {
        Self
    }
}
impl Default for DataSyncService {
    fn default() -> Self {
        Self::new()
    }
}
impl DataSyncService {
    /// 全量执行数据同步（三步：建表、补历史、补增量）
    /// 封装当前函数，减少行情数据调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    pub async fn run_sync_data_job(&self, inst_ids: &[String], periods: &[String]) -> Result<()> {
        info!(
            "📦 启动数据同步：inst_ids={:#?}，periods={:#?}",
            inst_ids, periods
        );
        self.init_create_table(inst_ids, periods).await?;
        self.init_all_candles(inst_ids, periods).await?;
        self.init_before_candles(inst_ids, periods).await?;
        info!("✅ 数据同步完成");
        Ok(())
    }
    /// 构建所需的K线表（不存在则建表）
    async fn init_create_table(&self, inst_ids: &[String], periods: &[String]) -> Result<()> {
        info!("🛠️ 初始化K线表结构");
        let tickers = TicketsModel::new().get_all(&inst_ids.to_vec()).await?;
        let model = CandlesModel::new();
        for ticker in tickers {
            for period in periods {
                model
                    .create_table(ticker.inst_id.as_str(), period)
                    .await
                    .map_err(|e| {
                        anyhow!(
                            "创建表失败: inst_id={}, period={}, err={}",
                            ticker.inst_id,
                            period,
                            e
                        )
                    })?;
            }
        }
        Ok(())
    }
    /// 回填历史K线（老数据）
    async fn init_all_candles(&self, inst_ids: &[String], periods: &[String]) -> Result<()> {
        info!("📚 回填历史K线");
        let tickers = TicketsModel::new().get_all(&inst_ids.to_vec()).await?;
        if tickers.is_empty() {
            warn!("无可用Ticker记录，跳过历史K线回填");
            return Ok(());
        }
        let exchange = ExchangeFactory::create_default_market_data()?;
        let model = CandlesModel::new();
        for ticker in tickers {
            for period in periods {
                // 清理未确认数据
                if let Some(unconfirmed) = model
                    .get_older_un_confirm_data(ticker.inst_id.as_str(), period)
                    .await?
                {
                    model
                        .delete_lg_time(ticker.inst_id.as_str(), period, unconfirmed.ts)
                        .await?;
                }
                let limit = self.period_backfill_limit(period);
                let current = model
                    .get_new_count(ticker.inst_id.as_str(), period, Some(limit as i32))
                    .await?;
                if current > limit as i64 {
                    debug!(
                        "跳过历史回填: inst_id={}, period={}, 当前数量 {} ≥ 限制 {}",
                        ticker.inst_id, period, current, limit
                    );
                    continue;
                }
                let mut after = model
                    .get_oldest_data(ticker.inst_id.as_str(), period)
                    .await?
                    .map(|c| c.ts)
                    .unwrap_or_else(|| Utc::now().timestamp_millis());
                loop {
                    sleep(Duration::from_millis(100)).await;
                    let raw = match exchange
                        .fetch_candles(ticker.inst_id.as_str(), period, Some(after), None, None)
                        .await
                    {
                        Ok(data) => data,
                        Err(e) => {
                            warn!(
                                "获取历史K线失败: inst_id={}, period={}, err={}",
                                ticker.inst_id, period, e
                            );
                            continue;
                        }
                    };
                    let candles = Self::convert_candles(raw);
                    if candles.is_empty() {
                        debug!(
                            "无更多历史K线: inst_id={}, period={}",
                            ticker.inst_id, period
                        );
                        break;
                    }
                    model.add(candles, ticker.inst_id.as_str(), period).await?;
                    let count = model
                        .get_new_count(ticker.inst_id.as_str(), period, Some(limit as i32))
                        .await?;
                    if count > limit as i64 {
                        info!(
                            "已达到历史回填上限: inst_id={}, period={}, 条数={}",
                            ticker.inst_id, period, count
                        );
                        break;
                    }
                    after = model
                        .get_oldest_data(ticker.inst_id.as_str(), period)
                        .await?
                        .map(|c| c.ts)
                        .unwrap_or(after);
                }
            }
        }
        Ok(())
    }
    /// 回填最新的增量K线（向前补足）
    async fn init_before_candles(&self, inst_ids: &[String], periods: &[String]) -> Result<()> {
        info!("⏩ 回填增量K线");
        let tickers = TicketsModel::new().get_all(&inst_ids.to_vec()).await?;
        if tickers.is_empty() {
            warn!("无可用Ticker记录，跳过增量K线回填");
            return Ok(());
        }
        let exchange = ExchangeFactory::create_default_market_data()?;
        let model = CandlesModel::new();
        for ticker in tickers {
            for period in periods {
                let mut before = model
                    .get_new_data(ticker.inst_id.as_str(), period)
                    .await?
                    .map(|c| c.ts)
                    .unwrap_or_else(|| Utc::now().timestamp_millis());
                loop {
                    sleep(Duration::from_millis(200)).await;
                    let (begin, after) = match self
                        .get_sync_begin_with_end(&model, ticker.inst_id.as_str(), period)
                        .await
                    {
                        Ok(win) => win,
                        Err(e) => {
                            warn!(
                                "计算同步窗口失败: inst_id={}, period={}, err={}",
                                ticker.inst_id, period, e
                            );
                            break;
                        }
                    };
                    let raw = match exchange
                        .fetch_candles(ticker.inst_id.as_str(), period, after, begin, Some(300))
                        .await
                    {
                        Ok(data) => data,
                        Err(e) => {
                            warn!(
                                "获取增量K线失败: inst_id={}, period={}, err={}",
                                ticker.inst_id, period, e
                            );
                            continue;
                        }
                    };
                    let candles = Self::convert_candles(raw);
                    if candles.is_empty() {
                        debug!("无新增K线: inst_id={}, period={}", ticker.inst_id, period);
                        break;
                    }
                    model.add(candles, ticker.inst_id.as_str(), period).await?;
                    if let Some(latest) =
                        model.get_new_data(ticker.inst_id.as_str(), period).await?
                    {
                        before = latest.ts;
                    } else {
                        break;
                    }
                }
                debug!(
                    "增量回填完成: inst_id={}, period={}, 最新时间={}",
                    ticker.inst_id, period, before
                );
            }
        }
        Ok(())
    }
    /// 提供periodbackfilllimit的集中实现，避免行情数据调用方重复处理相同细节。
    fn period_backfill_limit(&self, period: &str) -> usize {
        match period {
            "1m" => 28_800,
            "5m" => 28_800,
            "1H" => 28_800,
            "4H" => 28_800,
            "1D" => 28_800,
            "1Dutc" => 28_800,
            _ => 28_800,
        }
    }
    /// 提供convertK 线的集中实现，避免行情数据调用方重复处理相同细节。
    fn convert_candles(raw: Vec<serde_json::Value>) -> Vec<CandleOkxRespDto> {
        raw.into_iter()
            .filter_map(|value| {
                serde_json::from_value::<CandleOkxRespDto>(value.clone())
                    .ok()
                    .or_else(|| {
                        value.as_array().and_then(|arr| {
                            if arr.len() < 9 {
                                return None;
                            }
                            let fields: Vec<String> = arr
                                .iter()
                                .map(|v| v.as_str().unwrap_or("").to_string())
                                .collect();
                            Some(CandleOkxRespDto::from_vec(fields))
                        })
                    })
            })
            .collect()
    }
    /// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
    async fn get_sync_begin_with_end(
        &self,
        model: &CandlesModel,
        inst_id: &str,
        period: &str,
    ) -> Result<(Option<i64>, Option<i64>)> {
        if let Some(c) = model.get_new_data(inst_id, period).await? {
            let after = ts_add_n_period(c.ts, period, 100)?;
            Ok((Some(c.ts), Some(after)))
        } else {
            Ok((None, None))
        }
    }
}
