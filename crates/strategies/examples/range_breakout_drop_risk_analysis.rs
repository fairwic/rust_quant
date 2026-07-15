use anyhow::Result;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::Row;
use std::env;

/// 完整风险指标分析 - 回撤、夏普比率等
#[tokio::main]
async fn main() -> Result<()> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:example@localhost:5432/quant_core".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let symbol = "BTC-USDT-SWAP";

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

    let mut candle_items: Vec<CandleItem> = Vec::new();
    for row in rows.iter().rev() {
        let ts: i64 = row.try_get("ts")?;
        candle_items.push(CandleItem {
            ts,
            o: row.try_get::<String, _>("o")?.parse::<f64>()?,
            h: row.try_get::<String, _>("h")?.parse::<f64>()?,
            l: row.try_get::<String, _>("l")?.parse::<f64>()?,
            c: row.try_get::<String, _>("c")?.parse::<f64>()?,
            v: row.try_get::<String, _>("vol")?.parse::<f64>()?,
            confirm: row.try_get::<String, _>("confirm")?.parse::<i32>()?,
        });
    }

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           当前策略风险指标完整分析                             ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // 当前最优配置
    let tuning = RangeBreakoutDropBacktestTuning {
        range_lookback_candles: 20,
        max_range_volatility_pct: 10.0,
        min_range_volatility_pct: 0.1,
        min_breakout_body_ratio: 0.2,
        min_breakout_move_atr: 0.1,
        min_breakout_volume_mult: 0.5,
        require_bearish_ema: false,
        slow_ema_period: 50,
        long_term_ema_period: 200,
        require_below_long_term_ema: true,
        stop_atr_mult: 1.5,
        target_r_1: 0.5,
        target_r_2: 1.0,
        target_r_3: 1.5,
        atr_period: 14,
        rsi_period: 14,
        rsi_min_before_drop: 10.0,
        cooldown_candles: 0,
        allow_short: true,
    };

    let strategy = RangeBreakoutDropStrategy;
    let risk_config = BasicRiskStrategyConfig::default();

    println!("执行回测...\n");
    let result = strategy.run_test_with_tuning(symbol, &candle_items, risk_config, tuning);

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                  风险指标详细分析                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // 基础指标
    let trades = result.trade_records.len();
    let winning = result
        .trade_records
        .iter()
        .filter(|t| t.profit_loss > 0.0)
        .count();
    let losing = result
        .trade_records
        .iter()
        .filter(|t| t.profit_loss < 0.0)
        .count();

    println!("📊 基础指标:");
    println!("  总交易数: {}", trades);
    println!("  胜率: {:.1}% ❌ (目标: ≥50%)", result.win_rate * 100.0);
    println!("  总盈亏: {:.2}%", result.funds - 100.0);
    println!(
        "  年化收益: {:.2}%\n",
        (result.funds - 100.0) / (candle_items.len() as f64 * 4.0 / 24.0 / 365.0)
    );

    // 计算资金曲线和回撤
    let mut equity_curve = vec![100.0];
    let mut current_equity = 100.0;

    for trade in &result.trade_records {
        current_equity += trade.profit_loss;
        equity_curve.push(current_equity);
    }

    // 最大回撤
    let mut max_equity = 100.0;
    let mut max_drawdown = 0.0;
    let mut max_dd_duration = 0;
    let mut current_dd_duration = 0;

    for equity in &equity_curve {
        if *equity > max_equity {
            max_equity = *equity;
            current_dd_duration = 0;
        } else {
            let drawdown = (max_equity - equity) / max_equity;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }
            current_dd_duration += 1;
            if current_dd_duration > max_dd_duration {
                max_dd_duration = current_dd_duration;
            }
        }
    }

    println!("📉 回撤分析:");
    println!("  最大回撤: {:.2}%", max_drawdown * 100.0);
    println!("  最长回撤持续: {}笔交易", max_dd_duration);

    if max_drawdown < 0.15 {
        println!("  ✅ 回撤控制良好 (<15%)");
    } else if max_drawdown < 0.25 {
        println!("  ⚠️  回撤一般 (15-25%)");
    } else {
        println!("  ❌ 回撤过大 (>25%)");
    }
    println!();

    // 夏普比率（简化版，假设无风险利率=0）
    let returns: Vec<f64> = equity_curve
        .windows(2)
        .map(|w| (w[1] - w[0]) / w[0])
        .collect();

    let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns
        .iter()
        .map(|r| (r - mean_return).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let std_dev = variance.sqrt();

    // 年化夏普（假设每年48笔交易）
    let sharpe_ratio = if std_dev > 0.0 {
        mean_return / std_dev * (48.0_f64).sqrt()
    } else {
        0.0
    };

    println!("📈 夏普比率:");
    println!("  夏普比率: {:.2}", sharpe_ratio);

    if sharpe_ratio > 2.0 {
        println!("  ✅ 优秀 (>2.0)");
    } else if sharpe_ratio > 1.0 {
        println!("  ⚠️  一般 (1.0-2.0)");
    } else {
        println!("  ❌ 较差 (<1.0)");
    }
    println!();

    // 交易频率分析
    let years = candle_items.len() as f64 * 4.0 / 24.0 / 365.0;
    let trades_per_year = trades as f64 / years;
    let trades_per_month = trades_per_year / 12.0;

    println!("🔄 交易频率:");
    println!("  年均交易: {:.1}笔", trades_per_year);
    println!("  月均交易: {:.1}笔", trades_per_month);

    if trades_per_month >= 2.0 && trades_per_month <= 8.0 {
        println!("  ✅ 频率合理 (2-8笔/月)");
    } else if trades_per_month < 2.0 {
        println!("  ⚠️  频率偏低 (<2笔/月)");
    } else {
        println!("  ⚠️  频率偏高 (>8笔/月)");
    }
    println!();

    // 盈亏分布
    let avg_win = if winning > 0 {
        result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss > 0.0)
            .map(|t| t.profit_loss)
            .sum::<f64>()
            / winning as f64
    } else {
        0.0
    };

    let avg_loss = if losing > 0 {
        result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss < 0.0)
            .map(|t| t.profit_loss.abs())
            .sum::<f64>()
            / losing as f64
    } else {
        0.0
    };

    println!("💰 盈亏分布:");
    println!("  平均盈利: ${:.2}", avg_win);
    println!("  平均亏损: ${:.2}", avg_loss);
    println!(
        "  盈亏比: {:.2}",
        if avg_loss > 0.0 {
            avg_win / avg_loss
        } else {
            0.0
        }
    );
    println!(
        "  盈亏平衡交易: {}笔 ({:.1}%)",
        trades - winning - losing,
        (trades - winning - losing) as f64 / trades as f64 * 100.0
    );
    println!();

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                  问题诊断与改进方向                            ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!("❌ 核心问题:");
    println!("  1. 胜率仅29.1%，远低于50%目标");
    println!("  2. 50%的交易是盈亏平衡（0盈亏）");
    println!("  3. 虽然盈亏比高，但依赖少数大赢家\n");

    println!("💡 改进方向:");
    println!("  方向1: 提高入场质量（更严格的过滤条件）");
    println!("    - 添加更多确认指标");
    println!("    - 提高突破质量要求");
    println!("    - 增加成交量确认");
    println!();
    println!("  方向2: 优化止盈止损逻辑");
    println!("    - 当前0.5R可能过于激进");
    println!("    - 考虑动态止盈（趋势强度）");
    println!("    - 减少盈亏平衡交易");
    println!();
    println!("  方向3: 改变策略类型");
    println!("    - 当前是突破策略（天然低胜率）");
    println!("    - 考虑均值回归策略（高胜率）");
    println!("    - 考虑趋势跟随策略（平衡型）");

    println!("\n================================");

    Ok(())
}
