use anyhow::Result;
use rust_quant_common::CandleItem;
use sqlx::Row;
use std::env;

/// Vegas增强策略 v2 - 更严格的过滤
///
/// 核心改进：
/// 1. 提高最低分数要求（5分）
/// 2. 增加冷却期（避免连续开仓）
/// 3. 更严格的EMA距离控制
/// 4. 布林带过滤（避免追高）

#[derive(Debug, Clone)]
struct Signal {
    index: usize,
    entry_price: f64,
    stop_loss: f64,
    atr: f64,
    score: f64,
}

#[derive(Debug)]
struct Trade {
    pnl: f64,
    is_win: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:example@localhost:5432/quant_core".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         Vegas增强策略 v2 - 严格过滤版                         ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let rows = sqlx::query(
        r#"
        SELECT ts, o, h, l, c, vol, confirm
        FROM "btc-usdt-swap_candles_4h"
        WHERE confirm = '1'
        ORDER BY ts DESC
        LIMIT 5000
        "#,
    )
    .fetch_all(&pool)
    .await?;

    let mut candles: Vec<CandleItem> = Vec::new();
    for row in rows.iter().rev() {
        let ts: i64 = row.try_get("ts")?;
        candles.push(CandleItem {
            ts,
            o: row.try_get::<String, _>("o")?.parse::<f64>()?,
            h: row.try_get::<String, _>("h")?.parse::<f64>()?,
            l: row.try_get::<String, _>("l")?.parse::<f64>()?,
            c: row.try_get::<String, _>("c")?.parse::<f64>()?,
            v: row.try_get::<String, _>("vol")?.parse::<f64>()?,
            confirm: row.try_get::<String, _>("confirm")?.parse::<i32>()?,
        });
    }

    println!("✅ 加载 {} 根K线\n", candles.len());

    let mut signals = Vec::new();
    let mut last_signal_index = 0usize;
    let cooldown = 12; // 12根K线（2天）冷却期

    for i in 200..candles.len() {
        // 冷却期检查
        if i - last_signal_index < cooldown {
            continue;
        }

        // 计算指标
        let ema50 = calculate_ema(&candles[i.saturating_sub(50)..=i], 50);
        let ema144 = calculate_ema(&candles[i.saturating_sub(144)..=i], 144);
        let ema200 = calculate_ema(&candles[i.saturating_sub(200)..=i], 200);
        let atr = calculate_atr(&candles[i.saturating_sub(15)..=i], 14);
        let rsi = calculate_rsi(&candles[i.saturating_sub(15)..=i], 14);

        // 布林带
        let (bb_upper, bb_middle, bb_lower) =
            calculate_bollinger_bands(&candles[i.saturating_sub(20)..=i], 20, 2.0);

        let current = &candles[i];
        let prev = if i > 0 { &candles[i - 1] } else { current };

        let avg_volume = candles[i.saturating_sub(20)..i]
            .iter()
            .map(|c| c.v)
            .sum::<f64>()
            / 20.0;

        // ===== 信号打分系统 =====
        let mut score = 0.0;

        // 1. EMA完美排列（必须）
        let ema_aligned = ema50 > ema144 && ema144 > ema200;
        if !ema_aligned {
            continue; // 直接跳过
        }
        score += 2.0;

        // 2. 价格回调到EMA50附近（关键）
        let near_ema50 = current.l <= ema50 * 1.015 && current.c > ema50 * 0.995;
        if near_ema50 {
            score += 3.0;
        }

        // 3. 成交量放大（重要）
        let volume_surge = current.v > avg_volume * 1.8;
        if volume_surge {
            score += 1.5;
        }

        // 4. RSI健康区间（50-70）
        let rsi_healthy = rsi > 50.0 && rsi < 70.0;
        if rsi_healthy {
            score += 1.0;
        }

        // 5. 强势阳线
        let strong_bullish =
            current.c > current.o && (current.c - current.o) / (current.h - current.l) > 0.6;
        if strong_bullish {
            score += 1.0;
        }

        // 6. 布林带位置（不在上轨附近）
        let not_overbought = current.c < bb_upper * 0.98;
        if not_overbought {
            score += 0.5;
        } else {
            continue; // 过滤掉超买区域
        }

        // 7. EMA50向上趋势
        if i >= 5 {
            let prev_ema50 = calculate_ema(&candles[i.saturating_sub(50) - 5..i - 5], 50);
            if ema50 > prev_ema50 * 1.002 {
                score += 1.0;
            }
        }

        // ===== 严格过滤器 =====

        // 过滤1：价格必须在200EMA之上且有足够距离
        if current.c <= ema200 * 1.01 {
            continue;
        }

        // 过滤2：价格不能离EMA50太远
        let distance_from_ema50 = (current.c - ema50) / ema50;
        if distance_from_ema50 > 0.03 {
            continue;
        }

        // 过滤3：RSI不能太低
        if rsi < 45.0 {
            continue;
        }

        // 过滤4：ATR不能太小（流动性）
        if atr / current.c < 0.01 {
            continue;
        }

        // 过滤5：最低分数要求（提高到5分）
        if score < 5.0 {
            continue;
        }

        // 过滤6：前一根K线不能是大阴线
        if prev.c < prev.o && (prev.o - prev.c) / (prev.h - prev.l) > 0.7 {
            continue;
        }

        // 生成信号
        signals.push(Signal {
            index: i,
            entry_price: current.c,
            stop_loss: ema50 - atr * 2.0,
            atr,
            score,
        });
        last_signal_index = i;
    }

    println!("🔍 发现 {} 个信号\n", signals.len());

    if signals.is_empty() {
        println!("⚠️  没有信号，过滤太严格");
        return Ok(());
    }

    let trades = backtest_signals(&signals, &candles);

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                     回测结果                                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let total_trades = trades.len();
    let winning = trades.iter().filter(|t| t.is_win).count();
    let win_rate = winning as f64 / total_trades as f64;
    let total_pnl: f64 = trades.iter().map(|t| t.pnl).sum();

    let avg_win = if winning > 0 {
        trades
            .iter()
            .filter(|t| t.is_win)
            .map(|t| t.pnl)
            .sum::<f64>()
            / winning as f64
    } else {
        0.0
    };

    let avg_loss = if total_trades > winning {
        trades
            .iter()
            .filter(|t| !t.is_win)
            .map(|t| t.pnl.abs())
            .sum::<f64>()
            / (total_trades - winning) as f64
    } else {
        0.0
    };

    let profit_factor = if avg_loss > 0.0 {
        avg_win / avg_loss
    } else {
        0.0
    };

    let mut equity = 100.0;
    let mut peak = 100.0;
    let mut max_dd = 0.0;

    for trade in &trades {
        equity += equity * trade.pnl / 100.0;
        if equity > peak {
            peak = equity;
        }
        let dd = (peak - equity) / peak;
        if dd > max_dd {
            max_dd = dd;
        }
    }

    let returns: Vec<f64> = trades.iter().map(|t| t.pnl / 100.0).collect();
    let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns
        .iter()
        .map(|r| (r - mean_return).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let std_dev = variance.sqrt();
    let sharpe = if std_dev > 0.0 {
        mean_return / std_dev * (12.0_f64).sqrt()
    } else {
        0.0
    };

    let years = 2.28;
    let annual_return = ((equity / 100.0).powf(1.0 / years) - 1.0) * 100.0;

    println!("📊 基础指标:");
    println!("  交易数: {}", total_trades);
    println!("  胜率: {:.2}%", win_rate * 100.0);
    println!("  总盈亏: {:.2}%", total_pnl);
    println!("  年化收益: {:.2}%", annual_return);
    println!();

    println!("💰 盈亏分析:");
    println!("  平均盈利: {:.2}%", avg_win);
    println!("  平均亏损: {:.2}%", avg_loss);
    println!("  盈亏比: {:.2}", profit_factor);
    println!();

    println!("📉 风险指标:");
    println!("  最大回撤: {:.2}%", max_dd * 100.0);
    println!("  夏普比率: {:.2}", sharpe);
    println!("  最终资金: ${:.2}", equity);
    println!();

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                  目标达成评估                                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let wr_ok = win_rate >= 0.50;
    let annual_ok = annual_return > 50.0;
    let dd_ok = max_dd < 0.20;
    let sharpe_ok = sharpe > 1.5;

    println!(
        "  {} 胜率 ≥ 50% (当前: {:.1}%)",
        if wr_ok { "✅" } else { "❌" },
        win_rate * 100.0
    );
    println!(
        "  {} 年化 > 50% (当前: {:.1}%)",
        if annual_ok { "✅" } else { "❌" },
        annual_return
    );
    println!(
        "  {} 回撤 < 20% (当前: {:.1}%)",
        if dd_ok { "✅" } else { "❌" },
        max_dd * 100.0
    );
    println!(
        "  {} 夏普 > 1.5 (当前: {:.2})",
        if sharpe_ok { "✅" } else { "❌" },
        sharpe
    );
    println!();

    let score = (wr_ok as u8) + (annual_ok as u8) + (dd_ok as u8) + (sharpe_ok as u8);

    if score == 4 {
        println!("🎉🎉🎉 所有目标达成！");
    } else if score >= 2 {
        println!("✅ 策略有明显改进");
        println!("\n对比Vegas基准（胜率56%，年化~240%，回撤14.8%，夏普2.54）：");
        println!("  仍有差距，但方向正确");
    } else {
        println!("❌ 需要继续调整");
    }

    println!("\n================================");

    Ok(())
}

fn backtest_signals(signals: &[Signal], candles: &[CandleItem]) -> Vec<Trade> {
    let mut trades = Vec::new();

    for signal in signals {
        let entry_price = signal.entry_price;
        let mut stop_loss = signal.stop_loss;
        let atr = signal.atr;

        let tp1 = entry_price + atr * 3.0;
        let tp2 = entry_price + atr * 5.0;
        let tp3 = entry_price + atr * 8.0;

        let mut position_size = 1.0;
        let mut total_pnl = 0.0;
        let mut max_profit_rate = 0.0;

        for i in (signal.index + 1)..(signal.index + 121).min(candles.len()) {
            let candle = &candles[i];
            let profit_rate = (candle.c - entry_price) / entry_price;

            if profit_rate > max_profit_rate {
                max_profit_rate = profit_rate;
            }

            if candle.l <= stop_loss {
                let pnl = ((stop_loss - entry_price) / entry_price) * 100.0 * position_size;
                total_pnl += pnl;
                break;
            }

            if max_profit_rate > 0.015 && stop_loss < entry_price {
                stop_loss = entry_price;
            }

            if max_profit_rate > 0.04 {
                let trailing = entry_price + (candle.c - entry_price) * 0.6;
                if trailing > stop_loss {
                    stop_loss = trailing;
                }
            }

            if position_size == 1.0 && candle.h >= tp1 {
                let pnl = ((tp1 - entry_price) / entry_price) * 100.0 * 0.33;
                total_pnl += pnl;
                position_size = 0.67;
                stop_loss = entry_price.max(stop_loss);
                continue;
            }

            if position_size > 0.6 && candle.h >= tp2 {
                let pnl = ((tp2 - entry_price) / entry_price) * 100.0 * 0.33;
                total_pnl += pnl;
                position_size = 0.34;
                stop_loss = tp1.max(stop_loss);
                continue;
            }

            if position_size > 0.3 && candle.h >= tp3 {
                let pnl = ((tp3 - entry_price) / entry_price) * 100.0 * 0.34;
                total_pnl += pnl;
                position_size = 0.0;
                break;
            }

            if i > signal.index + 12 {
                let ema50 = calculate_ema(&candles[i.saturating_sub(50)..=i], 50);
                if candle.c < ema50 && position_size > 0.0 {
                    let pnl = ((candle.c - entry_price) / entry_price) * 100.0 * position_size;
                    total_pnl += pnl;
                    break;
                }
            }

            if i == (signal.index + 120).min(candles.len() - 1) && position_size > 0.0 {
                let pnl = ((candle.c - entry_price) / entry_price) * 100.0 * position_size;
                total_pnl += pnl;
                break;
            }
        }

        trades.push(Trade {
            pnl: total_pnl,
            is_win: total_pnl > 0.0,
        });
    }

    trades
}

fn calculate_ema(candles: &[CandleItem], period: usize) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut ema = candles[0].c;
    for candle in &candles[1..] {
        ema = (candle.c - ema) * multiplier + ema;
    }
    ema
}

fn calculate_atr(candles: &[CandleItem], period: usize) -> f64 {
    if candles.len() < period + 1 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 1..=period {
        let tr = (candles[i].h - candles[i].l)
            .max((candles[i].h - candles[i - 1].c).abs())
            .max((candles[i].l - candles[i - 1].c).abs());
        sum += tr;
    }
    sum / period as f64
}

fn calculate_rsi(candles: &[CandleItem], period: usize) -> f64 {
    if candles.len() < period + 1 {
        return 50.0;
    }
    let mut gains = 0.0;
    let mut losses = 0.0;
    for i in 1..=period {
        let change = candles[i].c - candles[i - 1].c;
        if change > 0.0 {
            gains += change;
        } else {
            losses += change.abs();
        }
    }
    let avg_gain = gains / period as f64;
    let avg_loss = losses / period as f64;
    if avg_loss == 0.0 {
        return 100.0;
    }
    100.0 - (100.0 / (1.0 + avg_gain / avg_loss))
}

fn calculate_bollinger_bands(
    candles: &[CandleItem],
    period: usize,
    std_dev_mult: f64,
) -> (f64, f64, f64) {
    if candles.len() < period {
        let price = candles.last().unwrap().c;
        return (price, price, price);
    }
    let recent = &candles[candles.len() - period..];
    let sum: f64 = recent.iter().map(|c| c.c).sum();
    let mean = sum / period as f64;
    let variance: f64 = recent.iter().map(|c| (c.c - mean).powi(2)).sum::<f64>() / period as f64;
    let std_dev = variance.sqrt();
    (
        mean + std_dev_mult * std_dev,
        mean,
        mean - std_dev_mult * std_dev,
    )
}
