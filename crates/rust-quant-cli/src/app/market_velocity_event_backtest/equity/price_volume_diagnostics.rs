use super::super::{
    relative_volume_at_time::relative_volume_at_time_10d_ratio_raw, BacktestCandle, ConfirmedEvent,
    MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection, MS_15M,
};
use super::{build_framework_equity_trade_reports, FrameworkEquityTradeReport};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// 单笔实际成交在 setup 完成时可见的量价形态，以及仅用于归因的后续三棒路径。
#[derive(Debug, Clone, PartialEq)]
struct PriceVolumeDiagnosticFeatures {
    /// setup 实体占整根振幅的比例，用来区分价格有效推进与高量低效率吸收。
    body_ratio: f64,
    /// 收盘相对趋势延伸极值退回的比例；越高表示越接近反转方向收盘。
    directional_close_rejection_ratio: f64,
    /// 反转方向影线占整根振幅的比例，用来识别当棒拒绝而非实体延续。
    directional_wick_ratio: f64,
    /// 当前成交量除以本策略冻结的连续均量或 RVAT10 均量；历史不足时不输出量比桶。
    volume_ratio: Option<f64>,
    /// 当前振幅除以前 `entry_period` 根平均振幅；历史不足时不输出扩张桶。
    range_expansion_ratio: Option<f64>,
    /// setup 实体相对当前策略方向的关系；延续策略单独标记历史趋势同向推进。
    body_alignment: &'static str,
    /// setup 后三根收盘是否收回 setup 开盘价；该字段只能用于诊断，不能回填原入场。
    setup_open_reclaim_3: &'static str,
}

/// 一个互斥诊断桶内实际已成交交易的固定初始 R 汇总。
#[derive(Debug, Default)]
struct DiagnosticStats {
    /// 该桶覆盖的实际交易对，避免把交易数误当成币种覆盖。
    symbols: BTreeSet<String>,
    /// 固定初始 R 完整的实际成交数量。
    trades: usize,
    /// 净 R 大于零的成交数量。
    wins: usize,
    /// 净 R 小于零的成交数量。
    losses: usize,
    /// 扣除回测成本后的固定初始 R 合计。
    net_sum_r: f64,
    /// 所有正净 R 的合计，用于计算 Profit Factor。
    gross_profit_r: f64,
    /// 所有负净 R 的绝对值合计，用于计算 Profit Factor。
    gross_loss_r: f64,
}

impl DiagnosticStats {
    /// 只累计基线回放真实产生且具备固定初始 R 的成交，不重新打开被持仓锁跳过的信号。
    fn observe(&mut self, symbol: &str, net_r: f64) {
        if !net_r.is_finite() {
            return;
        }
        self.symbols.insert(symbol.to_string());
        self.trades += 1;
        self.net_sum_r += net_r;
        if net_r > 0.0 {
            self.wins += 1;
            self.gross_profit_r += net_r;
        } else if net_r < 0.0 {
            self.losses += 1;
            self.gross_loss_r += -net_r;
        }
    }

    /// 返回该桶的每笔净期望；空桶不产生误导性的零值。
    fn expectancy_r(&self) -> Option<f64> {
        (self.trades > 0).then_some(self.net_sum_r / self.trades as f64)
    }

    /// 返回固定初始 R Profit Factor；没有亏损样本时保持为空。
    fn profit_factor(&self) -> Option<f64> {
        (self.gross_loss_r > 0.0).then_some(self.gross_profit_r / self.gross_loss_r)
    }

    /// 返回已决交易胜率；零收益交易不进入分母。
    fn win_rate(&self) -> Option<f64> {
        let resolved = self.wins + self.losses;
        (resolved > 0).then_some(self.wins as f64 / resolved as f64 * 100.0)
    }
}

/// 打印实际成交的 15m 量价诊断；未来三棒只做归因，禁止据此改写这些交易的入场。
pub(super) fn print_price_volume_diagnostic_reports(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
) {
    let trades = build_framework_equity_trade_reports(confirmed, candles_15m, target_r, args);
    let mut buckets = BTreeMap::<(&'static str, &'static str, bool), DiagnosticStats>::new();
    let mut missing_fixed_r = 0usize;
    let mut missing_setup = 0usize;

    for trade in &trades {
        let Some(net_r) = trade.net_profit_r else {
            missing_fixed_r += 1;
            continue;
        };
        let Some(features) = diagnostic_features_for_trade(trade, candles_15m, args) else {
            missing_setup += 1;
            continue;
        };
        observe_bucket(
            &mut buckets,
            "direction",
            direction_bucket(trade.direction),
            true,
            trade,
            net_r,
        );
        observe_bucket(
            &mut buckets,
            "setup_body_alignment",
            features.body_alignment,
            true,
            trade,
            net_r,
        );
        observe_bucket(
            &mut buckets,
            "setup_body_ratio",
            four_part_ratio_bucket(features.body_ratio),
            true,
            trade,
            net_r,
        );
        observe_bucket(
            &mut buckets,
            "directional_close_rejection_ratio",
            four_part_ratio_bucket(features.directional_close_rejection_ratio),
            true,
            trade,
            net_r,
        );
        observe_bucket(
            &mut buckets,
            "directional_wick_ratio",
            wick_ratio_bucket(features.directional_wick_ratio),
            true,
            trade,
            net_r,
        );
        if let Some(volume_ratio) = features.volume_ratio {
            observe_bucket(
                &mut buckets,
                if args.entry_relative_volume_at_time_10d {
                    "relative_volume_at_time_10d_ratio"
                } else {
                    "volume_ratio"
                },
                volume_ratio_bucket(volume_ratio),
                true,
                trade,
                net_r,
            );
        }
        if let Some(range_expansion_ratio) = features.range_expansion_ratio {
            observe_bucket(
                &mut buckets,
                "range_expansion_ratio",
                range_expansion_bucket(range_expansion_ratio),
                true,
                trade,
                net_r,
            );
        }
        observe_bucket(
            &mut buckets,
            "setup_open_reclaim_3",
            features.setup_open_reclaim_3,
            false,
            trade,
            net_r,
        );
    }

    println!(
        "framework_equity_15m_diagnostic_summary\ttarget={}R\tactual_trades={}\tmissing_fixed_r={}\tmissing_setup={}\treplay_filtered_signals=false\tfuture_path_changes_entry=false",
        target_r,
        trades.len(),
        missing_fixed_r,
        missing_setup,
    );
    for ((feature, bucket, causal_at_entry), stats) in buckets {
        println!(
            "framework_equity_15m_diagnostic_result\ttarget={}R\tfeature={}\tbucket={}\tcausal_at_entry={}\tsymbols={}\ttrades={}\tnet_sum_r={}\tnet_expectancy_r={}\tnet_profit_factor={}\twin_rate={}",
            target_r,
            feature,
            bucket,
            causal_at_entry,
            stats.symbols.len(),
            stats.trades,
            stats.net_sum_r,
            format_optional(stats.expectancy_r()),
            format_optional(stats.profit_factor()),
            format_optional(stats.win_rate()),
        );
    }
}

/// 从原始 setup K 线提取特征；下一至三棒的结果单独标记为非入场时可见。
fn diagnostic_features_for_trade(
    trade: &FrameworkEquityTradeReport,
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    args: &MarketVelocityEventBacktestArgs,
) -> Option<PriceVolumeDiagnosticFeatures> {
    let candles = candles_15m.get(&trade.symbol)?;
    let setup_idx = candles.partition_point(|candle| candle.ts + MS_15M <= trade.signal_ts);
    let setup_idx = setup_idx.checked_sub(1)?;
    let setup = candles.get(setup_idx)?;
    if setup.ts + MS_15M != trade.signal_ts {
        return None;
    }
    let range = setup.high - setup.low;
    if !range.is_finite() || range <= 0.0 {
        return None;
    }
    let body_ratio = (setup.close - setup.open).abs() / range;
    let (directional_close_rejection_ratio, directional_wick_ratio, body_alignment) =
        match trade.direction {
            MarketVelocityTradeDirection::Long => (
                (setup.close - setup.low) / range,
                (setup.open.min(setup.close) - setup.low) / range,
                if setup.close > setup.open {
                    if args.entry_extreme_volume_continuation {
                        "historical_trend_continuation"
                    } else {
                        "reversal_direction"
                    }
                } else {
                    "prior_trend_continuation"
                },
            ),
            MarketVelocityTradeDirection::Short => (
                (setup.high - setup.close) / range,
                (setup.high - setup.open.max(setup.close)) / range,
                if setup.close < setup.open {
                    if args.entry_extreme_volume_continuation {
                        "historical_trend_continuation"
                    } else {
                        "reversal_direction"
                    }
                } else {
                    "prior_trend_continuation"
                },
            ),
            MarketVelocityTradeDirection::Both => return None,
        };
    let previous = (args.entry_period > 0 && setup_idx >= args.entry_period)
        .then(|| &candles[setup_idx - args.entry_period..setup_idx]);
    let volume_ratio = if args.entry_relative_volume_at_time_10d {
        relative_volume_at_time_10d_ratio_raw(candles, setup_idx)
    } else {
        previous
            .and_then(|items| positive_average(items.iter().map(|candle| candle.volume)))
            .filter(|average| *average > 0.0)
            .map(|average| setup.volume / average)
    };
    let range_expansion_ratio = previous
        .and_then(|items| positive_average(items.iter().map(|candle| candle.high - candle.low)))
        .filter(|average| *average > 0.0)
        .map(|average| range / average);
    Some(PriceVolumeDiagnosticFeatures {
        body_ratio,
        directional_close_rejection_ratio,
        directional_wick_ratio,
        volume_ratio,
        range_expansion_ratio,
        body_alignment,
        setup_open_reclaim_3: setup_open_reclaim_bucket(candles, setup_idx, trade.direction),
    })
}

/// 把同一特征桶绑定到基线实际成交，避免子集重放改变同币持仓锁和样本身份。
fn observe_bucket(
    buckets: &mut BTreeMap<(&'static str, &'static str, bool), DiagnosticStats>,
    feature: &'static str,
    bucket: &'static str,
    causal_at_entry: bool,
    trade: &FrameworkEquityTradeReport,
    net_r: f64,
) {
    buckets
        .entry((feature, bucket, causal_at_entry))
        .or_default()
        .observe(&trade.symbol, net_r);
}

/// 计算严格正数序列均值；零成交量或无效振幅会让该特征保持缺失而不是伪造零值。
fn positive_average(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut count = 0usize;
    let mut sum = 0.0;
    for value in values {
        if !value.is_finite() || value <= 0.0 {
            return None;
        }
        count += 1;
        sum += value;
    }
    (count > 0).then_some(sum / count as f64)
}

/// 只按后续已完成 15m 收盘判断 setup 开盘价是否被收回，不使用棒内路径猜测成交。
fn setup_open_reclaim_bucket(
    candles: &[BacktestCandle],
    setup_idx: usize,
    direction: MarketVelocityTradeDirection,
) -> &'static str {
    let Some(setup) = candles.get(setup_idx) else {
        return "insufficient_future_3";
    };
    let future = candles
        .iter()
        .skip(setup_idx + 1)
        .take(3)
        .collect::<Vec<_>>();
    if future.len() < 3 {
        return "insufficient_future_3";
    }
    for (offset, candle) in future.into_iter().enumerate() {
        let reclaimed = match direction {
            MarketVelocityTradeDirection::Long => candle.close > setup.open,
            MarketVelocityTradeDirection::Short => candle.close < setup.open,
            MarketVelocityTradeDirection::Both => false,
        };
        if reclaimed {
            return if offset == 0 { "next_bar" } else { "bars_2_3" };
        }
    }
    "not_within_3"
}

fn direction_bucket(direction: MarketVelocityTradeDirection) -> &'static str {
    match direction {
        MarketVelocityTradeDirection::Long => "long",
        MarketVelocityTradeDirection::Short => "short",
        MarketVelocityTradeDirection::Both => "both",
    }
}

fn four_part_ratio_bucket(value: f64) -> &'static str {
    if value < 0.20 {
        "lt20pct"
    } else if value < 0.50 {
        "20_50pct"
    } else if value < 0.80 {
        "50_80pct"
    } else {
        "80pct_plus"
    }
}

fn wick_ratio_bucket(value: f64) -> &'static str {
    if value < 0.10 {
        "lt10pct"
    } else if value < 0.25 {
        "10_25pct"
    } else if value < 0.40 {
        "25_40pct"
    } else {
        "40pct_plus"
    }
}

fn volume_ratio_bucket(value: f64) -> &'static str {
    if value < 2.0 {
        "lt2x"
    } else if value < 3.0 {
        "2_3x"
    } else if value < 5.0 {
        "3_5x"
    } else {
        "5x_plus"
    }
}

fn range_expansion_bucket(value: f64) -> &'static str {
    if value < 1.4 {
        "lt1_4x"
    } else if value < 2.0 {
        "1_4_2x"
    } else if value < 3.0 {
        "2_3x"
    } else {
        "3x_plus"
    }
}

fn format_optional(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_string(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(ts: i64, open: f64, high: f64, low: f64, close: f64) -> BacktestCandle {
        BacktestCandle {
            ts,
            open,
            high,
            low,
            close,
            volume: 10.0,
        }
    }

    #[test]
    fn setup_open_reclaim_uses_only_later_completed_closes() {
        let candles = vec![
            candle(0, 100.0, 102.0, 90.0, 91.0),
            candle(MS_15M, 91.0, 99.0, 90.0, 98.0),
            candle(MS_15M * 2, 98.0, 102.0, 97.0, 101.0),
            candle(MS_15M * 3, 101.0, 103.0, 100.0, 102.0),
        ];
        assert_eq!(
            setup_open_reclaim_bucket(&candles, 0, MarketVelocityTradeDirection::Long),
            "bars_2_3"
        );
    }

    #[test]
    fn diagnostic_stats_use_fixed_initial_r_without_subset_replay() {
        let mut stats = DiagnosticStats::default();
        stats.observe("BTC-USDT-SWAP", 2.0);
        stats.observe("BTC-USDT-SWAP", -1.0);
        assert_eq!(stats.trades, 2);
        assert_eq!(stats.symbols.len(), 1);
        assert_eq!(stats.expectancy_r(), Some(0.5));
        assert_eq!(stats.profit_factor(), Some(2.0));
        assert_eq!(stats.win_rate(), Some(50.0));
    }
}
