use dotenv::dotenv;
use ndarray::{Array1, s};
use ta::{Close, Next};
use ta::indicators::TrueRange;
use ta::indicators::AverageTrueRange;
use rust_quant::app_config::db::init_db;
use rust_quant::time_util;
use rust_quant::trading;
use rust_quant::trading::model::market::candles::CandlesEntity;

// 定义一个通用的类型别名，可以在项目中切换浮动点类型
// 可以在此处替换为 `f32` 或 `f64` 来决定浮点精度

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

#[tokio::test]
async fn test_atr_calculation() {
    // 假设的 CandlesEntity 数据 (OHLC 数据)
    dotenv().ok();
    init_db().await;
    let inst_id = "BTC-USDT-SWAP";
    let time = "4H";

    // 获取 K 线数据
    let mysql_candles = trading::task::get_candle_data(inst_id, time).await.unwrap();

    // 转换 CandlesEntity 为 PriceType 型价格数据，并保留时间戳
    let prices: Vec<(f64, f64, f64, f64, i64)> =
        mysql_candles.iter().map(|c| to_price(c)).collect();
    let close_prices: Vec<f64> = prices.iter().map(|(_, _, _, c, _)| *c).collect();
    let high_prices: Vec<f64> = prices.iter().map(|(_, h, _, _, _)| *h).collect();
    let low_prices: Vec<f64> = prices.iter().map(|(_, _, l, _, _)| *l).collect();
    let timestamps: Vec<i64> = prices.iter().map(|(_, _, _, _, ts)| *ts).collect(); // 获取时间戳

    // 使用 ndarray 存储数据
    let close = Array1::from(close_prices);
    let high = Array1::from(high_prices);
    let low = Array1::from(low_prices);

    // 计算 ATR (Average True Range)
    let atr_period = 14; // ATR 计算周期
    let atr_indicator = AverageTrueRange::new(atr_period);

    let mut atr_values = Vec::new();
    for i in 0..close.len() {
        let tr = TrueRange::new(high[i], low[i], if i == 0 { 0.0 } else { close[i-1] });
        let atr = atr_indicator.next(&tr);
        atr_values.push(atr);
    }

    // 输出每个时间点的 ATR 值和时间戳
    for (i, &atr_value) in atr_values.iter().enumerate() {
        // 将时间戳转换为可读的日期时间格式
        let time_str = time_util::mill_time_to_datetime_shanghai(timestamps[i]).unwrap();

        println!(
            "Time: {}, ATR: {:.2}, High: {:.2}, Low: {:.2}, Close: {:.2}",
            time_str, atr_value, high[i], low[i], close[i]
        );
    }
}
