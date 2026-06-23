use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use hmac::{Hmac, Mac};
use rust_decimal::Decimal;
use rust_quant_services::rust_quan_web::UserExchangeConfig;
use serde_json::Value;
use sha2::Sha256;
use std::str::FromStr;
use std::time::Duration;

use super::{BinanceEthMicroLiveValidationConfig, BinancePositionMode, BinanceSymbolFilters};

type HmacSha256 = Hmac<Sha256>;

pub(super) struct BinanceFuturesHttp {
    /// 复用连接池的 HTTP 客户端。
    client: reqwest::Client,
    /// Binance Futures API 基础地址。
    base_url: String,
    /// Binance API Key。
    api_key: String,
    /// Binance API Secret，用于签名私有请求。
    api_secret: String,
}

impl BinanceFuturesHttp {
    /// 构建 Binance Futures HTTP 客户端并配置超时、代理和签名密钥。
    pub(super) fn new(
        config: &BinanceEthMicroLiveValidationConfig,
        user_config: &UserExchangeConfig,
    ) -> Result<Self> {
        if !user_config.exchange.eq_ignore_ascii_case("binance")
            && user_config.exchange.trim() != "币安"
        {
            bail!("resolved credential exchange is not Binance");
        }
        let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(15));
        if let Some(proxy_url) = config.proxy_url.as_deref() {
            builder = builder.proxy(reqwest::Proxy::all(proxy_url)?);
        }
        Ok(Self {
            client: builder.build()?,
            base_url: config
                .binance_fapi_base_url
                .trim_end_matches('/')
                .to_string(),
            api_key: user_config.api_key.clone(),
            api_secret: user_config.api_secret.clone(),
        })
    }

    /// 读取 Binance exchangeInfo 并提取指定 symbol 的交易过滤器。
    pub(super) async fn exchange_info_filters(&self, symbol: &str) -> Result<BinanceSymbolFilters> {
        let url = format!("{}/fapi/v1/exchangeInfo", self.base_url);
        let response = self
            .client
            .get(url)
            .query(&[("symbol", symbol)])
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        parse_symbol_filters(&response, symbol)
    }

    /// 读取 Binance 标记价。
    pub(super) async fn mark_price(&self, symbol: &str) -> Result<Decimal> {
        let url = format!("{}/fapi/v1/premiumIndex", self.base_url);
        let response = self
            .client
            .get(url)
            .query(&[("symbol", symbol)])
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        decimal_field(&response, "markPrice")
            .or_else(|| decimal_field(&response, "lastPrice"))
            .ok_or_else(|| anyhow!("Binance premiumIndex response missing markPrice"))
    }

    /// 读取 Binance 账户持仓模式。
    pub(super) async fn position_mode(&self) -> Result<BinancePositionMode> {
        let response = self
            .signed_get_json("/fapi/v1/positionSide/dual", Vec::new())
            .await?;
        match response.get("dualSidePosition").and_then(Value::as_bool) {
            Some(true) => Ok(BinancePositionMode::Hedge),
            Some(false) => Ok(BinancePositionMode::OneWay),
            None => bail!("Binance positionSide/dual response missing dualSidePosition"),
        }
    }

    /// 发送 Binance 私有 GET 请求并附加 HMAC 签名。
    pub(super) async fn signed_get_json(
        &self,
        path: &str,
        mut params: Vec<(String, String)>,
    ) -> Result<Value> {
        params.push((
            "timestamp".to_string(),
            Utc::now().timestamp_millis().to_string(),
        ));
        params.push(("recvWindow".to_string(), "5000".to_string()));
        let unsigned_query = params
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&");
        let mut mac = HmacSha256::new_from_slice(self.api_secret.as_bytes())
            .map_err(|_| anyhow!("invalid Binance API secret for HMAC"))?;
        mac.update(unsigned_query.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        let query = format!("{unsigned_query}&signature={signature}");
        let url = format!("{}{}?{}", self.base_url, path, query);
        Ok(self
            .client
            .get(url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?)
    }
}

/// 从 exchangeInfo 响应中解析数量、价格和名义金额限制。
fn parse_symbol_filters(response: &Value, symbol: &str) -> Result<BinanceSymbolFilters> {
    let symbol_info = response
        .get("symbols")
        .and_then(Value::as_array)
        .and_then(|symbols| {
            symbols.iter().find(|item| {
                item.get("symbol")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.eq_ignore_ascii_case(symbol))
            })
        })
        .ok_or_else(|| anyhow!("Binance exchangeInfo missing symbol {symbol}"))?;
    let filters = symbol_info
        .get("filters")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Binance exchangeInfo missing filters for {symbol}"))?;
    let price_tick = filter_decimal(filters, "PRICE_FILTER", "tickSize")?;
    let market_step = filter_decimal(filters, "MARKET_LOT_SIZE", "stepSize")
        .or_else(|_| filter_decimal(filters, "LOT_SIZE", "stepSize"))?;
    let min_quantity = filter_decimal(filters, "MARKET_LOT_SIZE", "minQty")
        .or_else(|_| filter_decimal(filters, "LOT_SIZE", "minQty"))?;
    let min_notional = filter_decimal(filters, "MIN_NOTIONAL", "notional")
        .or_else(|_| filter_decimal(filters, "MIN_NOTIONAL", "minNotional"))?;
    Ok(BinanceSymbolFilters {
        quantity_step: market_step,
        min_quantity,
        min_notional,
        price_tick,
    })
}

/// 从 Binance filter 中读取 Decimal 字段。
fn filter_decimal(filters: &[Value], filter_type: &str, field: &str) -> Result<Decimal> {
    filters
        .iter()
        .find(|filter| {
            filter
                .get("filterType")
                .and_then(Value::as_str)
                .is_some_and(|value| value == filter_type)
        })
        .and_then(|filter| decimal_field(filter, field))
        .ok_or_else(|| anyhow!("Binance filter {filter_type}.{field} missing or invalid"))
}

/// 确认 ETH 仓位为空，避免验证前存在外部持仓。
pub(super) fn ensure_eth_position_flat(account: &Value, symbol: &str) -> Result<()> {
    let positions = account
        .get("positions")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Binance account response missing positions"))?;
    for position in positions {
        let position_symbol = position
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !position_symbol.eq_ignore_ascii_case(symbol) {
            continue;
        }
        let amount = decimal_field(position, "positionAmt").unwrap_or(Decimal::ZERO);
        if amount != Decimal::ZERO {
            bail!("{symbol} position is not flat; signed read-only positionAmt is non-zero");
        }
    }
    Ok(())
}

/// 确认交易对没有未完成订单。
pub(super) fn ensure_no_open_orders(open_orders: &Value, symbol: &str) -> Result<()> {
    let Some(orders) = open_orders.as_array() else {
        bail!("Binance openOrders response is not an array");
    };
    if !orders.is_empty() {
        bail!("{symbol} has {} open Binance Futures orders", orders.len());
    }
    Ok(())
}

/// 读取合约账户可用 USDT 余额。
pub(super) fn available_usdt_balance(account: &Value) -> Result<Decimal> {
    let assets = account
        .get("assets")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Binance account response missing assets"))?;
    assets
        .iter()
        .find(|asset| {
            asset
                .get("asset")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "USDT")
        })
        .and_then(|asset| decimal_field(asset, "availableBalance"))
        .ok_or_else(|| anyhow!("Binance account response missing USDT availableBalance"))
}

/// 读取 Binance symbolConfig 响应中的当前杠杆倍数。
pub(super) fn symbol_config_leverage(response: &Value, symbol: &str) -> Result<Decimal> {
    let configs = response
        .as_array()
        .ok_or_else(|| anyhow!("Binance symbolConfig response is not an array"))?;
    configs
        .iter()
        .find(|item| {
            item.get("symbol")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case(symbol))
        })
        .and_then(|item| decimal_field(item, "leverage"))
        .filter(|value| *value > Decimal::ZERO)
        .ok_or_else(|| anyhow!("Binance symbolConfig missing positive leverage for {symbol}"))
}

/// 从 JSON 字段中解析 Decimal。
fn decimal_field(value: &Value, field: &str) -> Option<Decimal> {
    value.get(field).and_then(|raw| match raw {
        Value::String(text) => Decimal::from_str(text.trim()).ok(),
        Value::Number(number) => Decimal::from_str(&number.to_string()).ok(),
        _ => None,
    })
}
