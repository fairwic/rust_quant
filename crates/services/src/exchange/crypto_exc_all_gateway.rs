use crypto_exc_all::{
    AccountBill, AccountBillQuery, Balance, BinanceExchangeConfig, BitgetExchangeConfig,
    BybitExchangeConfig, CancelOrderRequest, Candle, CandleQuery, CryptoSdk, Error, ExchangeId,
    Fill, FillListQuery, GateExchangeConfig, Instrument, MarginMode, OkxExchangeConfig, Order,
    OrderAck, OrderBook, OrderBookQuery, OrderListQuery, OrderQuery, OrderSide, OrderType,
    PlaceOrderRequest, Position, PositionHistory, PositionHistoryQuery,
    PrepareOrderSettingsRequest, PrepareOrderSettingsResult, ProtectiveOrderQuery,
    ProtectiveOrderRequest, Result, SdkConfig, Ticker, TimeInForce,
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
    pub attached_stop_loss_price: Option<String>,
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
            attached_stop_loss_price: self.attached_stop_loss_price,
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
                    request_expiration_ms: okx_request_expiration_ms_from_env(),
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
                    proxy_url: binance_proxy_url_from_env(),
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
            ExchangeId::Bybit => SdkConfig {
                bybit: Some(BybitExchangeConfig {
                    api_key,
                    api_secret,
                    api_url: None,
                    api_timeout_ms: None,
                    recv_window_ms: None,
                    proxy_url: None,
                    category: None,
                }),
                ..SdkConfig::default()
            },
            ExchangeId::Gate => SdkConfig {
                gate: Some(GateExchangeConfig {
                    api_key,
                    api_secret,
                    api_url: None,
                    api_timeout_ms: None,
                    proxy_url: None,
                    settle: None,
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

    pub async fn tickers(
        &self,
        exchange: ExchangeId,
        _instrument_type: &str,
    ) -> Result<Vec<Ticker>> {
        Err(Error::Unsupported {
            exchange,
            capability: "bulk tickers via crypto_exc_all",
        })
    }

    pub async fn orderbook(
        &self,
        exchange: ExchangeId,
        query: OrderBookQuery,
    ) -> Result<OrderBook> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.market(exchange)?.orderbook(query).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run orderbook",
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

    pub async fn prepare_order_settings(
        &self,
        exchange: ExchangeId,
        request: PrepareOrderSettingsRequest,
    ) -> Result<PrepareOrderSettingsResult> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.account(exchange)?.prepare_order_settings(request).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run prepare_order_settings",
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
                    "attached_stop_loss_price": request.attached_stop_loss_price,
                }),
            }),
        }
    }

    pub async fn place_protective_order(
        &self,
        exchange: ExchangeId,
        request: ProtectiveOrderRequest,
    ) -> Result<OrderAck> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.trade(exchange)?.place_protective_order(request).await,
            GatewayMode::DryRun => Ok(OrderAck {
                exchange,
                exchange_symbol: request.instrument.symbol_for(exchange),
                instrument: request.instrument,
                order_id: Some(format!(
                    "dry-run-protective-{}",
                    request
                        .client_order_id
                        .clone()
                        .unwrap_or_else(|| "order".to_string())
                )),
                client_order_id: request.client_order_id,
                status: Some("dry_run".to_string()),
                raw: json!({
                    "dry_run": true,
                    "protective": true,
                    "side": request.side,
                    "stop_price": request.stop_price,
                    "position_side": request.position_side,
                    "reduce_only": request.reduce_only,
                    "close_position": request.close_position,
                    "working_type": request.working_type,
                    "price_protect": request.price_protect,
                }),
            }),
        }
    }

    pub async fn order(&self, exchange: ExchangeId, query: OrderQuery) -> Result<Order> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.orders(exchange)?.get(query).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run order query",
            }),
        }
    }

    pub async fn protective_order(
        &self,
        exchange: ExchangeId,
        query: ProtectiveOrderQuery,
    ) -> Result<Order> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.orders(exchange)?.get_protective_order(query).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run protective order query",
            }),
        }
    }

    pub async fn open_orders(
        &self,
        exchange: ExchangeId,
        query: OrderListQuery,
    ) -> Result<Vec<Order>> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.orders(exchange)?.open(query).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run open orders query",
            }),
        }
    }

    pub async fn order_history(
        &self,
        exchange: ExchangeId,
        query: OrderListQuery,
    ) -> Result<Vec<Order>> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.orders(exchange)?.history(query).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run order history query",
            }),
        }
    }

    pub async fn position_history(
        &self,
        exchange: ExchangeId,
        query: PositionHistoryQuery,
    ) -> Result<Vec<PositionHistory>> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.positions(exchange)?.history(query).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run position history query",
            }),
        }
    }

    pub async fn fills(&self, exchange: ExchangeId, query: FillListQuery) -> Result<Vec<Fill>> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.fills(exchange)?.list(query).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run fills query",
            }),
        }
    }

    pub async fn balances(&self, exchange: ExchangeId) -> Result<Vec<Balance>> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.account(exchange)?.balances().await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run balance query",
            }),
        }
    }

    pub async fn account_bills(
        &self,
        exchange: ExchangeId,
        query: AccountBillQuery,
    ) -> Result<Vec<AccountBill>> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.account(exchange)?.bills(query).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run account bills query",
            }),
        }
    }

    pub async fn positions(
        &self,
        exchange: ExchangeId,
        instrument: Option<&Instrument>,
    ) -> Result<Vec<Position>> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.positions(exchange)?.list(instrument).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run position query",
            }),
        }
    }

    pub async fn cancel_order(
        &self,
        exchange: ExchangeId,
        request: CancelOrderRequest,
    ) -> Result<OrderAck> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.trade(exchange)?.cancel_order(request).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run cancel order",
            }),
        }
    }

    pub async fn cancel_protective_order(
        &self,
        exchange: ExchangeId,
        request: CancelOrderRequest,
    ) -> Result<OrderAck> {
        match &self.mode {
            GatewayMode::Live(sdk) => sdk.trade(exchange)?.cancel_protective_order(request).await,
            GatewayMode::DryRun => Err(Error::Unsupported {
                exchange,
                capability: "dry-run protective order cancellation",
            }),
        }
    }
}

fn binance_proxy_url_from_env() -> Option<String> {
    std::env::var("BINANCE_PROXY_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn okx_request_expiration_ms_from_env() -> Option<i64> {
    std::env::var("OKX_REQUEST_EXPIRATION_MS")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }

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
            attached_stop_loss_price: Some("2200.5".to_string()),
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
        assert_eq!(
            mapped.attached_stop_loss_price,
            request.attached_stop_loss_price
        );
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
            attached_stop_loss_price: Some("2200.5".to_string()),
        };

        let ack = gateway.place_order(request).await.unwrap();

        assert_eq!(ack.exchange, ExchangeId::Okx);
        assert_eq!(ack.exchange_symbol, "BTC-USDT-SWAP");
        assert_eq!(ack.client_order_id.as_deref(), Some("rq-dry-run"));
        assert_eq!(ack.status.as_deref(), Some("dry_run"));
        assert_eq!(ack.raw["dry_run"], true);
        assert_eq!(ack.raw["attached_stop_loss_price"], "2200.5");
    }

    #[tokio::test]
    async fn dry_run_place_protective_order_returns_simulated_ack_without_credentials() {
        let gateway = CryptoExcAllGateway::dry_run();
        let instrument = Instrument::perp("eth", "usdt").with_settlement("usdt");
        let request = ProtectiveOrderRequest::stop_market(instrument, OrderSide::Sell, "2200")
            .with_close_position(true)
            .with_client_order_id("rq-sl-42");

        let ack = gateway
            .place_protective_order(ExchangeId::Binance, request)
            .await
            .unwrap();

        assert_eq!(ack.exchange, ExchangeId::Binance);
        assert_eq!(ack.exchange_symbol, "ETHUSDT");
        assert_eq!(ack.client_order_id.as_deref(), Some("rq-sl-42"));
        assert_eq!(ack.status.as_deref(), Some("dry_run"));
        assert_eq!(ack.raw["protective"], true);
        assert_eq!(ack.raw["close_position"], true);
    }

    #[tokio::test]
    async fn dry_run_rejects_live_account_and_cancel_queries() {
        let gateway = CryptoExcAllGateway::dry_run();
        let instrument = Instrument::perp("eth", "usdt").with_settlement("usdt");

        assert!(gateway
            .ticker(ExchangeId::Binance, &instrument)
            .await
            .is_err());
        assert!(gateway
            .orderbook(
                ExchangeId::Binance,
                OrderBookQuery::new(instrument.clone()).with_limit(5),
            )
            .await
            .is_err());
        assert!(gateway.balances(ExchangeId::Binance).await.is_err());
        assert!(gateway
            .positions(ExchangeId::Binance, Some(&instrument))
            .await
            .is_err());
        assert!(gateway
            .cancel_order(
                ExchangeId::Binance,
                CancelOrderRequest::by_client_order_id(instrument, "rq-cancel"),
            )
            .await
            .is_err());
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

    #[test]
    fn single_exchange_okx_runtime_config_does_not_force_request_expiration_window() {
        let _guard = env_lock();
        let previous = std::env::var("OKX_REQUEST_EXPIRATION_MS").ok();

        std::env::remove_var("OKX_REQUEST_EXPIRATION_MS");
        assert_eq!(okx_request_expiration_ms_from_env(), None);

        std::env::set_var("OKX_REQUEST_EXPIRATION_MS", "450000");
        assert_eq!(okx_request_expiration_ms_from_env(), Some(450_000));

        std::env::set_var("OKX_REQUEST_EXPIRATION_MS", "0");
        assert_eq!(okx_request_expiration_ms_from_env(), None);

        match previous {
            Some(value) => std::env::set_var("OKX_REQUEST_EXPIRATION_MS", value),
            None => std::env::remove_var("OKX_REQUEST_EXPIRATION_MS"),
        }
    }

    #[test]
    fn single_exchange_binance_runtime_config_reads_proxy_env() {
        let _guard = env_lock();
        let previous = std::env::var("BINANCE_PROXY_URL").ok();

        std::env::set_var("BINANCE_PROXY_URL", " http://127.0.0.1:7897 ");
        assert_eq!(
            binance_proxy_url_from_env().as_deref(),
            Some("http://127.0.0.1:7897")
        );

        match previous {
            Some(value) => std::env::set_var("BINANCE_PROXY_URL", value),
            None => std::env::remove_var("BINANCE_PROXY_URL"),
        }
    }
}
