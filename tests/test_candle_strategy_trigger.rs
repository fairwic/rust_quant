use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::services::candle_service::candle_service::CandleService;
use okx::dto::market_dto::CandleOkxRespDto;
use tracing::info;
use std::env;

#[tokio::test]
async fn test_candle_strategy_trigger() {
    // 设置必要的环境变量
    env::set_var("APP_ENV", "local");

    // 初始化日志
    setup_logging().await.expect("Failed to setup logging");

    info!("🧪 开始测试K线确认触发策略功能");

    // 创建 CandleService
    let candle_service = CandleService::new();

    // 模拟K线数据 - 未确认
    let unconfirmed_candle = vec![CandleOkxRespDto {
        ts: "1700000000000".to_string(),
        o: "50000.0".to_string(),
        h: "50100.0".to_string(),
        l: "49900.0".to_string(),
        c: "50050.0".to_string(),
        v: "100.5".to_string(),
        vol_ccy: "5000000.0".to_string(),
        vol_ccy_quote: "5000000.0".to_string(),
        confirm: "0".to_string(), // 未确认
    }];

    // 模拟K线数据 - 已确认
    let confirmed_candle = vec![CandleOkxRespDto {
        ts: "1700000060000".to_string(), // 1分钟后
        o: "50050.0".to_string(),
        h: "50200.0".to_string(),
        l: "50000.0".to_string(),
        c: "50150.0".to_string(),
        v: "120.8".to_string(),
        vol_ccy: "6000000.0".to_string(),
        vol_ccy_quote: "6000000.0".to_string(),
        confirm: "1".to_string(), // 已确认
    }];
    
    info!("📊 更新未确认K线数据");
    // 更新未确认K线 - 不应该触发策略
    if let Err(e) = candle_service
        .update_candle(unconfirmed_candle, "BTC-USDT-SWAP", "1m")
        .await
    {
        tracing::error!("更新未确认K线失败: {}", e);
    }
    
    // 等待一下，确保异步任务完成
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    info!("✅ 更新已确认K线数据");
    // 更新已确认K线 - 应该触发策略
    if let Err(e) = candle_service
        .update_candle(confirmed_candle, "BTC-USDT-SWAP", "1m")
        .await
    {
        tracing::error!("更新已确认K线失败: {}", e);
    }
    
    // 等待策略执行完成
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    info!("🎉 K线确认触发策略功能测试完成");
}

#[tokio::test]
async fn test_multiple_candle_updates() {
    // 设置必要的环境变量
    env::set_var("APP_ENV", "local");

    // 初始化日志
    setup_logging().await.expect("Failed to setup logging");

    info!("🧪 开始测试多次K线更新");

    let candle_service = CandleService::new();

    // 模拟多次K线更新
    for i in 0..5 {
        let ts = 1700000000000i64 + (i as i64 * 60000); // 每分钟一次
        let confirm = if i == 4 { "1" } else { "0" }; // 最后一次确认

        let candle = vec![CandleOkxRespDto {
            ts: ts.to_string(),
            o: format!("{}.0", 50000 + i * 10),
            h: format!("{}.0", 50100 + i * 10),
            l: format!("{}.0", 49900 + i * 10),
            c: format!("{}.0", 50050 + i * 10),
            v: format!("{}.0", 100 + i),
            vol_ccy: format!("{}.0", 5000000 + i * 100000),
            vol_ccy_quote: format!("{}.0", 5000000 + i * 100000),
            confirm: confirm.to_string(),
        }];
        
        info!("📊 更新第{}次K线数据 (确认状态: {})", i + 1, confirm);
        
        if let Err(e) = candle_service
            .update_candle(candle, "ETH-USDT-SWAP", "1m")
            .await
        {
            tracing::error!("更新K线失败: {}", e);
        }
        
        // 短暂等待
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
    
    // 等待所有异步任务完成
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    
    info!("🎉 多次K线更新测试完成");
}
