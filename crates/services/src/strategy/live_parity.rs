use super::live_decision::apply_live_decision;
use anyhow::{anyhow, Result};
use rust_quant_common::CandleItem;
use rust_quant_domain::StrategyConfig;
use rust_quant_market::models::CandlesEntity;
use rust_quant_strategies::framework::backtest::{
    BasicRiskStrategyConfig, TradeRecord, TradingState,
};
use rust_quant_strategies::framework::strategy_trait::StrategyExecutor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperOrderRecord {
    /// 订单 ID。
    pub order_id: String,
    /// 动作类型。
    pub action: String,
    /// 时间字段。
    pub event_time: String,
    /// 价格。
    pub price: f64,
    /// 数量。
    pub quantity: f64,
    /// 类型标识。
    pub close_type: Option<String>,
    /// 状态值。
    pub signal_status: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveReplayResult {
    /// 列表数据。
    pub trade_records: Vec<TradeRecord>,
    /// 列表数据。
    pub paper_orders: Vec<PaperOrderRecord>,
    /// 金额数值。
    pub final_funds: f64,
    /// wins，用于交易策略计算。
    pub wins: i64,
    /// losses，用于交易策略计算。
    pub losses: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParityTradeRow {
    /// 类型标识。
    pub option_type: String,
    /// 开仓时间。
    pub open_position_time: String,
    /// 平仓时间。
    pub close_position_time: Option<String>,
    /// 价格数值。
    pub open_price: f64,
    /// 离场价格。
    pub close_price: Option<f64>,
    /// 收益亏损，用于展示或持久化查询结果。
    pub profit_loss: f64,
    /// 数量。
    pub quantity: f64,
    /// 类型标识。
    pub close_type: String,
    /// 状态值。
    pub signal_status: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityDifference {
    /// index，用于交易策略计算。
    pub index: usize,
    /// field，用于交易策略计算。
    pub field: String,
    /// simulated，用于交易策略计算。
    pub simulated: String,
    /// expected，用于交易策略计算。
    pub expected: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityComparisonReport {
    /// simulated数量。
    pub simulated_count: usize,
    /// expected数量。
    pub expected_count: usize,
    /// matched数据行，用于展示或持久化查询结果。
    pub matched_rows: usize,
    /// onlysimulated，用于展示或持久化查询结果。
    pub only_simulated: usize,
    /// onlyexpected，用于展示或持久化查询结果。
    pub only_expected: usize,
    /// 列表数据。
    pub differences: Vec<ParityDifference>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TimePair {
    /// 开仓时间。
    pub open_position_time: String,
    /// 平仓时间。
    pub close_position_time: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingParityReport {
    /// simulated数量。
    pub simulated_count: usize,
    /// expected数量。
    pub expected_count: usize,
    /// matched时间pairs，用于展示或持久化查询结果。
    pub matched_time_pairs: usize,
    /// onlysimulatedpairs，用于展示或持久化查询结果。
    pub only_simulated_pairs: usize,
    /// onlyexpectedpairs，用于展示或持久化查询结果。
    pub only_expected_pairs: usize,
    /// pair精度。
    pub pair_precision: f64,
    /// pairrecall，用于展示或持久化查询结果。
    pub pair_recall: f64,
    /// pairf1，用于展示或持久化查询结果。
    pub pair_f1: f64,
    /// matched开盘times，用于展示或持久化查询结果。
    pub matched_open_times: usize,
    /// 未平仓精度。
    pub open_precision: f64,
    /// 开盘recall，用于展示或持久化查询结果。
    pub open_recall: f64,
    /// matched收盘times，用于展示或持久化查询结果。
    pub matched_close_times: usize,
    /// close精度。
    pub close_precision: f64,
    /// 收盘recall，用于展示或持久化查询结果。
    pub close_recall: f64,
    /// 列表数据。
    pub only_expected_pair_samples: Vec<TimePair>,
    /// 列表数据。
    pub only_simulated_pair_samples: Vec<TimePair>,
}
/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
fn candle_entity_to_item(c: &CandlesEntity) -> Result<CandleItem> {
    let o =
        c.o.parse::<f64>()
            .map_err(|e| anyhow!("解析开盘价失败: {}", e))?;
    let h =
        c.h.parse::<f64>()
            .map_err(|e| anyhow!("解析最高价失败: {}", e))?;
    let l =
        c.l.parse::<f64>()
            .map_err(|e| anyhow!("解析最低价失败: {}", e))?;
    let close =
        c.c.parse::<f64>()
            .map_err(|e| anyhow!("解析收盘价失败: {}", e))?;
    let v = c
        .vol_ccy
        .parse::<f64>()
        .map_err(|e| anyhow!("解析成交量失败: {}", e))?;
    let confirm = c
        .confirm
        .parse::<i32>()
        .map_err(|e| anyhow!("解析 confirm 失败: {}", e))?;
    Ok(CandleItem {
        o,
        h,
        l,
        c: close,
        v,
        ts: c.ts,
        confirm,
    })
}
/// 提供replaylivewithwarmup的集中实现，避免回测策略调用方重复处理相同细节。
pub async fn replay_live_with_warmup(
    executor: Arc<dyn StrategyExecutor>,
    strategy_config: &StrategyConfig,
    candles: &[CandlesEntity],
    warmup_candles: usize,
    initial_funds: f64,
) -> Result<LiveReplayResult> {
    if candles.len() <= warmup_candles {
        return Err(anyhow!(
            "K线数量不足: total={}, warmup={}",
            candles.len(),
            warmup_candles
        ));
    }
    let inst_id = strategy_config.symbol.as_str();
    let period = strategy_config.timeframe.as_str();
    let decision_risk: BasicRiskStrategyConfig =
        serde_json::from_value(strategy_config.risk_config.clone())
            .map_err(|e| anyhow!("解析风控配置失败: {}", e))?;
    let strategy_cfg =
        rust_quant_strategies::framework::config::strategy_config::StrategyConfig::new(
            strategy_config.id,
            strategy_config.strategy_type,
            strategy_config.symbol.clone(),
            strategy_config.timeframe,
            strategy_config.parameters.clone(),
            strategy_config.risk_config.clone(),
        );
    let mut sorted = candles.to_vec();
    sorted.sort_unstable_by_key(|a| a.ts);
    let warmup_items = sorted
        .iter()
        .take(warmup_candles)
        .map(candle_entity_to_item)
        .collect::<Result<Vec<_>>>()?;
    executor
        .initialize_data(&strategy_cfg, inst_id, period, warmup_items)
        .await?;
    let mut state = TradingState {
        funds: initial_funds,
        ..TradingState::default()
    };
    let mut paper_orders = Vec::new();
    let mut order_seq: usize = 0;
    for candle in sorted.iter().skip(warmup_candles) {
        let candle_item = candle_entity_to_item(candle)?;
        let mut signal = executor
            .execute(inst_id, period, strategy_config, Some(candle_item.clone()))
            .await
            .map_err(|e| anyhow!("执行策略失败: {}", e))?;
        let before = state.trade_records.len();
        let _outcome = apply_live_decision(&mut state, &mut signal, &candle_item, decision_risk);
        let new_records = &state.trade_records[before..];
        for record in new_records {
            order_seq += 1;
            let (action, event_time, price) = if record.option_type == "open" {
                (
                    "OPEN".to_string(),
                    record.open_position_time.clone(),
                    record.open_price,
                )
            } else {
                (
                    "CLOSE".to_string(),
                    record
                        .close_position_time
                        .clone()
                        .unwrap_or_else(|| record.open_position_time.clone()),
                    record.close_price.unwrap_or(record.open_price),
                )
            };
            paper_orders.push(PaperOrderRecord {
                order_id: format!("paper-{}-{}", strategy_config.id, order_seq),
                action,
                event_time,
                price,
                quantity: record.quantity,
                close_type: if record.close_type.is_empty() {
                    None
                } else {
                    Some(record.close_type.clone())
                },
                signal_status: record.signal_status,
            });
        }
    }
    Ok(LiveReplayResult {
        trade_records: state.trade_records.clone(),
        paper_orders,
        final_funds: state.funds,
        wins: state.wins,
        losses: state.losses,
    })
}
/// 将内部模型转换为输出结构，避免 回测与策略研究 的内部字段直接外泄。
pub fn to_parity_trade_rows(records: &[TradeRecord]) -> Vec<ParityTradeRow> {
    records
        .iter()
        .map(|r| ParityTradeRow {
            option_type: r.option_type.clone(),
            open_position_time: r.open_position_time.clone(),
            close_position_time: r.close_position_time.clone(),
            open_price: r.open_price,
            close_price: r.close_price,
            profit_loss: r.profit_loss,
            quantity: r.quantity,
            close_type: r.close_type.clone(),
            signal_status: r.signal_status,
        })
        .collect()
}
fn approx_eq(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}
/// 提供ratio的集中实现，避免回测策略调用方重复处理相同细节。
fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}
/// 提供multisetcounts的集中实现，避免回测策略调用方重复处理相同细节。
fn multiset_counts<K, I>(iter: I) -> HashMap<K, usize>
where
    K: Eq + std::hash::Hash,
    I: IntoIterator<Item = K>,
{
    let mut map: HashMap<K, usize> = HashMap::new();
    for item in iter {
        *map.entry(item).or_insert(0usize) += 1;
    }
    map
}
/// 提供multisetintersection数量的集中实现，避免回测策略调用方重复处理相同细节。
fn multiset_intersection_count<K>(left: &HashMap<K, usize>, right: &HashMap<K, usize>) -> usize
where
    K: Eq + std::hash::Hash,
{
    left.iter()
        .map(|(key, left_count)| {
            let right_count = right.get(key).copied().unwrap_or(0usize);
            (*left_count).min(right_count)
        })
        .sum()
}
/// 提供multisetdiffsamples的集中实现，避免回测策略调用方重复处理相同细节。
fn multiset_diff_samples<K>(
    base: &HashMap<K, usize>,
    subtract: &HashMap<K, usize>,
    limit: usize,
) -> Vec<K>
where
    K: Clone + Eq + std::hash::Hash,
{
    let mut output = Vec::new();
    if limit == 0 {
        return output;
    }
    for (key, base_count) in base {
        let subtract_count = subtract.get(key).copied().unwrap_or(0usize);
        let remain = base_count.saturating_sub(subtract_count);
        for _ in 0..remain {
            output.push(key.clone());
            if output.len() >= limit {
                return output;
            }
        }
    }
    output
}
/// 提供数据行to时间pair的集中实现，避免回测策略调用方重复处理相同细节。
fn row_to_time_pair(row: &ParityTradeRow) -> TimePair {
    TimePair {
        open_position_time: row.open_position_time.clone(),
        close_position_time: row.close_position_time.clone(),
    }
}
/// 提供comparetimingparity的集中实现，避免回测策略调用方重复处理相同细节。
pub fn compare_timing_parity(
    simulated: &[ParityTradeRow],
    expected: &[ParityTradeRow],
    sample_limit: usize,
) -> TimingParityReport {
    let simulated_pairs = multiset_counts(simulated.iter().map(row_to_time_pair));
    let expected_pairs = multiset_counts(expected.iter().map(row_to_time_pair));
    let matched_time_pairs = multiset_intersection_count(&simulated_pairs, &expected_pairs);
    let simulated_open = multiset_counts(simulated.iter().map(|r| r.open_position_time.clone()));
    let expected_open = multiset_counts(expected.iter().map(|r| r.open_position_time.clone()));
    let matched_open_times = multiset_intersection_count(&simulated_open, &expected_open);
    let simulated_close = multiset_counts(simulated.iter().map(|r| r.close_position_time.clone()));
    let expected_close = multiset_counts(expected.iter().map(|r| r.close_position_time.clone()));
    let matched_close_times = multiset_intersection_count(&simulated_close, &expected_close);
    let pair_precision = ratio(matched_time_pairs, simulated.len());
    let pair_recall = ratio(matched_time_pairs, expected.len());
    let pair_f1 = if pair_precision + pair_recall > 0.0 {
        2.0 * pair_precision * pair_recall / (pair_precision + pair_recall)
    } else {
        0.0
    };
    TimingParityReport {
        simulated_count: simulated.len(),
        expected_count: expected.len(),
        matched_time_pairs,
        only_simulated_pairs: simulated.len().saturating_sub(matched_time_pairs),
        only_expected_pairs: expected.len().saturating_sub(matched_time_pairs),
        pair_precision,
        pair_recall,
        pair_f1,
        matched_open_times,
        open_precision: ratio(matched_open_times, simulated.len()),
        open_recall: ratio(matched_open_times, expected.len()),
        matched_close_times,
        close_precision: ratio(matched_close_times, simulated.len()),
        close_recall: ratio(matched_close_times, expected.len()),
        only_expected_pair_samples: multiset_diff_samples(
            &expected_pairs,
            &simulated_pairs,
            sample_limit,
        ),
        only_simulated_pair_samples: multiset_diff_samples(
            &simulated_pairs,
            &expected_pairs,
            sample_limit,
        ),
    }
}
/// 提供compareparityrows的集中实现，避免回测策略调用方重复处理相同细节。
pub fn compare_parity_rows(
    simulated: &[ParityTradeRow],
    expected: &[ParityTradeRow],
    price_eps: f64,
    pnl_eps: f64,
) -> ParityComparisonReport {
    let mut differences = Vec::new();
    let pair_len = simulated.len().min(expected.len());
    let mut matched_rows = 0usize;
    for idx in 0..pair_len {
        let left = &simulated[idx];
        let right = &expected[idx];
        let mut row_ok = true;
        let push_diff = |field: &str,
                         simulated_val: String,
                         expected_val: String,
                         diffs: &mut Vec<ParityDifference>| {
            diffs.push(ParityDifference {
                index: idx,
                field: field.to_string(),
                simulated: simulated_val,
                expected: expected_val,
            });
        };
        if left.option_type != right.option_type {
            row_ok = false;
            push_diff(
                "option_type",
                left.option_type.clone(),
                right.option_type.clone(),
                &mut differences,
            );
        }
        if left.open_position_time != right.open_position_time {
            row_ok = false;
            push_diff(
                "open_position_time",
                left.open_position_time.clone(),
                right.open_position_time.clone(),
                &mut differences,
            );
        }
        if left.close_position_time != right.close_position_time {
            row_ok = false;
            push_diff(
                "close_position_time",
                format!("{:?}", left.close_position_time),
                format!("{:?}", right.close_position_time),
                &mut differences,
            );
        }
        if !approx_eq(left.open_price, right.open_price, price_eps) {
            row_ok = false;
            push_diff(
                "open_price",
                left.open_price.to_string(),
                right.open_price.to_string(),
                &mut differences,
            );
        }
        if !left
            .close_price
            .zip(right.close_price)
            .map(|(a, b)| approx_eq(a, b, price_eps))
            .unwrap_or(left.close_price == right.close_price)
        {
            row_ok = false;
            push_diff(
                "close_price",
                format!("{:?}", left.close_price),
                format!("{:?}", right.close_price),
                &mut differences,
            );
        }
        if !approx_eq(left.profit_loss, right.profit_loss, pnl_eps) {
            row_ok = false;
            push_diff(
                "profit_loss",
                left.profit_loss.to_string(),
                right.profit_loss.to_string(),
                &mut differences,
            );
        }
        if !approx_eq(left.quantity, right.quantity, price_eps) {
            row_ok = false;
            push_diff(
                "quantity",
                left.quantity.to_string(),
                right.quantity.to_string(),
                &mut differences,
            );
        }
        if left.close_type != right.close_type {
            row_ok = false;
            push_diff(
                "close_type",
                left.close_type.clone(),
                right.close_type.clone(),
                &mut differences,
            );
        }
        if left.signal_status != right.signal_status {
            row_ok = false;
            push_diff(
                "signal_status",
                left.signal_status.to_string(),
                right.signal_status.to_string(),
                &mut differences,
            );
        }
        if row_ok {
            matched_rows += 1;
        }
    }
    ParityComparisonReport {
        simulated_count: simulated.len(),
        expected_count: expected.len(),
        matched_rows,
        only_simulated: simulated.len().saturating_sub(expected.len()),
        only_expected: expected.len().saturating_sub(simulated.len()),
        differences,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// 构造样例row，集中维护回测策略的载荷组装规则。
    fn sample_row(option_type: &str, open_price: f64, profit_loss: f64) -> ParityTradeRow {
        ParityTradeRow {
            option_type: option_type.to_string(),
            open_position_time: "2026-01-01 00:00:00".to_string(),
            close_position_time: Some("2026-01-01 04:00:00".to_string()),
            open_price,
            close_price: Some(open_price + 1.0),
            profit_loss,
            quantity: 1.0,
            close_type: "Signal_Kline_Stop_Loss".to_string(),
            signal_status: 0,
        }
    }
    #[test]
    fn parity_compare_reports_exact_match() {
        let simulated = vec![sample_row("close", 100.0, 1.2)];
        let expected = vec![sample_row("close", 100.0, 1.2)];
        let report = compare_parity_rows(&simulated, &expected, 1e-9, 1e-9);
        assert_eq!(report.simulated_count, 1);
        assert_eq!(report.expected_count, 1);
        assert_eq!(report.matched_rows, 1);
        assert!(report.differences.is_empty());
    }
    #[test]
    fn parity_compare_reports_field_diff() {
        let simulated = vec![sample_row("close", 100.0, 1.2)];
        let expected = vec![sample_row("close", 101.0, 1.2)];
        let report = compare_parity_rows(&simulated, &expected, 1e-9, 1e-9);
        assert_eq!(report.matched_rows, 0);
        assert!(!report.differences.is_empty());
        assert!(report.differences.iter().any(|d| d.field == "open_price"));
    }
    #[test]
    fn parity_compare_reports_len_diff() {
        let simulated = vec![
            sample_row("open", 100.0, 0.0),
            sample_row("close", 100.0, 1.2),
        ];
        let expected = vec![sample_row("open", 100.0, 0.0)];
        let report = compare_parity_rows(&simulated, &expected, 1e-9, 1e-9);
        assert_eq!(report.only_simulated, 1);
        assert_eq!(report.only_expected, 0);
    }
    #[test]
    fn timing_parity_reports_exact_match() {
        let simulated = vec![sample_row("close", 100.0, 1.2)];
        let expected = vec![sample_row("close", 100.0, 1.2)];
        let report = compare_timing_parity(&simulated, &expected, 10);
        assert_eq!(report.matched_time_pairs, 1);
        assert_eq!(report.only_expected_pairs, 0);
        assert_eq!(report.only_simulated_pairs, 0);
        assert!((report.pair_f1 - 1.0).abs() < f64::EPSILON);
    }
    #[test]
    fn timing_parity_reports_partial_match() {
        let simulated = vec![
            sample_row("close", 100.0, 1.2),
            sample_row("close", 101.0, 1.5),
        ];
        let mut expected = vec![
            sample_row("close", 100.0, 1.2),
            sample_row("close", 102.0, 1.5),
        ];
        expected[1].open_position_time = "2026-01-01 08:00:00".to_string();
        expected[1].close_position_time = Some("2026-01-01 12:00:00".to_string());
        let report = compare_timing_parity(&simulated, &expected, 10);
        assert_eq!(report.matched_time_pairs, 1);
        assert_eq!(report.only_expected_pairs, 1);
        assert_eq!(report.only_simulated_pairs, 1);
    }
}
