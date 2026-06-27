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
pub use account_service::AccountService;
use anyhow::{anyhow, Context, Result};
pub use asset_service::AssetService;
use chrono::Utc;
pub use contracts_service::ContractsService;
use crypto_exc_all::{Candle as ExchangeCandle, CandleQuery, ExchangeId, Instrument};
pub use data_sync_service::DataSyncService;
pub use dune_market_sync_service::{DuneMarketSyncService, DuneSqlRunner};
pub use economic_calendar_sync_service::{EconomicCalendarSyncService, EconomicEventQueryService};
pub use exchange_symbol_sync_service::{
    default_exchange_symbol_sync_sources, normalize_exchange_symbol_sync_source,
    parse_exchange_symbol_sync_sources, BinanceExchangeInfoProvider,
    ExchangeSymbolAssetIconCandidate, ExchangeSymbolSyncService, LiveBinanceExchangeInfoProvider,
    MajorExchangeListingSignal, StaticExchangeInfoProvider,
};
pub use external_market_sync_service::{
    normalize_external_market_symbol, ExternalMarketDataProvider, ExternalMarketSource,
    ExternalMarketSyncService, HyperliquidExternalMarketDataProvider,
};
pub use public_data_service::PublicDataService;
use rust_quant_domain::traits::CandleRepository;
use rust_quant_domain::{Candle, Price, Timeframe, Volume};
use rust_quant_infrastructure::repositories::PostgresCandleRepository;
use rust_quant_market::models::{tickers::TickersDataEntity, CandlesEntity};
use sqlx::{postgres::PgPoolOptions, Postgres, QueryBuilder};
use std::str::FromStr;
mod scanner_service;
pub use scanner_service::ScannerService;
mod market_velocity_entry;
pub use market_velocity_entry::{
    build_market_velocity_entry_confirmation_from_candles, MarketVelocityEntryConfirmation,
    MarketVelocityEntryConfirmationBlocker, MarketVelocityEntryConfirmationConfig,
    MarketVelocityEntryConfirmationDecision,
};
mod market_velocity_signal;
pub use market_velocity_signal::{
    build_market_velocity_strategy_signal_request,
    build_market_velocity_strategy_signal_request_with_entry_confirmation,
    build_market_velocity_strategy_signal_request_with_entry_confirmation_and_selected_entry,
    dispatch_market_velocity_strategy_signal_if_enabled,
    dispatch_market_velocity_strategy_signal_with_config_and_entry_confirmation,
    dispatch_market_velocity_strategy_signal_with_entry_confirmation_if_enabled,
    market_velocity_signal_direct_dispatch_allowed, market_velocity_signal_dispatch_is_enabled,
    market_velocity_strategy_signal_needs_entry_confirmation, MarketVelocityFvgEntryMode,
    MarketVelocitySelectedEntry, MarketVelocityStrategySignalBlocker,
    MarketVelocityStrategySignalConfig, MarketVelocityStrategySignalDecision,
};
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
    /// repository，用于行情、K 线或市场扫描。
    repository: Box<dyn CandleRepository>,
}
impl CandleService {
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
    pub async fn get_latest_candle(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<Candle>> {
        self.repository.get_latest_candle(symbol, timeframe).await
    }
    pub async fn save_candles(&self, candles: Vec<Candle>) -> Result<usize> {
        self.repository.save_candles(candles).await
    }
    /// 从交易所获取K线数据
    /// # Arguments
    /// * `inst_id` - 交易对
    /// * `bar` - K线周期
    /// * `after` - 开始时间
    /// * `before` - 结束时间
    /// * `limit` - 数量限制
    /// # Returns
    /// * K线数据列表
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE），支持多交易所扩展
    pub async fn fetch_candles_from_exchange(
        &self,
        inst_id: &str,
        bar: &str,
        after: Option<&str>,
        before: Option<&str>,
        limit: Option<&str>,
    ) -> Result<Vec<CandlesEntity>> {
        use rust_quant_infrastructure::ExchangeFactory;
        let exchange = ExchangeFactory::create_default_market_data()?;
        let after_i64 = after.and_then(|s| s.parse().ok());
        let before_i64 = before.and_then(|s| s.parse().ok());
        let limit_usize = limit.and_then(|s| s.parse().ok());
        let candles_json = exchange
            .fetch_candles(inst_id, bar, after_i64, before_i64, limit_usize)
            .await?;
        let candles: Vec<CandlesEntity> = candles_json
            .into_iter()
            .filter_map(exchange_candle_value_to_entity)
            .collect();
        Ok(candles)
    }
    /// 从交易所获取最新K线数据
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE），支持多交易所扩展
    pub async fn fetch_latest_candles_from_exchange(
        &self,
        inst_id: &str,
        bar: &str,
        limit: Option<&str>,
    ) -> Result<Vec<CandlesEntity>> {
        use rust_quant_infrastructure::ExchangeFactory;
        let exchange = ExchangeFactory::create_default_market_data()?;
        let limit_usize = limit.and_then(|s| s.parse().ok());
        let candles_json = exchange
            .fetch_latest_candles(inst_id, bar, limit_usize)
            .await?;
        let candles: Vec<CandlesEntity> = candles_json
            .into_iter()
            .filter_map(exchange_candle_value_to_entity)
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
        let gateway = crypto_exc_all_gateway_from_env(exchange_id)?;
        let exchange_period = crypto_exc_all_candle_period(exchange_id, period);
        let mut query = CandleQuery::new(instrument, exchange_period).with_limit(limit);
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
    /// # 参数
    /// * `candles` - K线实体列表
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
/// 封装当前函数，减少行情数据调用方重复实现相同细节。
/// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 采用 async 以支持数据库/网络 I/O 的并发调度，避免阻塞。
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
    let candles = model.fetch_candles_from_postgres(dto).await?;
    if candles.is_empty() {
        return Err(anyhow::anyhow!(
            "K线数据为空: inst_id={}, period={}",
            inst_id,
            period
        ));
    }
    Ok(candles)
}
/// 判断 行情与市场数据 条件是否满足，给上层流程提供布尔决策。
pub fn should_use_quant_core_candle_source() -> Result<bool> {
    let source = std::env::var("CANDLE_SOURCE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if source.is_empty() || matches!(source.as_str(), "quant_core" | "postgres" | "pg") {
        return Ok(true);
    }
    Err(anyhow!("不支持的 CANDLE_SOURCE: {}", source))
}
/// 提供cryptoexcallgatewayfrom环境变量的集中实现，避免行情数据调用方重复处理相同细节。
fn crypto_exc_all_gateway_from_env(exchange: ExchangeId) -> Result<crate::CryptoExcAllGateway> {
    if exchange == ExchangeId::Okx {
        return crate::CryptoExcAllGateway::from_single_exchange_credentials(
            ExchangeId::Okx,
            std::env::var("OKX_API_KEY").unwrap_or_else(|_| "public-market-only".to_string()),
            std::env::var("OKX_API_SECRET").unwrap_or_else(|_| "public-market-only".to_string()),
            Some(
                std::env::var("OKX_PASSPHRASE")
                    .unwrap_or_else(|_| "public-market-only".to_string()),
            ),
            false,
        )
        .map_err(|error| anyhow!("创建 OKX crypto_exc_all gateway 失败: {}", error));
    }
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
/// 提供cryptoexcallK 线period的集中实现，避免行情数据调用方重复处理相同细节。
fn crypto_exc_all_candle_period(exchange: ExchangeId, period: &str) -> String {
    if exchange == ExchangeId::Binance {
        return match period.trim().to_ascii_lowercase().as_str() {
            "1dutc" => "1d".to_string(),
            value => value.to_string(),
        };
    }
    if exchange != ExchangeId::Okx {
        return period.to_string();
    }
    match period.trim() {
        "1Dutc" | "1DUTC" => "1Dutc".to_string(),
        value if value.ends_with('h') || value.ends_with('H') => {
            format!("{}H", &value[..value.len() - 1])
        }
        value if value.ends_with('d') || value.ends_with('D') => {
            format!("{}D", &value[..value.len() - 1])
        }
        value => value.to_string(),
    }
}
/// 提供默认交易所ID的集中实现，避免行情数据调用方重复处理相同细节。
fn default_exchange_id() -> Result<ExchangeId> {
    let exchange = std::env::var("DEFAULT_EXCHANGE")
        .or_else(|_| std::env::var("EXCHANGE_NAME"))
        .unwrap_or_else(|_| "okx".to_string());
    ExchangeId::from_str(&exchange).map_err(|error| anyhow!(error))
}
/// 提供交易所K 线值toentity的集中实现，避免行情数据调用方重复处理相同细节。
fn exchange_candle_value_to_entity(value: serde_json::Value) -> Option<CandlesEntity> {
    if let Some(values) = value.as_array() {
        let field = |index: usize| -> Option<String> {
            values.get(index).and_then(|value| match value {
                serde_json::Value::String(text) => Some(text.clone()),
                serde_json::Value::Number(number) => Some(number.to_string()),
                _ => None,
            })
        };
        let ts = field(0)?.parse::<i64>().ok()?;
        let vol = field(5).unwrap_or_default();
        return Some(CandlesEntity {
            id: None,
            ts,
            o: field(1)?,
            h: field(2)?,
            l: field(3)?,
            c: field(4)?,
            vol: vol.clone(),
            vol_ccy: field(6).unwrap_or_else(|| vol.clone()),
            confirm: field(8).unwrap_or_else(|| "1".to_string()),
            created_at: None,
            updated_at: None,
        });
    }
    let field = |key: &str| -> Option<String> {
        value.get(key).and_then(|value| match value {
            serde_json::Value::String(text) => Some(text.clone()),
            serde_json::Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
    };
    let ts = field("ts")?.parse::<i64>().ok()?;
    let vol = field("v").or_else(|| field("vol")).unwrap_or_default();
    Some(CandlesEntity {
        id: None,
        ts,
        o: field("o")?,
        h: field("h")?,
        l: field("l")?,
        c: field("c")?,
        vol: vol.clone(),
        vol_ccy: field("vol_ccy")
            .or_else(|| field("volCcy"))
            .or_else(|| field("vol_ccy_quote"))
            .unwrap_or(vol),
        confirm: field("confirm").unwrap_or_else(|| "1".to_string()),
        created_at: None,
        updated_at: None,
    })
}
/// 提供instrumentfrominstID的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 提供交易所K 线todomain的集中实现，避免行情数据调用方重复处理相同细节。
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
/// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 如果数据库中已存在该inst_id的记录，则更新；否则插入新记录
    /// # Arguments
    /// * `ticker` - Ticker数据
    /// # Returns
    /// * `true` - 插入新记录
    /// * `false` - 更新现有记录
    pub async fn save_or_update_ticker(&self, ticker: &TickersDataEntity) -> Result<bool> {
        use rust_quant_market::models::tickers::TicketsModel;
        use tracing::debug;
        let model = TicketsModel::new();
        let existing = model.find_one(&ticker.inst_id).await?;
        if existing.is_empty() {
            debug!("Ticker不存在，插入新数据: {}", ticker.inst_id);
            model.add_entities(vec![ticker.clone()]).await?;
            Ok(true)
        } else {
            debug!("Ticker已存在，更新数据: {}", ticker.inst_id);
            model.update_entity(ticker).await?;
            Ok(false)
        }
    }
    /// 批量更新Ticker数据
    /// 遍历tickers列表，对于在inst_ids中的ticker，执行保存或更新操作
    /// # Arguments
    /// * `tickers` - Ticker数据列表
    /// * `inst_ids` - 需要同步的交易对列表（如果为空，则同步所有ticker）
    /// # Returns
    /// * 成功处理的ticker数量
    pub async fn batch_update_tickers(
        &self,
        tickers: Vec<TickersDataEntity>,
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
                    model.add_entities(vec![ticker]).await?;
                } else {
                    debug!("已经存在，更新数据: {}", inst_id);
                    model.update_entity(&ticker).await?;
                }
                count += 1;
            }
        }
        Ok(count)
    }
    /// 查询指定交易对的Ticker数据（从数据库）
    /// # Arguments
    /// * `inst_id` - 交易对ID
    /// # Returns
    /// * Ticker数据列表（可能为空）
    pub async fn find_ticker(&self, inst_id: &str) -> Result<Vec<TickersDataEntity>> {
        use rust_quant_market::models::tickers::TicketsModel;
        let model = TicketsModel::new();
        model.find_one(inst_id).await
    }
    /// 从交易所获取单个Ticker数据
    /// # Arguments
    /// * `inst_id` - 交易对ID
    /// # Returns
    /// * Ticker数据（可能为空）
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE），支持多交易所扩展
    pub async fn fetch_ticker_from_exchange(
        &self,
        inst_id: &str,
    ) -> Result<Option<TickersDataEntity>> {
        let exchange_id = default_exchange_id()?;
        let gateway = crypto_exc_all_gateway_from_env(exchange_id)?;
        let instrument = instrument_from_inst_id(inst_id)?;
        let ticker = gateway
            .ticker(exchange_id, &instrument)
            .await
            .map_err(|error| anyhow!("通过 crypto_exc_all 获取Ticker失败: {}", error))?;
        Ok(Some(TickersDataEntity::from_exchange_ticker(&ticker)))
    }
    /// 从交易所批量获取Ticker数据
    /// # Arguments
    /// * `inst_type` - 合约类型（如"SWAP"）
    /// # Returns
    /// * Ticker数据列表
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE），支持多交易所扩展
    pub async fn fetch_tickers_from_exchange(
        &self,
        inst_type: &str,
    ) -> Result<Vec<TickersDataEntity>> {
        let exchange_id = default_exchange_id()?;
        if exchange_id != ExchangeId::Okx {
            return Err(anyhow!("批量Ticker同步当前仅支持 OKX: {:?}", exchange_id));
        }
        use okx::api::api_trait::OkxApiTrait;
        use okx::api::market::OkxMarket;
        let client = OkxMarket::from_env()?;
        let tickers = client
            .get_tickers(inst_type)
            .await
            .map_err(|error| anyhow!("通过 OKX 获取批量Ticker失败: {}", error))?;
        Ok(tickers
            .iter()
            .map(TickersDataEntity::from_okx_ticker)
            .collect())
    }
    /// 同步单个Ticker（从交易所获取并保存）
    /// 完整的业务流程：从交易所获取 → 保存到数据库
    /// # Arguments
    /// * `inst_id` - 交易对ID
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
    /// 完整的业务流程：从交易所获取 → 批量保存到数据库
    /// # Arguments
    /// * `inst_type` - 合约类型（如"SWAP"）
    /// * `inst_ids` - 需要同步的交易对列表（如果为空，则同步所有ticker）
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
    /// # Arguments
    /// * `inst_type` - 合约类型（如"SWAP"）
    /// * `top_n` - 返回前N个
    /// # Returns
    /// * 按交易量排序的前N个ticker
    pub async fn fetch_top_contracts_by_volume(
        &self,
        inst_type: &str,
        top_n: usize,
    ) -> Result<Vec<TickersDataEntity>> {
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
    pub async fn get_depth(&self, _symbol: &str, _depth: usize) -> Result<()> {
        // TODO: 实现市场深度查询
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    /// 封装环境变量lock，减少行情数据调用方重复实现相同细节。
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }
    /// 删除或清理 行情与市场数据 的临时数据，避免过期状态继续影响后续流程。
    fn clear_okx_env() -> Vec<(&'static str, Option<String>)> {
        let names = ["OKX_API_KEY", "OKX_API_SECRET", "OKX_PASSPHRASE"];
        names
            .into_iter()
            .map(|name| {
                let previous = std::env::var(name).ok();
                std::env::remove_var(name);
                (name, previous)
            })
            .collect()
    }
    /// 提供restore环境变量的集中实现，避免行情数据调用方重复处理相同细节。
    fn restore_env(previous: Vec<(&'static str, Option<String>)>) {
        for (name, value) in previous {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
    #[test]
    fn okx_market_gateway_is_available_without_private_credentials() {
        let _guard = env_lock();
        let previous = clear_okx_env();
        let gateway = crypto_exc_all_gateway_from_env(ExchangeId::Okx)
            .expect("OKX public market gateway should not require private credentials");
        let configured_exchanges = gateway.configured_exchanges();
        restore_env(previous);
        assert_eq!(configured_exchanges, vec![ExchangeId::Okx]);
    }
    #[test]
    fn okx_candle_period_uses_okx_bar_case() {
        assert_eq!(crypto_exc_all_candle_period(ExchangeId::Okx, "4h"), "4H");
        assert_eq!(crypto_exc_all_candle_period(ExchangeId::Okx, "1h"), "1H");
        assert_eq!(crypto_exc_all_candle_period(ExchangeId::Okx, "1d"), "1D");
        assert_eq!(
            crypto_exc_all_candle_period(ExchangeId::Okx, "1Dutc"),
            "1Dutc"
        );
        assert_eq!(crypto_exc_all_candle_period(ExchangeId::Okx, "15m"), "15m");
        assert_eq!(
            crypto_exc_all_candle_period(ExchangeId::Binance, "4h"),
            "4h"
        );
        assert_eq!(
            crypto_exc_all_candle_period(ExchangeId::Binance, "1H"),
            "1h"
        );
        assert_eq!(
            crypto_exc_all_candle_period(ExchangeId::Binance, "4H"),
            "4h"
        );
        assert_eq!(
            crypto_exc_all_candle_period(ExchangeId::Binance, "1DUTC"),
            "1d"
        );
    }
}
