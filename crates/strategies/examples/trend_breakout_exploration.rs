use anyhow::Result;
use rust_quant_common::CandleItem;
use sqlx::Row;
use std::env;

/// 趋势突破策略 - 高收益版本
///
/// 核心逻辑：
/// 1. 价格突破20日高点
/// 2. 价格在200EMA之上（确认上升趋势）
/// 3. 成交量放大（确认突破有效）
/// 4. 动态追踪止损（ATR倍数）
/// 5. 分批止盈（提高胜率）
///
/// 目标：年化>20%，胜率>45%，回撤<20%

#[derive(Debug, Clone)]
struct Signal {
    index: usize,
    entry_price: f64,
    stop_loss: f64,
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
    println!("║           趋势突破策略 - 探索原型                             ║");
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
    let lookback = 20;
    let ema_period = 200;
    let atr_period = 14;

    for i in ema_period..candles.len() {
        let current = &candles[i];

        // 计算20日高点
        let high_20 = candles[i.saturating_sub(lookback)..i]
            .iter()
            .map(|c| c.h)
            .fold(f64::NEG_INFINITY, f64::max);

        // 计算200EMA
        let ema200 = calculate_ema(&candles[i.saturating_sub(ema_period)..=i], ema_period);

        // 计算ATR
        let atr = calculate_atr(&candles[i.saturating_sub(atr_period + 1)..=i], atr_period);

        // 计算平均成交量
        let avg_volume = candles[i.saturating_sub(lookback)..i]
            .iter()
            .map(|c| c.v)
            .sum::<f64>()
            / lookback as f64;

        // 信号条件
        let breakout = current.c > high_20;
        let above_ema = current.c > ema200;
        let volume_confirm = current.v > avg_volume * 1.2; // 成交量放大20%

        if breakout && above_ema && volume_confirm {
            signals.push(Signal {
                index: i,
                entry_price: current.c,
                stop_loss: current.c - atr * 2.0,
                atr,
            });
        }
    }

    println!("🔍 发现 {} 个突破信号\n", signals.len());

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

    if meets_annual && meets_winrate && meets_drawdown {
        println!("🎉🎉🎉 策略验证通过！");
        println!("\n下一步：");
        println!("  1. 升级到生产模式（Gate 5-8）");
        println!("  2. 集成到 strategies/ 模块");
        println!("  3. 多币种验证");
    } else if (meets_annual as u8 + meets_winrate as u8 + meets_drawdown as u8) >= 2 {
        println!("✅ 策略接近目标，继续优化");
        println!("\n优化方向：");
        if !meets_annual {
            println!("  - 提高年化收益：优化止盈策略");
        }
        if !meets_winrate {
            println!("  - 提高胜率：增加确认条件");
        }
        if !meets_drawdown {
            println!("  - 降低回撤：优化止损逻辑");
        }
    } else {
        println!("❌ 需要调整策略逻辑");
        println!("\n可能方向：");
        println!("  - 调整突破参数（如15日/30日）");
        println!("  - 添加趋势强度过滤（ADX）");
        println!("  - 优化成交量确认阈值");
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

        let mut position_size = 1.0;
        let mut total_pnl = 0.0;
        let mut max_profit = 0.0;

        for i in (signal.index + 1)..(signal.index + 121).min(candles.len()) {
            // 最多持仓20天（120根4H K线）
            let candle = &candles[i];
            let current_profit = (candle.c - entry_price) / entry_price;

            if current_profit > max_profit {
                max_profit = current_profit;

                // 追踪止损：盈利后将止损上移
                if max_profit > 0.02 {
                    // 盈利>2%后启用追踪
                    let new_stop = entry_price + (candle.c - entry_price) * 0.6; // 保护60%利润
                    if new_stop > stop_loss {
                        stop_loss = new_stop;
                    }
                }
            }

            // 检查止损
            if candle.l <= stop_loss {
                let pnl = ((stop_loss - entry_price) / entry_price) * 100.0 * position_size;
                total_pnl += pnl;
                break;
            }

            // 分批止盈
            if position_size == 1.0 && current_profit > 0.05 {
                // 盈利5%，平50%仓位
                let pnl = ((candle.c - entry_price) / entry_price) * 100.0 * 0.5;
                total_pnl += pnl;
                position_size = 0.5;
                stop_loss = entry_price; // 剩余仓位保本
                continue;
            }

            if position_size == 0.5 && current_profit > 0.10 {
                // 盈利10%，平剩余50%
                let pnl = ((candle.c - entry_price) / entry_price) * 100.0 * 0.5;
                total_pnl += pnl;
                position_size = 0.0;
                break;
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
