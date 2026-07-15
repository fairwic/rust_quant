/// 单元测试 - 验证策略核心逻辑
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_creation() {
        let mut candles = Vec::new();
        let mut ts = 1704067200000i64;
        let base_price = 50000.0;

        // 生成足够的历史数据
        for i in 0..100 {
            candles.push(CandleItem {
                ts,
                o: base_price,
                h: base_price + 50.0,
                l: base_price - 50.0,
                c: base_price + ((i % 5) as f64 - 2.0) * 10.0,
                v: 1000.0,
                confirm: 1,
            });
            ts += 300_000;
        }

        let tuning = RangeBreakoutDropBacktestTuning::default();
        let adapter = RangeBreakoutDropBacktestAdapter::new("BTC-USDT-SWAP", tuning);

        let snapshot = adapter.snapshot(&candles);
        assert!(snapshot.is_some(), "快照应该能够成功创建");

        let snap = snapshot.unwrap();
        println!("快照创建成功:");
        println!("  价格: {:.2}", snap.price);
        println!("  震荡区间: {:.2} - {:.2}", snap.range_low, snap.range_high);
        println!("  震荡幅度: {:.2}%", snap.range_volatility_pct);
        println!("  震荡状态: {}", snap.in_ranging_mode);
        println!("  ATR: {:.2}", snap.atr);
        println!("  RSI: {:.2}", snap.rsi);
    }

    #[test]
    fn test_evaluate_with_perfect_setup() {
        // 构造一个完美的做空设置
        let snapshot = RangeBreakoutDropSignalSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            price: 49000.0,
            range_high: 50000.0,
            range_low: 49500.0,
            range_volatility_pct: 1.0, // 在合理范围内
            in_ranging_mode: true,
            breakout_confirmed: true,
            breakout_body_ratio: 0.7,  // 高实体比例
            breakout_move_atr: 1.2,    // 超过阈值
            breakout_volume_mult: 2.0, // 放量
            slow_ema: 50000.0,
            price_below_ema: true, // 价格低于EMA
            long_term_ema: 50000.0,
            price_below_long_term_ema: true,
            atr: 200.0,
            rsi: 50.0,            // 中性RSI
            candle_direction: -1, // 阴线
        };

        let thresholds = RangeBreakoutDropThresholds::default();
        let decision = RangeBreakoutDropStrategy::evaluate(&thresholds, &snapshot);

        println!("评估结果:");
        println!("  动作: {:?}", decision.action);
        println!("  原因: {:?}", decision.reasons);

        assert!(
            matches!(decision.action, RangeBreakoutDropAction::Short),
            "完美设置应该产生做空信号"
        );
    }

    #[test]
    fn test_evaluate_blocked_by_no_ranging() {
        let snapshot = RangeBreakoutDropSignalSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            price: 49000.0,
            range_high: 50000.0,
            range_low: 49500.0,
            range_volatility_pct: 1.0,
            in_ranging_mode: false, // 不在震荡模式
            breakout_confirmed: true,
            breakout_body_ratio: 0.7,
            breakout_move_atr: 1.2,
            breakout_volume_mult: 2.0,
            slow_ema: 50000.0,
            price_below_ema: true,
            long_term_ema: 50000.0,
            price_below_long_term_ema: true,
            atr: 200.0,
            rsi: 50.0,
            candle_direction: -1,
        };

        let thresholds = RangeBreakoutDropThresholds::default();
        let decision = RangeBreakoutDropStrategy::evaluate(&thresholds, &snapshot);

        println!("阻塞原因: {:?}", decision.reasons);

        assert!(
            matches!(decision.action, RangeBreakoutDropAction::Flat),
            "非震荡状态应该被过滤"
        );
        assert!(
            decision
                .reasons
                .contains(&"NOT_IN_RANGING_MODE".to_string()),
            "应该包含 NOT_IN_RANGING_MODE 原因"
        );
    }

    #[test]
    fn test_min_data_length() {
        let tuning = RangeBreakoutDropBacktestTuning::default();
        let adapter = RangeBreakoutDropBacktestAdapter::new("BTC-USDT-SWAP", tuning);

        let min_len = adapter.min_data_length();
        println!("最小数据长度: {}", min_len);

        // 应该是所有周期参数的最大值
        assert!(min_len >= 50, "最小数据长度应该至少为50（考虑EMA50）");
    }
}
