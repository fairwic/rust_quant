use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use rust_quant_market::models::CandlesEntity;
use rust_quant_services::market::get_confirmed_candles_for_backtest;
use rust_quant_strategies::framework::backtest::types::{
    BackTestResult, BasicRiskStrategyConfig, TradeRecord,
};
use rust_quant_strategies::implementations::rsi_divergence_strategy::{
    RsiDivergenceBacktestTuning, RsiDivergenceStrategy,
};
use rust_quant_strategies::CandleItem;

const DAY_MS: i64 = 86_400_000;
const SAMPLE_LIMIT: usize = 150_000;
const MAKER_FEE_RATE: f64 = 0.0002;
const SLIPPAGE_BPS: [f64; 4] = [0.0, 1.0, 2.0, 3.0];

/// 样本的 train/OOS 时间切分窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SplitWindow {
    train_start_ms: i64,
    train_end_ms: i64,
    oos_start_ms: i64,
    oos_end_ms: i64,
    oos_days: i64,
}

/// RSI Divergence OOS 汇总指标。
#[derive(Debug, Clone, Default, PartialEq)]
struct OosTradeSummary {
    trades: usize,
    wins: usize,
    losses: usize,
    pnl: f64,
    win_rate: f64,
    max_drawdown_percent: f64,
    monthly_pnl: f64,
    trades_per_month: f64,
}

/// 单个 OOS 验证用例。
#[derive(Debug, Clone, Copy)]
struct OosCase {
    label: &'static str,
    symbol: &'static str,
    period: &'static str,
    tuning: RsiDivergenceBacktestTuning,
}

/// 按完整平仓记录汇总 OOS 交易，并按每边 bps 滑点额外扣减。
fn summarize_closed_trades(
    records: &[TradeRecord],
    start_ms: i64,
    end_ms: i64,
    slippage_bps: f64,
) -> OosTradeSummary {
    let mut summary = OosTradeSummary::default();
    let mut equity = 100.0;
    let mut peak = equity;
    let mut max_drawdown = 0.0_f64;

    for record in records
        .iter()
        .filter(|record| record.full_close && record.option_type == "close")
    {
        let close_price = record.close_price.unwrap_or(record.open_price);
        let slippage_cost =
            record.quantity * (record.open_price + close_price) * slippage_bps / 10_000.0;
        let adjusted_profit = record.profit_loss - slippage_cost;

        summary.trades += 1;
        summary.pnl += adjusted_profit;
        if adjusted_profit > 0.0 {
            summary.wins += 1;
        } else if adjusted_profit < 0.0 {
            summary.losses += 1;
        }

        equity += adjusted_profit;
        peak = peak.max(equity);
        if peak > 0.0 {
            max_drawdown = max_drawdown.max((peak - equity) / peak);
        }
    }

    if summary.trades > 0 {
        summary.win_rate = summary.wins as f64 / summary.trades as f64 * 100.0;
    }

    let span_days = ((end_ms - start_ms).max(1) as f64) / 86_400_000.0;
    let months = (span_days / 30.0).max(1.0 / 30.0);
    summary.monthly_pnl = summary.pnl / months;
    summary.trades_per_month = summary.trades as f64 / months;
    summary.max_drawdown_percent = max_drawdown * 100.0;
    summary
}

/// 根据样本覆盖长度自动切分训练段和 OOS 段。
fn split_train_oos(first_ms: i64, last_ms: i64) -> SplitWindow {
    let span_ms = (last_ms - first_ms).max(DAY_MS);
    let span_days = (span_ms / DAY_MS).max(1);
    let max_oos_days = span_days.saturating_sub(1).max(1);
    let oos_days = (span_days / 3).clamp(30, 60).min(max_oos_days);
    let oos_start_ms = last_ms - oos_days * DAY_MS;

    SplitWindow {
        train_start_ms: first_ms,
        train_end_ms: oos_start_ms,
        oos_start_ms,
        oos_end_ms: last_ms,
        oos_days,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();
    std::env::var("QUANT_CORE_DATABASE_URL")
        .context("rsi_divergence_oos_report requires QUANT_CORE_DATABASE_URL")?;

    println!(
        "RSI Divergence OOS report | fee_rate={:.4}% per side | slippage_bps={:?} | sample_source=quant_core",
        MAKER_FEE_RATE * 100.0,
        SLIPPAGE_BPS
    );
    println!(
        "params: fixed user ETH/BTC tunings; no parameter scan inside OOS; pivot_pair_max_lag=disabled\n"
    );

    for case in oos_cases() {
        run_case(case).await?;
    }

    Ok(())
}

/// 固定用户给定参数，分别跑 BTC/ETH 的 1m、5m 与 15m 样本。
fn oos_cases() -> [OosCase; 6] {
    let eth_tuning = RsiDivergenceBacktestTuning {
        rsi_period: 6,
        lookback_period: 40,
        rsi_overbought: 70.0,
        rsi_oversold: 30.0,
        take_profit_atr_mult: 1.0,
        stop_loss_atr_mult: 0.5,
        atr_period: 14,
        enable_hidden_divergence: false,
        allow_short: true,
        allow_long: true,
    };
    let btc_tuning = RsiDivergenceBacktestTuning {
        rsi_period: 10,
        lookback_period: 40,
        rsi_overbought: 65.0,
        rsi_oversold: 35.0,
        take_profit_atr_mult: 1.0,
        stop_loss_atr_mult: 0.5,
        atr_period: 14,
        enable_hidden_divergence: false,
        allow_short: true,
        allow_long: true,
    };

    [
        OosCase {
            label: "ETH-user-1m",
            symbol: "ETH-USDT-SWAP",
            period: "1m",
            tuning: eth_tuning,
        },
        OosCase {
            label: "ETH-user-5m",
            symbol: "ETH-USDT-SWAP",
            period: "5m",
            tuning: eth_tuning,
        },
        OosCase {
            label: "ETH-user-15m",
            symbol: "ETH-USDT-SWAP",
            period: "15m",
            tuning: eth_tuning,
        },
        OosCase {
            label: "BTC-user-5m",
            symbol: "BTC-USDT-SWAP",
            period: "5m",
            tuning: btc_tuning,
        },
        OosCase {
            label: "BTC-user-1m",
            symbol: "BTC-USDT-SWAP",
            period: "1m",
            tuning: btc_tuning,
        },
        OosCase {
            label: "BTC-user-15m",
            symbol: "BTC-USDT-SWAP",
            period: "15m",
            tuning: btc_tuning,
        },
    ]
}

/// 加载样本、切分 train/OOS，并打印每个滑点档位的结果。
async fn run_case(case: OosCase) -> Result<()> {
    let candles = load_sharded_candles(case.symbol, case.period).await?;
    let (first_ms, last_ms) = candle_time_range(&candles)
        .with_context(|| format!("empty candles: {} {}", case.symbol, case.period))?;
    let split = split_train_oos(first_ms, last_ms);
    let train = select_candles(&candles, split.train_start_ms, split.train_end_ms, false);
    let oos = select_candles(&candles, split.oos_start_ms, split.oos_end_ms, true);

    println!(
        "case={} symbol={} tf={} sample={}..{} rows={} train={}..{} rows={} oos={}..{} rows={}",
        case.label,
        case.symbol,
        case.period,
        format_ms(first_ms),
        format_ms(last_ms),
        candles.len(),
        format_ms(split.train_start_ms),
        format_ms(split.train_end_ms),
        train.len(),
        format_ms(split.oos_start_ms),
        format_ms(split.oos_end_ms),
        oos.len()
    );

    run_and_print_phase(case, "train", &train)?;
    run_and_print_phase(case, "oos", &oos)?;
    println!();
    Ok(())
}

/// 在一个时间段内执行固定参数回测并输出 0/1/2/3bps 滑点结果。
fn run_and_print_phase(case: OosCase, phase: &str, candles: &[CandleItem]) -> Result<()> {
    let (start_ms, end_ms) = candle_time_range(candles)
        .with_context(|| format!("empty {phase} candles: {} {}", case.symbol, case.period))?;
    let result = RsiDivergenceStrategy::run_test_with_tuning(
        case.symbol,
        candles,
        risk_config(),
        case.tuning,
    );

    for slippage_bps in SLIPPAGE_BPS {
        let summary =
            summarize_closed_trades(&result.trade_records, start_ms, end_ms, slippage_bps);
        print_summary(phase, slippage_bps, &summary, &result);
    }
    Ok(())
}

/// 回测风控口径：maker 费率 0.02%，不启用额外杠杆。
fn risk_config() -> BasicRiskStrategyConfig {
    BasicRiskStrategyConfig {
        max_loss_percent: 0.02,
        is_used_signal_k_line_stop_loss: Some(true),
        atr_take_profit_ratio: Some(0.0),
        fixed_signal_kline_take_profit_ratio: Some(0.0),
        dynamic_max_loss: Some(false),
        dynamic_entry_amp_threshold: None,
        dynamic_entry_loss_percent: None,
        dynamic_entry_require_direction_mismatch: None,
        dynamic_range_threshold: None,
        dynamic_range_loss_percent: None,
        trade_fee_rate: Some(MAKER_FEE_RATE),
        position_leverage: None,
        tiered_take_profit_level_1_close_ratio: None,
        tiered_take_profit_level_2_close_ratio: None,
    }
}

/// 从 quant_core 分片 K 线表加载已确认 K 线。
async fn load_sharded_candles(symbol: &str, period: &str) -> Result<Vec<CandleItem>> {
    let entities = get_confirmed_candles_for_backtest(symbol, period, SAMPLE_LIMIT, None)
        .await
        .with_context(|| {
            format!("load quant_core sharded candles failed: symbol={symbol} period={period}")
        })?;
    let mut candles = entities
        .iter()
        .map(|entity| candle_entity_to_item(entity, symbol, period))
        .collect::<Result<Vec<_>>>()?;
    candles.sort_unstable_by_key(|candle| candle.ts);
    Ok(candles)
}

/// 把数据库 K 线实体转换为策略回测 CandleItem。
fn candle_entity_to_item(entity: &CandlesEntity, symbol: &str, period: &str) -> Result<CandleItem> {
    Ok(CandleItem {
        ts: entity.ts,
        o: parse_candle_number(&entity.o, "open", entity.ts, symbol, period)?,
        h: parse_candle_number(&entity.h, "high", entity.ts, symbol, period)?,
        l: parse_candle_number(&entity.l, "low", entity.ts, symbol, period)?,
        c: parse_candle_number(&entity.c, "close", entity.ts, symbol, period)?,
        v: parse_candle_number(&entity.vol_ccy, "volume", entity.ts, symbol, period)?,
        confirm: entity.confirm.parse::<i32>().unwrap_or(1),
    })
}

/// 按毫秒时间戳选取训练或 OOS K 线。
fn select_candles(
    candles: &[CandleItem],
    start_ms: i64,
    end_ms: i64,
    include_end: bool,
) -> Vec<CandleItem> {
    candles
        .iter()
        .filter(|candle| {
            candle.ts >= start_ms && (candle.ts < end_ms || (include_end && candle.ts <= end_ms))
        })
        .cloned()
        .collect()
}

/// 获取 K 线样本时间范围。
fn candle_time_range(candles: &[CandleItem]) -> Option<(i64, i64)> {
    Some((candles.first()?.ts, candles.last()?.ts))
}

/// 解析数据库字符串数字字段，带上 symbol/period/ts 方便定位脏数据。
fn parse_candle_number(
    value: &str,
    field: &str,
    ts: i64,
    symbol: &str,
    period: &str,
) -> Result<f64> {
    value.parse::<f64>().with_context(|| {
        format!("invalid candle {field}: symbol={symbol} period={period} ts={ts} value={value}")
    })
}

/// 输出单个阶段、单个滑点档位的汇总。
fn print_summary(
    phase: &str,
    slippage_bps: f64,
    summary: &OosTradeSummary,
    result: &BackTestResult,
) {
    println!(
        "  phase={:<5} slip={:>3.0}bps trades={:>4} win={:>5.1}% pnl={:>8.3}u monthly={:>8.3}u dd={:>5.2}% freq={:>6.1}/mo entries={}",
        phase,
        slippage_bps,
        summary.trades,
        summary.win_rate,
        summary.pnl,
        summary.monthly_pnl,
        summary.max_drawdown_percent,
        summary.trades_per_month,
        result.open_trades
    );
}

/// 将毫秒时间戳格式化为 UTC 时间，便于跨环境对齐。
fn format_ms(ts: i64) -> String {
    Utc.timestamp_millis_opt(ts)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| ts.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close_record(
        open_price: f64,
        close_price: f64,
        quantity: f64,
        profit_loss: f64,
    ) -> TradeRecord {
        TradeRecord {
            option_type: "close".to_string(),
            open_position_time: "2026-01-01 00:00:00".to_string(),
            signal_open_position_time: None,
            close_position_time: Some("2026-01-01 00:05:00".to_string()),
            open_price,
            signal_status: 0,
            close_price: Some(close_price),
            profit_loss,
            quantity,
            full_close: true,
            close_type: "TakeProfit".to_string(),
            win_num: 0,
            loss_num: 0,
            signal_value: None,
            signal_result: None,
            stop_loss_source: None,
            stop_loss_update_history: None,
            initial_stop_price: None,
            initial_risk_amount: None,
            net_profit_r: None,
        }
    }

    #[test]
    fn slippage_bps_reduces_each_round_trip_notional() {
        let records = [close_record(100.0, 110.0, 2.0, 12.0)];

        let summary = summarize_closed_trades(&records, 0, 30 * 24 * 60 * 60 * 1_000, 2.0);

        assert_eq!(summary.trades, 1);
        assert!((summary.pnl - 11.916).abs() < 1e-9);
        assert_eq!(summary.wins, 1);
        assert_eq!(summary.losses, 0);
    }

    #[test]
    fn max_drawdown_uses_adjusted_equity_curve() {
        let records = [
            close_record(100.0, 101.0, 1.0, 10.0),
            close_record(100.0, 99.0, 1.0, -20.0),
            close_record(100.0, 102.0, 1.0, 5.0),
        ];

        let summary = summarize_closed_trades(&records, 0, 30 * 24 * 60 * 60 * 1_000, 0.0);

        assert_eq!(summary.trades, 3);
        assert!((summary.pnl - -5.0).abs() < 1e-9);
        assert!((summary.max_drawdown_percent - (20.0 / 110.0 * 100.0)).abs() < 1e-9);
    }

    #[test]
    fn split_uses_last_third_as_oos_with_minimum_thirty_days() {
        let ninety_days = split_train_oos(0, 90 * DAY_MS);
        assert_eq!(ninety_days.train_start_ms, 0);
        assert_eq!(ninety_days.train_end_ms, 60 * DAY_MS);
        assert_eq!(ninety_days.oos_start_ms, 60 * DAY_MS);
        assert_eq!(ninety_days.oos_end_ms, 90 * DAY_MS);
        assert_eq!(ninety_days.oos_days, 30);

        let one_eighty_days = split_train_oos(0, 180 * DAY_MS);
        assert_eq!(one_eighty_days.train_end_ms, 120 * DAY_MS);
        assert_eq!(one_eighty_days.oos_start_ms, 120 * DAY_MS);
        assert_eq!(one_eighty_days.oos_days, 60);
    }

    #[test]
    fn sample_limit_covers_ninety_days_of_one_minute_candles() {
        assert!(SAMPLE_LIMIT >= 90 * 24 * 60);
    }

    #[test]
    fn oos_cases_include_one_minute_eth_and_btc() {
        let cases = oos_cases();
        let one_minute_cases = cases
            .iter()
            .filter(|case| case.period == "1m")
            .map(|case| case.label)
            .collect::<Vec<_>>();

        assert_eq!(one_minute_cases, vec!["ETH-user-1m", "BTC-user-1m"]);
    }
}
