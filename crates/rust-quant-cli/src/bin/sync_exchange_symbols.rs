use anyhow::{Context, Result};
use chrono::Utc;
use dotenv::dotenv;
use rust_quant_services::market::{ExchangeSymbolSyncService, MajorExchangeListingSignal};
use rust_quant_services::rust_quan_web::{
    ExecutionTaskClient, ExecutionTaskConfig, StrategySignalSubmitRequest,
};
use serde_json::json;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    rust_quant_core::logger::setup_logging().await?;

    let source = std::env::var("EXCHANGE_SYMBOL_SOURCE").unwrap_or_else(|_| "binance".to_string());
    let service = ExchangeSymbolSyncService::from_env().await?;

    let report = match source.to_ascii_lowercase().as_str() {
        "binance" | "binance_usdm" | "binance_perpetual" => {
            service
                .sync_binance_usdm_perpetual_symbols_with_report()
                .await?
        }
        "okx" | "okx_swap" | "okx_perpetual" => service.sync_okx_swap_symbols_with_report().await?,
        "bitget" | "bitget_usdt_futures" | "bitget_perpetual" => {
            service.sync_bitget_usdt_futures_symbols_with_report().await?
        }
        "gate" | "gate_usdt_futures" | "gate_perpetual" => {
            service.sync_gate_usdt_futures_symbols_with_report().await?
        }
        "kucoin" | "kucoin_futures" | "kucoin_perpetual" => {
            service.sync_kucoin_futures_symbols_with_report().await?
        }
        other => {
            return Err(anyhow::anyhow!(
                "unsupported EXCHANGE_SYMBOL_SOURCE={}, expected binance/binance_usdm/binance_perpetual/okx/okx_swap/bitget/bitget_usdt_futures/gate/gate_usdt_futures/kucoin/kucoin_futures",
                other
            ))
        }
    };

    info!(
        "exchange symbol sync completed: source={}, persisted_rows={}, first_seen_rows={}, major_listing_signals={}",
        source,
        report.persisted_count,
        report.first_seen_count,
        report.major_listing_signals.len()
    );
    println!(
        "exchange symbol sync completed: source={}, persisted_rows={}, first_seen_rows={}, major_listing_signals={}",
        source,
        report.persisted_count,
        report.first_seen_count,
        report.major_listing_signals.len()
    );
    for signal in &report.major_listing_signals {
        println!(
            "major_listing_signal: exchange={}, symbol={}, prior_non_mainstream_exchanges={}",
            signal.exchange,
            signal.normalized_symbol,
            signal.prior_non_mainstream_exchanges.join(",")
        );
    }
    if env_is_true("EXCHANGE_LISTING_SIGNAL_SUBMIT") {
        submit_major_listing_signals(&report.major_listing_signals).await?;
    }
    Ok(())
}

async fn submit_major_listing_signals(signals: &[MajorExchangeListingSignal]) -> Result<()> {
    if signals.is_empty() {
        return Ok(());
    }

    let client = ExecutionTaskClient::new(quant_web_execution_task_config_from_env()?)?;
    for signal in signals {
        let request = build_major_listing_strategy_signal_request(signal);
        let response = client
            .submit_strategy_signal(request)
            .await
            .with_context(|| {
                format!(
                    "submit major exchange listing signal failed: {} {}",
                    signal.exchange, signal.normalized_symbol
                )
            })?;
        info!(
            "submitted major exchange listing signal: exchange={}, symbol={}, generated_tasks={}",
            signal.exchange,
            signal.normalized_symbol,
            response.generated_tasks.len()
        );
        println!(
            "submitted major_listing_signal: exchange={}, symbol={}, generated_tasks={}",
            signal.exchange,
            signal.normalized_symbol,
            response.generated_tasks.len()
        );
    }

    Ok(())
}

fn build_major_listing_strategy_signal_request(
    signal: &MajorExchangeListingSignal,
) -> StrategySignalSubmitRequest {
    let generated_at = Utc::now().to_rfc3339();
    StrategySignalSubmitRequest {
        source: "rust_quant".to_string(),
        external_id: format!(
            "exchange-listing:{}:{}",
            signal.exchange, signal.normalized_symbol
        ),
        strategy_slug: "event_exchange_listing".to_string(),
        strategy_key: "exchange_listing".to_string(),
        symbol: signal.normalized_symbol.clone(),
        signal_type: "buy".to_string(),
        direction: "long".to_string(),
        title: format!(
            "{} listed on {} after prior non-mainstream listings",
            signal.base_asset,
            signal.exchange.to_ascii_uppercase()
        ),
        summary: Some(format!(
            "{} was already listed on {}; {} listing is treated as a major bullish catalyst.",
            signal.base_asset,
            signal.prior_non_mainstream_exchanges.join(", "),
            signal.exchange.to_ascii_uppercase()
        )),
        confidence: Some(0.9),
        payload_json: json!({
            "event_class": "exchange_listing",
            "signal_source": "exchange_symbol_sync",
            "exchange": &signal.exchange,
            "preferred_exchanges": [&signal.exchange],
            "execution_symbol": &signal.normalized_symbol,
            "side": "buy",
            "order_type": "market",
            "auto_execution_allowed": true,
            "listing_catalyst": {
                "classification": "major_exchange_listing_with_prior_non_mainstream_history",
                "source_exchange": &signal.exchange,
                "execution_exchange": &signal.exchange,
                "listed_on_major_execution_exchange": true,
                "auto_execution_allowed": true
            },
            "prior_non_mainstream_exchanges": &signal.prior_non_mainstream_exchanges,
            "base_asset": &signal.base_asset,
            "quote_asset": &signal.quote_asset,
            "market_type": &signal.market_type,
            "exchange_symbol": &signal.exchange_symbol
        })
        .to_string(),
        generated_at: Some(generated_at),
    }
}

fn quant_web_execution_task_config_from_env() -> Result<ExecutionTaskConfig> {
    let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
        .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
        .context(
            "EXCHANGE_LISTING_SIGNAL_SUBMIT=1 requires RUST_QUAN_WEB_BASE_URL/QUANT_WEB_BASE_URL",
        )?;
    let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .unwrap_or_else(|_| {
            warn!("RUST_QUAN_WEB_INTERNAL_SECRET/EXECUTION_EVENT_SECRET is not set");
            String::new()
        });
    Ok(ExecutionTaskConfig {
        base_url,
        internal_secret,
    })
}

fn env_is_true(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn major_listing_strategy_signal_request_targets_event_strategy() {
        let signal = MajorExchangeListingSignal {
            exchange: "binance".to_string(),
            market_type: "perpetual".to_string(),
            exchange_symbol: "TESTUSDT".to_string(),
            normalized_symbol: "TEST-USDT-SWAP".to_string(),
            base_asset: "TEST".to_string(),
            quote_asset: "USDT".to_string(),
            prior_non_mainstream_exchanges: vec!["bitget".to_string()],
        };

        let request = build_major_listing_strategy_signal_request(&signal);
        let payload: serde_json::Value =
            serde_json::from_str(&request.payload_json).expect("payload json");

        assert_eq!(request.strategy_slug, "event_exchange_listing");
        assert_eq!(request.strategy_key, "exchange_listing");
        assert_eq!(request.symbol, "TEST-USDT-SWAP");
        assert_eq!(request.signal_type, "buy");
        assert_eq!(request.direction, "long");
        assert_eq!(payload["exchange"], "binance");
        assert_eq!(payload["execution_symbol"], "TEST-USDT-SWAP");
        assert_eq!(payload["prior_non_mainstream_exchanges"], json!(["bitget"]));
        assert_eq!(
            payload["listing_catalyst"]["classification"],
            "major_exchange_listing_with_prior_non_mainstream_history"
        );
    }
}
