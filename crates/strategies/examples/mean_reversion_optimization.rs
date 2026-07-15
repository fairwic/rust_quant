use anyhow::Result;
use rust_quant_common::CandleItem;
use sqlx::Row;
use std::env;

/// 均值回归策略 - 参数优化测试
///
/// 测试3个方向：
/// 1. 更严格RSI（<25）
/// 2. 更宽布林带（2.5倍标准差）
/// 3. 去掉200EMA过滤

#[derive(Debug, Clone)]
struct Signal {
    index: usize,
    entry_price: f64,
    stop_loss: f64,
    take_profit: f64,
}

#[derive(Debug)]
struct Trade {
    pnl: f64,
    is_win: bool,
}

#[derive(Debug, Clone)]
struct TestConfig {
    name: &'static str,
    rsi_threshold: f64,
    bb_std_dev: f64,
    use_ema_filter: bool,
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
    println!("║           均值回归策略 - 参数优化                             ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // 加载数据
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

    // 测试配置
    let configs = vec![
        TestConfig {
            name: "基线（RSI<30, BB 2.0, 200EMA）",
            rsi_threshold: 30.0,
            bb_std_dev: 2.0,
            use_ema_filter: true,
        },
        TestConfig {
            name: "更严格RSI（<25）",
            rsi_threshold: 25.0,
            bb_std_dev: 2.0,
            use_ema_filter: true,
        },
        TestConfig {
            name: "更宽布林带（2.5倍）",
            rsi_threshold: 30.0,
            bb_std_dev: 2.5,
            use_ema_filter: true,
        },
        TestConfig {
            name: "去掉200EMA过滤",
            rsi_threshold: 30.0,
            bb_std_dev: 2.0,
            use_ema_filter: false,
        },
        TestConfig {
            name: "组合优化（RSI<25 + BB 2.5）",
            rsi_threshold: 25.0,
            bb_std_dev: 2.5,
            use_ema_filter: true,
        },
        TestConfig {
            name: "激进版（RSI<25，无EMA）",
            rsi_threshold: 25.0,
            bb_std_dev: 2.0,
            use_ema_filter: false,
        },
    ];

    let mut results = Vec::new();

    for config in configs {
        let (signals, trades) = run_backtest(&candles, &config);

        let win_rate = if trades.len() > 0 {
            trades.iter().filter(|t| t.is_win).count() as f64 / trades.len() as f64
        } else {
            0.0
        };

        let total_pnl: f64 = trades.iter().map(|t| t.pnl).sum();

        let avg_win = if trades.iter().any(|t| t.is_win) {
            trades
                .iter()
                .filter(|t| t.is_win)
                .map(|t| t.pnl)
                .sum::<f64>()
                / trades.iter().filter(|t| t.is_win).count() as f64
        } else {
            0.0
        };

        let avg_loss = if trades.iter().any(|t| !t.is_win) {
            trades
                .iter()
                .filter(|t| !t.is_win)
                .map(|t| t.pnl.abs())
                .sum::<f64>()
                / trades.iter().filter(|t| !t.is_win).count() as f64
        } else {
            0.0
        };

        let profit_factor = if avg_loss > 0.0 {
            avg_win / avg_loss
        } else {
            0.0
        };

        // 计算回撤
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

        results.push((
            config.name,
            signals.len(),
            trades.len(),
            win_rate,
            total_pnl,
            profit_factor,
            max_dd,
            equity,
        ));
    }

    // 按胜率排序
    results.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                  参数优化结果（按胜率排序）                    ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!(
        "{:<35} {:<8} {:<8} {:<10} {:<10} {:<10} {:<12}",
        "配置", "信号数", "交易数", "胜率", "总盈亏", "盈亏比", "最大回撤"
    );
    println!("{}", "-".repeat(100));

    for (name, signals, trades, wr, pnl, pf, dd, _equity) in &results {
        let wr_marker = if *wr >= 0.55 {
            "✅✅"
        } else if *wr >= 0.50 {
            "✅"
        } else {
            "  "
        };

        println!(
            "{} {:<33} {:<8} {:<8} {:<10.1}% {:<10.2}% {:<10.2} {:<12.2}%",
            wr_marker,
            name,
            signals,
            trades,
            wr * 100.0,
            pnl,
            pf,
            dd * 100.0
        );
    }

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                     最佳配置分析                               ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    if let Some((best_name, signals, trades, wr, pnl, pf, dd, equity)) = results.first() {
        println!("🏆 最佳配置: {}", best_name);
        println!("  信号数: {}", signals);
        println!("  交易数: {}", trades);
        println!("  胜率: {:.1}%", wr * 100.0);
        println!("  总盈亏: {:.2}%", pnl);
        println!("  盈亏比: {:.2}", pf);
        println!("  最大回撤: {:.2}%", dd * 100.0);
        println!("  最终资金: ${:.2}\n", equity);

        if *wr >= 0.55 && *dd < 0.20 {
            println!("✅✅✅ 策略验证通过！");
            println!("  ✅ 胜率 ≥ 55%");
            println!("  ✅ 回撤 < 20%");
            println!("\n建议：升级到生产模式（Gate 5-8）");
        } else if *wr >= 0.50 && *dd < 0.25 {
            println!("✅ 策略有潜力，接近目标");
            println!("\n建议：");
            if *wr < 0.55 {
                println!("  - 继续优化胜率（目标55%）");
            }
            if *dd > 0.20 {
                println!("  - 优化回撤控制（目标<20%）");
            }
        } else {
            println!("⚠️  策略需要进一步优化");
            println!("\n可能方向：");
            println!("  - 添加更多确认条件（如成交量）");
            println!("  - 调整止盈止损比例");
            println!("  - 考虑其他入场时机");
        }
    }

    println!("\n================================");

    Ok(())
}

fn run_backtest(candles: &[CandleItem], config: &TestConfig) -> (Vec<Signal>, Vec<Trade>) {
    let mut signals = Vec::new();

    let rsi_period = 14;
    let bb_period = 20;
    let ema_period = 200;
    let atr_period = 14;

    for i in ema_period..candles.len() {
        let rsi = calculate_rsi(&candles[i.saturating_sub(rsi_period + 1)..=i], rsi_period);
        let (_bb_upper, bb_middle, bb_lower) = calculate_bollinger_bands(
            &candles[i.saturating_sub(bb_period)..=i],
            bb_period,
            config.bb_std_dev,
        );
        let ema200 = calculate_ema(&candles[i.saturating_sub(ema_period)..=i], ema_period);
        let atr = calculate_atr(&candles[i.saturating_sub(atr_period + 1)..=i], atr_period);

        let current = &candles[i];

        let is_oversold = rsi < config.rsi_threshold;
        let touches_lower_band = current.l <= bb_lower * 1.01;
        let above_ema200 = !config.use_ema_filter || current.c > ema200;

        if is_oversold && touches_lower_band && above_ema200 {
            signals.push(Signal {
                index: i,
                entry_price: current.c,
                stop_loss: bb_lower - atr,
                take_profit: bb_middle,
            });
        }
    }

    let trades = backtest_signals(&signals, candles);
    (signals, trades)
}

fn backtest_signals(signals: &[Signal], candles: &[CandleItem]) -> Vec<Trade> {
    let mut trades = Vec::new();

    for signal in signals {
        let entry_price = signal.entry_price;
        let stop_loss = signal.stop_loss;
        let take_profit = signal.take_profit;

        let mut exit_price = entry_price;
        let mut is_win = false;

        for i in (signal.index + 1)..(signal.index + 49).min(candles.len()) {
            let candle = &candles[i];

            if candle.l <= stop_loss {
                exit_price = stop_loss;
                is_win = false;
                break;
            }

            if candle.h >= take_profit {
                exit_price = take_profit;
                is_win = true;
                break;
            }

            if i == (signal.index + 48).min(candles.len() - 1) {
                exit_price = candle.c;
                is_win = exit_price > entry_price;
            }
        }

        let pnl = ((exit_price - entry_price) / entry_price) * 100.0;
        trades.push(Trade { pnl, is_win });
    }

    trades
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

    let rs = avg_gain / avg_loss;
    100.0 - (100.0 / (1.0 + rs))
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

    let upper = mean + std_dev_mult * std_dev;
    let lower = mean - std_dev_mult * std_dev;

    (upper, mean, lower)
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
