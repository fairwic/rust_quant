use anyhow::Result;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::Row;
use std::env;

/// 测试长期EMA过滤器的效果
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
    println!("║           长期EMA过滤器效果测试（200EMA）                     ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!(
        "📊 测试数据: {}根K线 (约{:.1}年)\n",
        candle_items.len(),
        candle_items.len() as f64 * 4.0 / 24.0 / 365.0
    );

    let risk_config = BasicRiskStrategyConfig::default();

    // 测试1: 无长期EMA过滤（原配置）
    println!("🔵 测试1: 无长期EMA过滤（基线）");
    let tuning_baseline = RangeBreakoutDropBacktestTuning {
        range_lookback_candles: 20,
        max_range_volatility_pct: 10.0,
        min_range_volatility_pct: 0.1,
        min_breakout_body_ratio: 0.2,
        min_breakout_move_atr: 0.1,
        min_breakout_volume_mult: 0.5,
        require_bearish_ema: false,
        slow_ema_period: 50,
        long_term_ema_period: 200,
        require_below_long_term_ema: false, // 关闭
        stop_atr_mult: 1.5,
        target_r_1: 0.8,
        target_r_2: 1.6,
        target_r_3: 2.4,
        atr_period: 14,
        rsi_period: 14,
        rsi_min_before_drop: 10.0,
        cooldown_candles: 0,
        allow_short: true,
    };

    let strategy1 = RangeBreakoutDropStrategy;
    let result1 =
        strategy1.run_test_with_tuning(symbol, &candle_items, risk_config.clone(), tuning_baseline);

    let trades1 = result1.trade_records.len();
    let winning1 = result1
        .trade_records
        .iter()
        .filter(|t| t.profit_loss > 0.0)
        .count();
    let losing1 = result1
        .trade_records
        .iter()
        .filter(|t| t.profit_loss < 0.0)
        .count();

    let avg_win1 = if winning1 > 0 {
        result1
            .trade_records
            .iter()
            .filter(|t| t.profit_loss > 0.0)
            .map(|t| t.profit_loss)
            .sum::<f64>()
            / winning1 as f64
    } else {
        0.0
    };

    let avg_loss1 = if losing1 > 0 {
        result1
            .trade_records
            .iter()
            .filter(|t| t.profit_loss < 0.0)
            .map(|t| t.profit_loss.abs())
            .sum::<f64>()
            / losing1 as f64
    } else {
        0.0
    };

    println!("  交易数: {}", trades1);
    println!("  胜率: {:.1}%", result1.win_rate * 100.0);
    println!("  总盈亏: {:.2}%", result1.funds - 100.0);
    println!(
        "  盈亏比: {:.2}",
        if avg_loss1 > 0.0 {
            avg_win1 / avg_loss1
        } else {
            0.0
        }
    );
    println!("  最终资金: ${:.2}\n", result1.funds);

    // 测试2: 启用200EMA过滤
    println!("🟢 测试2: 启用200EMA过滤（价格必须低于200EMA）");
    let tuning_filtered = RangeBreakoutDropBacktestTuning {
        range_lookback_candles: 20,
        max_range_volatility_pct: 10.0,
        min_range_volatility_pct: 0.1,
        min_breakout_body_ratio: 0.2,
        min_breakout_move_atr: 0.1,
        min_breakout_volume_mult: 0.5,
        require_bearish_ema: false,
        slow_ema_period: 50,
        long_term_ema_period: 200,
        require_below_long_term_ema: true, // 开启
        stop_atr_mult: 1.5,
        target_r_1: 0.8,
        target_r_2: 1.6,
        target_r_3: 2.4,
        atr_period: 14,
        rsi_period: 14,
        rsi_min_before_drop: 10.0,
        cooldown_candles: 0,
        allow_short: true,
    };

    let strategy2 = RangeBreakoutDropStrategy;
    let result2 =
        strategy2.run_test_with_tuning(symbol, &candle_items, risk_config.clone(), tuning_filtered);

    let trades2 = result2.trade_records.len();
    let winning2 = result2
        .trade_records
        .iter()
        .filter(|t| t.profit_loss > 0.0)
        .count();
    let losing2 = result2
        .trade_records
        .iter()
        .filter(|t| t.profit_loss < 0.0)
        .count();

    let avg_win2 = if winning2 > 0 {
        result2
            .trade_records
            .iter()
            .filter(|t| t.profit_loss > 0.0)
            .map(|t| t.profit_loss)
            .sum::<f64>()
            / winning2 as f64
    } else {
        0.0
    };

    let avg_loss2 = if losing2 > 0 {
        result2
            .trade_records
            .iter()
            .filter(|t| t.profit_loss < 0.0)
            .map(|t| t.profit_loss.abs())
            .sum::<f64>()
            / losing2 as f64
    } else {
        0.0
    };

    println!("  交易数: {}", trades2);
    println!("  胜率: {:.1}%", result2.win_rate * 100.0);
    println!("  总盈亏: {:.2}%", result2.funds - 100.0);
    println!(
        "  盈亏比: {:.2}",
        if avg_loss2 > 0.0 {
            avg_win2 / avg_loss2
        } else {
            0.0
        }
    );
    println!("  最终资金: ${:.2}\n", result2.funds);

    // 对比分析
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                     对比分析                                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let pnl_improvement = (result2.funds - 100.0) - (result1.funds - 100.0);
    let wr_improvement = (result2.win_rate - result1.win_rate) * 100.0;
    let trades_reduction = trades1 as i32 - trades2 as i32;

    println!("📊 关键指标对比:");
    println!(
        "  总盈亏变化: {}{:.2}% ({:.2}% → {:.2}%)",
        if pnl_improvement > 0.0 { "+" } else { "" },
        pnl_improvement,
        result1.funds - 100.0,
        result2.funds - 100.0
    );
    println!(
        "  胜率变化: {}{:.1}% ({:.1}% → {:.1}%)",
        if wr_improvement > 0.0 { "+" } else { "" },
        wr_improvement,
        result1.win_rate * 100.0,
        result2.win_rate * 100.0
    );
    println!(
        "  交易数变化: {} ({} → {})",
        if trades_reduction > 0 {
            format!("-{}", trades_reduction)
        } else {
            format!("+{}", -trades_reduction)
        },
        trades1,
        trades2
    );

    let pf1 = if avg_loss1 > 0.0 {
        avg_win1 / avg_loss1
    } else {
        0.0
    };
    let pf2 = if avg_loss2 > 0.0 {
        avg_win2 / avg_loss2
    } else {
        0.0
    };
    println!("  盈亏比变化: {:.2} → {:.2}", pf1, pf2);

    println!("\n💡 结论:");
    if result2.funds > result1.funds && result2.win_rate > result1.win_rate {
        println!("  ✅ 长期EMA过滤显著改善策略表现！");
        println!(
            "  ✅ 盈利提升 {:.2}%，胜率提升 {:.1}%",
            pnl_improvement, wr_improvement
        );
        println!("  ✅ 通过过滤上涨市场，避免了逆势做空的亏损");
    } else if result2.funds > result1.funds {
        println!("  ✅ 长期EMA过滤改善了盈利");
        println!("  📈 盈利提升 {:.2}%", pnl_improvement);
    } else if result2.win_rate > result1.win_rate * 1.1 {
        println!("  ⚠️  长期EMA过滤提升了胜率，但总盈利下降");
        println!("  可能过度过滤，减少了盈利机会");
    } else {
        println!("  ❌ 长期EMA过滤效果不明显或有负面影响");
        println!("  需要调整过滤参数或使用其他方法");
    }

    println!("\n================================");

    Ok(())
}
