use anyhow::Result;
use rust_quant_common::CandleItem;
use sqlx::Row;
use std::env;

/// 均值回归策略 - 提高收益优化
///
/// 测试方向：
/// 1. 分批止盈（50%中轨，50%上轨）
/// 2. 放宽条件增加交易数
/// 3. 追踪止损提高盈亏比

#[derive(Debug, Clone)]
struct Signal {
    index: usize,
    entry_price: f64,
    stop_loss: f64,
    take_profit_1: f64, // 中轨
    take_profit_2: f64, // 上轨
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
    price_tolerance: f64,    // 布林带触及容差
    use_split_profit: bool,  // 是否分批止盈
    use_trailing_stop: bool, // 是否使用追踪止损
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
    println!("║           均值回归策略 - 提高收益迭代                          ║");
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

    let configs = vec![
        TestConfig {
            name: "基线（RSI<25, BB2.5, 单次止盈）",
            rsi_threshold: 25.0,
            bb_std_dev: 2.5,
            price_tolerance: 1.01,
            use_split_profit: false,
            use_trailing_stop: false,
        },
        TestConfig {
            name: "分批止盈（50%中轨+50%上轨）",
            rsi_threshold: 25.0,
            bb_std_dev: 2.5,
            price_tolerance: 1.01,
            use_split_profit: true,
            use_trailing_stop: false,
        },
        TestConfig {
            name: "放宽条件（RSI<28）",
            rsi_threshold: 28.0,
            bb_std_dev: 2.5,
            price_tolerance: 1.01,
            use_split_profit: false,
            use_trailing_stop: false,
        },
        TestConfig {
            name: "放宽容差（触及1.05倍）",
            rsi_threshold: 25.0,
            bb_std_dev: 2.5,
            price_tolerance: 1.05,
            use_split_profit: false,
            use_trailing_stop: false,
        },
        TestConfig {
            name: "组合1（分批+放宽RSI）",
            rsi_threshold: 28.0,
            bb_std_dev: 2.5,
            price_tolerance: 1.01,
            use_split_profit: true,
            use_trailing_stop: false,
        },
        TestConfig {
            name: "组合2（分批+追踪止损）",
            rsi_threshold: 25.0,
            bb_std_dev: 2.5,
            price_tolerance: 1.01,
            use_split_profit: true,
            use_trailing_stop: true,
        },
        TestConfig {
            name: "激进版（RSI<30 + 分批 + 追踪）",
            rsi_threshold: 30.0,
            bb_std_dev: 2.5,
            price_tolerance: 1.05,
            use_split_profit: true,
            use_trailing_stop: true,
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

        results.push((
            config.name,
            trades.len(),
            win_rate,
            total_pnl,
            annual_return,
            profit_factor,
            max_dd,
            equity,
        ));
    }

    // 按年化收益排序
    results.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap());

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║              优化结果（按年化收益排序）                        ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!(
        "{:<40} {:<8} {:<10} {:<12} {:<12} {:<10} {:<12}",
        "配置", "交易数", "胜率", "总盈亏", "年化收益", "盈亏比", "最大回撤"
    );
    println!("{}", "-".repeat(110));

    for (name, trades, wr, pnl, annual, pf, dd, _equity) in &results {
        let marker = if *annual > 20.0 && *wr >= 0.45 && *dd < 0.20 {
            "✅✅"
        } else if *annual > 15.0 && *wr >= 0.40 {
            "✅"
        } else {
            "  "
        };

        println!(
            "{} {:<38} {:<8} {:<10.1}% {:<12.2}% {:<12.2}% {:<10.2} {:<12.2}%",
            marker,
            name,
            trades,
            wr * 100.0,
            pnl,
            annual,
            pf,
            dd * 100.0
        );
    }

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                     最佳配置分析                               ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    if let Some((best_name, trades, wr, pnl, annual, pf, dd, equity)) = results.first() {
        println!("🏆 最佳配置: {}", best_name);
        println!("  交易数: {}", trades);
        println!("  胜率: {:.1}%", wr * 100.0);
        println!("  总盈亏: {:.2}%", pnl);
        println!("  年化收益: {:.2}%", annual);
        println!("  盈亏比: {:.2}", pf);
        println!("  最大回撤: {:.2}%", dd * 100.0);
        println!("  最终资金: ${:.2}\n", equity);

        // 评分
        let score = (if *annual > 20.0 {
            3
        } else if *annual > 15.0 {
            2
        } else if *annual > 10.0 {
            1
        } else {
            0
        }) + (if *wr >= 0.50 {
            3
        } else if *wr >= 0.45 {
            2
        } else if *wr >= 0.40 {
            1
        } else {
            0
        }) + (if *dd < 0.15 {
            3
        } else if *dd < 0.20 {
            2
        } else if *dd < 0.25 {
            1
        } else {
            0
        }) + (if *pf > 2.5 {
            2
        } else if *pf > 2.0 {
            1
        } else {
            0
        });

        println!("📊 评分: {}/11", score);
        println!();

        if score >= 9 && *annual > 20.0 {
            println!("🎉🎉🎉 策略优化成功！");
            println!("  ✅ 年化收益 > 20%");
            println!("  ✅ 胜率合理");
            println!("  ✅ 回撤可控");
            println!("\n建议：升级到生产模式");
        } else if score >= 6 {
            println!("✅ 策略改善显著，接近目标");
            println!("\n还需优化：");
            if *annual < 20.0 {
                println!("  - 年化收益（当前{:.1}%，目标>20%）", annual);
            }
            if *wr < 0.50 {
                println!("  - 胜率（当前{:.1}%，目标≥50%）", wr * 100.0);
            }
            if *dd > 0.15 {
                println!("  - 回撤（当前{:.1}%，目标<15%）", dd * 100.0);
            }
        } else {
            println!("⚠️  需要进一步优化");
            println!("\n关键指标差距：");
            println!("  年化收益: {:.1}% (目标>20%)", annual);
            println!("  胜率: {:.1}% (目标≥50%)", wr * 100.0);
            println!("  回撤: {:.1}% (目标<20%)", dd * 100.0);
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
        let (bb_upper, bb_middle, bb_lower) = calculate_bollinger_bands(
            &candles[i.saturating_sub(bb_period)..=i],
            bb_period,
            config.bb_std_dev,
        );
        let ema200 = calculate_ema(&candles[i.saturating_sub(ema_period)..=i], ema_period);
        let atr = calculate_atr(&candles[i.saturating_sub(atr_period + 1)..=i], atr_period);

        let current = &candles[i];

        let is_oversold = rsi < config.rsi_threshold;
        let touches_lower_band = current.l <= bb_lower * config.price_tolerance;
        let above_ema200 = current.c > ema200;

        if is_oversold && touches_lower_band && above_ema200 {
            signals.push(Signal {
                index: i,
                entry_price: current.c,
                stop_loss: bb_lower - atr,
                take_profit_1: bb_middle,
                take_profit_2: bb_upper,
            });
        }
    }

    let trades = backtest_signals(&signals, candles, config);
    (signals, trades)
}

fn backtest_signals(signals: &[Signal], candles: &[CandleItem], config: &TestConfig) -> Vec<Trade> {
    let mut trades = Vec::new();

    for signal in signals {
        let entry_price = signal.entry_price;
        let stop_loss = signal.stop_loss;

        if config.use_split_profit {
            // 分批止盈逻辑
            let mut position_size = 1.0;
            let mut total_pnl = 0.0;
            let mut trailing_stop = stop_loss;

            for i in (signal.index + 1)..(signal.index + 49).min(candles.len()) {
                let candle = &candles[i];

                // 检查止损
                if candle.l <= trailing_stop {
                    let pnl = ((trailing_stop - entry_price) / entry_price) * 100.0 * position_size;
                    total_pnl += pnl;
                    break;
                }

                // 第一批止盈（50%仓位）
                if position_size == 1.0 && candle.h >= signal.take_profit_1 {
                    let pnl = ((signal.take_profit_1 - entry_price) / entry_price) * 100.0 * 0.5;
                    total_pnl += pnl;
                    position_size = 0.5;

                    // 追踪止损移到盈亏平衡
                    if config.use_trailing_stop {
                        trailing_stop = entry_price;
                    }
                    continue;
                }

                // 第二批止盈（剩余50%）
                if position_size == 0.5 && candle.h >= signal.take_profit_2 {
                    let pnl = ((signal.take_profit_2 - entry_price) / entry_price) * 100.0 * 0.5;
                    total_pnl += pnl;
                    position_size = 0.0;
                    break;
                }

                // 追踪止损更新
                if config.use_trailing_stop && position_size > 0.0 {
                    let new_trailing = candle.c - (entry_price - stop_loss);
                    if new_trailing > trailing_stop {
                        trailing_stop = new_trailing;
                    }
                }

                // 超时平仓
                if i == (signal.index + 48).min(candles.len() - 1) && position_size > 0.0 {
                    let pnl = ((candle.c - entry_price) / entry_price) * 100.0 * position_size;
                    total_pnl += pnl;
                    break;
                }
            }

            trades.push(Trade {
                pnl: total_pnl,
                is_win: total_pnl > 0.0,
            });
        } else {
            // 单次止盈逻辑（原版）
            let mut exit_price = entry_price;
            let mut is_win = false;

            for i in (signal.index + 1)..(signal.index + 49).min(candles.len()) {
                let candle = &candles[i];

                if candle.l <= stop_loss {
                    exit_price = stop_loss;
                    is_win = false;
                    break;
                }

                if candle.h >= signal.take_profit_1 {
                    exit_price = signal.take_profit_1;
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
