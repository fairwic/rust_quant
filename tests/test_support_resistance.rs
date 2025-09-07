use anyhow::Result;
use dotenv::dotenv;

use rust_quant::time_util;
use rust_quant::trading::indicator::candle::Candle;
use rust_quant::trading::indicator::detect_support_resistance;
use rust_quant::{
    app_config::db::init_db, trading, trading::model::entity::candles::entity::CandlesEntity,
};
#[tokio::test]
async fn test_support_resistance() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    init_db().await;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "4H";
    let period = 2;

    // 获取K线数据
    let mysql_candles: Vec<CandlesEntity> =
        trading::task::basic::get_candle_data_confirm(inst_id, time, 100, None).await?;

    // 确保有数据
    if mysql_candles.is_empty() {
        println!("警告: 未获取到K线数据");
        return Ok(());
    }
    // 转换为内部Candle结构
    let candles: Vec<Candle> = mysql_candles.iter().map(|e| e.into()).collect();
    println!("{:#?}", mysql_candles);

    // 在示例里，就算只有几根K线，也演示调用
    let sr_levels = detect_support_resistance::service::detect_support_resistance_with_bos_choch(
        &candles, 5,   // lookback
        14,  // ATR period
        0.5, // merge_ratio: pivot间距小于0.5*ATR就合并
    );

    for level in sr_levels {
        println!(
            "index={}, ts={:?}, price={:.2}, type={:?}, breakout={:?}",
            level.index,
            time_util::mill_time_to_datetime(level.ts),
            level.price,
            level.level_type,
            level.breakout
        );
    }
    Ok(())
}
