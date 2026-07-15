use anyhow::Result;
use rust_quant_common::CandleItem;
use sqlx::Row;
use std::env;

/// Vegas增强策略 - 基于Vegas核心要素的改进版
///
/// 核心改进：
/// 1. 多EMA系统（50/144/200）
/// 2. 多维度信号打分
/// 3. 严格过滤器
/// 4. 完善风控
///
/// 目标：胜率>50%，年化>50%，回撤<20%

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
    println!("║           Vegas增强策略 - 改进版                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");
    println!("基于Vegas核心要素：多EMA+打分系统+严格过滤+完善风控\n");

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

    // 扫描信号
    let mut signals = Vec::new();

    for i in 200..candles.len() {
        // 计算指标
        let ema50 = calculate_ema(&candles[i.saturating_sub(50)..=i], 50);
        let ema144 = calculate_ema(&candles[i.saturating_sub(144)..=i], 144);
        let ema200 = calculate_ema(&candles[i.saturating_sub(200)..=i], 200);
        let atr = calculate_atr(&candles[i.saturating_sub(15)..=i], 14);
        let rsi = calculate_rsi(&candles[i.saturating_sub(15)..=i], 14);

        let current = &candles[i];
        let prev = if i > 0 { &candles[i - 1] } else { current };

        // 计算平均成交量（20根）
        let avg_volume = candles[i.saturating_sub(20)..i]
            .iter()
            .map(|c| c.v)
            .sum::<f64>()
            / 20.0;

        // ===== 信号打分系统 =====
        let mut score = 0.0;
        let mut reasons = Vec::new();

        // 1. EMA排列（核心）
        let ema_aligned = ema50 > ema144 && ema144 > ema200;
        if ema_aligned {
            score += 2.0;
            reasons.push("EMA排列");
        }

        // 2. 价格位置（回调到支撑位）
        let near_ema50 = current.l <= ema50 * 1.02 && current.c > ema50;
        if near_ema50 {
            score += 2.0;
            reasons.push("回调EMA50");
        }

        // 3. 成交量确认（放量）
        let volume_surge = current.v > avg_volume * 1.5;
        if volume_surge {
            score += 1.0;
            reasons.push("放量");
        }

        // 4. RSI动量（不超买不超卖）
        let rsi_ok = rsi > 40.0 && rsi < 70.0;
        if rsi_ok {
            score += 0.5;
        }

        // 5. K线形态（阳线）
        let bullish_candle = current.c > current.o;
        if bullish_candle {
            score += 0.5;
            reasons.push("阳线");
        }

        // 6. 突破确认（收盘价突破EMA144）
        let break_ema144 = prev.c <= ema144 && current.c > ema144;
        if break_ema144 {
            score += 1.5;
            reasons.push("突破EMA144");
        }

        // ===== 过滤器 =====

        // 过滤1：价格必须在200EMA之上（大趋势）
        if current.c <= ema200 {
            continue;
        }

        // 过滤2：价格不能离EMA50太远（追高风险）
        let distance_from_ema50 = (current.c - ema50) / ema50;
        if distance_from_ema50 > 0.05 {
            // 超过5%距离需要额外确认
            if !break_ema144 && !volume_surge {
                continue;
            }
        }

        // 过滤3：RSI不能过低（避免下跌趋势）
        if rsi < 35.0 {
            continue;
        }

        // 过滤4：最低分数要求
        if score < 3.0 {
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
    }

    println!("🔍 发现 {} 个信号\n", signals.len());

    if signals.is_empty() {
        println!("⚠️  没有信号，策略过于严格");
        return Ok(());
    }

    // 回测
    let trades = backtest_signals(&signals, &candles);

    // 统计
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

    // 回撤
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

    // 夏普
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

    // 决策
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
        println!("🎉🎉🎉 所有目标达成！策略验证通过");
        println!("\n下一步：升级到生产模式（Gate 5-8）");
    } else if score >= 3 {
        println!("✅✅ 策略接近目标，值得继续优化");
        println!("\n还需改进的指标：");
        if !wr_ok {
            println!("  - 胜率");
        }
        if !annual_ok {
            println!("  - 年化收益");
        }
        if !dd_ok {
            println!("  - 回撤控制");
        }
        if !sharpe_ok {
            println!("  - 夏普比率");
        }
    } else {
        println!("⚠️  策略仍需大幅改进");
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

        // 多级止盈
        let tp1 = entry_price + atr * 2.0;
        let tp2 = entry_price + atr * 4.0;
        let tp3 = entry_price + atr * 6.0;

        let mut position_size = 1.0;
        let mut total_pnl = 0.0;
        let mut max_profit_rate = 0.0;

        for i in (signal.index + 1)..(signal.index + 121).min(candles.len()) {
            let candle = &candles[i];
            let profit_rate = (candle.c - entry_price) / entry_price;

            // 追踪最大利润
            if profit_rate > max_profit_rate {
                max_profit_rate = profit_rate;
            }

            // 止损
            if candle.l <= stop_loss {
                let pnl = ((stop_loss - entry_price) / entry_price) * 100.0 * position_size;
                total_pnl += pnl;
                break;
            }

            // 保本止损（盈利超过2%后）
            if max_profit_rate > 0.02 && stop_loss < entry_price {
                stop_loss = entry_price;
            }

            // 移动止损（盈利超过5%后，保护50%利润）
            if max_profit_rate > 0.05 {
                let trailing = entry_price + (candle.c - entry_price) * 0.5;
                if trailing > stop_loss {
                    stop_loss = trailing;
                }
            }

            // 分批止盈
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

            // EMA反转出场
            if i > signal.index + 12 {
                let ema50 = calculate_ema(&candles[i.saturating_sub(50)..=i], 50);
                if candle.c < ema50 && position_size > 0.0 {
                    let pnl = ((candle.c - entry_price) / entry_price) * 100.0 * position_size;
                    total_pnl += pnl;
                    break;
                }
            }

            // 超时平仓
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
