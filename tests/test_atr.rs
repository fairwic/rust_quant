#[cfg(test)]
mod test {
    use dotenv::dotenv;
    use ta::indicators::AverageTrueRange;
    use ta::Next;
    use rust_quant::app_config::db::init_db;
    use rust_quant::app_config::log::setup_logging;
    use rust_quant::{time_util, trading};
    use rust_quant::trading::indicator::atr::ATR;
    use rust_quant::trading::indicator::bar::Bar;
    use rust_quant::trading::model::market::candles::CandlesEntity;

    #[tokio::test]
    async fn test_atr_calculation() -> anyhow::Result<()> {
        dotenv().ok();
        setup_logging().await?;
        init_db().await;
        let mut atr = ATR::new(10).unwrap();
        // 设置参数
        let inst_id = "BTC-USDT-SWAP";
        let period = "4H";
        let min_length = 70;
        let select_time = None;
        let candles = trading::task::basic::get_candle_data(inst_id, period, min_length, select_time).await?;
        println!("{:#?}", candles);
        for item in candles.iter() {
            println!("item c:{:#?}",item);
            let x = atr.next(item.h.parse()?, item.l.parse()?, item.c.parse()?);
            println!("my atr{}", x)
        }
        Ok(())
    }
    #[tokio::test]
    async fn test_ta_atr() ->anyhow::Result<()> {
        // 初始化环境和数据库连接
        dotenv().ok();
        init_db().await;

        // 设置参数
        let inst_id = "BTC-USDT-SWAP";
        let time = "4H";
        let period = 2;

        // 获取K线数据
        let mysql_candles: Vec<CandlesEntity> =
            trading::task::basic::get_candle_data(inst_id, time, 5, None).await?;
        println!("{:#?}", mysql_candles);

        // 确保有数据
        if mysql_candles.is_empty() {
            println!("警告: 未获取到K线数据");
            return Ok(());
        }

        let mut atr = AverageTrueRange::new(period).unwrap();

        // let mut sma = SMA::new(2); // 设置周期为15

        // 打印表头
        println!("\n{} {}K线 ATR({})计算结果:", inst_id, time, period);
        println!(
            "{:<25} {:<12} {:<12} {:<12} {:<12}",
            "时间", "最高价", "最低价", "收盘价", "ATR"
        );
        println!("{}", "=".repeat(75));

        // 计算并显示结果
        for candle in mysql_candles.iter() {
            // 解析价格数据
            let high = candle.h.parse::<f64>()?;
            let low = candle.l.parse::<f64>()?;
            let close = candle.c.parse::<f64>()?;
            // 计算ATR
            let atr_value = atr.next(&Bar::new().high(high).low(low).close(close));

            // 时间格式化
            let time_str = time_util::mill_time_to_datetime_shanghai(candle.ts).unwrap();

            // 输出结果
            println!(
                "{:<25} {:<12.2} {:<12.2} {:<12.2} {:<12.4}",
                time_str, high, low, close, atr_value
            );
        }

        Ok(())
    }
}