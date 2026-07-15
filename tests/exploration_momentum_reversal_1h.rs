// 1H Momentum Reversal 策略 - 探索模式原型
//
// 目标: 快速验证"动量衰竭反转"假设
// 周期: 1H
// 币种: Tier A (BTC/ETH)
// 预期: Win Rate > 55%, 月 PnL > 10u

use rust_quant_common::CandleItem;

#[cfg(test)]
mod tests {
    use super::*;

    /// 探索阶段快速原型 - 允许硬编码和简化
    #[test]
    #[ignore] // 需要准备数据后移除 ignore
    fn test_momentum_reversal_1h_exploration() {
        println!("=== 1H Momentum Reversal 探索测试 ===\n");

        // ============================================
        // Step 1: 加载数据（需要准备）
        // ============================================

        // TODO: 准备数据
        // 方式 1: 从数据库导出 2-3 个月的 1H 数据
        // 方式 2: 使用现有的 CSV fixture

        // let candles = load_csv_data("tests/fixtures/btc_1h_3months.csv");
        // assert!(candles.len() > 1000, "需要至少 1000 根 1H K 线（约 40 天）");

        let candles: Vec<CandleItem> = vec![]; // 占位符

        if candles.is_empty() {
            println!("⚠️  数据未准备，请先导出 BTC 1H 数据（2-3 个月）");
            println!("   建议: 2026-04-01 ~ 2026-06-30 (牛市+熊市混合)");
            return;
        }

        // ============================================
        // Step 2: 核心逻辑（简化版本）
        // ============================================

        let mut signals = Vec::new();
        let rsi_period = 14;
        let ema_period = 20;
        let atr_period = 14;

        // 需要足够的历史数据来计算指标
        let min_warmup = rsi_period.max(ema_period).max(atr_period) + 10;

        for i in min_warmup..candles.len() {
            let window = &candles[i.saturating_sub(50)..=i];
            let current = &candles[i];

            // 计算指标（简化版 - 生产环境应使用 indicators crate）
            let rsi = calculate_simple_rsi(window, rsi_period);
            let ema20 = calculate_simple_ema(window, ema_period);
            let atr = calculate_simple_atr(window, atr_period);

            // 价格偏离度
            let deviation = (current.close - ema20).abs() / ema20;
            let deviation_atr = (current.close - ema20).abs() / atr;

            // 反转形态检测（简化版）
            let is_hammer = detect_hammer_pattern(current);
            let is_engulfing = detect_engulfing_pattern(window);

            // ============================================
            // 信号生成逻辑
            // ============================================

            // 做多信号: RSI 超卖 + 偏离过大 + 反转形态
            if rsi < 30.0
                && deviation_atr > 2.0
                && (is_hammer || is_engulfing)
                && current.close < ema20
            {
                signals.push(Signal {
                    index: i,
                    timestamp: current.timestamp,
                    direction: Direction::Long,
                    entry_price: current.close,
                    rsi,
                    deviation_atr,
                    has_pattern: true,
                });
            }

            // 做空信号: RSI 超买 + 偏离过大 + 反转形态
            if rsi > 70.0
                && deviation_atr > 2.0
                && (is_hammer || is_engulfing)
                && current.close > ema20
            {
                signals.push(Signal {
                    index: i,
                    timestamp: current.timestamp,
                    direction: Direction::Short,
                    entry_price: current.close,
                    rsi,
                    deviation_atr,
                    has_pattern: true,
                });
            }
        }

        println!("生成信号数: {}", signals.len());

        // ============================================
        // Step 3: 简单回测（固定止盈止损）
        // ============================================

        let atr_stop_mult = 1.5;
        let target_r = 2.0; // 2R 止盈

        let mut trades = Vec::new();

        for signal in &signals {
            // 获取入场后的 K 线（最多看 24 根 = 24 小时）
            let max_hold = 24;
            let future_candles =
                &candles[signal.index + 1..(signal.index + 1 + max_hold).min(candles.len())];

            if future_candles.is_empty() {
                continue;
            }

            // 计算止损止盈
            let entry_atr = calculate_simple_atr(
                &candles[signal.index.saturating_sub(atr_period)..=signal.index],
                atr_period,
            );

            let (stop_loss, take_profit) = match signal.direction {
                Direction::Long => {
                    let sl = signal.entry_price - entry_atr * atr_stop_mult;
                    let tp = signal.entry_price + (entry_atr * atr_stop_mult * target_r);
                    (sl, tp)
                }
                Direction::Short => {
                    let sl = signal.entry_price + entry_atr * atr_stop_mult;
                    let tp = signal.entry_price - (entry_atr * atr_stop_mult * target_r);
                    (sl, tp)
                }
            };

            // 模拟交易执行
            let mut exit_reason = ExitReason::Timeout;
            let mut exit_price = future_candles.last().unwrap().close;

            for candle in future_candles {
                match signal.direction {
                    Direction::Long => {
                        if candle.low <= stop_loss {
                            exit_reason = ExitReason::StopLoss;
                            exit_price = stop_loss;
                            break;
                        }
                        if candle.high >= take_profit {
                            exit_reason = ExitReason::TakeProfit;
                            exit_price = take_profit;
                            break;
                        }
                    }
                    Direction::Short => {
                        if candle.high >= stop_loss {
                            exit_reason = ExitReason::StopLoss;
                            exit_price = stop_loss;
                            break;
                        }
                        if candle.low <= take_profit {
                            exit_reason = ExitReason::TakeProfit;
                            exit_price = take_profit;
                            break;
                        }
                    }
                }
            }

            // 计算 PnL
            let pnl_pct = match signal.direction {
                Direction::Long => (exit_price - signal.entry_price) / signal.entry_price,
                Direction::Short => (signal.entry_price - exit_price) / signal.entry_price,
            };

            let r_multiple = pnl_pct / (atr_stop_mult * entry_atr / signal.entry_price);

            trades.push(Trade {
                signal: signal.clone(),
                exit_reason,
                exit_price,
                pnl_pct,
                r_multiple,
            });
        }

        // ============================================
        // Step 4: 统计分析
        // ============================================

        if trades.is_empty() {
            println!("❌ 无交易信号，可能需要调整参数");
            return;
        }

        let total_trades = trades.len();
        let wins = trades.iter().filter(|t| t.pnl_pct > 0.0).count();
        let losses = total_trades - wins;
        let win_rate = wins as f64 / total_trades as f64;

        let total_pnl: f64 = trades.iter().map(|t| t.pnl_pct).sum();
        let avg_win: f64 = trades
            .iter()
            .filter(|t| t.pnl_pct > 0.0)
            .map(|t| t.pnl_pct)
            .sum::<f64>()
            / wins.max(1) as f64;
        let avg_loss: f64 = trades
            .iter()
            .filter(|t| t.pnl_pct <= 0.0)
            .map(|t| t.pnl_pct)
            .sum::<f64>()
            / losses.max(1) as f64;

        let avg_r: f64 = trades.iter().map(|t| t.r_multiple).sum::<f64>() / total_trades as f64;

        let max_consecutive_losses = calculate_max_consecutive_losses(&trades);

        // 估算月化指标（假设测试 2 个月数据）
        let months = 2.0; // 根据实际数据调整
        let trades_per_month = total_trades as f64 / months;

        // 假设账户 100u，单笔风险 1%，杠杆 3x
        let account = 100.0;
        let risk_per_trade = 0.01;
        let leverage = 3.0;
        let monthly_pnl = total_pnl * account * leverage / months;

        println!("\n============================================");
        println!("📊 回测结果");
        println!("============================================");
        println!("总交易数: {}", total_trades);
        println!(
            "胜率: {:.1}% ({} 胜 / {} 负)",
            win_rate * 100.0,
            wins,
            losses
        );
        println!("平均盈利: {:.2}%", avg_win * 100.0);
        println!("平均亏损: {:.2}%", avg_loss * 100.0);
        println!("平均 R 倍数: {:.2}R", avg_r);
        println!("最大连败: {} 次", max_consecutive_losses);
        println!("\n估算月化指标:");
        println!("  月交易频次: {:.1} 笔", trades_per_month);
        println!("  月 PnL: {:.2}u (账户 100u, 3x 杠杆)", monthly_pnl);

        // ============================================
        // Step 5: 决策点
        // ============================================

        println!("\n============================================");
        println!("🎯 决策建议");
        println!("============================================");

        if win_rate > 0.55 && trades_per_month > 15.0 && monthly_pnl > 8.0 {
            println!("✅ 策略有潜力！建议升级到生产模式");
            println!("\n下一步:");
            println!("  1. 扩展回测到 6 个月数据");
            println!("  2. 测试 ETH（Tier A 泛化）");
            println!("  3. 参数稳健性测试（RSI 阈值 ±5）");
            println!("  4. 集成到 strategies/ 模块");
            println!("\n  创建文档: docs/plans/TODO_momentum_reversal_1h.md");
        } else if win_rate >= 0.52 && win_rate <= 0.55 {
            println!("⚠️  边缘线，尝试优化:");
            println!("  - 调整 RSI 阈值（30 → 25 或 35）");
            println!("  - 调整偏离度（2 ATR → 1.5 或 2.5）");
            println!("  - 增加 MACD 背离确认");
            println!("  - 调整止盈止损比例");
        } else {
            println!("❌ 当前参数不成立");
            println!("\n可能原因:");
            println!("  - RSI 阈值过于严格（信号太少）");
            println!("  - 止盈止损比例不合理");
            println!("  - 市场环境不适合反转策略（强趋势市）");
            println!("\n建议:");
            println!("  1. 记录失败原因到 docs/exploration_log.md");
            println!("  2. 尝试调整参数");
            println!("  3. 或考虑其他策略方向");
        }

        // 样本量检查
        if total_trades < 20 {
            println!("\n⚠️  警告: 样本量不足（< 20 笔）");
            println!("   建议: 扩展数据到 3 个月，或放宽信号条件");
        }
    }

    // ============================================
    // 辅助函数（简化实现，生产环境应使用 indicators crate）
    // ============================================

    fn calculate_simple_rsi(window: &[CandleItem], period: usize) -> f64 {
        // TODO: 实现或使用 rust_quant_indicators::RsiIndicator
        50.0 // 占位符
    }

    fn calculate_simple_ema(window: &[CandleItem], period: usize) -> f64 {
        // TODO: 实现或使用 rust_quant_indicators::EmaIndicator
        window.last().map(|c| c.close).unwrap_or(0.0) // 占位符
    }

    fn calculate_simple_atr(window: &[CandleItem], period: usize) -> f64 {
        // TODO: 实现或使用 rust_quant_indicators::AtrIndicator
        100.0 // 占位符
    }

    fn detect_hammer_pattern(candle: &CandleItem) -> bool {
        // 简化版锤子线检测
        let body = (candle.close - candle.open).abs();
        let lower_shadow = candle.open.min(candle.close) - candle.low;
        let upper_shadow = candle.high - candle.open.max(candle.close);

        lower_shadow > body * 2.0 && upper_shadow < body * 0.5
    }

    fn detect_engulfing_pattern(window: &[CandleItem]) -> bool {
        if window.len() < 2 {
            return false;
        }

        let prev = &window[window.len() - 2];
        let curr = &window[window.len() - 1];

        // 简化版吞噬形态
        let prev_bullish = prev.close > prev.open;
        let curr_bullish = curr.close > curr.open;

        // 看涨吞噬
        if !prev_bullish && curr_bullish {
            return curr.close > prev.open && curr.open < prev.close;
        }

        // 看跌吞噬
        if prev_bullish && !curr_bullish {
            return curr.open > prev.close && curr.close < prev.open;
        }

        false
    }

    fn calculate_max_consecutive_losses(trades: &[Trade]) -> usize {
        let mut max_consecutive = 0;
        let mut current_consecutive = 0;

        for trade in trades {
            if trade.pnl_pct <= 0.0 {
                current_consecutive += 1;
                max_consecutive = max_consecutive.max(current_consecutive);
            } else {
                current_consecutive = 0;
            }
        }

        max_consecutive
    }

    // ============================================
    // 数据结构
    // ============================================

    #[derive(Debug, Clone)]
    struct Signal {
        index: usize,
        timestamp: i64,
        direction: Direction,
        entry_price: f64,
        rsi: f64,
        deviation_atr: f64,
        has_pattern: bool,
    }

    #[derive(Debug, Clone)]
    enum Direction {
        Long,
        Short,
    }

    #[derive(Debug, Clone)]
    struct Trade {
        signal: Signal,
        exit_reason: ExitReason,
        exit_price: f64,
        pnl_pct: f64,
        r_multiple: f64,
    }

    #[derive(Debug, Clone)]
    enum ExitReason {
        StopLoss,
        TakeProfit,
        Timeout,
    }
}
