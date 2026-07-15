use anyhow::Result;
use rust_quant_common::CandleItem;
use sqlx::Row;
use std::env;

/// 类Vegas策略 - 探索原型
///
/// 参考Vegas成功要素：
/// 1. EMA多重过滤（144/169/576/676）
/// 2. 趋势跟随（价格在EMA之上做多）
/// 3. 回踩入场（等待回调到支撑位）
/// 4. 严格止损（ATR倍数）
/// 5. 分批止盈
///
/// 简化版核心逻辑：
/// - EMA50/200双均线系统
/// - 价格在EMA200之上，回踩EMA50入场
/// - 止损：EMA50 - 2ATR
/// - 止盈：分批（2ATR / 4ATR / 6ATR）

#[derive(Debug, Clone)]
struct Signal {
    index: usize,
    entry_price: f64,
    stop_loss: f64,
    ema50: f64,
    atr: f64,
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
    println!("║           类Vegas策略 - 探索原型                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");
    println!("参考Vegas ETH 4H基准: 胜率56%, 回撤14.8%, 夏普2.54\n");

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
        let ema50 = calculate_ema(&candles[i.saturating_sub(50)..=i], 50);
        let ema200 = calculate_ema(&candles[i.saturating_sub(200)..=i], 200);
        let atr = calculate_atr(&candles[i.saturating_sub(15)..=i], 14);

        let current = &candles[i];
        let prev = &candles[i - 1];

        // 核心条件：
        // 1. 价格在200EMA之上（大趋势向上）
        // 2. 价格回踩到50EMA附近（±3%）
        // 3. K线收盘价守住50EMA（不跌破）
        // 4. 前一根K线的低点触及或接近50EMA

        let above_200ema = current.c > ema200;
        let near_50ema = current.c >= ema50 * 0.97 && current.c <= ema50 * 1.03;
        let prev_touched_50ema = prev.l <= ema50 * 1.02;
        let bullish_close = current.c > current.o; // 阳线

        if above_200ema && near_50ema && prev_touched_50ema && bullish_close {
            signals.push(Signal {
                index: i,
                entry_price: current.c,
                stop_loss: ema50 - atr * 2.0,
                ema50,
                atr,
            });
        }
    }

    println!("🔍 发现 {} 个信号\n", signals.len());

    // 回测
    let trades = backtest_signals(&signals, &candles);

    // 统计
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                     回测结果                                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let total_trades = trades.len();
    if total_trades == 0 {
        println!("⚠️  没有产生交易");
        return Ok(());
    }

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

    // 夏普比率（简化）
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

    // 与Vegas对比
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                  与Vegas基准对比                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!(
        "{:<20} {:<15} {:<15} {:<10}",
        "指标", "当前策略", "Vegas基准", "状态"
    );
    println!("{}", "-".repeat(65));

    let wr_status = if win_rate >= 0.56 {
        "✅"
    } else if win_rate >= 0.50 {
        "⚠️"
    } else {
        "❌"
    };
    let dd_status = if max_dd < 0.15 {
        "✅"
    } else if max_dd < 0.20 {
        "⚠️"
    } else {
        "❌"
    };
    let sharpe_status = if sharpe > 2.5 {
        "✅"
    } else if sharpe > 2.0 {
        "⚠️"
    } else if sharpe > 1.5 {
        "⚪"
    } else {
        "❌"
    };
    let annual_status = if annual_return > 200.0 {
        "✅"
    } else if annual_return > 100.0 {
        "⚠️"
    } else if annual_return > 20.0 {
        "⚪"
    } else {
        "❌"
    };

    println!(
        "{:<20} {:<15.2}% {:<15} {:<10}",
        "胜率",
        win_rate * 100.0,
        "56.04%",
        wr_status
    );
    println!(
        "{:<20} {:<15.2}% {:<15} {:<10}",
        "最大回撤",
        max_dd * 100.0,
        "14.80%",
        dd_status
    );
    println!(
        "{:<20} {:<15.2} {:<15} {:<10}",
        "夏普比率", sharpe, "2.54", sharpe_status
    );
    println!(
        "{:<20} {:<15.2}% {:<15} {:<10}",
        "年化收益", annual_return, "~240%", annual_status
    );

    println!("\n💡 结论:");
    let score = (if win_rate >= 0.50 { 1 } else { 0 })
        + (if max_dd < 0.20 { 1 } else { 0 })
        + (if sharpe > 1.5 { 1 } else { 0 })
        + (if annual_return > 20.0 { 1 } else { 0 });

    if score >= 3 && win_rate >= 0.50 && annual_return > 20.0 {
        println!("  🎉 策略接近Vegas水平，可以继续优化！");
    } else if score >= 2 {
        println!("  ⚠️  策略有潜力，但与Vegas仍有差距");
    } else {
        println!("  ❌ 策略表现不佳，需要重新设计");
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
        let ema50 = signal.ema50;

        let tp1 = entry_price + atr * 2.0; // 第一目标
        let tp2 = entry_price + atr * 4.0; // 第二目标
        let tp3 = entry_price + atr * 6.0; // 第三目标

        let mut position_size = 1.0;
        let mut total_pnl = 0.0;

        for i in (signal.index + 1)..(signal.index + 121).min(candles.len()) {
            let candle = &candles[i];

            // 检查止损
            if candle.l <= stop_loss {
                let pnl = ((stop_loss - entry_price) / entry_price) * 100.0 * position_size;
                total_pnl += pnl;
                break;
            }

            // 分批止盈
            if position_size == 1.0 && candle.h >= tp1 {
                let pnl = ((tp1 - entry_price) / entry_price) * 100.0 * 0.33;
                total_pnl += pnl;
                position_size = 0.67;
                stop_loss = entry_price; // 移到保本
                continue;
            }

            if position_size > 0.6 && position_size < 0.7 && candle.h >= tp2 {
                let pnl = ((tp2 - entry_price) / entry_price) * 100.0 * 0.33;
                total_pnl += pnl;
                position_size = 0.34;
                stop_loss = tp1; // 移到TP1
                continue;
            }

            if position_size > 0.3 && candle.h >= tp3 {
                let pnl = ((tp3 - entry_price) / entry_price) * 100.0 * 0.34;
                total_pnl += pnl;
                position_size = 0.0;
                break;
            }

            // 动态追踪（价格回到EMA50下方平仓）
            if i > signal.index + 10 {
                let current_ema50 = calculate_ema(&candles[i.saturating_sub(50)..=i], 50);
                if candle.c < current_ema50 && position_size > 0.0 {
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
        let high = candles[i].h;
        let low = candles[i].l;
        let prev_close = candles[i - 1].c;
        let tr = (high - low)
            .max((high - prev_close).abs())
            .max((low - prev_close).abs());
        sum += tr;
    }
    sum / period as f64
}
