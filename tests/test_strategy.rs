use rust_quant::trading::model::market::candles::CandlesEntity;
use rust_quant::trading::strategy::strategy_common::{run_back_test, SignalResult, TradeRecord, BasicRiskStrategyConfig};
use anyhow::Result;

#[tokio::test]
async fn test_strategy_signals() -> Result<()> {
    let mock_candles = create_mock_candles();
    
    let strategy_config = BasicRiskStrategyConfig {
        use_dynamic_tp: true,
        use_fibonacci_tp: false,
        max_loss_percent: 0.02,
        profit_threshold: 0.01,
        is_move_stop_loss: false,
        is_used_signal_k_line_stop_loss: false,
    };

    // 打印每根K线的信息
    println!("\n模拟K线数据:");
    for (i, candle) in mock_candles.iter().enumerate() {
        println!("K{}: O={}, H={}, L={}, C={}, ts={}", 
            i+1, candle.o, candle.h, candle.l, candle.c, candle.ts);
    }

    // let result = run_back_test(
    //     |candles| mock_strategy(candles),
    //     &mock_candles,
    //     &vec![],  // 不使用斐波那契水平
    //     strategy_config,
    //     3,     // 使用3根K线
    //     false, // 禁用斐波那契止盈
    //     true,  // 允许做多
    //     false, // 禁用做空
    // );

    println!("\n回测结果: {:#?}", result);
    assert!(!result.trade_records.is_empty(), "应该有交易记录生成");
    verify_trade_signals(&result.trade_records);

    Ok(())
}

#[tokio::test]
async fn test_strategy_scenarios() -> Result<()> {
    // 场景1: 做多盈利平仓
    let candles_profit = create_profit_scenario();
    verify_scenario("盈利平仓", candles_profit).await?;

    Ok(())
}

#[tokio::test]
async fn test_2() -> Result<()> {
    // 场景2: 做多止损
    let candles_stoploss = create_stoploss_scenario(); 
    verify_scenario("止损平仓", candles_stoploss).await?;
    Ok(())
}

#[tokio::test]
async fn test_3() -> Result<()> {
    // 场景3: 动态止盈
    let candles_dynamic = create_dynamic_tp_scenario();
    verify_scenario("动态止盈", candles_dynamic).await?;

    Ok(())
}

#[tokio::test]
async fn test_short_scenarios() -> Result<()> {
    // 场景4: 做空盈利平仓
    // let candles_short_profit = create_short_profit_scenario();
    // verify_short_scenario("做空盈利平仓", candles_short_profit).await?;

    // 场景5: 做空止损
    let candles_short_stoploss = create_short_stoploss_scenario();
    verify_short_scenario("做空止损平仓", candles_short_stoploss).await?;

    // 场景6: 做空动态止盈
    let candles_short_dynamic = create_short_dynamic_tp_scenario();
    verify_short_scenario("做空动态止盈", candles_short_dynamic).await?;

    Ok(())
}

/// 验证单个场景
async fn verify_scenario(name: &str, mock_candles: Vec<CandlesEntity>) -> Result<()> {
    println!("\n测试场景: {}", name);
    
    let strategy_config = BasicRiskStrategyConfig {
        use_dynamic_tp: true,
        use_fibonacci_tp: false,
        max_loss_percent: 0.02,    // 2%止损
        profit_threshold: 0.01,    // 1%启用动态止盈
        is_move_stop_loss: false,
        is_used_signal_k_line_stop_loss: false,
    };

    // let result = run_back_test(
    //     |candles| mock_strategy(candles),
    //     &mock_candles,
    //     &vec![],
    //     strategy_config,
    //     3,
    //     false,
    //     true,
    //     false,
    // );

    println!("回测结果: {:#?}", result);
    verify_trade_signals(&result.trade_records);
    Ok(())
}

/// 验证做空场景
async fn verify_short_scenario(name: &str, mock_candles: Vec<CandlesEntity>) -> Result<()> {
    println!("\n测试场景: {}", name);
    
    let strategy_config = BasicRiskStrategyConfig {
        use_dynamic_tp: true,
        use_fibonacci_tp: false,
        max_loss_percent: 0.02,    // 2%止损
        profit_threshold: 0.01,    // 1%启用动态止盈
        is_move_stop_loss: false,
        is_used_signal_k_line_stop_loss: false,
    };

    // let result = run_back_test(
    //     |candles| mock_short_strategy(candles),
    //     &mock_candles,
    //     &vec![],
    //     strategy_config,
    //     3,
    //     false,
    //     false,  // 禁用做多
    //     true,   // 启用做空
    // );

    // println!("回测结果: {:#?}", result);
    // verify_trade_signals(&result.trade_records);
    // Ok(())
}

/// 创建模拟K线数据
fn create_mock_candles() -> Vec<CandlesEntity> {
    let mut candles = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();
    let five_min = 5 * 60 * 1000;
    let base_price = 100.0;
    
    // 构建初始数据
    add_mock_candle(&mut candles, base_price, now - five_min * 7, "100,101,99,100");   // K1
    add_mock_candle(&mut candles, base_price, now - five_min * 6, "100,101,99,100");   // K2
    add_mock_candle(&mut candles, base_price, now - five_min * 5, "100,101,99,100");   // K3
    
    // 触发做多信号
    add_mock_candle(&mut candles, base_price, now - five_min * 4, "100,103,100,103");  // K4: +3%
    
    // 上涨趋势
    add_mock_candle(&mut candles, base_price, now - five_min * 3, "103,105,103,105");  // K5: +1.9%
    add_mock_candle(&mut candles, base_price, now - five_min * 2, "105,107,105,107");  // K6: +1.9%

    add_mock_candle(&mut candles, base_price, now - five_min * 1, "107,107,102,106");  // K7: -4.7%
    // 回落触发止盈
    add_mock_candle(&mut candles, base_price, now , "107,107,102,102");  // K8: -4.7%

    candles
}

/// 添加模拟K线
fn add_mock_candle(candles: &mut Vec<CandlesEntity>, base_price: f64, ts: i64, prices: &str) {
    let (o, h, l, c) = parse_prices(prices);
    candles.push(CandlesEntity {
        o: o.to_string(),
        h: h.to_string(),
        l: l.to_string(),
        c: c.to_string(),
        ts,
        vol: "1000.0".to_string(),
        vol_ccy: "1000.0".to_string(),
        confirm: "0".to_string(),
    });
}

/// 解析价格字符串 "open,high,low,close"
fn parse_prices(prices: &str) -> (f64, f64, f64, f64) {
    let parts: Vec<f64> = prices.split(',')
        .map(|p| p.parse::<f64>().unwrap())
        .collect();
    (parts[0], parts[1], parts[2], parts[3])
}

/// 模拟策略
fn mock_strategy(candles: &[CandlesEntity]) -> SignalResult {
    let current = candles.last().unwrap();
    let price = current.c.parse::<f64>().unwrap();
    let mut signal = SignalResult {
        should_buy: false,
        should_sell: false,
        open_price: price,
        signal_kline_stop_loss_price: None,
        ts: current.ts,
        single_value: None,
        single_result: None,
    };

    if candles.len() < 3 {
        return signal;
    }

    let prev = &candles[candles.len() - 2];
    let prev_close = prev.c.parse::<f64>().unwrap();
    let change = (price - prev_close) / prev_close;

    // 找到开仓价格（第一个103）
    let entry_price = candles.iter()
        .find(|c| c.c.parse::<f64>().unwrap() == 103.0)
        .map(|c| c.c.parse::<f64>().unwrap());

    println!("\nK线分析: K{} -> K{}, 前收={}, 当前={}, 变化率={:.2}%", 
        candles.len() - 1, candles.len(), prev_close, price, change * 100.0);

    // 开仓信号：从100.0开始的上涨
    if prev_close >= 99.9 && prev_close <= 100.1 && change > 0.02 {
        signal.should_buy = true;
        signal.should_sell = false;
        signal.single_value = Some("做多信号".to_string());
        println!(">>> 触发做多信号 <<< 开仓价格: {}", price);
    }
    // 止损信号：相对开仓价下跌超过2%
    else if let Some(entry) = entry_price {
        let loss_pct = (price - entry) / entry;
        if loss_pct < -0.02 {
            signal.should_buy = false;
            signal.should_sell = true;
            signal.single_value = Some(format!("止损信号: 跌幅{:.2}% > 止损线2%", loss_pct * 100.0));
            println!(">>> 触发止损信号 <<< 开仓价:{}, 当前价:{}, 跌幅{:.2}% > 止损线2%", 
                entry, price, loss_pct * 100.0);
        }
    }

    signal
}

/// 做空策略
fn mock_short_strategy(candles: &[CandlesEntity]) -> SignalResult {
    let current = candles.last().unwrap();
    let price = current.c.parse::<f64>().unwrap();
    let mut signal = SignalResult {
        should_buy: false,
        should_sell: false,
        open_price: price,
        signal_kline_stop_loss_price: None,
        ts: current.ts,
        single_value: None,
        single_result: None,
    };

    if candles.len() < 3 {
        return signal;
    }

    let prev = &candles[candles.len() - 2];
    let prev_close = prev.c.parse::<f64>().unwrap();
    let change = (price - prev_close) / prev_close;

    println!("\nK线分析: K{} -> K{}, 前收={}, 当前={}, 变化率={:.2}%", 
        candles.len() - 1, candles.len(), prev_close, price, change * 100.0);

    // 开仓信号：从100.0开始的下跌
    if prev_close >= 99.9 && prev_close <= 100.1 && change < -0.02 {
        signal.should_sell = true;
        signal.should_buy = false;
        signal.single_value = Some("做空信号".to_string());
        println!(">>> 触发做空信号 <<< 开仓价格: {}", price);
    }

    signal
}

/// 验证交易信号
fn verify_trade_signals(trade_records: &Vec<TradeRecord>) {
    let mut long_entries = 0;
    let mut long_exits = 0;
    let mut short_entries = 0;
    let mut short_exits = 0;

    for record in trade_records {
        println!("交易记录: {:?}", record);
        match record.option_type.as_str() {
            "LONG" | "long" => long_entries += 1,
            "SHORT" | "short" => short_entries += 1,
            "close" => {
                // 根据开仓类型判断是多仓平仓还是空仓平仓
                if trade_records.iter().any(|r| r.option_type == "LONG" || r.option_type == "long") {
                    long_exits += 1;
                } else {
                    short_exits += 1;
                }
            },
            _ => println!("未知交易类型: {}", record.option_type),
        }
    }

    println!("\n交易统计:");
    println!("做多开仓次数: {}", long_entries);
    println!("做多平仓次数: {}", long_exits);
    println!("做空开仓次数: {}", short_entries);
    println!("做空平仓次数: {}", short_exits);

    // 根据是否有多空交易分别验证
    if long_entries > 0 {
        assert!(long_exits > 0, "应该有做多平仓");
    }
    if short_entries > 0 {
        assert!(short_exits > 0, "应该有做空平仓");
    }
    assert!(long_entries > 0 || short_entries > 0, "应该有开仓交易");
}

/// 创建盈利平仓场景
fn create_profit_scenario() -> Vec<CandlesEntity> {
    let mut candles = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();
    let five_min = 5 * 60 * 1000;
    let base_price = 100.0;
    
    add_mock_candle(&mut candles, base_price, now - five_min * 6, "100,101,99,100");   // K1: 基准价
    add_mock_candle(&mut candles, base_price, now - five_min * 5, "100,101,99,100");   // K2: 盘整
    add_mock_candle(&mut candles, base_price, now - five_min * 4, "100,101,99,100");   // K3: 盘整
    add_mock_candle(&mut candles, base_price, now - five_min * 3, "100,103,100,103");  // K4: 开多信号
    add_mock_candle(&mut candles, base_price, now - five_min * 2, "103,107,103,107");  // K5: 持续上涨
    add_mock_candle(&mut candles, base_price, now - five_min * 1, "107,107,102,102");  // K6: 回落平仓
    add_mock_candle(&mut candles, base_price, now, "102,103,101,101");                 // K7: 收尾K线

    candles
}

/// 创建止损场景
fn create_stoploss_scenario() -> Vec<CandlesEntity> {
    let mut candles = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();
    let five_min = 5 * 60 * 1000;
    let base_price = 100.0;
    
    add_mock_candle(&mut candles, base_price, now - five_min * 6, "100,101,99,100");   // K1: 基准价
    add_mock_candle(&mut candles, base_price, now - five_min * 5, "100,101,99,100");   // K2: 盘整
    add_mock_candle(&mut candles, base_price, now - five_min * 4, "100,101,99,100");   // K3: 盘整
    add_mock_candle(&mut candles, base_price, now - five_min * 3, "100,103,100,103");  // K4: 开多信号
    add_mock_candle(&mut candles, base_price, now - five_min * 2, "103,103,99,102");    // K5: 小幅回落
    add_mock_candle(&mut candles, base_price, now - five_min * 1, "99,99,97,100");      // K6: 跌破止损
    add_mock_candle(&mut candles, base_price, now, "97,98,96,100");                     // K7: 收尾K线

    candles
}

// 创建动态止盈场景
fn create_dynamic_tp_scenario() -> Vec<CandlesEntity> {
    let mut candles = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();
    let five_min = 5 * 60 * 1000;
    let base_price = 100.0;
    
    add_mock_candle(&mut candles, base_price, now - five_min * 7, "100,101,99,100");   // K1: 基准价
    add_mock_candle(&mut candles, base_price, now - five_min * 6, "100,101,99,100");   // K2: 盘整
    add_mock_candle(&mut candles, base_price, now - five_min * 5, "100,101,99,100");   // K3: 盘整
    add_mock_candle(&mut candles, base_price, now - five_min * 4, "100,103,100,103");  // K4: 开多信号
    add_mock_candle(&mut candles, base_price, now - five_min * 3, "103,106,103,106");  // K5: 上涨
    add_mock_candle(&mut candles, base_price, now - five_min * 2, "106,108,106,109");  // K6: 继续上涨
    add_mock_candle(&mut candles, base_price, now - five_min * 1, "108,108,102,105");  // K7: 大幅回落
    add_mock_candle(&mut candles, base_price, now, "102,103,101,102");                 // K8: 收尾K线

    candles
}

/// 创建做空盈利场景
fn create_short_profit_scenario() -> Vec<CandlesEntity> {
    let mut candles = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();
    let five_min = 5 * 60 * 1000;
    let base_price = 100.0;
    
    add_mock_candle(&mut candles, base_price, now - five_min * 6, "100,101,99,100");   // K1: 基准价
    add_mock_candle(&mut candles, base_price, now - five_min * 5, "100,101,99,100");   // K2: 盘整
    add_mock_candle(&mut candles, base_price, now - five_min * 4, "100,100,97,97");    // K3: 开空信号
    add_mock_candle(&mut candles, base_price, now - five_min * 3, "97,97,94,94");      // K4: 继续下跌
    add_mock_candle(&mut candles, base_price, now - five_min * 2, "94,94,91,91");      // K5: 继续下跌
    add_mock_candle(&mut candles, base_price, now - five_min * 1, "91,94,91,94");      // K6: 反弹平仓
    add_mock_candle(&mut candles, base_price, now, "94,95,93,95");                     // K7: 收尾K线

    candles
}

/// 创建做空止损场景
fn create_short_stoploss_scenario() -> Vec<CandlesEntity> {
    let mut candles = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();
    let five_min = 5 * 60 * 1000;
    let base_price = 100.0;
    
    add_mock_candle(&mut candles, base_price, now - five_min * 6, "100,101,99,100");   // K1: 基准价
    add_mock_candle(&mut candles, base_price, now - five_min * 5, "100,101,99,100");   // K2: 盘整
    add_mock_candle(&mut candles, base_price, now - five_min * 4, "100,100,97,97");    // K3: 开空信号
    add_mock_candle(&mut candles, base_price, now - five_min * 3, "97,99,97,98");      // K4: 小幅反弹
    add_mock_candle(&mut candles, base_price, now - five_min * 2, "99,100,99,100");    // K5: 触发止损
    add_mock_candle(&mut candles, base_price, now - five_min * 1, "100,101,99,99");    // K6: 收尾K线

    candles
}

/// 创建做空动态止盈场景
fn create_short_dynamic_tp_scenario() -> Vec<CandlesEntity> {
    let mut candles = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();
    let five_min = 5 * 60 * 1000;
    let base_price = 100.0;
    
    add_mock_candle(&mut candles, base_price, now - five_min * 7, "100,101,99,100");   // K1: 基准价
    add_mock_candle(&mut candles, base_price, now - five_min * 6, "100,101,99,100");   // K2: 盘整
    add_mock_candle(&mut candles, base_price, now - five_min * 5, "100,100,97,97");    // K3: 开空信号
    add_mock_candle(&mut candles, base_price, now - five_min * 4, "97,97,94,94");      // K4: 继续下跌
    add_mock_candle(&mut candles, base_price, now - five_min * 3, "94,94,91,95");      // K5: 继续下跌
    add_mock_candle(&mut candles, base_price, now - five_min * 2, "91,91,89,89");      // K6: 新低
    add_mock_candle(&mut candles, base_price, now - five_min * 1, "89,92,89,96");      // K7: 反弹触发动态止盈
    add_mock_candle(&mut candles, base_price, now, "92,93,91,93");                     // K8: 收尾K线

    candles
} 