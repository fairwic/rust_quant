use anyhow::Result;
use rust_quant_common::CandleItem;
use sqlx::Row;
use std::env;

/// 双均线交叉策略 - 探索原型
///
/// 核心逻辑：
/// 1. EMA50上穿EMA200 → 做多信号
/// 2. 价格回踩EMA50不跌破 → 入场
/// 3. EMA50下穿EMA200 → 平仓
/// 4. 固定止损：2 ATR
///
/// 目标：年化>20%，胜率>45%，回撤<20%

#[derive(Debug, Clone)]
struct Signal {
    index: usize,
    entry_price: f64,
    stop_loss: f64,
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
    println!("║           双均线交叉策略 - 探索原型                            ║");
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

    // 扫描信号
    let mut signals = Vec::new();
    let ema_short = 50;
    let ema_long = 200;
    let atr_period = 14;

    let mut in_uptrend = false;

    for i in ema_long..candles.len() {
        let ema50 = calculate_ema(&candles[i.saturating_sub(ema_short)..=i], ema_short);
        let ema200 = calculate_ema(&candles[i.saturating_sub(ema_long)..=i], ema_long);
        let atr = calculate_atr(&candles[i.saturating_sub(atr_period + 1)..=i], atr_period);

        let current = &candles[i];

        // 检测金叉
        if i > ema_long {
            let prev_ema50 = calculate_ema(&candles[i.saturating_sub(ema_short) - 1..i], ema_short);
            let prev_ema200 = calculate_ema(&candles[i.saturating_sub(ema_long) - 1..i], ema_long);

            // 金叉：EMA50上穿EMA200
            if prev_ema50 <= prev_ema200 && ema50 > ema200 {
                in_uptrend = true;
            }

            // 死叉：EMA50下穿EMA200
            if prev_ema50 >= prev_ema200 && ema50 < ema200 {
                in_uptrend = false;
            }
        }

        // 在上升趋势中，价格回踩EMA50附近入场
        if in_uptrend && current.l <= ema50 * 1.02 && current.c > ema50 {
            signals.push(Signal {
                index: i,
                entry_price: current.c,
                stop_loss: ema50 - atr * 2.0,
            });

            // 避免重复入场
            in_uptrend = false;
        }
    }

    println!("🔍 发现 {} 个交叉信号\n", signals.len());

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

    let years = 2.28;
    let annual_return = ((equity / 100.0).powf(1.0 / years) - 1.0) * 100.0;

    println!("📊 基础指标:");
    println!("  交易数: {}", total_trades);
    println!("  胜率: {:.1}%", win_rate * 100.0);
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
    println!("  最终资金: ${:.2}", equity);
    println!();

    // 决策
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                     决策建议                                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let meets_annual = annual_return > 20.0;
    let meets_winrate = win_rate >= 0.45;
    let meets_drawdown = max_dd < 0.20;

    println!("目标达成情况:");
    println!(
        "  {} 年化收益 > 20% (当前: {:.1}%)",
        if meets_annual { "✅" } else { "❌" },
        annual_return
    );
    println!(
        "  {} 胜率 ≥ 45% (当前: {:.1}%)",
        if meets_winrate { "✅" } else { "❌" },
        win_rate * 100.0
    );
    println!(
        "  {} 回撤 < 20% (当前: {:.1}%)",
        if meets_drawdown { "✅" } else { "❌" },
        max_dd * 100.0
    );
    println!();

    let score = (meets_annual as u8) + (meets_winrate as u8) + (meets_drawdown as u8);

    if score == 3 {
        println!("🎉🎉🎉 策略验证通过！所有目标达成");
        println!("\n下一步：升级到生产模式");
    } else if score >= 2 {
        println!("✅ 策略接近目标，值得继续优化");
    } else {
        println!("❌ 策略不符合要求");
        println!("\n建议：调整策略或尝试其他方向");
    }

    println!("\n================================");

    Ok(())
}

fn backtest_signals(signals: &[Signal], candles: &[CandleItem]) -> Vec<Trade> {
    let mut trades = Vec::new();

    for signal in signals {
        let entry_price = signal.entry_price;
        let stop_loss = signal.stop_loss;

        let mut exit_price = entry_price;
        let mut is_win = false;

        // 查找出场点（检测死叉或止损）
        for i in (signal.index + 1)..(signal.index + 241).min(candles.len()) {
            // 最多持仓40天
            let candle = &candles[i];

            // 检查止损
            if candle.l <= stop_loss {
                exit_price = stop_loss;
                is_win = false;
                break;
            }

            // 检测死叉（EMA50下穿EMA200）
            if i > signal.index + 50 {
                // 至少持仓50根K线后才检测死叉
                let ema50 = calculate_ema(&candles[i.saturating_sub(50)..=i], 50);
                let ema200 = calculate_ema(&candles[i.saturating_sub(200)..=i], 200);
                let prev_ema50 = calculate_ema(&candles[i.saturating_sub(50) - 1..i], 50);
                let prev_ema200 = calculate_ema(&candles[i.saturating_sub(200) - 1..i], 200);

                if prev_ema50 >= prev_ema200 && ema50 < ema200 {
                    exit_price = candle.c;
                    is_win = exit_price > entry_price;
                    break;
                }
            }

            // 超时平仓
            if i == (signal.index + 240).min(candles.len() - 1) {
                exit_price = candle.c;
                is_win = exit_price > entry_price;
            }
        }

        let pnl = ((exit_price - entry_price) / entry_price) * 100.0;
        trades.push(Trade { pnl, is_win });
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
