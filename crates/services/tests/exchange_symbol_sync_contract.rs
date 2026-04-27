use anyhow::Result;
use async_trait::async_trait;
use rust_quant_domain::entities::{ExchangeSymbol, ExchangeSymbolListingEvent};
use rust_quant_domain::traits::ExchangeSymbolRepository;
use rust_quant_services::market::{
    parse_exchange_symbol_sync_sources, BinanceExchangeInfoProvider, ExchangeSymbolSyncService,
    LiveBinanceExchangeInfoProvider, StaticExchangeInfoProvider,
};
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const EXCHANGE_SYMBOL_LISTING_EVENTS_MIGRATION: &str =
    include_str!("../../../migrations/20260424165000_create_exchange_symbol_listing_events.sql");
const POSTGRES_QUANT_CORE_DDL: &str = include_str!("../../../sql/postgres_quant_core.sql");

#[derive(Default)]
struct InMemoryExchangeSymbolRepository {
    rows: Mutex<Vec<ExchangeSymbol>>,
    listing_events: Mutex<Vec<ExchangeSymbolListingEvent>>,
}

#[async_trait]
impl ExchangeSymbolRepository for InMemoryExchangeSymbolRepository {
    async fn upsert_many(&self, symbols: Vec<ExchangeSymbol>) -> Result<u64> {
        let count = symbols.len() as u64;
        self.rows.lock().unwrap().extend(symbols);
        Ok(count)
    }

    async fn find_by_exchange(
        &self,
        exchange: &str,
        status: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<ExchangeSymbol>> {
        let mut rows: Vec<_> = self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|row| row.exchange == exchange)
            .filter(|row| status.map(|value| row.status == value).unwrap_or(true))
            .cloned()
            .collect();
        if let Some(limit) = limit {
            rows.truncate(limit.max(0) as usize);
        }
        Ok(rows)
    }

    async fn find_by_asset(
        &self,
        base_asset: &str,
        quote_asset: &str,
        market_type: &str,
    ) -> Result<Vec<ExchangeSymbol>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|row| row.base_asset.eq_ignore_ascii_case(base_asset))
            .filter(|row| row.quote_asset.eq_ignore_ascii_case(quote_asset))
            .filter(|row| row.market_type.eq_ignore_ascii_case(market_type))
            .cloned()
            .collect())
    }

    async fn record_first_seen_many(
        &self,
        symbols: &[ExchangeSymbol],
    ) -> Result<Vec<ExchangeSymbolListingEvent>> {
        let mut events = self.listing_events.lock().unwrap();
        let mut inserted = Vec::new();

        for symbol in symbols {
            let exists = events.iter().any(|event| {
                event.exchange == symbol.exchange
                    && event.market_type == symbol.market_type
                    && event.exchange_symbol == symbol.exchange_symbol
            });
            if exists {
                continue;
            }

            let event = ExchangeSymbolListingEvent::from_exchange_symbol(symbol, "test");
            events.push(event.clone());
            inserted.push(event);
        }

        Ok(inserted)
    }

    async fn find_listing_events_by_asset(
        &self,
        base_asset: &str,
        quote_asset: &str,
        market_type: &str,
    ) -> Result<Vec<ExchangeSymbolListingEvent>> {
        Ok(self
            .listing_events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.base_asset.eq_ignore_ascii_case(base_asset))
            .filter(|event| event.quote_asset.eq_ignore_ascii_case(quote_asset))
            .filter(|event| event.market_type.eq_ignore_ascii_case(market_type))
            .cloned()
            .collect())
    }
}

fn sample_binance_exchange_info() -> serde_json::Value {
    json!({
        "timezone": "UTC",
        "symbols": [
            {
                "symbol": "BTCUSDT",
                "status": "TRADING",
                "contractType": "PERPETUAL",
                "baseAsset": "BTC",
                "quoteAsset": "USDT",
                "pricePrecision": 2,
                "quantityPrecision": 3,
                "filters": [
                    {
                        "filterType": "PRICE_FILTER",
                        "tickSize": "0.10"
                    },
                    {
                        "filterType": "LOT_SIZE",
                        "minQty": "0.001",
                        "maxQty": "1000",
                        "stepSize": "0.001"
                    },
                    {
                        "filterType": "MIN_NOTIONAL",
                        "notional": "100"
                    }
                ]
            },
            {
                "symbol": "ETHUSDT_240628",
                "status": "TRADING",
                "contractType": "CURRENT_QUARTER",
                "baseAsset": "ETH",
                "quoteAsset": "USDT",
                "pricePrecision": 2,
                "quantityPrecision": 3,
                "filters": []
            }
        ]
    })
}

fn sample_okx_swap_instruments() -> serde_json::Value {
    json!({
        "code": "0",
        "data": [
            {
                "instType": "SWAP",
                "instId": "TEST-USDT-SWAP",
                "state": "live",
                "baseCcy": "TEST",
                "quoteCcy": "USDT",
                "ctType": "linear",
                "tickSz": "0.0001",
                "lotSz": "1",
                "minSz": "1"
            },
            {
                "instType": "SPOT",
                "instId": "TEST-USDT",
                "state": "live",
                "baseCcy": "TEST",
                "quoteCcy": "USDT"
            }
        ]
    })
}

fn sample_bitget_usdt_futures_contracts() -> serde_json::Value {
    json!({
        "code": "00000",
        "data": [
            {
                "symbol": "TESTUSDT",
                "baseCoin": "TEST",
                "quoteCoin": "USDT",
                "symbolStatus": "normal",
                "pricePlace": "4",
                "volumePlace": "2",
                "minTradeNum": "1",
                "sizeMultiplier": "0.01",
                "priceEndStep": "1"
            }
        ]
    })
}

fn sample_gate_usdt_futures_contracts() -> serde_json::Value {
    json!([
        {
            "name": "TEST_USDT",
            "status": "trading",
            "type": "direct",
            "contract_type": "",
            "quanto_multiplier": "0.01",
            "order_size_min": 1,
            "order_size_max": 900000,
            "order_price_round": "0.0001",
            "mark_price_round": "0.0001",
            "in_delisting": false
        },
        {
            "name": "BABA_USDT",
            "status": "trading",
            "type": "direct",
            "contract_type": "stocks",
            "quanto_multiplier": "0.01",
            "order_size_min": 1,
            "order_size_max": 1000,
            "order_price_round": "0.01",
            "in_delisting": false
        }
    ])
}

fn sample_kucoin_futures_contracts() -> serde_json::Value {
    json!({
        "code": "200000",
        "data": [
            {
                "symbol": "TESTUSDTM",
                "rootSymbol": "USDT",
                "type": "FFWCSX",
                "baseCurrency": "TEST",
                "quoteCurrency": "USDT",
                "status": "Open",
                "lotSize": 1,
                "tickSize": 0.0001,
                "multiplier": 0.01
            }
        ]
    })
}

fn listing_event(exchange: &str, base_asset: &str) -> ExchangeSymbolListingEvent {
    ExchangeSymbolListingEvent {
        id: None,
        exchange: exchange.to_string(),
        market_type: "perpetual".to_string(),
        exchange_symbol: format!("{base_asset}USDT"),
        normalized_symbol: format!("{base_asset}-USDT-SWAP"),
        base_asset: base_asset.to_string(),
        quote_asset: "USDT".to_string(),
        status: "TRADING".to_string(),
        first_seen_at: Some(
            chrono::DateTime::parse_from_rfc3339("2026-04-24T08:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        ),
        source: "test".to_string(),
        raw_payload: Some(json!({"source": "test"})),
        created_at: None,
        updated_at: None,
    }
}

fn exchange_symbol(exchange: &str, base_asset: &str) -> ExchangeSymbol {
    ExchangeSymbol::new(
        exchange.to_string(),
        "perpetual".to_string(),
        format!("{base_asset}USDT"),
        format!("{base_asset}-USDT-SWAP"),
        base_asset.to_string(),
        "USDT".to_string(),
        "TRADING".to_string(),
    )
}

#[test]
fn detects_supported_mainstream_listing_after_non_mainstream_history() {
    let new_listing = listing_event("binance", "TEST");
    let history = vec![listing_event("bitget", "TEST"), new_listing.clone()];

    let signal = ExchangeSymbolSyncService::detect_major_exchange_listing(&new_listing, &history)
        .expect("binance after bitget should be a major listing signal");

    assert_eq!(signal.exchange, "binance");
    assert_eq!(signal.base_asset, "TEST");
    assert_eq!(signal.normalized_symbol, "TEST-USDT-SWAP");
    assert_eq!(signal.prior_non_mainstream_exchanges, vec!["bitget"]);
}

#[test]
fn detects_okx_listing_after_non_mainstream_history() {
    let new_listing = listing_event("okx", "TEST");
    let history = vec![listing_event("gate", "TEST"), new_listing.clone()];

    let signal = ExchangeSymbolSyncService::detect_major_exchange_listing(&new_listing, &history)
        .expect("okx after gate should be a major listing signal");

    assert_eq!(signal.exchange, "okx");
    assert_eq!(signal.prior_non_mainstream_exchanges, vec!["gate"]);
}

#[test]
fn ignores_bitget_listing_even_when_asset_was_already_on_other_exchange() {
    let new_listing = listing_event("bitget", "TEST");
    let history = vec![listing_event("gate", "TEST"), new_listing.clone()];

    let signal = ExchangeSymbolSyncService::detect_major_exchange_listing(&new_listing, &history);

    assert!(signal.is_none());
}

#[test]
fn treats_first_direct_major_exchange_listing_as_neutral() {
    let new_listing = listing_event("binance", "TEST");
    let history = vec![new_listing.clone()];

    let signal = ExchangeSymbolSyncService::detect_major_exchange_listing(&new_listing, &history);

    assert!(signal.is_none());
}

#[test]
fn ignores_major_listing_when_asset_was_already_on_major_exchange() {
    let new_listing = listing_event("binance", "TEST");
    let history = vec![
        listing_event("okx", "TEST"),
        listing_event("gate", "TEST"),
        new_listing.clone(),
    ];

    let signal = ExchangeSymbolSyncService::detect_major_exchange_listing(&new_listing, &history);

    assert!(signal.is_none());
}

#[test]
fn detects_major_listing_from_current_non_mainstream_symbol_facts() {
    let new_listing = listing_event("binance", "TEST");
    let history = vec![new_listing.clone()];
    let current_symbols = vec![
        exchange_symbol("bitget", "TEST"),
        exchange_symbol("binance", "TEST"),
    ];

    let signal = ExchangeSymbolSyncService::detect_major_exchange_listing_with_current_symbols(
        &new_listing,
        &history,
        &current_symbols,
    )
    .expect("binance after existing bitget symbol fact should be a major listing signal");

    assert_eq!(signal.exchange, "binance");
    assert_eq!(signal.prior_non_mainstream_exchanges, vec!["bitget"]);
}

#[test]
fn current_major_symbol_fact_blocks_duplicate_major_listing_signal() {
    let new_listing = listing_event("binance", "TEST");
    let history = vec![new_listing.clone()];
    let current_symbols = vec![
        exchange_symbol("okx", "TEST"),
        exchange_symbol("bitget", "TEST"),
    ];

    let signal = ExchangeSymbolSyncService::detect_major_exchange_listing_with_current_symbols(
        &new_listing,
        &history,
        &current_symbols,
    );

    assert!(signal.is_none());
}

#[test]
fn exchange_symbol_listing_events_ddl_has_table_and_column_comments() {
    for ddl in [
        EXCHANGE_SYMBOL_LISTING_EVENTS_MIGRATION,
        POSTGRES_QUANT_CORE_DDL,
    ] {
        assert!(
            ddl.contains("COMMENT ON TABLE exchange_symbol_listing_events"),
            "exchange_symbol_listing_events table comment is required"
        );
        for column in [
            "id",
            "exchange",
            "market_type",
            "exchange_symbol",
            "normalized_symbol",
            "base_asset",
            "quote_asset",
            "status",
            "first_seen_at",
            "source",
            "raw_payload",
            "created_at",
            "updated_at",
        ] {
            assert!(
                ddl.contains(&format!(
                    "COMMENT ON COLUMN exchange_symbol_listing_events.{column}"
                )),
                "exchange_symbol_listing_events.{column} column comment is required"
            );
        }
    }
}

#[test]
fn exchange_symbol_sync_runs_ddl_has_table_and_column_comments() {
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("CREATE TABLE IF NOT EXISTS exchange_symbol_sync_runs"),
        "exchange_symbol_sync_runs table DDL is required"
    );
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("COMMENT ON TABLE exchange_symbol_sync_runs"),
        "exchange_symbol_sync_runs table comment is required"
    );
    for column in [
        "id",
        "run_id",
        "trigger_source",
        "requested_sources",
        "run_status",
        "started_at",
        "finished_at",
        "duration_ms",
        "persisted_rows",
        "first_seen_rows",
        "major_listing_signals",
        "error_message",
        "report_json",
        "created_at",
        "updated_at",
    ] {
        assert!(
            POSTGRES_QUANT_CORE_DDL.contains(&format!(
                "COMMENT ON COLUMN exchange_symbol_sync_runs.{column}"
            )),
            "exchange_symbol_sync_runs.{column} column comment is required"
        );
    }
}

#[test]
fn exchange_symbol_sync_sources_accept_default_csv_and_space_separated_values() {
    assert_eq!(
        parse_exchange_symbol_sync_sources(None).expect("default sources"),
        vec!["binance", "okx", "bitget", "gate", "kucoin"]
    );
    assert_eq!(
        parse_exchange_symbol_sync_sources(Some(" okx, gate kucoin ")).expect("mixed separators"),
        vec!["okx", "gate", "kucoin"]
    );
}

#[test]
fn parse_binance_exchange_info_only_keeps_perpetual_contracts() {
    let rows = ExchangeSymbolSyncService::parse_binance_usdm_exchange_info(
        &sample_binance_exchange_info(),
    )
    .expect("binance exchange info should parse");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].exchange, "binance");
    assert_eq!(rows[0].market_type, "perpetual");
    assert_eq!(rows[0].exchange_symbol, "BTCUSDT");
    assert_eq!(rows[0].normalized_symbol, "BTC-USDT-SWAP");
    assert_eq!(rows[0].base_asset, "BTC");
    assert_eq!(rows[0].quote_asset, "USDT");
    assert_eq!(rows[0].status, "TRADING");
    assert_eq!(rows[0].contract_type.as_deref(), Some("PERPETUAL"));
    assert_eq!(rows[0].tick_size.as_deref(), Some("0.10"));
    assert_eq!(rows[0].step_size.as_deref(), Some("0.001"));
    assert_eq!(rows[0].min_qty.as_deref(), Some("0.001"));
    assert_eq!(rows[0].max_qty.as_deref(), Some("1000"));
    assert_eq!(rows[0].min_notional.as_deref(), Some("100"));
}

#[test]
fn parse_okx_swap_instruments_only_keeps_swaps() {
    let rows =
        ExchangeSymbolSyncService::parse_okx_swap_instruments(&sample_okx_swap_instruments())
            .expect("okx instruments should parse");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].exchange, "okx");
    assert_eq!(rows[0].market_type, "perpetual");
    assert_eq!(rows[0].exchange_symbol, "TEST-USDT-SWAP");
    assert_eq!(rows[0].normalized_symbol, "TEST-USDT-SWAP");
    assert_eq!(rows[0].base_asset, "TEST");
    assert_eq!(rows[0].quote_asset, "USDT");
    assert_eq!(rows[0].status, "live");
    assert_eq!(rows[0].contract_type.as_deref(), Some("linear"));
    assert_eq!(rows[0].tick_size.as_deref(), Some("0.0001"));
    assert_eq!(rows[0].step_size.as_deref(), Some("1"));
    assert_eq!(rows[0].min_qty.as_deref(), Some("1"));
}

#[test]
fn parse_bitget_usdt_futures_contracts_normalizes_to_swap_symbols() {
    let rows = ExchangeSymbolSyncService::parse_bitget_usdt_futures_contracts(
        &sample_bitget_usdt_futures_contracts(),
    )
    .expect("bitget contracts should parse");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].exchange, "bitget");
    assert_eq!(rows[0].market_type, "perpetual");
    assert_eq!(rows[0].exchange_symbol, "TESTUSDT");
    assert_eq!(rows[0].normalized_symbol, "TEST-USDT-SWAP");
    assert_eq!(rows[0].base_asset, "TEST");
    assert_eq!(rows[0].quote_asset, "USDT");
    assert_eq!(rows[0].status, "normal");
    assert_eq!(rows[0].price_precision, Some(4));
    assert_eq!(rows[0].quantity_precision, Some(2));
    assert_eq!(rows[0].min_qty.as_deref(), Some("1"));
    assert_eq!(rows[0].step_size.as_deref(), Some("0.01"));
    assert_eq!(rows[0].tick_size.as_deref(), Some("1"));
}

#[test]
fn parse_gate_usdt_futures_contracts_skips_stock_contracts() {
    let rows = ExchangeSymbolSyncService::parse_gate_usdt_futures_contracts(
        &sample_gate_usdt_futures_contracts(),
    )
    .expect("gate contracts should parse");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].exchange, "gate");
    assert_eq!(rows[0].market_type, "perpetual");
    assert_eq!(rows[0].exchange_symbol, "TEST_USDT");
    assert_eq!(rows[0].normalized_symbol, "TEST-USDT-SWAP");
    assert_eq!(rows[0].base_asset, "TEST");
    assert_eq!(rows[0].quote_asset, "USDT");
    assert_eq!(rows[0].status, "trading");
    assert_eq!(rows[0].contract_type.as_deref(), Some("direct"));
    assert_eq!(rows[0].min_qty.as_deref(), Some("1"));
    assert_eq!(rows[0].max_qty.as_deref(), Some("900000"));
    assert_eq!(rows[0].step_size.as_deref(), Some("0.01"));
    assert_eq!(rows[0].tick_size.as_deref(), Some("0.0001"));
}

#[test]
fn parse_kucoin_futures_contracts_normalizes_usdt_m_symbols() {
    let rows = ExchangeSymbolSyncService::parse_kucoin_futures_contracts(
        &sample_kucoin_futures_contracts(),
    )
    .expect("kucoin contracts should parse");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].exchange, "kucoin");
    assert_eq!(rows[0].market_type, "perpetual");
    assert_eq!(rows[0].exchange_symbol, "TESTUSDTM");
    assert_eq!(rows[0].normalized_symbol, "TEST-USDT-SWAP");
    assert_eq!(rows[0].base_asset, "TEST");
    assert_eq!(rows[0].quote_asset, "USDT");
    assert_eq!(rows[0].status, "Open");
    assert_eq!(rows[0].contract_type.as_deref(), Some("FFWCSX"));
    assert_eq!(rows[0].min_qty.as_deref(), Some("1"));
    assert_eq!(rows[0].step_size.as_deref(), Some("1"));
    assert_eq!(rows[0].tick_size.as_deref(), Some("0.0001"));
}

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn live_kucoin_provider_uses_base_url_override() {
    let _guard = ENV_LOCK.lock().unwrap();
    let (base_url, received) = start_json_server(sample_kucoin_futures_contracts().to_string());
    std::env::set_var("KUCOIN_FUTURES_BASE_URL", &base_url);

    let payload = LiveBinanceExchangeInfoProvider
        .fetch_kucoin_futures_contracts()
        .await
        .expect("provider should fetch from overridden base URL");

    std::env::remove_var("KUCOIN_FUTURES_BASE_URL");
    assert_eq!(payload["code"], "200000");
    assert!(
        received
            .lock()
            .unwrap()
            .as_deref()
            .unwrap_or_default()
            .starts_with("GET /api/v1/contracts/active "),
        "provider should request KuCoin contracts path from overridden base URL"
    );
}

fn start_json_server(body: String) -> (String, Arc<Mutex<Option<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("test server addr");
    listener
        .set_nonblocking(true)
        .expect("set test server nonblocking");
    let received = Arc::new(Mutex::new(None));
    let received_for_thread = received.clone();

    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = [0_u8; 4096];
                    let size = stream.read(&mut buffer).expect("read request");
                    let request = String::from_utf8_lossy(&buffer[..size]).to_string();
                    *received_for_thread.lock().unwrap() =
                        request.lines().next().map(ToString::to_string);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream
                        .write_all(response.as_bytes())
                        .expect("write response");
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept request: {error}"),
            }
        }
    });

    (format!("http://{addr}"), received)
}

#[tokio::test]
async fn sync_binance_symbols_fetches_and_persists_rows() {
    let repo = Arc::new(InMemoryExchangeSymbolRepository::default());
    let provider: Arc<dyn BinanceExchangeInfoProvider> = Arc::new(StaticExchangeInfoProvider::new(
        sample_binance_exchange_info(),
    ));
    let service = ExchangeSymbolSyncService::with_repo_and_provider(repo.clone(), provider);

    let count = service
        .sync_binance_usdm_perpetual_symbols()
        .await
        .expect("sync should succeed");

    assert_eq!(count, 1);

    let saved = repo
        .find_by_exchange("binance", Some("TRADING"), Some(10))
        .await
        .expect("saved rows should be queryable");
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].normalized_symbol, "BTC-USDT-SWAP");
}

#[tokio::test]
async fn sync_binance_symbols_records_first_seen_history() {
    let repo = Arc::new(InMemoryExchangeSymbolRepository::default());
    let provider: Arc<dyn BinanceExchangeInfoProvider> = Arc::new(StaticExchangeInfoProvider::new(
        sample_binance_exchange_info(),
    ));
    let service = ExchangeSymbolSyncService::with_repo_and_provider(repo.clone(), provider);

    let report = service
        .sync_binance_usdm_perpetual_symbols_with_report()
        .await
        .expect("sync report should succeed");

    assert_eq!(report.persisted_count, 1);
    assert_eq!(report.first_seen_count, 1);
    assert!(report.major_listing_signals.is_empty());

    let events = repo
        .find_listing_events_by_asset("BTC", "USDT", "perpetual")
        .await
        .expect("listing events should be queryable");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].exchange, "binance");
}
