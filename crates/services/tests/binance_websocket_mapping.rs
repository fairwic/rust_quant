use anyhow::Result;
use rust_quant_domain::Timeframe;
use rust_quant_services::market::binance_websocket::{
    binance_kline_stream_name, parse_binance_kline_message,
};
use serde_json::json;

#[test]
fn maps_combined_binance_kline_to_existing_inst_id_and_timeframe() -> Result<()> {
    let message = json!({
        "stream": "ethusdt@kline_4h",
        "data": {
            "e": "kline",
            "E": 1710000000000_i64,
            "s": "ETHUSDT",
            "k": {
                "t": 1709990400000_i64,
                "T": 1710004799999_i64,
                "s": "ETHUSDT",
                "i": "4h",
                "f": 1_i64,
                "L": 2_i64,
                "o": "3500.10",
                "c": "3510.20",
                "h": "3525.00",
                "l": "3488.00",
                "v": "123.45",
                "n": 100_i64,
                "x": true,
                "q": "433000.55",
                "V": "50.00",
                "Q": "175000.00",
                "B": "0"
            }
        }
    });

    let update = parse_binance_kline_message(&message, "ETH-USDT-SWAP", "4H")?;

    assert_eq!(update.inst_id, "ETH-USDT-SWAP");
    assert_eq!(update.time_interval, "4H");
    assert_eq!(update.candle_entity.ts, 1_709_990_400_000);
    assert_eq!(update.candle_entity.o, "3500.10");
    assert_eq!(update.candle_entity.h, "3525.00");
    assert_eq!(update.candle_entity.l, "3488.00");
    assert_eq!(update.candle_entity.c, "3510.20");
    assert_eq!(update.candle_entity.vol, "123.45");
    assert_eq!(update.candle_entity.vol_ccy, "433000.55");
    assert_eq!(update.candle_entity.confirm, "1");
    assert_eq!(update.domain_candle.symbol, "ETH-USDT-SWAP");
    assert_eq!(update.domain_candle.timeframe, Timeframe::H4);
    assert!(update.domain_candle.confirmed);

    Ok(())
}

#[test]
fn builds_binance_kline_stream_without_changing_internal_symbol() {
    assert_eq!(
        binance_kline_stream_name("ETH-USDT-SWAP", "4H"),
        "ethusdt@kline_4h"
    );
    assert_eq!(
        binance_kline_stream_name("BTCUSDT", "1m"),
        "btcusdt@kline_1m"
    );
}
