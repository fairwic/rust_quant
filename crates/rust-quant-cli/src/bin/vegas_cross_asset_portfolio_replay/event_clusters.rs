use super::{quoted_4h_candle_table, wilson_interval_95, ActivePosition, CandidateTrade};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const FOUR_HOURS_MS: i64 = 4 * 60 * 60 * 1_000;
const EVENT_WINDOW_MS: i64 = 12 * 60 * 60 * 1_000;
const CORRELATION_DAYS: i64 = 90;
const CORRELATION_THRESHOLD: f64 = 0.70;
const COMPLETE_DAY_MASK: u8 = 0b11_1111;

/// 信号时点前已完成 BTC 4H K 线对应的 Vegas 长周期状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum BtcRegime {
    Bull,
    Bear,
    Neutral,
}

/// 一个按冻结规则归并的市场事件；收益为固定执行集合的实际组合贡献。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct EventCluster {
    pub(super) event_id: usize,
    pub(super) anchor_open_ts: i64,
    pub(super) side: String,
    pub(super) btc_regime: Option<BtcRegime>,
    pub(super) trades: usize,
    pub(super) symbols: Vec<String>,
    pub(super) net_profit: f64,
    pub(super) minimum_anchor_correlation: Option<f64>,
}

/// 事件口径样本量、集中度和移除头部贡献审计。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct EventClusterReport {
    pub(super) event_window_hours: usize,
    pub(super) trailing_complete_days: usize,
    pub(super) correlation_threshold: f64,
    pub(super) sector_mapping_available: bool,
    pub(super) accepted_trades: usize,
    pub(super) effective_events: usize,
    pub(super) effective_events_per_month: Option<f64>,
    pub(super) profitable_events: usize,
    pub(super) event_win_rate_pct: f64,
    pub(super) event_win_rate_wilson_95_low_pct: Option<f64>,
    pub(super) event_win_rate_wilson_95_high_pct: Option<f64>,
    pub(super) correlation_comparisons: usize,
    pub(super) missing_90d_comparisons: usize,
    pub(super) largest_positive_cluster_profit: Option<f64>,
    pub(super) largest_positive_cluster_profit_share_pct: Option<f64>,
    pub(super) fixed_execution_net_profit: f64,
    pub(super) fixed_execution_profit_without_top1_cluster: f64,
    pub(super) fixed_execution_profit_without_top3_clusters: f64,
    pub(super) by_btc_regime: Vec<RegimeEventReport>,
    pub(super) clusters: Vec<EventCluster>,
}

/// BTC 因果市场状态下的交易、有效事件和收益覆盖。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct RegimeEventReport {
    pub(super) btc_regime: String,
    pub(super) trades: usize,
    pub(super) effective_events: usize,
    pub(super) profitable_events: usize,
    pub(super) net_profit: f64,
}

/// 只读预加载的 BTC 状态和各币种完整 UTC 日收益，避免聚类过程中逐笔访问数据库。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct EventContext {
    btc_regimes: BTreeMap<i64, BtcRegime>,
    daily_returns: HashMap<String, BTreeMap<i64, f64>>,
}

/// 从确认 4H K 线构建因果事件上下文；查询范围止于最后一笔候选开仓。
pub(super) async fn load_event_context(
    pool: &PgPool,
    trades: &[CandidateTrade],
) -> Result<EventContext> {
    if trades.is_empty() {
        return Ok(EventContext {
            btc_regimes: BTreeMap::new(),
            daily_returns: HashMap::new(),
        });
    }
    let min_open_ts = trades
        .iter()
        .map(|trade| trade.open_ts)
        .min()
        .expect("non-empty trades");
    let max_open_ts = trades
        .iter()
        .map(|trade| trade.open_ts)
        .max()
        .expect("non-empty trades");
    let btc_regimes = load_btc_regimes(pool, max_open_ts).await?;
    let mut symbols = trades
        .iter()
        .map(|trade| trade.symbol.clone())
        .collect::<BTreeSet<_>>();
    symbols.insert("BTC-USDT-SWAP".to_string());
    let mut daily_returns = HashMap::with_capacity(symbols.len());
    let history_start = min_open_ts - (CORRELATION_DAYS + 2) * DAY_MS;
    for symbol in symbols {
        daily_returns.insert(
            symbol.clone(),
            load_complete_daily_returns(pool, &symbol, history_start, max_open_ts).await?,
        );
    }
    Ok(EventContext {
        btc_regimes,
        daily_returns,
    })
}

/// 对共享账户实际接纳的仓位按预声明规则聚类，不使用最终收益决定归组。
pub(super) fn build_event_cluster_report(
    positions: &[ActivePosition],
    context: &EventContext,
) -> Result<EventClusterReport> {
    let mut ordered = positions.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.trade
            .open_ts
            .cmp(&right.trade.open_ts)
            .then_with(|| left.trade.detail_id.cmp(&right.trade.detail_id))
    });
    let mut assigned = vec![false; ordered.len()];
    let mut clusters = Vec::new();
    let mut correlation_comparisons = 0_usize;
    let mut missing_90d_comparisons = 0_usize;

    for anchor_index in 0..ordered.len() {
        if assigned[anchor_index] {
            continue;
        }
        assigned[anchor_index] = true;
        let anchor = ordered[anchor_index];
        let anchor_regime = context.regime_before(anchor.trade.open_ts);
        let mut member_indices = vec![anchor_index];
        let mut correlations = Vec::new();
        for candidate_index in (anchor_index + 1)..ordered.len() {
            if assigned[candidate_index] {
                continue;
            }
            let candidate = ordered[candidate_index];
            if candidate.trade.open_ts > anchor.trade.open_ts + EVENT_WINDOW_MS {
                break;
            }
            if candidate.trade.side != anchor.trade.side
                || anchor_regime.is_none()
                || context.regime_before(candidate.trade.open_ts) != anchor_regime
            {
                continue;
            }
            correlation_comparisons += 1;
            let correlation = context.trailing_correlation(
                &anchor.trade.symbol,
                &candidate.trade.symbol,
                candidate.trade.open_ts,
            );
            let Some(correlation) = correlation else {
                missing_90d_comparisons += 1;
                continue;
            };
            if correlation >= CORRELATION_THRESHOLD {
                assigned[candidate_index] = true;
                member_indices.push(candidate_index);
                correlations.push(correlation);
            }
        }
        let mut symbols = member_indices
            .iter()
            .map(|index| ordered[*index].trade.symbol.clone())
            .collect::<Vec<_>>();
        symbols.sort();
        symbols.dedup();
        let net_profit = member_indices
            .iter()
            .map(|index| ordered[*index].entry_equity * ordered[*index].trade.normalized_return)
            .sum();
        clusters.push(EventCluster {
            event_id: clusters.len() + 1,
            anchor_open_ts: anchor.trade.open_ts,
            side: anchor.trade.side.clone(),
            btc_regime: anchor_regime,
            trades: member_indices.len(),
            symbols,
            net_profit,
            minimum_anchor_correlation: correlations.into_iter().reduce(f64::min),
        });
    }
    Ok(summarize_clusters(
        ordered.as_slice(),
        clusters,
        correlation_comparisons,
        missing_90d_comparisons,
    ))
}

impl EventContext {
    fn regime_before(&self, open_ts: i64) -> Option<BtcRegime> {
        self.btc_regimes
            .range(..open_ts)
            .next_back()
            .map(|(_, regime)| *regime)
    }

    fn trailing_correlation(&self, left: &str, right: &str, open_ts: i64) -> Option<f64> {
        let left = self.daily_returns.get(left)?;
        let right = self.daily_returns.get(right)?;
        let cutoff_day = open_ts.div_euclid(DAY_MS);
        let mut left_values = Vec::with_capacity(CORRELATION_DAYS as usize);
        let mut right_values = Vec::with_capacity(CORRELATION_DAYS as usize);
        for day in (cutoff_day - CORRELATION_DAYS)..cutoff_day {
            left_values.push(*left.get(&day)?);
            right_values.push(*right.get(&day)?);
        }
        pearson_correlation(&left_values, &right_values)
    }
}

async fn load_btc_regimes(pool: &PgPool, max_open_ts: i64) -> Result<BTreeMap<i64, BtcRegime>> {
    let rows = sqlx::query(
        r#"
        SELECT ts, c::double precision AS close
          FROM "btc-usdt-swap_candles_4h"
         WHERE ts < $1 AND confirm = '1'
         ORDER BY ts
        "#,
    )
    .bind(max_open_ts)
    .fetch_all(pool)
    .await
    .context("load BTC 4H regime candles")?;
    let mut ema288 = None::<f64>;
    let mut ema338 = None::<f64>;
    let mut regimes = BTreeMap::new();
    for (index, row) in rows.into_iter().enumerate() {
        let ts = row.try_get::<i64, _>("ts")?;
        let close = row.try_get::<f64, _>("close")?;
        if !close.is_finite() || close <= 0.0 {
            bail!("invalid BTC close at {ts}");
        }
        ema288 = Some(update_ema(ema288, close, 288));
        ema338 = Some(update_ema(ema338, close, 338));
        if index + 1 < 338 {
            continue;
        }
        let regime = if close > ema288.expect("initialized") && close > ema338.expect("initialized")
        {
            BtcRegime::Bull
        } else if close < ema288.expect("initialized") && close < ema338.expect("initialized") {
            BtcRegime::Bear
        } else {
            BtcRegime::Neutral
        };
        regimes.insert(ts, regime);
    }
    Ok(regimes)
}

async fn load_complete_daily_returns(
    pool: &PgPool,
    symbol: &str,
    start_ts: i64,
    end_ts: i64,
) -> Result<BTreeMap<i64, f64>> {
    let table = quoted_4h_candle_table(symbol)?;
    let query = format!(
        "SELECT ts, c::double precision AS close FROM {table} \
         WHERE ts >= $1 AND ts < $2 AND confirm = '1' ORDER BY ts"
    );
    let rows = sqlx::query(&query)
        .bind(start_ts)
        .bind(end_ts)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load complete UTC days for {symbol}"))?;
    let mut days = BTreeMap::<i64, (u8, i64, f64)>::new();
    for row in rows {
        let ts = row.try_get::<i64, _>("ts")?;
        let close = row.try_get::<f64, _>("close")?;
        let offset = ts.rem_euclid(DAY_MS);
        if !close.is_finite() || close <= 0.0 || offset % FOUR_HOURS_MS != 0 {
            continue;
        }
        let slot = offset / FOUR_HOURS_MS;
        if !(0..6).contains(&slot) {
            continue;
        }
        let day = ts.div_euclid(DAY_MS);
        let entry = days.entry(day).or_insert((0, ts, close));
        entry.0 |= 1 << slot;
        if ts >= entry.1 {
            entry.1 = ts;
            entry.2 = close;
        }
    }
    let complete_closes = days
        .into_iter()
        .filter_map(|(day, (mask, _, close))| (mask == COMPLETE_DAY_MASK).then_some((day, close)))
        .collect::<BTreeMap<_, _>>();
    let mut returns = BTreeMap::new();
    for (&day, &close) in &complete_closes {
        if let Some(previous_close) = complete_closes.get(&(day - 1)) {
            returns.insert(day, close / previous_close - 1.0);
        }
    }
    Ok(returns)
}

fn summarize_clusters(
    ordered: &[&ActivePosition],
    clusters: Vec<EventCluster>,
    correlation_comparisons: usize,
    missing_90d_comparisons: usize,
) -> EventClusterReport {
    let fixed_execution_net_profit = clusters.iter().map(|cluster| cluster.net_profit).sum();
    let profitable_events = clusters
        .iter()
        .filter(|cluster| cluster.net_profit > 0.0)
        .count();
    let interval = wilson_interval_95(profitable_events, clusters.len());
    let mut positive_profits = clusters
        .iter()
        .map(|cluster| cluster.net_profit)
        .filter(|profit| *profit > 0.0)
        .collect::<Vec<_>>();
    positive_profits.sort_by(|left, right| right.total_cmp(left));
    let top1 = positive_profits.first().copied().unwrap_or(0.0);
    let top3 = positive_profits.iter().take(3).sum::<f64>();
    let positive_total = positive_profits.iter().sum::<f64>();
    let active_months = ordered
        .first()
        .zip(ordered.last())
        .and_then(|(first, last)| {
            let months =
                (last.trade.open_ts - first.trade.open_ts) as f64 / DAY_MS as f64 / 30.4375;
            (months > 0.0).then_some(months)
        });
    let mut by_regime = BTreeMap::<String, (usize, usize, usize, f64)>::new();
    for cluster in &clusters {
        let label = match cluster.btc_regime {
            Some(BtcRegime::Bull) => "bull",
            Some(BtcRegime::Bear) => "bear",
            Some(BtcRegime::Neutral) => "neutral",
            None => "unavailable",
        };
        let entry = by_regime.entry(label.to_string()).or_default();
        entry.0 += cluster.trades;
        entry.1 += 1;
        entry.2 += usize::from(cluster.net_profit > 0.0);
        entry.3 += cluster.net_profit;
    }
    let by_btc_regime = by_regime
        .into_iter()
        .map(
            |(btc_regime, (trades, effective_events, profitable_events, net_profit))| {
                RegimeEventReport {
                    btc_regime,
                    trades,
                    effective_events,
                    profitable_events,
                    net_profit,
                }
            },
        )
        .collect();
    EventClusterReport {
        event_window_hours: 12,
        trailing_complete_days: CORRELATION_DAYS as usize,
        correlation_threshold: CORRELATION_THRESHOLD,
        sector_mapping_available: false,
        accepted_trades: ordered.len(),
        effective_events: clusters.len(),
        effective_events_per_month: active_months.map(|months| clusters.len() as f64 / months),
        profitable_events,
        event_win_rate_pct: if clusters.is_empty() {
            0.0
        } else {
            profitable_events as f64 / clusters.len() as f64 * 100.0
        },
        event_win_rate_wilson_95_low_pct: interval.map(|value| value.0),
        event_win_rate_wilson_95_high_pct: interval.map(|value| value.1),
        correlation_comparisons,
        missing_90d_comparisons,
        largest_positive_cluster_profit: positive_profits.first().copied(),
        largest_positive_cluster_profit_share_pct: (positive_total > 0.0)
            .then_some(top1 / positive_total * 100.0),
        fixed_execution_net_profit,
        fixed_execution_profit_without_top1_cluster: fixed_execution_net_profit - top1,
        fixed_execution_profit_without_top3_clusters: fixed_execution_net_profit - top3,
        by_btc_regime,
        clusters,
    }
}

fn update_ema(previous: Option<f64>, close: f64, period: usize) -> f64 {
    let alpha = 2.0 / (period as f64 + 1.0);
    previous.map_or(close, |ema| close * alpha + ema * (1.0 - alpha))
}

fn pearson_correlation(left: &[f64], right: &[f64]) -> Option<f64> {
    if left.len() != right.len() || left.len() < 2 {
        return None;
    }
    let left_mean = left.iter().sum::<f64>() / left.len() as f64;
    let right_mean = right.iter().sum::<f64>() / right.len() as f64;
    let covariance = left
        .iter()
        .zip(right)
        .map(|(left, right)| (left - left_mean) * (right - right_mean))
        .sum::<f64>();
    let left_variance = left
        .iter()
        .map(|value| (value - left_mean).powi(2))
        .sum::<f64>();
    let right_variance = right
        .iter()
        .map(|value| (value - right_mean).powi(2))
        .sum::<f64>();
    if left_variance <= 1e-24 || right_variance <= 1e-24 {
        return None;
    }
    let denominator = (left_variance * right_variance).sqrt();
    Some((covariance / denominator).clamp(-1.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlation_requires_every_complete_day_in_the_frozen_window() {
        let mut left = BTreeMap::new();
        let mut right = BTreeMap::new();
        for day in 10..100 {
            left.insert(day, day as f64 / 1000.0);
            right.insert(day, day as f64 / 500.0);
        }
        let context = EventContext {
            btc_regimes: BTreeMap::new(),
            daily_returns: HashMap::from([("A".to_string(), left), ("B".to_string(), right)]),
        };

        assert!(
            (context
                .trailing_correlation("A", "B", 100 * DAY_MS)
                .unwrap()
                - 1.0)
                .abs()
                < 1e-12
        );
        assert!(context
            .trailing_correlation("A", "B", 101 * DAY_MS)
            .is_none());
    }

    #[test]
    fn constant_returns_do_not_create_a_false_correlation_cluster() {
        assert!(pearson_correlation(&[0.01; 90], &[0.02; 90]).is_none());
    }
}
