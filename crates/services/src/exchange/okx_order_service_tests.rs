use super::*;
use std::sync::{Mutex, OnceLock};
const TEST_DIRECT_CONFIRM_ENV: &str = "LEGACY_DIRECT_LIVE_ORDER_CONFIRM";
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
struct EnvSnapshot {
    /// direct确认标记；为空时表示该条件不启用。
    direct_confirm: Option<String>,
}
impl EnvSnapshot {
    fn capture() -> Self {
        Self {
            direct_confirm: std::env::var(TEST_DIRECT_CONFIRM_ENV).ok(),
        }
    }
}
impl Drop for EnvSnapshot {
    fn drop(&mut self) {
        match &self.direct_confirm {
            Some(value) => std::env::set_var(TEST_DIRECT_CONFIRM_ENV, value),
            None => std::env::remove_var(TEST_DIRECT_CONFIRM_ENV),
        }
    }
}
fn dummy_okx_api_config_without_passphrase() -> ExchangeApiConfig {
    ExchangeApiConfig::new(
        1,
        "okx".to_string(),
        "dummy_api_key".to_string(),
        "dummy_api_secret".to_string(),
        None,
        true,
        true,
        Some("unit-test".to_string()),
    )
}
fn allow_legacy_direct_live_order_for_test() -> EnvSnapshot {
    let snapshot = EnvSnapshot::capture();
    std::env::set_var(
        TEST_DIRECT_CONFIRM_ENV,
        "I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS",
    );
    snapshot
}
#[test]
fn legacy_direct_live_exchange_order_requires_explicit_confirmation() {
    let err = OkxOrderService::ensure_legacy_direct_live_exchange_order_allowed_from_env(None)
        .expect_err("legacy direct OKX mutation should be blocked by default");
    let message = err.to_string();
    assert!(message.contains("LEGACY_DIRECT_LIVE_ORDER_CONFIRM"));
    assert!(message.contains("I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS"));
}
#[test]
fn legacy_direct_live_exchange_order_accepts_exact_confirmation() {
    OkxOrderService::ensure_legacy_direct_live_exchange_order_allowed_from_env(Some(
        "I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS",
    ))
    .expect("exact legacy confirmation token should allow direct OKX mutation");
}
#[test]
fn legacy_signed_read_only_requires_explicit_confirmation() {
    let err = OkxOrderService::ensure_legacy_signed_read_only_allowed_from_env(None, None)
        .expect_err("legacy direct OKX account reads should be blocked by default");
    let message = err.to_string();
    assert!(message.contains("LEGACY_SIGNED_READ_ONLY_CONFIRM"));
    assert!(message.contains("I_UNDERSTAND_LEGACY_SIGNED_READ_ONLY_ACCOUNT_READS"));
}
#[test]
fn legacy_signed_read_only_accepts_exact_confirmation() {
    OkxOrderService::ensure_legacy_signed_read_only_allowed_from_env(
        Some("I_UNDERSTAND_LEGACY_SIGNED_READ_ONLY_ACCOUNT_READS"),
        None,
    )
    .expect("exact legacy signed-read confirmation token should allow OKX account reads");
}
#[test]
fn legacy_signed_read_only_accepts_direct_mutation_confirmation() {
    OkxOrderService::ensure_legacy_signed_read_only_allowed_from_env(
        None,
        Some("I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS"),
    )
    .expect("direct mutation confirmation should also allow prerequisite OKX account reads");
}
#[tokio::test]
async fn legacy_place_order_rejects_unprotected_entry_before_okx_client() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _snapshot = allow_legacy_direct_live_order_for_test();
    let api_config = dummy_okx_api_config_without_passphrase();
    let error = OkxOrderService
        .place_order(
            &api_config,
            "ETH-USDT-SWAP",
            Side::Buy,
            PositionSide::Long,
            "1".to_string(),
            Some("unit-test".to_string()),
        )
        .await
        .expect_err("legacy direct entry order without stop-loss must be blocked");
    let message = error.to_string();
    assert!(
        message.contains("stop-loss") || message.contains("止损"),
        "unexpected error: {message}"
    );
    assert!(
        !message.contains("Passphrase"),
        "stop-loss guard must run before OKX client creation: {message}"
    );
}
#[tokio::test]
async fn legacy_algo_order_rejects_take_profit_without_stop_loss_before_okx_client() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _snapshot = allow_legacy_direct_live_order_for_test();
    let api_config = dummy_okx_api_config_without_passphrase();
    let error = OkxOrderService
        .place_order_with_algo_orders(
            &api_config,
            "ETH-USDT-SWAP",
            Side::Buy,
            PositionSide::Long,
            "1".to_string(),
            Some(3600.0),
            None,
            Some("unit-test".to_string()),
        )
        .await
        .expect_err("take-profit-only entry order must be blocked");
    let message = error.to_string();
    assert!(
        message.contains("stop-loss") || message.contains("止损"),
        "unexpected error: {message}"
    );
    assert!(
        !message.contains("Passphrase"),
        "stop-loss guard must run before OKX client creation: {message}"
    );
}
#[tokio::test]
async fn legacy_execute_order_from_signal_rejects_missing_stop_loss_before_okx_client() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _snapshot = allow_legacy_direct_live_order_for_test();
    let api_config = dummy_okx_api_config_without_passphrase();
    let signal = SignalResult {
        should_buy: true,
        open_price: 3000.0,
        ..Default::default()
    };
    let error = OkxOrderService
        .execute_order_from_signal(
            &api_config,
            "ETH-USDT-SWAP",
            &signal,
            "1".to_string(),
            None,
            None,
            Some("unit-test".to_string()),
        )
        .await
        .expect_err("signal execution without stop-loss must be blocked");
    let message = error.to_string();
    assert!(
        message.contains("stop-loss") || message.contains("止损"),
        "unexpected error: {message}"
    );
    assert!(
        !message.contains("Passphrase"),
        "stop-loss guard must run before OKX client creation: {message}"
    );
}
