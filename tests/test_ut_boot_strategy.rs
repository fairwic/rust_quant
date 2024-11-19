use anyhow::Result;
use dotenv::dotenv;
use rust_quant::{
    app_config::db::init_db,
    time_util,
    trading,
    trading::model::market::candles::CandlesEntity,
    trading::indicator::atr::ATR,
};

#[tokio::test]
async fn test_atr_calculation() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    init_db().await;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "4H";
    let period = 10;

    // 获取K线数据
    let mysql_candles: Vec<CandlesEntity> = trading::task::get_candle_data(inst_id, time).await?;

    // 确保有数据
    if mysql_candles.is_empty() {
        println!("警告: 未获取到K线数据");
        return Ok(());
    }

    let mut atr = ATR::new(period);

    // 打印表头
    println!("\n{} {}K线 ATR({})计算结果:", inst_id, time, period);
    println!("{:<25} {:<12} {:<12} {:<12} {:<12}",
             "时间", "最高价", "最低价", "收盘价", "ATR");
    println!("{}", "=".repeat(75));

    // 计算并显示结果
    for candle in mysql_candles.iter() {
        // 解析价格数据
        let high = candle.h.parse::<f64>()?;
        let low = candle.l.parse::<f64>()?;
        let close = candle.c.parse::<f64>()?;

        // 计算ATR
        let atr_value = atr.next(high, low, close);

        // 时间格式化
        let time_str = time_util::mill_time_to_datetime_shanghai(candle.ts).unwrap();

        // 输出结果
        println!("{:<25} {:<12.2} {:<12.2} {:<12.2} {:<12.4}",
                 time_str, high, low, close, atr_value
        );
    }

    Ok(())
}