use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}

#[test]
fn binance_live_strategy_path_uses_confirmed_kline_and_explicit_stream_routing() {
    let root = repo_root();
    let websocket_path = root
        .join("crates")
        .join("services")
        .join("src")
        .join("market")
        .join("binance_websocket.rs");
    let websocket = fs::read_to_string(&websocket_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", websocket_path.display(), error));

    assert!(websocket.contains("if update.candle_entity.confirm == \"1\" {"));
    assert!(websocket.contains("trigger(\n                            update.inst_id.clone(),"));
    assert!(websocket
        .contains("if let Some(stream) = message.get(\"stream\").and_then(Value::as_str) {"));
    assert!(websocket.contains("if stream_targets.len() == 1 {"));
    assert!(websocket.contains("Err(anyhow!(\"Binance websocket 已关闭\"))"));

    let bootstrap_path = root
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("app")
        .join("bootstrap.rs");
    let bootstrap = fs::read_to_string(&bootstrap_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", bootstrap_path.display(), error));

    assert!(bootstrap.contains("if market_exchange == \"binance\" {"));
    assert!(bootstrap.contains(
        "rust_quant_services::market::binance_websocket::run_binance_websocket_with_strategy_trigger("
    ));
}
