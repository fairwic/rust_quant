use dotenv::dotenv;
use rust_quant::app_config::db::init_db;
use rust_quant::trading::indicator::atr::ATR;
use rust_quant::trading::model::market::candles::CandlesEntity;
use rust_quant::{time_util, trading};
use ta::{Close, High, Low};

// 将 CandlesEntity 转换为 PriceType 类型的价格数据，并返回时间戳
fn to_price(candle: &CandlesEntity) -> (f64, f64, f64, f64, i64) {
    (
        candle.o.parse::<f64>().unwrap_or(0.0),
        candle.h.parse::<f64>().unwrap_or(0.0),
        candle.l.parse::<f64>().unwrap_or(0.0),
        candle.c.parse::<f64>().unwrap_or(0.0),
        candle.ts, // 返回时间戳
    )
}

pub fn calculate_atr(candles: &Vec<CandlesEntity>, period: usize) -> f64 {
    let mut atr = ATR::new(period);

    // 遍历所有K线数据计算ATR
    let last_atr = candles.iter().fold(0.0, |_, candle| {
        let current_atr = atr.next(candle.high(), candle.low(), candle.close());

        println!(
            "time:{:?},current_price{:?},current_atr:{}",
            time_util::mill_time_to_datetime(candle.ts),
            candle.close(),
            current_atr
        );
        return current_atr;
    });

    last_atr
}
#[tokio::test]
async fn test_atr_strategy() {
    // 假设的 CandlesEntity 数据 (OHLC 数据)
    dotenv().ok();
    init_db().await;
    let inst_id = "BTC-USDT-SWAP";
    let time = "4H";

    // 获取 K 线数据
    let mysql_candles: Vec<CandlesEntity> =
        trading::task::get_candle_data(inst_id, time).await.unwrap();

    let res = calculate_atr(&mysql_candles, 10);
    println!("{}", res)

    // rust_quant::trading::strategy::ut_boot_strategy::UtBootStrategy::get_trade_signal(
    //     &mysql_candles,
    //     2.0,
    //     3,
    //     false,
    // );
}
