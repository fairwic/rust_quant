//! 市场数据服务模块
//!
//! 提供市场数据的统一访问接口，协调 infrastructure 和 market 包

mod account_service;
mod asset_service;
pub mod binance_websocket;
mod contracts_service;
mod data_sync_service;
pub mod dune_market_sync_service;
pub mod economic_calendar_sync_service;
pub mod exchange_symbol_sync_service;
pub mod external_market_sync_service;
pub mod funding_rate_sync_service;
mod public_data_service;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use crypto_exc_all::{Candle as ExchangeCandle, CandleQuery, ExchangeId, Instrument};
use okx::dto::market_dto::TickerOkxResDto;
use rust_quant_domain::traits::CandleRepository;
use rust_quant_domain::{Candle, Price, Timeframe, Volume};
use rust_quant_infrastructure::repositories::PostgresCandleRepository;
use rust_quant_market::models::tickers::TickersDataEntity;
use sqlx::{postgres::PgPoolOptions, Postgres, QueryBuilder};
use std::str::FromStr;

pub use account_service::AccountService;
pub use asset_service::AssetService;
pub use contracts_service::ContractsService;
pub use data_sync_service::DataSyncService;
pub use dune_market_sync_service::{DuneMarketSyncService, DuneSqlRunner};
pub use economic_calendar_sync_service::{EconomicCalendarSyncService, EconomicEventQueryService};
pub use exchange_symbol_sync_service::{
    default_exchange_symbol_sync_sources, normalize_exchange_symbol_sync_source,
    parse_exchange_symbol_sync_sources, BinanceExchangeInfoProvider, ExchangeSymbolSyncService,
    LiveBinanceExchangeInfoProvider, MajorExchangeListingSignal, StaticExchangeInfoProvider,
};
pub use external_market_sync_service::{
    normalize_external_market_symbol, ExternalMarketDataProvider, ExternalMarketSource,
    ExternalMarketSyncService, HyperliquidExternalMarketDataProvider,
};
pub use public_data_service::PublicDataService;

mod scanner_service;
pub use scanner_service::ScannerService;

mod flow_analyzer;
pub use flow_analyzer::FlowAnalyzer;

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

    /// 通过 crypto_exc_all 聚合 SDK 获取交易所 K线，并转换成领域 Candle。
    pub async fn fetch_candles_from_crypto_exc_all(
        &self,
        exchange: &str,
        inst_id: &str,
        period: &str,
        after: Option<u64>,
        before: Option<u64>,
        limit: u32,
    ) -> Result<Vec<Candle>> {
        let exchange_id = ExchangeId::from_str(exchange).map_err(|error| anyhow!(error))?;
        let timeframe =
            Timeframe::from_str(period).map_err(|error| anyhow!("无效的K线周期: {}", error))?;
        let instrument = instrument_from_inst_id(inst_id)?;
        let interval = exchange_kline_interval(period);
        let gateway = crypto_exc_all_gateway_from_env(exchange_id)?;

        let mut query = CandleQuery::new(instrument, interval).with_limit(limit);
        if let Some(after) = after {
            query = query.with_start_time(after);
        }
        if let Some(before) = before {
            query = query.with_end_time(before);
        }

        let candles = gateway
            .candles(exchange_id, query)
            .await
            .map_err(|error| anyhow!("通过 crypto_exc_all 获取K线失败: {}", error))?;

        candles
            .iter()
            .map(|candle| exchange_candle_to_domain(candle, inst_id, timeframe))
            .collect()
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
        get_confirmed_candles_for_backtest(inst_id, period, limit, select_time).await
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

pub async fn get_confirmed_candles_for_backtest(
    inst_id: &str,
    period: &str,
    limit: usize,
    select_time: Option<rust_quant_market::models::SelectTime>,
) -> Result<Vec<rust_quant_market::models::CandlesEntity>> {
    if should_use_quant_core_candle_source()? {
        return get_quant_core_sharded_candles_for_backtest(inst_id, period, limit, select_time)
            .await;
    }

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

pub fn should_use_quant_core_candle_source() -> Result<bool> {
    let source = std::env::var("CANDLE_SOURCE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if source.is_empty() || source == "mysql" || source == "legacy_mysql" {
        return Ok(false);
    }
    if matches!(source.as_str(), "quant_core" | "postgres" | "pg") {
        return Ok(true);
    }
    Err(anyhow!("不支持的 CANDLE_SOURCE: {}", source))
}

fn crypto_exc_all_gateway_from_env(exchange: ExchangeId) -> Result<crate::CryptoExcAllGateway> {
    if exchange == ExchangeId::Binance {
        return crate::CryptoExcAllGateway::from_single_exchange_credentials(
            ExchangeId::Binance,
            std::env::var("BINANCE_API_KEY").unwrap_or_else(|_| "public-market-only".to_string()),
            std::env::var("BINANCE_API_SECRET")
                .unwrap_or_else(|_| "public-market-only".to_string()),
            Option::<String>::None,
            false,
        )
        .map_err(|error| anyhow!("创建 Binance crypto_exc_all gateway 失败: {}", error));
    }

    crate::CryptoExcAllGateway::from_env()
        .map_err(|error| anyhow!("创建 crypto_exc_all gateway 失败: {}", error))
}

fn instrument_from_inst_id(inst_id: &str) -> Result<Instrument> {
    let parts: Vec<&str> = inst_id.split('-').collect();
    let base = parts
        .first()
        .copied()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("无法解析交易对 base: {}", inst_id))?;
    let quote = parts
        .get(1)
        .copied()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("无法解析交易对 quote: {}", inst_id))?;

    Ok(Instrument::perp(base, quote).with_settlement(quote))
}

fn exchange_kline_interval(period: &str) -> String {
    match period {
        "1Dutc" | "1DUTC" => "1d".to_string(),
        value => value.to_ascii_lowercase(),
    }
}

fn exchange_candle_to_domain(
    candle: &ExchangeCandle,
    inst_id: &str,
    timeframe: Timeframe,
) -> Result<Candle> {
    let timestamp = candle
        .open_time
        .ok_or_else(|| anyhow!("交易所K线缺少 open_time"))? as i64;
    let volume = candle
        .quote_volume
        .as_deref()
        .unwrap_or(&candle.volume)
        .parse::<f64>()
        .with_context(|| format!("解析成交量失败: {:?}", candle.quote_volume))?;

    let mut domain = Candle::new(
        inst_id.to_string(),
        timeframe,
        timestamp,
        Price::new(candle.open.parse::<f64>()?)
            .map_err(|error| anyhow!("创建开盘价失败: {:?}", error))?,
        Price::new(candle.high.parse::<f64>()?)
            .map_err(|error| anyhow!("创建最高价失败: {:?}", error))?,
        Price::new(candle.low.parse::<f64>()?)
            .map_err(|error| anyhow!("创建最低价失败: {:?}", error))?,
        Price::new(candle.close.parse::<f64>()?)
            .map_err(|error| anyhow!("创建收盘价失败: {:?}", error))?,
        Volume::new(volume).map_err(|error| anyhow!("创建成交量失败: {:?}", error))?,
    );

    let now_ms = Utc::now().timestamp_millis() as u64;
    let confirmed = candle
        .closed
        .unwrap_or_else(|| candle.close_time.map(|ts| ts <= now_ms).unwrap_or(true));
    if confirmed {
        domain.confirm();
    }

    Ok(domain)
}

async fn get_quant_core_sharded_candles_for_backtest(
    inst_id: &str,
    period: &str,
    limit: usize,
    select_time: Option<rust_quant_market::models::SelectTime>,
) -> Result<Vec<rust_quant_market::models::CandlesEntity>> {
    use rust_quant_market::models::TimeDirect;

    let timeframe =
        Timeframe::from_str(period).map_err(|error| anyhow!("无效的K线周期: {}", error))?;
    let table_name = PostgresCandleRepository::quoted_table_name(inst_id, timeframe)?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .context("CANDLE_SOURCE=quant_core 时必须设置 QUANT_CORE_DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_lazy(&database_url)
        .context("创建 quant_core Postgres K线连接池失败")?;

    let mut query = QueryBuilder::<Postgres>::new(format!(
        "SELECT id, ts, o, h, l, c, vol, vol_ccy, confirm, created_at, updated_at FROM {} WHERE confirm = '1'",
        table_name
    ));

    if let Some(select_time) = select_time {
        match select_time.direct {
            TimeDirect::BEFORE => {
                query.push(" AND ts <= ").push_bind(select_time.start_time);
                if let Some(end_time) = select_time.end_time {
                    query.push(" AND ts >= ").push_bind(end_time);
                }
            }
            TimeDirect::AFTER => {
                query.push(" AND ts >= ").push_bind(select_time.start_time);
                if let Some(end_time) = select_time.end_time {
                    query.push(" AND ts <= ").push_bind(end_time);
                }
            }
        }
    }

    query
        .push(" ORDER BY ts DESC LIMIT ")
        .push_bind(limit as i64);
    let mut candles = query
        .build_query_as::<rust_quant_market::models::CandlesEntity>()
        .fetch_all(&pool)
        .await
        .with_context(|| format!("查询 quant_core Postgres K线分表失败: {}", table_name))?;
    candles.sort_unstable_by_key(|candle| candle.ts);

    if candles.is_empty() {
        return Err(anyhow!(
            "K线数据为空: source=quant_core inst_id={} period={}",
            inst_id,
            period
        ));
    }

    Ok(candles)
}

/// Ticker数据服务
///
/// 提供实时行情数据访问接口，封装Ticker的业务逻辑
pub struct TickerService;

impl TickerService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TickerService {
    fn default() -> Self {
        Self::new()
    }
}

impl TickerService {
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
}

impl Default for MarketDepthService {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketDepthService {
    /// 获取市场深度数据
    pub async fn get_depth(&self, _symbol: &str, _depth: usize) -> Result<()> {
        // TODO: 实现市场深度查询
        Ok(())
    }
}
