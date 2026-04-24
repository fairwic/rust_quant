use crypto_exc_all::{
    BinanceExchangeConfig, BitgetExchangeConfig, Candle, CandleQuery, CryptoSdk, Error, ExchangeId,
    Instrument, MarginMode, OkxExchangeConfig, OrderAck, OrderSide, OrderType, PlaceOrderRequest,
    Result, SdkConfig, Ticker, TimeInForce,
};
use serde_json::json;

#[derive(Debug, Clone, PartialEq)]
pub struct OrderPlacementRequest {
    pub exchange: ExchangeId,
    pub instrument: Instrument,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub size: String,
    pub price: Option<String>,
    pub margin_mode: Option<MarginMode>,
    pub margin_coin: Option<String>,
    pub position_side: Option<String>,
    pub trade_side: Option<String>,
    pub client_order_id: Option<String>,
    pub reduce_only: Option<bool>,
    pub time_in_force: Option<TimeInForce>,
}

impl OrderPlacementRequest {
    pub fn into_place_order_request(self) -> PlaceOrderRequest {
        PlaceOrderRequest {
            instrument: self.instrument,
            side: self.side,
            order_type: self.order_type,
            size: self.size,
            price: self.price,
            margin_mode: self.margin_mode,
            margin_coin: self.margin_coin,
            position_side: self.position_side,
            trade_side: self.trade_side,
            client_order_id: self.client_order_id,
            reduce_only: self.reduce_only,
            time_in_force: self.time_in_force,
        }
    }
}

enum GatewayMode {
    Live(CryptoSdk),
    DryRun,
}

pub struct CryptoExcAllGateway {
    mode: GatewayMode,
}

impl CryptoExcAllGateway {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            mode: GatewayMode::Live(CryptoSdk::from_env()?),
        })
    }

    pub fn from_sdk(sdk: CryptoSdk) -> Self {
        Self {
            mode: GatewayMode::Live(sdk),
        }
    }

    pub fn dry_run() -> Self {
        Self {
            mode: GatewayMode::DryRun,
        }
    }

    pub fn from_single_exchange_credentials(
        exchange: ExchangeId,
        api_key: impl Into<String>,
        api_secret: impl Into<String>,
        passphrase: Option<impl Into<String>>,
        simulated: bool,
    ) -> Result<Self> {
        let api_key = api_key.into();
        let api_secret = api_secret.into();
        let passphrase = passphrase.map(Into::into);
        let config = match exchange {
            ExchangeId::Okx => SdkConfig {
                okx: Some(OkxExchangeConfig {
                    api_key,
                    api_secret,
                    passphrase: passphrase.ok_or_else(|| {
                        Error::Config("OKX exchange credentials require passphrase".to_string())
                    })?,
                    simulated,
                    api_url: None,
                    request_expiration_ms: None,
                }),
                ..SdkConfig::default()
            },
            ExchangeId::Binance => SdkConfig {
                binance: Some(BinanceExchangeConfig {
                    api_key,
                    api_secret,
                    api_url: None,
                    sapi_api_url: None,
                    web_api_url: None,
                    ws_stream_url: None,
                    api_timeout_ms: None,
                    recv_window_ms: None,
                    proxy_url: None,
                }),
                ..SdkConfig::default()
            },
            ExchangeId::Bitget => SdkConfig {
                bitget: Some(BitgetExchangeConfig {
                    api_key,
                    api_secret,
                    passphrase: passphrase.ok_or_else(|| {
                        Error::Config("Bitget exchange credentials require passphrase".to_string())
                    })?,
                    api_url: None,
                    api_timeout_ms: None,
                    proxy_url: None,
                    product_type: None,
                }),
                ..SdkConfig::default()
            },
        };

        Ok(Self::from_sdk(CryptoSdk::from_config(config)?))
    }

    pub fn configured_exchanges(&self) -> Vec<ExchangeId> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.configured_exchanges(),
            GatewayMode::DryRun => Vec::new(),
        }
    }

    pub async fn ticker(&self, exchange: ExchangeId, instrument: &Instrument) -> Result<Ticker> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.market(exchange)?.ticker(instrument).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run ticker",
            }),
        }
    }

    pub async fn candles(&self, exchange: ExchangeId, query: CandleQuery) -> Result<Vec<Candle>> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.market(exchange)?.candles(query).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run candles",
            }),
        }
    }

    pub async fn place_order(&self, request: OrderPlacementRequest) -> Result<OrderAck> {
        match &self.mode {
            GatewayMode::Live(sdk) => {
                let trade = sdk.trade(request.exchange)?;
                trade.place_order(request.into_place_order_request()).await
            }
            GatewayMode::DryRun => Ok(OrderAck {
                exchange: request.exchange,
                exchange_symbol: request.instrument.symbol_for(request.exchange),
                instrument: request.instrument,
                order_id: Some(format!(
                    "dry-run-{}",
                    request
                        .client_order_id
                        .clone()
                        .unwrap_or_else(|| "order".to_string())
                )),
                client_order_id: request.client_order_id,
                status: Some("dry_run".to_string()),
                raw: json!({
                    "dry_run": true,
                    "side": request.side,
                    "order_type": request.order_type,
                    "size": request.size,
                    "price": request.price,
                }),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_our_request_to_crypto_exc_all_request() {
        let request = OrderPlacementRequest {
            exchange: ExchangeId::Okx,
            instrument: Instrument::perp("btc", "usdt").with_settlement("usdt"),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            size: "1.5".to_string(),
            price: Some("65000".to_string()),
            margin_mode: Some(MarginMode::Isolated),
            margin_coin: Some("USDT".to_string()),
            position_side: Some("long".to_string()),
            trade_side: Some("open".to_string()),
            client_order_id: Some("rq-1".to_string()),
            reduce_only: Some(false),
            time_in_force: Some(TimeInForce::Gtc),
        };

        let mapped = request.clone().into_place_order_request();
        assert_eq!(mapped.instrument, request.instrument);
        assert_eq!(mapped.side, request.side);
        assert_eq!(mapped.order_type, request.order_type);
        assert_eq!(mapped.size, request.size);
        assert_eq!(mapped.price, request.price);
        assert_eq!(mapped.margin_mode, request.margin_mode);
        assert_eq!(mapped.margin_coin, request.margin_coin);
        assert_eq!(mapped.position_side, request.position_side);
        assert_eq!(mapped.trade_side, request.trade_side);
        assert_eq!(mapped.client_order_id, request.client_order_id);
        assert_eq!(mapped.reduce_only, request.reduce_only);
        assert_eq!(mapped.time_in_force, request.time_in_force);
    }

    #[tokio::test]
    async fn dry_run_place_order_returns_simulated_ack_without_credentials() {
        let gateway = CryptoExcAllGateway::dry_run();
        let request = OrderPlacementRequest {
            exchange: ExchangeId::Okx,
            instrument: Instrument::perp("btc", "usdt").with_settlement("usdt"),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            size: "0.01".to_string(),
            price: None,
            margin_mode: Some(MarginMode::Cross),
            margin_coin: Some("USDT".to_string()),
            position_side: Some("long".to_string()),
            trade_side: Some("open".to_string()),
            client_order_id: Some("rq-dry-run".to_string()),
            reduce_only: Some(false),
            time_in_force: None,
        };

        let ack = gateway.place_order(request).await.unwrap();

        assert_eq!(ack.exchange, ExchangeId::Okx);
        assert_eq!(ack.exchange_symbol, "BTC-USDT-SWAP");
        assert_eq!(ack.client_order_id.as_deref(), Some("rq-dry-run"));
        assert_eq!(ack.status.as_deref(), Some("dry_run"));
        assert_eq!(ack.raw["dry_run"], true);
    }

    #[test]
    fn builds_gateway_from_single_okx_runtime_config() {
        let gateway = CryptoExcAllGateway::from_single_exchange_credentials(
            ExchangeId::Okx,
            "api-key",
            "api-secret",
            Some("passphrase"),
            true,
        )
        .unwrap();

        assert_eq!(gateway.configured_exchanges(), vec![ExchangeId::Okx]);
    }
}
