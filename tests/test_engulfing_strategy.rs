// tests/test_engulfing_strategy.rs

use chrono::Utc;
use rust_quant::trading::model::entity::candles::entity::CandlesEntity;
use rust_quant::trading::strategy::engulfing_strategy::EngulfingStrategy;
use rust_quant::trading::strategy::strategy_common::run_back_test; // 导入工具函数

#[tokio::test]
async fn test_engulfing_strategy() {
    println!("111");

    println!("111");

    // 测试做多
    // 生成测试数据，4根K线，其中最后一根K线满足牛市吞没形态
    let candles = vec![
        CandlesEntity {
            o: "98.0".to_string(),
            h: "102.0".to_string(),
            l: "96.0".to_string(),
            c: "96.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 2,
            confirm: "".to_string(),
            update_time: None,
        },
        CandlesEntity {
            o: "97.0".to_string(),
            h: "100.0".to_string(),
            l: "95.0".to_string(),
            c: "95.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 3,
            confirm: "".to_string(),
            update_time: None,
        },
        CandlesEntity {
            o: "90.0".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "89.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
        CandlesEntity {
            o: "93.0".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "100.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
        CandlesEntity {
            o: "93.0".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "120.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
        CandlesEntity {
            o: "93.0".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "130.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
    ];

    let num_bars = 3;
    let fib_levels = vec![0.236, 0.382, 0.500, 0.618, 0.786, 1.0];
    let max_loss_percent = 0.02;

    // // 执行回测
    // let (final_funds, win_rate, open_trades, trade_records) = run_back_test(
    //     |candles| EngulfingStrategy::get_trade_signal(candles, num_bars),
    //     &candles,
    //     &fib_levels,
    //     max_loss_percent,
    //     num_bars + 1,
    // );

    println!("111111");
    // 断言测试结果是否符合预期
    // assert_eq!(final_funds, 128.4896); // 预期的最终资金

    // 断言测试结果是否符合预期
    //测试做空
    // 生成测试数据，4根K线，其中最后一根K线满足牛市吞没形态
    let candles = vec![
        CandlesEntity {
            o: "95.5".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "96.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
        CandlesEntity {
            o: "96.5".to_string(),
            h: "100.0".to_string(),
            l: "95.0".to_string(),
            c: "97.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 3,
            confirm: "".to_string(),
            update_time: None,
        },
        CandlesEntity {
            o: "100.5".to_string(),
            h: "102.0".to_string(),
            l: "96.0".to_string(),
            c: "101.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 2,
            confirm: "".to_string(),
            update_time: None,
        },
        CandlesEntity {
            o: "100.0".to_string(),
            h: "105.0".to_string(),
            l: "95.0".to_string(),
            c: "100.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 1,
            confirm: "".to_string(),
            update_time: None,
        },
        CandlesEntity {
            o: "100.0".to_string(),
            h: "105.0".to_string(),
            l: "95.0".to_string(),
            c: "98.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 1,
            confirm: "".to_string(),
            update_time: None,
        },
    ];

    let num_bars = 3;
    let fib_levels = vec![0.236, 0.382, 0.500, 0.618, 0.786, 1.0];
    let max_loss_percent = 0.02;

    // // 执行回测
    // let (final_funds, win_rate, open_trades, trade_records) = run_back_test(
    //     |candles| EngulfingStrategy::get_trade_signal(candles, num_bars),
    //     &candles,
    //     &fib_levels,
    //     max_loss_percent,
    //     num_bars + 1,
    // );

    // println!(
    //     "final_funds: {},win_rate:{},open_trades:{},trade-records:{:#?}",
    //     final_funds, win_rate, open_trades, trade_records
    // );
    // // 断言测试结果是否符合预期
    // println!("22222222");
    // assert_eq!(final_funds, 102.0); // 预期的最终资金

    //--------
    // 断言测试结果是否符合预期
    //测试做空
    // 生成测试数据，4根K线，其中最后一根K线满足牛市吞没形态

    let candles = vec![
        CandlesEntity {
            o: "98.0".to_string(),
            h: "102.0".to_string(),
            l: "96.0".to_string(),
            c: "96.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 2,
            confirm: "".to_string(),
            update_time: None,
        },
        CandlesEntity {
            o: "97.0".to_string(),
            h: "100.0".to_string(),
            l: "95.0".to_string(),
            c: "95.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 3,
            confirm: "".to_string(),
            update_time: None,
        },
        CandlesEntity {
            o: "90.0".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "89.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
        CandlesEntity {
            o: "93.0".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "100.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
        CandlesEntity {
            o: "93.0".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "125.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
        CandlesEntity {
            o: "93.0".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "180.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
        CandlesEntity {
            o: "93.0".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "180.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
        CandlesEntity {
            o: "93.0".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "180.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
        CandlesEntity {
            o: "93.0".to_string(),
            h: "110.0".to_string(),
            l: "94.0".to_string(),
            c: "180.0".to_string(),
            vol: "".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            ts: 4,
            confirm: "".to_string(),
            update_time: None,
        }, // 牛市吞没
    ];

    let num_bars = 3;
    let fib_levels = vec![0.236, 0.382, 0.500, 0.618, 0.786, 1.0];
    let max_loss_percent = 0.02;

    // // 执行回测
    // let (final_funds, win_rate, open_trades, trade_records) = run_back_test(
    //     |candles| EngulfingStrategy::get_trade_signal(candles, num_bars),
    //     &candles,
    //     &fib_levels,
    //     max_loss_percent,
    //     num_bars + 1,
    // );

    // println!(
    //     "final_funds: {},win_rate:{},open_trades:{},trade-records:{:#?}",
    //     final_funds, win_rate, open_trades, trade_records
    // );
    // 断言测试结果是否符合预期
    println!("3333333333333");
    // assert_eq!(final_funds, 102.0); // 预期的最终资金
}

fn generate_test_data() -> Vec<CandlesEntity> {
    let mut candles = Vec::new();
    let mut timestamp = Utc::now().timestamp_millis();

    // 生成20条测试数据
    for i in 0..20 {
        candles.push(CandlesEntity {
            ts: timestamp,
            o: (100.0 + i as f64).to_string(),
            h: (105.0 + i as f64).to_string(),
            l: (95.0 + i as f64).to_string(),
            c: (100.0 + i as f64).to_string(),
            vol: "1000".to_string(),
            vol_ccy: "".to_string(),
            // vol_ccy_quote: "".to_string(),
            update_time: None,
            confirm: "".to_string(),
        });
        timestamp += 300_000; // 5分钟间隔
    }

    candles
}
