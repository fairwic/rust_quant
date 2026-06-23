#[test]
fn maps_task_payload_to_order_request() {
    let task = task(json!({
        "exchange": "okx",
        "symbol": "BTC-USDT-SWAP",
        "side": "buy",
        "order_type": "market",
        "size": "0.01",
        "margin_mode": "cross",
        "position_side": "long",
        "trade_side": "open"
    }));
    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let order = request.to_order_request().unwrap();
    assert_eq!(order.exchange.as_str(), "okx");
    assert_eq!(order.instrument.symbol_for(order.exchange), "BTC-USDT-SWAP");
    assert_eq!(order.size, "0.01");
    assert_eq!(order.client_order_id.as_deref(), Some("rqtask42"));
}
#[test]
fn maps_nested_news_signal_payload_to_order_request() {
    let task = task(json!({
        "symbol": "BTC-USDT-SWAP",
        "signal_type": "buy",
        "payload_json": "{\"exchange\":\"okx\",\"side\":\"buy\",\"size\":\"0.001\",\"order_type\":\"market\",\"client_order_id\":\"smoke-dry-run-42\"}"
    }));
    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let order = request.to_order_request().unwrap();
    assert_eq!(order.exchange.as_str(), "okx");
    assert_eq!(order.size, "0.001");
    assert_eq!(order.client_order_id.as_deref(), Some("smoke-dry-run-42"));
}
#[test]
fn maps_web_execution_payload_to_order_request() {
    let task = task(json!({
        "source": "rust_quant",
        "symbol": "ETH-USDT-SWAP",
        "signal_type": "entry",
        "direction": "long",
        "payload_json": "{\"signal\":{\"open_price\":3500.0},\"client_order_id\":\"rq421704067200000\"}",
        "execution": {
            "exchange": "binance",
            "symbol": "ETH-USDT-SWAP",
            "side": "buy",
            "order_type": "market",
            "size_usdt": 35.0
        },
        "risk_settings": {
            "max_position_usdt": 35.0,
            "risk_acknowledged": true,
            "status": "active"
        }
    }));
    let request = ExecutionOrderTask::from_task(&task).unwrap();
    let order = request.to_order_request().unwrap();
    assert_eq!(order.exchange.as_str(), "binance");
    assert_eq!(request.symbol, "ETH-USDT-SWAP");
    assert_eq!(order.instrument.symbol_for(order.exchange), "ETHUSDT");
    assert_eq!(order.size, "0.01");
    assert_eq!(order.client_order_id.as_deref(), Some("rq421704067200000"));
}
