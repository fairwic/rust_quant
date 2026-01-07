//! 市场数据服务模块
//!
//! 提供市场数据的统一访问接口，协调 infrastructure 和 market 包

mod account_service;
mod asset_service;
mod contracts_service;
mod data_sync_service;
mod public_data_service;

use anyhow::Result;
use okx::dto::market_dto::TickerOkxResDto;
use rust_quant_domain::traits::CandleRepository;
use rust_quant_domain::{Candle, Timeframe};
use rust_quant_market::models::tickers::TickersDataEntity;

pub use account_service::AccountService;
pub use asset_service::AssetService;
pub use contracts_service::ContractsService;
pub use data_sync_service::DataSyncService;
pub use public_data_service::PublicDataService;

/// K线数据服务
///
/// 协调 infrastructure 和业务逻辑，提供统一的K线数据访问接口
///
/// # 架构原则
/// - 依赖 domain::traits::CandleRepository（接口）
/// - 不依赖 infrastructure 具体实现
/// - 通过构造函数注入实现
pub struct CandleService {
    repository: Box<dyn CandleRepository>,
}

impl CandleService {
    /// 创建服务实例（通过依赖注入）
    ///
    /// # 参数
    /// * `repository` - CandleRepository 实现（通常在应用入口注入）
    ///
    /// # 示例
    /// ```rust,ignore
    /// use rust_quant_infrastructure::SqlxCandleRepository;
    /// let repo = SqlxCandleRepository::new();
    /// let service = CandleService::new(Box::new(repo));
    /// ```
    pub fn new(repository: Box<dyn CandleRepository>) -> Self {
        Self { repository }
    }

    /// 获取指定时间范围的K线数据
    pub async fn get_candles(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_time: i64,
        end_time: i64,
        limit: Option<usize>,
    ) -> Result<Vec<Candle>> {
        self.repository
            .find_candles(symbol, timeframe, start_time, end_time, limit)
            .await
    }

    /// 获取最新的K线
    pub async fn get_latest_candle(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<Candle>> {
        self.repository.get_latest_candle(symbol, timeframe).await
    }

    /// 批量保存K线数据
    pub async fn save_candles(&self, candles: Vec<Candle>) -> Result<usize> {
        self.repository.save_candles(candles).await
    }

    /// 从交易所获取K线数据
    ///
    /// # Arguments
    /// * `inst_id` - 交易对
    /// * `bar` - K线周期
    /// * `after` - 开始时间
    /// * `before` - 结束时间
    /// * `limit` - 数量限制
    ///
    /// # Returns
    /// * K线数据列表
    ///
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE），支持多交易所扩展
    pub async fn fetch_candles_from_exchange(
        &self,
        inst_id: &str,
        bar: &str,
        after: Option<&str>,
        before: Option<&str>,
        limit: Option<&str>,
    ) -> Result<Vec<okx::dto::market_dto::CandleOkxRespDto>> {
        use rust_quant_infrastructure::ExchangeFactory;

        let exchange = ExchangeFactory::create_default_market_data()?;

        let after_i64 = after.and_then(|s| s.parse().ok());
        let before_i64 = before.and_then(|s| s.parse().ok());
        let limit_usize = limit.and_then(|s| s.parse().ok());

        let candles_json = exchange
            .fetch_candles(inst_id, bar, after_i64, before_i64, limit_usize)
            .await?;

        // 转换JSON数组为CandleOkxRespDto列表（保持向后兼容）
        let candles: Vec<okx::dto::market_dto::CandleOkxRespDto> = candles_json
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();

        Ok(candles)
    }

    /// 从交易所获取最新K线数据
    ///
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE），支持多交易所扩展
    pub async fn fetch_latest_candles_from_exchange(
        &self,
        inst_id: &str,
        bar: &str,
        limit: Option<&str>,
    ) -> Result<Vec<okx::dto::market_dto::CandleOkxRespDto>> {
        use rust_quant_infrastructure::ExchangeFactory;

        let exchange = ExchangeFactory::create_default_market_data()?;

        let limit_usize = limit.and_then(|s| s.parse().ok());

        let candles_json = exchange
            .fetch_latest_candles(inst_id, bar, limit_usize)
            .await?;

        // 转换JSON数组为CandleOkxRespDto列表（保持向后兼容）
        let candles: Vec<okx::dto::market_dto::CandleOkxRespDto> = candles_json
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();

        Ok(candles)
    }

    /// 从数据库获取确认的K线数据（用于回测）
    ///
    /// # 参数
    /// * `inst_id` - 交易对
    /// * `period` - 时间周期
    /// * `limit` - 数量限制
    /// * `select_time` - 时间选择条件
    ///
    /// # 返回
    /// * K线实体列表
    ///
    /// # 注意
    /// 此方法用于向后兼容，直接使用 market 包的 CandlesModel
    pub async fn get_confirmed_candles_for_backtest(
        &self,
        inst_id: &str,
        period: &str,
        limit: usize,
        select_time: Option<rust_quant_market::models::SelectTime>,
    ) -> Result<Vec<rust_quant_market::models::CandlesEntity>> {
        use rust_quant_market::models::{CandlesModel, SelectCandleReqDto};

        let dto = SelectCandleReqDto {
            inst_id: inst_id.to_string(),
            time_interval: period.to_string(),
            limit,
            select_time,
            confirm: Some(1),
        };

        let model = CandlesModel::new();
        let candles = model.fetch_candles_from_mysql(dto).await?;

        if candles.is_empty() {
            return Err(anyhow::anyhow!(
                "K线数据为空: inst_id={}, period={}",
                inst_id,
                period
            ));
        }

        Ok(candles)
    }

    /// 将 CandlesEntity 转换为 CandleItem（用于回测）
    ///
    /// # 参数
    /// * `candles` - K线实体列表
    ///
    /// # 返回
    /// * CandleItem 列表
    pub fn convert_candles_to_items(
        &self,
        candles: &[rust_quant_market::models::CandlesEntity],
    ) -> Vec<rust_quant_common::CandleItem> {
        use rust_quant_common::CandleItem;
        use tracing::warn;

        candles
            .iter()
            .filter_map(|candle| {
                CandleItem::builder()
                    .c(candle.c.parse::<f64>().unwrap_or(0.0))
                    .o(candle.o.parse::<f64>().unwrap_or(0.0))
                    .h(candle.h.parse::<f64>().unwrap_or(0.0))
                    .l(candle.l.parse::<f64>().unwrap_or(0.0))
                    .v(candle.vol_ccy.parse::<f64>().unwrap_or(0.0))
                    .ts(candle.ts)
                    .build()
                    .map_err(|e| {
                        warn!("构建CandleItem失败: {}, 跳过该条记录", e);
                        e
                    })
                    .ok()
            })
            .collect()
    }
}

/// Ticker数据服务
///
/// 提供实时行情数据访问接口，封装Ticker的业务逻辑
pub struct TickerService;

impl TickerService {
    pub fn new() -> Self {
        Self
    }

    /// 保存或更新单个Ticker数据
    ///
    /// 如果数据库中已存在该inst_id的记录，则更新；否则插入新记录
    ///
    /// # Arguments
    /// * `ticker` - Ticker数据
    ///
    /// # Returns
    /// * `true` - 插入新记录
    /// * `false` - 更新现有记录
    pub async fn save_or_update_ticker(&self, ticker: &TickerOkxResDto) -> Result<bool> {
        use rust_quant_market::models::tickers::TicketsModel;
        use tracing::debug;

        let model = TicketsModel::new();
        let existing = model.find_one(&ticker.inst_id).await?;

        if existing.is_empty() {
            debug!("Ticker不存在，插入新数据: {}", ticker.inst_id);
            // TickerOkxResDto没有实现Clone，需要手动构造
            let ticker_to_add = TickerOkxResDto {
                inst_type: ticker.inst_type.clone(),
                inst_id: ticker.inst_id.clone(),
                last: ticker.last.clone(),
                last_sz: ticker.last_sz.clone(),
                ask_px: ticker.ask_px.clone(),
                ask_sz: ticker.ask_sz.clone(),
                bid_px: ticker.bid_px.clone(),
                bid_sz: ticker.bid_sz.clone(),
                open24h: ticker.open24h.clone(),
                high24h: ticker.high24h.clone(),
                low24h: ticker.low24h.clone(),
                vol_ccy24h: ticker.vol_ccy24h.clone(),
                vol24h: ticker.vol24h.clone(),
                sod_utc0: ticker.sod_utc0.clone(),
                sod_utc8: ticker.sod_utc8.clone(),
                ts: ticker.ts.clone(),
            };
            model.add(vec![ticker_to_add]).await?;
            Ok(true)
        } else {
            debug!("Ticker已存在，更新数据: {}", ticker.inst_id);
            model.update(ticker).await?;
            Ok(false)
        }
    }

    /// 批量更新Ticker数据
    ///
    /// 遍历tickers列表，对于在inst_ids中的ticker，执行保存或更新操作
    ///
    /// # Arguments
    /// * `tickers` - Ticker数据列表
    /// * `inst_ids` - 需要同步的交易对列表（如果为空，则同步所有ticker）
    ///
    /// # Returns
    /// * 成功处理的ticker数量
    pub async fn batch_update_tickers(
        &self,
        tickers: Vec<TickerOkxResDto>,
        inst_ids: &[String],
    ) -> Result<usize> {
        use rust_quant_market::models::tickers::TicketsModel;
        use tracing::debug;

        if tickers.is_empty() {
            return Ok(0);
        }

        let model = TicketsModel::new();
        let mut count = 0;

        for ticker in tickers {
            let inst_id = ticker.inst_id.clone();
            let is_valid = true; // 原始代码中为true，表示不过滤

            // 如果inst_ids为空，或者ticker在inst_ids中，则处理
            if inst_ids.is_empty() || !is_valid || inst_ids.contains(&inst_id) {
                let existing = model.find_one(&inst_id).await?;

                if existing.is_empty() {
                    debug!("不存在，插入新数据: {}", inst_id);
                    // TickerOkxResDto没有实现Clone，需要手动构造
                    let ticker_to_add = TickerOkxResDto {
                        inst_type: ticker.inst_type.clone(),
                        inst_id: ticker.inst_id.clone(),
                        last: ticker.last.clone(),
                        last_sz: ticker.last_sz.clone(),
                        ask_px: ticker.ask_px.clone(),
                        ask_sz: ticker.ask_sz.clone(),
                        bid_px: ticker.bid_px.clone(),
                        bid_sz: ticker.bid_sz.clone(),
                        open24h: ticker.open24h.clone(),
                        high24h: ticker.high24h.clone(),
                        low24h: ticker.low24h.clone(),
                        vol_ccy24h: ticker.vol_ccy24h.clone(),
                        vol24h: ticker.vol24h.clone(),
                        sod_utc0: ticker.sod_utc0.clone(),
                        sod_utc8: ticker.sod_utc8.clone(),
                        ts: ticker.ts.clone(),
                    };
                    model.add(vec![ticker_to_add]).await?;
                } else {
                    debug!("已经存在，更新数据: {}", inst_id);
                    model.update(&ticker).await?;
                }
                count += 1;
            }
        }

        Ok(count)
    }

    /// 查询指定交易对的Ticker数据（从数据库）
    ///
    /// # Arguments
    /// * `inst_id` - 交易对ID
    ///
    /// # Returns
    /// * Ticker数据列表（可能为空）
    pub async fn find_ticker(&self, inst_id: &str) -> Result<Vec<TickersDataEntity>> {
        use rust_quant_market::models::tickers::TicketsModel;
        let model = TicketsModel::new();
        model.find_one(inst_id).await
    }

    /// 从交易所获取单个Ticker数据
    ///
    /// # Arguments
    /// * `inst_id` - 交易对ID
    ///
    /// # Returns
    /// * Ticker数据（可能为空）
    ///
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE），支持多交易所扩展
    pub async fn fetch_ticker_from_exchange(
        &self,
        inst_id: &str,
    ) -> Result<Option<TickerOkxResDto>> {
        use rust_quant_infrastructure::ExchangeFactory;

        let exchange = ExchangeFactory::create_default_market_data()?;
        let ticker_json = exchange.fetch_ticker(inst_id).await?;

        // 解析JSON数组，获取第一个ticker
        if let Some(arr) = ticker_json.as_array() {
            if let Some(first) = arr.first() {
                let ticker: TickerOkxResDto = serde_json::from_value(first.clone())?;
                return Ok(Some(ticker));
            }
        }

        Ok(None)
    }

    /// 从交易所批量获取Ticker数据
    ///
    /// # Arguments
    /// * `inst_type` - 合约类型（如"SWAP"）
    ///
    /// # Returns
    /// * Ticker数据列表
    ///
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE），支持多交易所扩展
    pub async fn fetch_tickers_from_exchange(
        &self,
        inst_type: &str,
    ) -> Result<Vec<TickerOkxResDto>> {
        use rust_quant_infrastructure::ExchangeFactory;

        let exchange = ExchangeFactory::create_default_market_data()?;
        let tickers_json = exchange.fetch_tickers(inst_type).await?;

        // 转换JSON数组为TickerOkxResDto列表
        let tickers: Vec<TickerOkxResDto> = tickers_json
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();

        Ok(tickers)
    }

    /// 同步单个Ticker（从交易所获取并保存）
    ///
    /// 完整的业务流程：从交易所获取 → 保存到数据库
    ///
    /// # Arguments
    /// * `inst_id` - 交易对ID
    ///
    /// # Returns
    /// * `true` - 插入新记录
    /// * `false` - 更新现有记录
    /// * `None` - Ticker数据为空
    pub async fn sync_ticker_from_exchange(&self, inst_id: &str) -> Result<Option<bool>> {
        if let Some(ticker) = self.fetch_ticker_from_exchange(inst_id).await? {
            Ok(Some(self.save_or_update_ticker(&ticker).await?))
        } else {
            Ok(None)
        }
    }

    /// 批量同步Tickers（从交易所获取并保存）
    ///
    /// 完整的业务流程：从交易所获取 → 批量保存到数据库
    ///
    /// # Arguments
    /// * `inst_type` - 合约类型（如"SWAP"）
    /// * `inst_ids` - 需要同步的交易对列表（如果为空，则同步所有ticker）
    ///
    /// # Returns
    /// * 成功处理的ticker数量
    pub async fn sync_tickers_from_exchange(
        &self,
        inst_type: &str,
        inst_ids: &[String],
    ) -> Result<usize> {
        let tickers = self.fetch_tickers_from_exchange(inst_type).await?;
        self.batch_update_tickers(tickers, inst_ids).await
    }

    /// 获取头部合约（按交易量排序）
    ///
    /// # Arguments
    /// * `inst_type` - 合约类型（如"SWAP"）
    /// * `top_n` - 返回前N个
    ///
    /// # Returns
    /// * 按交易量排序的前N个ticker
    pub async fn fetch_top_contracts_by_volume(
        &self,
        inst_type: &str,
        top_n: usize,
    ) -> Result<Vec<TickerOkxResDto>> {
        let mut tickers = self.fetch_tickers_from_exchange(inst_type).await?;

        // 按vol24h排序
        tickers.sort_by(|a, b| {
            let vol_a: f64 = a.vol24h.parse().unwrap_or(0.0);
            let vol_b: f64 = b.vol24h.parse().unwrap_or(0.0);
            vol_b
                .partial_cmp(&vol_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 取前N个
        tickers.truncate(top_n);

        Ok(tickers)
    }
}

/// 市场深度服务
pub struct MarketDepthService {
    // TODO: 添加市场深度数据访问
}

impl MarketDepthService {
    pub fn new() -> Self {
        Self {}
    }

    /// 获取市场深度数据
    pub async fn get_depth(&self, symbol: &str, depth: usize) -> Result<()> {
        // TODO: 实现市场深度查询
        Ok(())
    }
}
