use rust_quant_strategies::framework::backtest::types::{
    BackTestResult, BasicRiskStrategyConfig, TradeRecord,
};
use rust_quant_strategies::CandleItem;
use serde_json::Value;
use std::collections::BTreeMap;

const LEGACY_BACKTEST_TRADE_FEE_RATE: f64 = 0.0007;

/// Research-only 短窗口失效退出参数，按入场后的完整 1m K 线数量检查。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct KeltnerFailureExitConfig {
    pub(crate) bars: usize,
    pub(crate) min_progress_r: f64,
}

/// Keltner failure-exit 诊断使用的方向枚举，避免依赖交易所订单侧字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeltnerFailureExitSide {
    Long,
    Short,
}

impl KeltnerFailureExitSide {
    fn from_option_type(value: &str) -> Option<Self> {
        match value {
            "long" => Some(Self::Long),
            "short" => Some(Self::Short),
            _ => None,
        }
    }

    fn progress(self, open_price: f64, close_price: f64) -> f64 {
        match self {
            Self::Long => close_price - open_price,
            Self::Short => open_price - close_price,
        }
    }
}

/// 默认只作为诊断输出的小网格，验证短窗口失效退出是否值得升级到回测框架。
pub(crate) const KELTNER_FAILURE_EXIT_PROFILES: [KeltnerFailureExitConfig; 6] = [
    KeltnerFailureExitConfig {
        bars: 3,
        min_progress_r: 0.10,
    },
    KeltnerFailureExitConfig {
        bars: 3,
        min_progress_r: 0.25,
    },
    KeltnerFailureExitConfig {
        bars: 5,
        min_progress_r: 0.10,
    },
    KeltnerFailureExitConfig {
        bars: 5,
        min_progress_r: 0.25,
    },
    KeltnerFailureExitConfig {
        bars: 8,
        min_progress_r: 0.10,
    },
    KeltnerFailureExitConfig {
        bars: 8,
        min_progress_r: 0.25,
    },
];

/// 单笔短窗口失效退出决策，保留触发价格、R 进度和手续费后 PnL。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct KeltnerFailureExitDecision {
    pub(crate) exit_price: f64,
    pub(crate) progress_r: f64,
    pub(crate) pnl: f64,
}

/// 对单笔交易执行短窗口失效退出判定；未触发时返回 None。
pub(crate) fn keltner_failure_exit_decision(
    side: KeltnerFailureExitSide,
    open_price: f64,
    stop_price: f64,
    quantity: f64,
    fee_rate: f64,
    candles_after_entry: &[CandleItem],
    config: KeltnerFailureExitConfig,
) -> Option<KeltnerFailureExitDecision> {
    if config.bars == 0 || open_price <= 0.0 || quantity <= 0.0 {
        return None;
    }
    let stop_distance = (open_price - stop_price).abs();
    if stop_distance <= f64::EPSILON {
        return None;
    }
    let check_candle = candles_after_entry.get(config.bars - 1)?;
    let progress = side.progress(open_price, check_candle.c);
    let progress_r = progress / stop_distance;
    if progress_r >= config.min_progress_r {
        return None;
    }
    let fee = quantity * (open_price + check_candle.c) * fee_rate;
    Some(KeltnerFailureExitDecision {
        exit_price: check_candle.c,
        progress_r,
        pnl: progress * quantity - fee,
    })
}

/// 单个交易对/样本在 failure-exit overlay 后的逐笔结果。
#[derive(Debug, Clone)]
pub(crate) struct KeltnerFailureExitOverlayReport {
    label: String,
    days: f64,
    trades: Vec<KeltnerFailureExitTrade>,
}

/// 汇总多个样本后的 failure-exit overlay 结果。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct KeltnerFailureExitOverlaySummary {
    pub(crate) entries: usize,
    pub(crate) wins: usize,
    pub(crate) losses: usize,
    pub(crate) win_rate_pct: f64,
    pub(crate) pnl: f64,
    pub(crate) delta_pnl: f64,
    pub(crate) max_drawdown_pct: f64,
    pub(crate) trades_per_day: f64,
    pub(crate) early_win_rate_pct: f64,
    pub(crate) early_pnl: f64,
    pub(crate) late_win_rate_pct: f64,
    pub(crate) late_pnl: f64,
    pub(crate) remove_top5_pnl: f64,
    pub(crate) triggered: usize,
}

#[derive(Debug, Clone)]
struct KeltnerFailureExitTrade {
    open_time: String,
    original_pnl: f64,
    pnl: f64,
    triggered: bool,
}

#[derive(Debug, Clone)]
struct KeltnerPositionOutcome {
    open_time: String,
    side: KeltnerFailureExitSide,
    entry_index: usize,
    first_exit_index: Option<usize>,
    open_price: f64,
    stop_price: f64,
    quantity: f64,
    original_pnl: f64,
}

/// 以已有入场点为固定事实，叠加模拟短窗口失效退出后的逐笔结果。
pub(crate) fn keltner_failure_exit_overlay_report(
    label: &str,
    candles: &[CandleItem],
    result: &BackTestResult,
    risk: BasicRiskStrategyConfig,
    config: KeltnerFailureExitConfig,
) -> KeltnerFailureExitOverlayReport {
    let fee_rate = risk
        .trade_fee_rate
        .unwrap_or(LEGACY_BACKTEST_TRADE_FEE_RATE);
    let positions = keltner_position_outcomes(candles, &result.trade_records);
    let trades = positions
        .into_iter()
        .map(|position| {
            let check_index = position.entry_index.saturating_add(config.bars);
            let should_keep_original = position
                .first_exit_index
                .is_some_and(|exit_index| exit_index <= check_index);
            let decision = if should_keep_original {
                None
            } else {
                keltner_failure_exit_decision(
                    position.side,
                    position.open_price,
                    position.stop_price,
                    position.quantity,
                    fee_rate,
                    candles.get(position.entry_index + 1..).unwrap_or_default(),
                    config,
                )
            };
            match decision {
                Some(decision) => KeltnerFailureExitTrade {
                    open_time: position.open_time,
                    original_pnl: position.original_pnl,
                    pnl: decision.pnl,
                    triggered: true,
                },
                None => KeltnerFailureExitTrade {
                    open_time: position.open_time,
                    original_pnl: position.original_pnl,
                    pnl: position.original_pnl,
                    triggered: false,
                },
            }
        })
        .collect::<Vec<_>>();

    KeltnerFailureExitOverlayReport {
        label: label.to_string(),
        days: candle_span_days(candles),
        trades,
    }
}

/// 汇总多个交易对的 failure-exit overlay 指标，用于和原始 Keltner leader 对照。
pub(crate) fn summarize_keltner_failure_exit_overlay_reports(
    reports: &[KeltnerFailureExitOverlayReport],
) -> KeltnerFailureExitOverlaySummary {
    let mut trades = reports
        .iter()
        .flat_map(|report| report.trades.iter().cloned())
        .collect::<Vec<_>>();
    trades.sort_unstable_by(|left, right| left.open_time.cmp(&right.open_time));
    let entries = trades.len();
    let wins = trades.iter().filter(|trade| trade.pnl > 0.0).count();
    let losses = trades.iter().filter(|trade| trade.pnl < 0.0).count();
    let pnl = trades.iter().map(|trade| trade.pnl).sum::<f64>();
    let delta_pnl = trades
        .iter()
        .map(|trade| trade.pnl - trade.original_pnl)
        .sum::<f64>();
    let mid = trades.len() / 2;
    let (early_win_rate_pct, early_pnl) = summarize_overlay_trades(&trades[..mid]);
    let (late_win_rate_pct, late_pnl) = summarize_overlay_trades(&trades[mid..]);
    let mut without_top5 = trades.clone();
    without_top5.sort_unstable_by(|left, right| right.pnl.total_cmp(&left.pnl));
    let remove_top5_pnl = without_top5
        .iter()
        .skip(5)
        .map(|trade| trade.pnl)
        .sum::<f64>();
    let combo_days = reports.iter().map(|report| report.days).fold(0.0, f64::max);
    KeltnerFailureExitOverlaySummary {
        entries,
        wins,
        losses,
        win_rate_pct: ratio_pct(wins, wins + losses),
        pnl,
        delta_pnl,
        max_drawdown_pct: overlay_max_drawdown_pct(&trades),
        trades_per_day: if combo_days > 0.0 {
            entries as f64 / combo_days
        } else {
            0.0
        },
        early_win_rate_pct,
        early_pnl,
        late_win_rate_pct,
        late_pnl,
        remove_top5_pnl,
        triggered: trades.iter().filter(|trade| trade.triggered).count(),
    }
}

/// 打印 Keltner failure-exit 小网格诊断，保持和现有 scan 输出的单行格式一致。
pub(crate) fn print_keltner_failure_exit_overlay_summaries(
    report_sets: &[(
        KeltnerFailureExitConfig,
        Vec<KeltnerFailureExitOverlayReport>,
    )],
) {
    for (config, reports) in report_sets {
        if reports.is_empty() {
            continue;
        }
        let labels = reports
            .iter()
            .map(|report| report.label.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let summary = summarize_keltner_failure_exit_overlay_reports(reports);
        println!(
            "keltner_failure_exit_overlay bars={} min_progress_r={:.2} labels={} entries={} triggered={} wins={} losses={} win_rate={:.2}% pnl={:.4} delta_pnl={:.4} max_dd={:.2}% trades_per_day={:.2} early_wr={:.2}% early_pnl={:.4} late_wr={:.2}% late_pnl={:.4} remove_top5_pnl={:.4}",
            config.bars,
            config.min_progress_r,
            labels,
            summary.entries,
            summary.triggered,
            summary.wins,
            summary.losses,
            summary.win_rate_pct,
            summary.pnl,
            summary.delta_pnl,
            summary.max_drawdown_pct,
            summary.trades_per_day,
            summary.early_win_rate_pct,
            summary.early_pnl,
            summary.late_win_rate_pct,
            summary.late_pnl,
            summary.remove_top5_pnl
        );
    }
}

fn keltner_position_outcomes(
    candles: &[CandleItem],
    records: &[TradeRecord],
) -> Vec<KeltnerPositionOutcome> {
    let time_index = candle_time_index(candles);
    let mut positions = records
        .iter()
        .filter(|record| record.option_type != "close")
        .filter_map(|record| {
            let side = KeltnerFailureExitSide::from_option_type(&record.option_type)?;
            let entry_index = time_index
                .get(record.open_position_time.as_str())
                .copied()?;
            let reasons = parse_entry_reasons(record.signal_result.as_deref().unwrap_or_default());
            let stop_price = reason_value(&reasons, "STOP_PRICE")?;
            Some((
                record.open_position_time.clone(),
                KeltnerPositionOutcome {
                    open_time: record.open_position_time.clone(),
                    side,
                    entry_index,
                    first_exit_index: None,
                    open_price: record.open_price,
                    stop_price,
                    quantity: record.quantity,
                    original_pnl: 0.0,
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();

    for record in records
        .iter()
        .filter(|record| record.option_type == "close")
    {
        let Some(position) = positions.get_mut(record.open_position_time.as_str()) else {
            continue;
        };
        position.original_pnl += record.profit_loss;
        if let Some(exit_index) = record
            .close_position_time
            .as_deref()
            .and_then(|time| time_index.get(time).copied())
        {
            position.first_exit_index = Some(
                position
                    .first_exit_index
                    .map_or(exit_index, |current| current.min(exit_index)),
            );
        }
    }

    positions.into_values().collect()
}

fn candle_time_index(candles: &[CandleItem]) -> BTreeMap<String, usize> {
    candles
        .iter()
        .enumerate()
        .filter_map(|(index, candle)| {
            rust_quant_common::utils::time::mill_time_to_datetime(candle.ts)
                .ok()
                .map(|time| (time, index))
        })
        .collect()
}

fn parse_entry_reasons(payload: &str) -> Vec<String> {
    serde_json::from_str::<Value>(payload)
        .ok()
        .and_then(|value| {
            value.get("reasons")?.as_array().map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect()
            })
        })
        .unwrap_or_default()
}

fn reason_value(reasons: &[String], prefix: &str) -> Option<f64> {
    reasons
        .iter()
        .find_map(|reason| reason.strip_prefix(prefix)?.strip_prefix(':')?.parse().ok())
}

fn summarize_overlay_trades(trades: &[KeltnerFailureExitTrade]) -> (f64, f64) {
    let wins = trades.iter().filter(|trade| trade.pnl > 0.0).count();
    let losses = trades.iter().filter(|trade| trade.pnl < 0.0).count();
    let pnl = trades.iter().map(|trade| trade.pnl).sum::<f64>();
    (ratio_pct(wins, wins + losses), pnl)
}

fn overlay_max_drawdown_pct(trades: &[KeltnerFailureExitTrade]) -> f64 {
    let mut equity = 100.0;
    let mut peak = equity;
    let mut max_drawdown = 0.0;
    for trade in trades {
        equity += trade.pnl;
        if equity > peak {
            peak = equity;
        }
        if peak > 0.0 {
            let drawdown = (peak - equity) / peak * 100.0;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }
        }
    }
    max_drawdown
}

fn candle_span_days(candles: &[CandleItem]) -> f64 {
    match (candles.first(), candles.last()) {
        (Some(first), Some(last)) if last.ts > first.ts => {
            (last.ts - first.ts) as f64 / 86_400_000.0
        }
        _ => 0.0,
    }
}

fn ratio_pct(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}
