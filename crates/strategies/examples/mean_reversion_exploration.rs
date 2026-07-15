use anyhow::Result;
use rust_quant_common::CandleItem;
use sqlx::Row;
use std::env;

/// 均值回归策略 - 探索原型
///
/// 核心逻辑：
/// 1. RSI < 30（超卖）
/// 2. 价格触及布林带下轨
/// 3. 价格 > 200EMA（大趋势过滤）
/// 4. 做多回归到均值
///
/// 目标：胜率 > 55%，回撤 < 20%，夏普 > 1.5

#[derive(Debug, Clone)]
struct Signal {
    index: usize,
    entry_price: f64,
    stop_loss: f64,
    take_profit: f64,
}

#[derive(Debug)]
struct Trade {
    entry_price: f64,
    exit_price: f64,
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
    println!("║           均值回归策略 - 探索原型                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // 1. 加载数据（5000根4H K线，约2年）
    println!("📊 加载历史数据...");
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

    // 2. 计算指标并扫描信号
    println!("🔍 扫描信号...");
    let mut signals = Vec::new();

    let rsi_period = 14;
    let bb_period = 20;
    let bb_std_dev = 2.0;
    let ema_period = 200;
    let atr_period = 14;

    for i in ema_period..candles.len() {
        // 计算RSI
        let rsi = calculate_rsi(&candles[i.saturating_sub(rsi_period + 1)..=i], rsi_period);

        // 计算布林带
        let (bb_upper, bb_middle, bb_lower) = calculate_bollinger_bands(
            &candles[i.saturating_sub(bb_period)..=i],
            bb_period,
            bb_std_dev,
        );

        // 计算200EMA
        let ema200 = calculate_ema(&candles[i.saturating_sub(ema_period)..=i], ema_period);

        // 计算ATR
        let atr = calculate_atr(&candles[i.saturating_sub(atr_period + 1)..=i], atr_period);

        let current = &candles[i];

        // 信号条件
        let is_oversold = rsi < 30.0;
        let touches_lower_band = current.l <= bb_lower * 1.01; // 允许1%误差
        let above_ema200 = current.c > ema200;

        if is_oversold && touches_lower_band && above_ema200 {
            signals.push(Signal {
                index: i,
                entry_price: current.c,
                stop_loss: bb_lower - atr,
                take_profit: bb_middle,
            });
        }
    }

    println!("✅ 发现 {} 个信号\n", signals.len());

    // 3. 回测信号
    println!("📈 执行回测...");
    let trades = backtest_signals(&signals, &candles);

    // 4. 统计分析
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                     回测结果                                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let total_trades = trades.len();
    let winning_trades = trades.iter().filter(|t| t.is_win).count();
    let losing_trades = total_trades - winning_trades;
    let win_rate = winning_trades as f64 / total_trades as f64;

    let total_pnl: f64 = trades.iter().map(|t| t.pnl).sum();
    let avg_win: f64 = trades
        .iter()
        .filter(|t| t.is_win)
        .map(|t| t.pnl)
        .sum::<f64>()
        / winning_trades.max(1) as f64;
    let avg_loss: f64 = trades
        .iter()
        .filter(|t| !t.is_win)
        .map(|t| t.pnl.abs())
        .sum::<f64>()
        / losing_trades.max(1) as f64;

    println!("📊 基础指标:");
    println!("  总交易数: {}", total_trades);
    println!("  盈利交易: {} ({:.1}%)", winning_trades, win_rate * 100.0);
    println!("  亏损交易: {}", losing_trades);
    println!("  胜率: {:.1}%", win_rate * 100.0);

    if win_rate >= 0.55 {
        println!("  ✅✅ 胜率达标！(≥55%)");
    } else if win_rate >= 0.50 {
        println!("  ✅ 胜率接近目标 (50-55%)");
    } else {
        println!("  ❌ 胜率不达标 (<50%)");
    }
    println!();

    println!("💰 盈亏分析:");
    println!("  总盈亏: {:.2}%", total_pnl);
    println!("  平均盈利: {:.2}%", avg_win);
    println!("  平均亏损: {:.2}%", avg_loss);
    println!(
        "  盈亏比: {:.2}",
        if avg_loss > 0.0 {
            avg_win / avg_loss
        } else {
            0.0
        }
    );
    println!();

    // 计算最大回撤
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

    println!("📉 风险指标:");
    println!("  最终资金: {:.2}", equity);
    println!("  最大回撤: {:.2}%", max_dd * 100.0);

    if max_dd < 0.20 {
        println!("  ✅ 回撤控制良好 (<20%)");
    } else if max_dd < 0.30 {
        println!("  ⚠️  回撤一般 (20-30%)");
    } else {
        println!("  ❌ 回撤过大 (>30%)");
    }
    println!();

    // 简化夏普比率
    let returns: Vec<f64> = trades.iter().map(|t| t.pnl / 100.0).collect();
    let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns
        .iter()
        .map(|r| (r - mean_return).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let std_dev = variance.sqrt();
    let sharpe = if std_dev > 0.0 {
        mean_return / std_dev * (12.0_f64).sqrt() // 假设月度
    } else {
        0.0
    };

    println!("📈 夏普比率: {:.2}", sharpe);
    if sharpe > 1.5 {
        println!("  ✅ 优秀 (>1.5)");
    } else if sharpe > 1.0 {
        println!("  ⚠️  一般 (1.0-1.5)");
    } else {
        println!("  ❌ 较差 (<1.0)");
    }
    println!();

    // 决策
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                     决策建议                                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let score = (if win_rate >= 0.55 {
        3
    } else if win_rate >= 0.50 {
        2
    } else {
        0
    }) + (if max_dd < 0.20 {
        2
    } else if max_dd < 0.30 {
        1
    } else {
        0
    }) + (if sharpe > 1.5 {
        2
    } else if sharpe > 1.0 {
        1
    } else {
        0
    });

    if score >= 6 {
        println!("✅✅✅ 策略验证通过！建议升级到生产模式");
        println!("\n下一步：");
        println!("  1. 创建 docs/plans/TODO_mean_reversion.md");
        println!("  2. 集成到 strategies/ 模块");
        println!("  3. 完整分层测试（BTC/ETH/其他币种）");
    } else if score >= 4 {
        println!("⚠️  策略有潜力，建议优化后再测");
        println!("\n优化方向：");
        if win_rate < 0.55 {
            println!("  - 提高胜率：增加更多确认条件");
        }
        if max_dd > 0.20 {
            println!("  - 降低回撤：优化止损逻辑");
        }
        if sharpe < 1.5 {
            println!("  - 提高夏普：改善风险调整后收益");
        }
    } else {
        println!("❌ 当前假设不成立，建议调整");
        println!("\n可能的问题：");
        println!("  - RSI阈值不合适？试试 < 25");
        println!("  - 布林带参数不对？试试 (20, 2.5)");
        println!("  - 200EMA过滤太严？试试去掉或改100EMA");
    }

    println!("\n================================");

    Ok(())
}

// ========== 指标计算函数 ==========

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

// ========== 回测函数 ==========

fn backtest_signals(signals: &[Signal], candles: &[CandleItem]) -> Vec<Trade> {
    let mut trades = Vec::new();

    for signal in signals {
        let entry_price = signal.entry_price;
        let stop_loss = signal.stop_loss;
        let take_profit = signal.take_profit;

        // 查找出场点（最多看后48根K线，约8天）
        let mut exit_price = entry_price;
        let mut is_win = false;

        for i in (signal.index + 1)..(signal.index + 49).min(candles.len()) {
            let candle = &candles[i];

            // 检查止损
            if candle.l <= stop_loss {
                exit_price = stop_loss;
                is_win = false;
                break;
            }

            // 检查止盈
            if candle.h >= take_profit {
                exit_price = take_profit;
                is_win = true;
                break;
            }

            // 最后一根K线强制平仓
            if i == (signal.index + 48).min(candles.len() - 1) {
                exit_price = candle.c;
                is_win = exit_price > entry_price;
            }
        }

        let pnl = ((exit_price - entry_price) / entry_price) * 100.0;
        trades.push(Trade {
            entry_price,
            exit_price,
            pnl,
            is_win,
        });
    }

    trades
}
