use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use crypto_exc_all::raw::binance::api::market::BinanceMarket;
use reqwest::Client;
use rust_quant_domain::entities::{ExchangeSymbol, ExchangeSymbolListingEvent};
use rust_quant_domain::traits::ExchangeSymbolRepository;
use rust_quant_infrastructure::repositories::PostgresExchangeSymbolRepository;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::time::Duration;

const BINANCE_EXCHANGE: &str = "binance";
const OKX_EXCHANGE: &str = "okx";
const BITGET_EXCHANGE: &str = "bitget";
const GATE_EXCHANGE: &str = "gate";
const KUCOIN_EXCHANGE: &str = "kucoin";
const KUCOIN_FUTURES_BASE_URL_ENV: &str = "KUCOIN_FUTURES_BASE_URL";
const KUCOIN_FUTURES_DEFAULT_BASE_URL: &str = "https://api-futures.kucoin.com";
const PERPETUAL_MARKET_TYPE: &str = "perpetual";
const PERPETUAL_CONTRACT_TYPE: &str = "PERPETUAL";
const MAJOR_LISTING_EXCHANGES: &[&str] = &["binance", "okx"];
const DEFAULT_EXCHANGE_SYMBOL_SYNC_SOURCES: &[&str] =
    &["binance", "okx", "bitget", "gate", "kucoin"];

pub fn parse_exchange_symbol_sync_sources(input: Option<&str>) -> Result<Vec<String>> {
    let raw_sources = input.unwrap_or("binance okx bitget gate kucoin");
    let mut sources = Vec::new();
    for raw_source in raw_sources.split([',', ' ', '\n', '\t']) {
        if raw_source.trim().is_empty() {
            continue;
        }
        let normalized = normalize_exchange_symbol_sync_source(raw_source)?;
        if !sources.iter().any(|source| source == normalized) {
            sources.push(normalized.to_string());
        }
    }

    if sources.is_empty() {
        return Err(anyhow!("exchange symbol sync sources must not be empty"));
    }

    Ok(sources)
}

pub fn default_exchange_symbol_sync_sources() -> Vec<String> {
    DEFAULT_EXCHANGE_SYMBOL_SYNC_SOURCES
        .iter()
        .map(|source| (*source).to_string())
        .collect()
}

pub fn normalize_exchange_symbol_sync_source(source: &str) -> Result<&'static str> {
    match source.trim().to_ascii_lowercase().as_str() {
        "" => Err(anyhow!("empty exchange symbol sync source")),
        "binance" | "binance_usdm" | "binance_perpetual" => Ok("binance"),
        "okx" | "okx_swap" | "okx_perpetual" => Ok("okx"),
        "bitget" | "bitget_usdt_futures" | "bitget_perpetual" => Ok("bitget"),
        "gate" | "gate_usdt_futures" | "gate_perpetual" => Ok("gate"),
        "kucoin" | "kucoin_futures" | "kucoin_perpetual" => Ok("kucoin"),
        other => Err(anyhow!(
            "unsupported exchange symbol sync source={}, expected binance/okx/bitget/gate/kucoin",
            other
        )),
    }
}

#[async_trait]
pub trait BinanceExchangeInfoProvider: Send + Sync {
    async fn fetch_usdm_exchange_info(&self) -> Result<Value>;

    async fn fetch_okx_swap_instruments(&self) -> Result<Value> {
        Err(anyhow!("OKX swap instruments provider is not configured"))
    }

    async fn fetch_bitget_usdt_futures_contracts(&self) -> Result<Value> {
        Err(anyhow!(
            "Bitget USDT futures contracts provider is not configured"
        ))
    }

    async fn fetch_gate_usdt_futures_contracts(&self) -> Result<Value> {
        Err(anyhow!(
            "Gate USDT futures contracts provider is not configured"
        ))
    }

    async fn fetch_kucoin_futures_contracts(&self) -> Result<Value> {
        Err(anyhow!(
            "KuCoin futures contracts provider is not configured"
        ))
    }
}

pub struct LiveBinanceExchangeInfoProvider;

#[async_trait]
impl BinanceExchangeInfoProvider for LiveBinanceExchangeInfoProvider {
    async fn fetch_usdm_exchange_info(&self) -> Result<Value> {
        let market = BinanceMarket::new_public().context("create Binance public market client")?;
        market
            .get_exchange_info()
            .await
            .map_err(|error| anyhow!("fetch Binance exchangeInfo failed: {}", error))
    }

    async fn fetch_okx_swap_instruments(&self) -> Result<Value> {
        fetch_json("https://www.okx.com/api/v5/public/instruments?instType=SWAP")
            .await
            .context("fetch OKX swap instruments failed")
    }

    async fn fetch_bitget_usdt_futures_contracts(&self) -> Result<Value> {
        fetch_json("https://api.bitget.com/api/v2/mix/market/contracts?productType=USDT-FUTURES")
            .await
            .context("fetch Bitget USDT futures contracts failed")
    }

    async fn fetch_gate_usdt_futures_contracts(&self) -> Result<Value> {
        fetch_json("https://api.gateio.ws/api/v4/futures/usdt/contracts")
            .await
            .context("fetch Gate USDT futures contracts failed")
    }

    async fn fetch_kucoin_futures_contracts(&self) -> Result<Value> {
        fetch_json(&kucoin_futures_contracts_url())
            .await
            .context("fetch KuCoin futures contracts failed")
    }
}

pub struct StaticExchangeInfoProvider {
    payload: Value,
}

impl StaticExchangeInfoProvider {
    pub fn new(payload: Value) -> Self {
        Self { payload }
    }
}

#[async_trait]
impl BinanceExchangeInfoProvider for StaticExchangeInfoProvider {
    async fn fetch_usdm_exchange_info(&self) -> Result<Value> {
        Ok(self.payload.clone())
    }
}

pub struct ExchangeSymbolSyncService {
    repo: Arc<dyn ExchangeSymbolRepository>,
    provider: Arc<dyn BinanceExchangeInfoProvider>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MajorExchangeListingSignal {
    pub exchange: String,
    pub market_type: String,
    pub exchange_symbol: String,
    pub normalized_symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub prior_non_mainstream_exchanges: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExchangeSymbolSyncReport {
    pub persisted_count: usize,
    pub first_seen_count: usize,
    pub major_listing_signals: Vec<MajorExchangeListingSignal>,
}

impl ExchangeSymbolSyncService {
    pub async fn from_env() -> Result<Self> {
        let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
            .context("exchange symbol sync requires QUANT_CORE_DATABASE_URL")?;
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&database_url)
            .await
            .context("connect quant_core Postgres for exchange symbol sync")?;

        Ok(Self {
            repo: Arc::new(PostgresExchangeSymbolRepository::new(pool)),
            provider: Arc::new(LiveBinanceExchangeInfoProvider),
        })
    }

    pub fn with_repo_and_provider(
        repo: Arc<dyn ExchangeSymbolRepository>,
        provider: Arc<dyn BinanceExchangeInfoProvider>,
    ) -> Self {
        Self { repo, provider }
    }

    pub async fn sync_binance_usdm_perpetual_symbols(&self) -> Result<usize> {
        Ok(self
            .sync_binance_usdm_perpetual_symbols_with_report()
            .await?
            .persisted_count)
    }

    pub async fn sync_binance_usdm_perpetual_symbols_with_report(
        &self,
    ) -> Result<ExchangeSymbolSyncReport> {
        let payload = self
            .provider
            .fetch_usdm_exchange_info()
            .await
            .context("fetch Binance USD-M exchange info")?;
        let symbols = Self::parse_binance_usdm_exchange_info(&payload)?;
        self.persist_symbols_with_report(symbols).await
    }

    pub async fn sync_okx_swap_symbols_with_report(&self) -> Result<ExchangeSymbolSyncReport> {
        let payload = self
            .provider
            .fetch_okx_swap_instruments()
            .await
            .context("fetch OKX swap instruments")?;
        let symbols = Self::parse_okx_swap_instruments(&payload)?;
        self.persist_symbols_with_report(symbols).await
    }

    pub async fn sync_bitget_usdt_futures_symbols_with_report(
        &self,
    ) -> Result<ExchangeSymbolSyncReport> {
        let payload = self
            .provider
            .fetch_bitget_usdt_futures_contracts()
            .await
            .context("fetch Bitget USDT futures contracts")?;
        let symbols = Self::parse_bitget_usdt_futures_contracts(&payload)?;
        self.persist_symbols_with_report(symbols).await
    }

    pub async fn sync_gate_usdt_futures_symbols_with_report(
        &self,
    ) -> Result<ExchangeSymbolSyncReport> {
        let payload = self
            .provider
            .fetch_gate_usdt_futures_contracts()
            .await
            .context("fetch Gate USDT futures contracts")?;
        let symbols = Self::parse_gate_usdt_futures_contracts(&payload)?;
        self.persist_symbols_with_report(symbols).await
    }

    pub async fn sync_kucoin_futures_symbols_with_report(
        &self,
    ) -> Result<ExchangeSymbolSyncReport> {
        let payload = self
            .provider
            .fetch_kucoin_futures_contracts()
            .await
            .context("fetch KuCoin futures contracts")?;
        let symbols = Self::parse_kucoin_futures_contracts(&payload)?;
        self.persist_symbols_with_report(symbols).await
    }

    pub async fn sync_source_with_report(&self, source: &str) -> Result<ExchangeSymbolSyncReport> {
        match normalize_exchange_symbol_sync_source(source)? {
            "binance" => self.sync_binance_usdm_perpetual_symbols_with_report().await,
            "okx" => self.sync_okx_swap_symbols_with_report().await,
            "bitget" => self.sync_bitget_usdt_futures_symbols_with_report().await,
            "gate" => self.sync_gate_usdt_futures_symbols_with_report().await,
            "kucoin" => self.sync_kucoin_futures_symbols_with_report().await,
            other => Err(anyhow!(
                "unsupported normalized exchange symbol source={other}"
            )),
        }
    }

    async fn persist_symbols_with_report(
        &self,
        symbols: Vec<ExchangeSymbol>,
    ) -> Result<ExchangeSymbolSyncReport> {
        let count = symbols.len();
        let first_seen = self.repo.record_first_seen_many(&symbols).await?;
        self.repo.upsert_many(symbols).await?;

        let mut major_listing_signals = Vec::new();
        for listing in &first_seen {
            let history = self
                .repo
                .find_listing_events_by_asset(
                    &listing.base_asset,
                    &listing.quote_asset,
                    &listing.market_type,
                )
                .await?;
            let current_symbols = self
                .repo
                .find_by_asset(
                    &listing.base_asset,
                    &listing.quote_asset,
                    &listing.market_type,
                )
                .await?;
            if let Some(signal) = Self::detect_major_exchange_listing_with_current_symbols(
                listing,
                &history,
                &current_symbols,
            ) {
                major_listing_signals.push(signal);
            }
        }

        Ok(ExchangeSymbolSyncReport {
            persisted_count: count,
            first_seen_count: first_seen.len(),
            major_listing_signals,
        })
    }

    pub fn parse_binance_usdm_exchange_info(payload: &Value) -> Result<Vec<ExchangeSymbol>> {
        let symbols = payload
            .get("symbols")
            .and_then(Value::as_array)
            .context("Binance exchangeInfo missing symbols array")?;

        let mut rows = Vec::new();
        for symbol in symbols {
            let contract_type = symbol
                .get("contractType")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if contract_type != PERPETUAL_CONTRACT_TYPE {
                continue;
            }

            let exchange_symbol = required_str(symbol, "symbol")?.to_string();
            let base_asset = required_str(symbol, "baseAsset")?.to_uppercase();
            let quote_asset = required_str(symbol, "quoteAsset")?.to_uppercase();
            let status = required_str(symbol, "status")?.to_string();

            let mut row = ExchangeSymbol::new(
                BINANCE_EXCHANGE.to_string(),
                PERPETUAL_MARKET_TYPE.to_string(),
                exchange_symbol,
                format!("{base_asset}-{quote_asset}-SWAP"),
                base_asset,
                quote_asset,
                status,
            );

            row.contract_type = Some(contract_type.to_string());
            row.price_precision = symbol
                .get("pricePrecision")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok());
            row.quantity_precision = symbol
                .get("quantityPrecision")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok());
            row.min_qty = filter_value(symbol, "LOT_SIZE", &["minQty"]);
            row.max_qty = filter_value(symbol, "LOT_SIZE", &["maxQty"]);
            row.step_size = filter_value(symbol, "LOT_SIZE", &["stepSize"]);
            row.tick_size = filter_value(symbol, "PRICE_FILTER", &["tickSize"]);
            row.min_notional = filter_value(symbol, "MIN_NOTIONAL", &["notional", "minNotional"]);
            row.raw_payload = Some(symbol.clone());
            rows.push(row);
        }

        Ok(rows)
    }

    pub fn parse_okx_swap_instruments(payload: &Value) -> Result<Vec<ExchangeSymbol>> {
        let instruments = payload
            .get("data")
            .and_then(Value::as_array)
            .context("OKX instruments missing data array")?;

        let mut rows = Vec::new();
        for instrument in instruments {
            let inst_type = instrument
                .get("instType")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if inst_type != "SWAP" {
                continue;
            }

            let exchange_symbol =
                required_str_for(instrument, "instId", "OKX instruments")?.to_string();
            let base_asset =
                required_str_for(instrument, "baseCcy", "OKX instruments")?.to_uppercase();
            let quote_asset =
                required_str_for(instrument, "quoteCcy", "OKX instruments")?.to_uppercase();
            let status = required_str_for(instrument, "state", "OKX instruments")?.to_string();

            let mut row = ExchangeSymbol::new(
                OKX_EXCHANGE.to_string(),
                PERPETUAL_MARKET_TYPE.to_string(),
                exchange_symbol.clone(),
                normalize_swap_symbol(&base_asset, &quote_asset, Some(&exchange_symbol)),
                base_asset,
                quote_asset,
                status,
            );
            row.contract_type = optional_string(instrument, "ctType")
                .or_else(|| optional_string(instrument, "instType"));
            row.min_qty = optional_string(instrument, "minSz");
            row.step_size = optional_string(instrument, "lotSz");
            row.tick_size = optional_string(instrument, "tickSz");
            row.raw_payload = Some(instrument.clone());
            rows.push(row);
        }

        Ok(rows)
    }

    pub fn parse_bitget_usdt_futures_contracts(payload: &Value) -> Result<Vec<ExchangeSymbol>> {
        let contracts = payload
            .get("data")
            .and_then(Value::as_array)
            .context("Bitget contracts missing data array")?;

        let mut rows = Vec::new();
        for contract in contracts {
            let exchange_symbol =
                required_str_for(contract, "symbol", "Bitget contracts")?.to_string();
            let base_asset =
                required_str_for(contract, "baseCoin", "Bitget contracts")?.to_uppercase();
            let quote_asset =
                required_str_for(contract, "quoteCoin", "Bitget contracts")?.to_uppercase();
            let status =
                required_str_for(contract, "symbolStatus", "Bitget contracts")?.to_string();

            let mut row = ExchangeSymbol::new(
                BITGET_EXCHANGE.to_string(),
                PERPETUAL_MARKET_TYPE.to_string(),
                exchange_symbol,
                normalize_swap_symbol(&base_asset, &quote_asset, None),
                base_asset,
                quote_asset,
                status,
            );
            row.contract_type = optional_string(contract, "symbolType");
            row.price_precision = optional_i32(contract, "pricePlace");
            row.quantity_precision = optional_i32(contract, "volumePlace");
            row.min_qty = optional_string(contract, "minTradeNum");
            row.step_size = optional_string(contract, "sizeMultiplier");
            row.tick_size = optional_string(contract, "priceEndStep");
            row.min_notional = optional_string(contract, "minTradeUSDT");
            row.raw_payload = Some(contract.clone());
            rows.push(row);
        }

        Ok(rows)
    }

    pub fn parse_gate_usdt_futures_contracts(payload: &Value) -> Result<Vec<ExchangeSymbol>> {
        let contracts = payload
            .as_array()
            .context("Gate contracts missing root array")?;

        let mut rows = Vec::new();
        for contract in contracts {
            let contract_type = optional_string(contract, "contract_type").unwrap_or_default();
            if contract_type.eq_ignore_ascii_case("stocks") {
                continue;
            }

            let exchange_symbol = required_str_for(contract, "name", "Gate contracts")?.to_string();
            let Some((base_asset, quote_asset)) = exchange_symbol
                .split_once('_')
                .map(|(base, quote)| (base.to_uppercase(), quote.to_uppercase()))
            else {
                continue;
            };
            if quote_asset != "USDT" {
                continue;
            }
            let status = required_str_for(contract, "status", "Gate contracts")?.to_string();

            let mut row = ExchangeSymbol::new(
                GATE_EXCHANGE.to_string(),
                PERPETUAL_MARKET_TYPE.to_string(),
                exchange_symbol,
                normalize_swap_symbol(&base_asset, &quote_asset, None),
                base_asset,
                quote_asset,
                status,
            );
            row.contract_type = optional_string(contract, "type");
            row.min_qty = optional_scalar_string(contract, "order_size_min");
            row.max_qty = optional_scalar_string(contract, "order_size_max");
            row.step_size = optional_string(contract, "quanto_multiplier");
            row.tick_size = optional_string(contract, "order_price_round")
                .or_else(|| optional_string(contract, "mark_price_round"));
            row.raw_payload = Some(contract.clone());
            rows.push(row);
        }

        Ok(rows)
    }

    pub fn parse_kucoin_futures_contracts(payload: &Value) -> Result<Vec<ExchangeSymbol>> {
        let contracts = payload
            .get("data")
            .and_then(Value::as_array)
            .context("KuCoin contracts missing data array")?;

        let mut rows = Vec::new();
        for contract in contracts {
            let quote_asset =
                required_str_for(contract, "quoteCurrency", "KuCoin contracts")?.to_uppercase();
            if quote_asset != "USDT" {
                continue;
            }

            let exchange_symbol =
                required_str_for(contract, "symbol", "KuCoin contracts")?.to_string();
            let base_asset =
                required_str_for(contract, "baseCurrency", "KuCoin contracts")?.to_uppercase();
            let status = required_str_for(contract, "status", "KuCoin contracts")?.to_string();

            let mut row = ExchangeSymbol::new(
                KUCOIN_EXCHANGE.to_string(),
                PERPETUAL_MARKET_TYPE.to_string(),
                exchange_symbol,
                normalize_swap_symbol(&base_asset, &quote_asset, None),
                base_asset,
                quote_asset,
                status,
            );
            row.contract_type = optional_string(contract, "type");
            row.min_qty = optional_scalar_string(contract, "lotSize");
            row.step_size = optional_scalar_string(contract, "lotSize");
            row.tick_size = optional_scalar_string(contract, "tickSize");
            row.raw_payload = Some(contract.clone());
            rows.push(row);
        }

        Ok(rows)
    }

    pub fn detect_major_exchange_listing(
        new_listing: &ExchangeSymbolListingEvent,
        history: &[ExchangeSymbolListingEvent],
    ) -> Option<MajorExchangeListingSignal> {
        Self::detect_major_exchange_listing_with_current_symbols(new_listing, history, &[])
    }

    pub fn detect_major_exchange_listing_with_current_symbols(
        new_listing: &ExchangeSymbolListingEvent,
        history: &[ExchangeSymbolListingEvent],
        current_symbols: &[ExchangeSymbol],
    ) -> Option<MajorExchangeListingSignal> {
        if !is_major_listing_exchange(&new_listing.exchange) {
            return None;
        }

        let mut prior_non_mainstream_exchanges = Vec::new();
        for event in history.iter().filter(|event| {
            event
                .base_asset
                .eq_ignore_ascii_case(&new_listing.base_asset)
                && event
                    .quote_asset
                    .eq_ignore_ascii_case(&new_listing.quote_asset)
                && event
                    .market_type
                    .eq_ignore_ascii_case(&new_listing.market_type)
                && !event.exchange.eq_ignore_ascii_case(&new_listing.exchange)
        }) {
            if is_major_listing_exchange(&event.exchange) {
                return None;
            }
            push_unique_exchange(&mut prior_non_mainstream_exchanges, &event.exchange);
        }

        for symbol in current_symbols.iter().filter(|symbol| {
            symbol
                .base_asset
                .eq_ignore_ascii_case(&new_listing.base_asset)
                && symbol
                    .quote_asset
                    .eq_ignore_ascii_case(&new_listing.quote_asset)
                && symbol
                    .market_type
                    .eq_ignore_ascii_case(&new_listing.market_type)
                && !symbol.exchange.eq_ignore_ascii_case(&new_listing.exchange)
        }) {
            if is_major_listing_exchange(&symbol.exchange) {
                return None;
            }
            push_unique_exchange(&mut prior_non_mainstream_exchanges, &symbol.exchange);
        }

        if prior_non_mainstream_exchanges.is_empty() {
            return None;
        }

        Some(MajorExchangeListingSignal {
            exchange: normalize_exchange(&new_listing.exchange),
            market_type: new_listing.market_type.clone(),
            exchange_symbol: new_listing.exchange_symbol.clone(),
            normalized_symbol: new_listing.normalized_symbol.clone(),
            base_asset: new_listing.base_asset.clone(),
            quote_asset: new_listing.quote_asset.clone(),
            prior_non_mainstream_exchanges,
        })
    }
}

fn normalize_exchange(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn is_major_listing_exchange(exchange: &str) -> bool {
    let exchange = normalize_exchange(exchange);
    MAJOR_LISTING_EXCHANGES
        .iter()
        .any(|candidate| candidate == &exchange)
}

fn push_unique_exchange(exchanges: &mut Vec<String>, raw_exchange: &str) {
    let exchange = normalize_exchange(raw_exchange);
    if !exchange.is_empty() && !exchanges.iter().any(|existing| existing == &exchange) {
        exchanges.push(exchange);
    }
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    required_str_for(value, key, "Binance exchangeInfo")
}

fn required_str_for<'a>(value: &'a Value, key: &str, context: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing {} field: {}", context, key))
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn optional_scalar_string(value: &Value, key: &str) -> Option<String> {
    let raw = value.get(key)?;
    raw.as_str()
        .map(ToString::to_string)
        .or_else(|| raw.as_i64().map(|value| value.to_string()))
        .or_else(|| raw.as_u64().map(|value| value.to_string()))
        .or_else(|| raw.as_f64().map(|value| trim_float_string(value)))
}

fn optional_i32(value: &Value, key: &str) -> Option<i32> {
    value
        .get(key)
        .and_then(|raw| raw.as_i64().or_else(|| raw.as_str()?.parse::<i64>().ok()))
        .and_then(|value| i32::try_from(value).ok())
}

fn trim_float_string(value: f64) -> String {
    let value = value.to_string();
    value
        .strip_suffix(".0")
        .map(ToString::to_string)
        .unwrap_or(value)
}

fn normalize_swap_symbol(
    base_asset: &str,
    quote_asset: &str,
    exchange_symbol: Option<&str>,
) -> String {
    if let Some(exchange_symbol) = exchange_symbol {
        if exchange_symbol.to_ascii_uppercase().ends_with("-SWAP") {
            return exchange_symbol.to_ascii_uppercase();
        }
    }
    format!(
        "{}-{}-SWAP",
        base_asset.to_ascii_uppercase(),
        quote_asset.to_ascii_uppercase()
    )
}

fn filter_value(symbol: &Value, filter_type: &str, field_names: &[&str]) -> Option<String> {
    let filters = symbol.get("filters")?.as_array()?;
    let filter = filters.iter().find(|candidate| {
        candidate
            .get("filterType")
            .and_then(Value::as_str)
            .map(|value| value == filter_type)
            .unwrap_or(false)
    })?;

    field_names.iter().find_map(|field_name| {
        filter
            .get(*field_name)
            .and_then(Value::as_str)
            .map(ToString::to_string)
    })
}

async fn fetch_json(url: &str) -> Result<Value> {
    Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .map_err(Into::into)
}

fn kucoin_futures_contracts_url() -> String {
    let base_url = std::env::var(KUCOIN_FUTURES_BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| KUCOIN_FUTURES_DEFAULT_BASE_URL.to_string());
    format!("{base_url}/api/v1/contracts/active")
}
