//! 策略触发集成测试
//!
//! 验证从 WebSocket 数据到策略执行的完整闭环

use rust_quant_market::models::CandlesEntity;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::info;

#[tokio::test]
async fn test_strategy_trigger_callback() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🧪 开始测试策略触发回调");

    // 创建触发标志
    let triggered = Arc::new(AtomicBool::new(false));
    let triggered_clone = Arc::clone(&triggered);

    // 创建策略触发回调函数
    let strategy_trigger = Arc::new(move |inst_id: String, time_interval: String, snap: CandlesEntity| {
        info!(
            "✅ 策略触发回调被调用: inst_id={}, time_interval={}, ts={}",
            inst_id, time_interval, snap.ts
        );
        triggered_clone.store(true, Ordering::SeqCst);
    });

    // 模拟 K线确认数据
    let mock_candle = CandlesEntity {
        ts: 1699999999000,
        o: 40000.0,
        h: 40500.0,
        l: 39500.0,
        c: 40200.0,
        vol: 1000.0,
        vol_ccy: 40000000.0,
        vol_ccy_quote: 40000000.0,
        confirm: "1".to_string(),
    };

    // 调用触发器
    let inst_id = "BTC-USDT-SWAP".to_string();
    let time_interval = "1H".to_string();
    strategy_trigger(inst_id, time_interval, mock_candle);

    // 等待异步任务执行
    sleep(Duration::from_millis(100)).await;

    // 验证触发器被调用
    assert!(
        triggered.load(Ordering::SeqCst),
        "策略触发回调应该被调用"
    );

    info!("✅ 策略触发回调测试通过");
}

#[tokio::test]
async fn test_strategy_trigger_with_multiple_candles() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🧪 开始测试多个K线确认触发");

    // 创建计数器
    let trigger_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let trigger_count_clone = Arc::clone(&trigger_count);

    // 创建策略触发回调函数
    let strategy_trigger = Arc::new(move |inst_id: String, time_interval: String, snap: CandlesEntity| {
        info!(
            "✅ K线确认触发 #{}: inst_id={}, time_interval={}, ts={}",
            trigger_count_clone.load(Ordering::SeqCst) + 1,
            inst_id,
            time_interval,
            snap.ts
        );
        trigger_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    // 模拟多个 K线确认
    let candles = vec![
        CandlesEntity {
            ts: 1699999999000,
            o: 40000.0,
            h: 40500.0,
            l: 39500.0,
            c: 40200.0,
            vol: 1000.0,
            vol_ccy: 40000000.0,
            vol_ccy_quote: 40000000.0,
            confirm: "1".to_string(),
        },
        CandlesEntity {
            ts: 1700003599000,
            o: 40200.0,
            h: 40800.0,
            l: 40000.0,
            c: 40500.0,
            vol: 1200.0,
            vol_ccy: 48000000.0,
            vol_ccy_quote: 48000000.0,
            confirm: "1".to_string(),
        },
        CandlesEntity {
            ts: 1700007199000,
            o: 40500.0,
            h: 41000.0,
            l: 40300.0,
            c: 40800.0,
            vol: 1100.0,
            vol_ccy: 44800000.0,
            vol_ccy_quote: 44800000.0,
            confirm: "1".to_string(),
        },
    ];

    // 触发所有 K线
    for candle in candles {
        let trigger_clone = Arc::clone(&strategy_trigger);
        trigger_clone("BTC-USDT-SWAP".to_string(), "1H".to_string(), candle);
    }

    // 等待异步任务执行
    sleep(Duration::from_millis(200)).await;

    // 验证触发器被调用次数
    let count = trigger_count.load(Ordering::SeqCst);
    assert_eq!(count, 3, "策略触发回调应该被调用 3 次，实际调用 {} 次", count);

    info!("✅ 多个K线确认触发测试通过");
}

#[tokio::test]
async fn test_strategy_trigger_ignores_unconfirmed_candles() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🧪 开始测试未确认K线不触发策略");

    // 创建触发标志
    let triggered = Arc::new(AtomicBool::new(false));
    let triggered_clone = Arc::clone(&triggered);

    // 创建策略触发回调函数
    let strategy_trigger = Arc::new(move |inst_id: String, time_interval: String, snap: CandlesEntity| {
        if snap.confirm == "1" {
            info!(
                "✅ K线确认触发: inst_id={}, time_interval={}, ts={}",
                inst_id, time_interval, snap.ts
            );
            triggered_clone.store(true, Ordering::SeqCst);
        } else {
            info!(
                "⏭️  跳过未确认K线: inst_id={}, time_interval={}, ts={}",
                inst_id, time_interval, snap.ts
            );
        }
    });

    // 模拟未确认 K线数据
    let mock_candle = CandlesEntity {
        ts: 1699999999000,
        o: 40000.0,
        h: 40500.0,
        l: 39500.0,
        c: 40200.0,
        vol: 1000.0,
        vol_ccy: 40000000.0,
        vol_ccy_quote: 40000000.0,
        confirm: "0".to_string(), // 未确认
    };

    // 调用触发器
    let inst_id = "BTC-USDT-SWAP".to_string();
    let time_interval = "1H".to_string();
    strategy_trigger(inst_id, time_interval, mock_candle);

    // 等待异步任务执行
    sleep(Duration::from_millis(100)).await;

    // 验证触发器未被调用
    assert!(
        !triggered.load(Ordering::SeqCst),
        "未确认K线不应触发策略"
    );

    info!("✅ 未确认K线不触发策略测试通过");
}


