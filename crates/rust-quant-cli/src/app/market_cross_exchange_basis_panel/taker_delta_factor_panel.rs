use super::binance_klines::{BinanceCandle, BinanceKlineAudit};
use super::{
    load_binance_klines, load_okx_candles, CrossExchangeBasisPanelArgs, HistoricalUniverseManifest,
    UniverseSchedule, DAY_MS, MS_15M, MS_4H,
};
use anyhow::{Context, Result};
use rust_quant_strategies::CandleItem;
use sqlx::postgres::PgPoolOptions;
use std::collections::BTreeMap;

const RULE_VERSION: &str = "okx15m_binance15m_cumulative_taker_delta_1h_incremental_factor_v1";
const CURRENT_FACTOR_BARS: usize = 4;
const BASELINE_HOURS: usize = 20;
const BASELINE_BARS: usize = BASELINE_HOURS * CURRENT_FACTOR_BARS;
const FACTOR_LOOKBACK_BARS: usize = BASELINE_BARS + CURRENT_FACTOR_BARS;
const FORWARD_1H_BARS: usize = 4;
const FORWARD_4H_BARS: usize = 16;
const MIN_FACTOR_COVERAGE: f64 = 0.80;
const MIN_PAIRED_POINTS: usize = 100;
const ECONOMIC_EDGE_RATE: f64 = 0.0016;

/// 记录累计 Delta 因子从同步覆盖到配对比较的完整漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TakerDeltaFactorStages {
    /// 年度窗口内 UTC 4 小时决策时点数。
    pub decision_points: usize,
    /// 同步可见因子低于当月成员 80% 的时点数。
    pub coverage_blocked: usize,
    /// 通过覆盖门后、只使用决策前数据形成的因子观察数。
    pub factor_observations: usize,
    /// 缺少严格连续未来 1h/4h outcome 的观察数。
    pub incomplete_outcomes: usize,
    /// 具有完整 1h/4h outcome 的四象限观察数。
    pub outcome_observations: usize,
    /// 下跌背景中至少一个可见分层同时包含背离与同向组的时点数。
    pub paired_long_points: usize,
    /// 上涨背景中至少一个可见分层同时包含背离与同向组的时点数。
    pub paired_short_points: usize,
}

/// 一个价格方向与 Delta 方向象限的毛 forward return 摘要。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TakerDeltaFactorSummary {
    /// 当前象限的 symbol-time 观察数。
    pub observations: usize,
    /// 按反转方向解释的平均未来 1h 收益。
    pub mean_directed_1h: Option<f64>,
    /// 按反转方向解释的平均未来 4h 收益。
    pub mean_directed_4h: Option<f64>,
    /// 未来 1h 方向收益为正的比例，单位百分比。
    pub positive_rate_1h_pct: Option<f64>,
    /// 未来 4h 方向收益为正的比例，单位百分比。
    pub positive_rate_4h_pct: Option<f64>,
}

/// 按决策时间、价格幅度与相对总量控制后的 Delta 增量摘要。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TakerDeltaPairedSummary {
    /// 至少存在一个可配对分层的独立决策时点数。
    pub decision_points: usize,
    /// 背离组减同向组的平均未来 1h 方向收益差。
    pub mean_spread_1h: Option<f64>,
    /// 背离组减同向组的平均未来 4h 方向收益差。
    pub mean_spread_4h: Option<f64>,
    /// 1h 配对差为正的决策时点比例，单位百分比。
    pub positive_rate_1h_pct: Option<f64>,
    /// 4h 配对差为正的决策时点比例，单位百分比。
    pub positive_rate_4h_pct: Option<f64>,
}

/// 单个冻结年度窗口的累计 Delta 增量因子报告。
#[derive(Debug, Clone, PartialEq)]
pub struct TakerDeltaFactorPanelReport {
    /// 冻结因子规则身份。
    pub rule_version: String,
    /// current-live crypto-only 历史币池版本。
    pub universe_version: String,
    /// OKX 月度成员的唯一并集数量。
    pub symbols: usize,
    /// Binance 当前合约映射、官方月包和解析行审计。
    pub binance_audit: BinanceKlineAudit,
    /// 覆盖、方向、outcome 与配对漏斗。
    pub stages: TakerDeltaFactorStages,
    /// 下跌价格、正 Delta 的反转多背离象限。
    pub down_price_positive_delta: TakerDeltaFactorSummary,
    /// 下跌价格、负 Delta 的同向价格对照象限。
    pub down_price_negative_delta: TakerDeltaFactorSummary,
    /// 上涨价格、负 Delta 的反转空背离象限。
    pub up_price_negative_delta: TakerDeltaFactorSummary,
    /// 上涨价格、正 Delta 的同向价格对照象限。
    pub up_price_positive_delta: TakerDeltaFactorSummary,
    /// 全年度下跌背景配对增量。
    pub long_paired_overall: TakerDeltaPairedSummary,
    /// 前半年下跌背景配对增量。
    pub long_paired_first_half: TakerDeltaPairedSummary,
    /// 后半年下跌背景配对增量。
    pub long_paired_second_half: TakerDeltaPairedSummary,
    /// 全年度上涨背景配对增量。
    pub short_paired_overall: TakerDeltaPairedSummary,
    /// 前半年上涨背景配对增量。
    pub short_paired_first_half: TakerDeltaPairedSummary,
    /// 后半年上涨背景配对增量。
    pub short_paired_second_half: TakerDeltaPairedSummary,
    /// 是否通过当前年度全部预注册信息增量门槛。
    pub factor_gate_passed: bool,
}

/// 因子面板只区分价格上涨和下跌，不把零变化强行归类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PriceDirection {
    Down,
    Up,
}

/// 决策时间可见的 1h 价格、累计 Delta 与相对总量状态。
#[derive(Debug, Clone, PartialEq)]
struct VisibleFactor {
    symbol: String,
    decision_ts: i64,
    price_direction: PriceDirection,
    divergent: bool,
    absolute_price_move: f64,
    relative_volume: f64,
    flow_1h: f64,
}

/// 可见因子附加未来 1h/4h 毛 outcome 后的离线观察。
#[derive(Debug, Clone, PartialEq)]
struct FactorObservation {
    symbol: String,
    decision_ts: i64,
    price_direction: PriceDirection,
    divergent: bool,
    absolute_price_move: f64,
    relative_volume: f64,
    flow_1h: f64,
    directed_1h: f64,
    directed_4h: f64,
}

/// 单个决策时点在相同价格方向下的背离减同向收益差。
#[derive(Debug, Clone, Copy, PartialEq)]
struct PairedPoint {
    decision_ts: i64,
    price_direction: PriceDirection,
    spread_1h: f64,
    spread_4h: f64,
}

/// 一个价格幅度和相对总量分层内的两组 outcome 累加器。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct StratumAccumulator {
    divergent_count: usize,
    divergent_sum_1h: f64,
    divergent_sum_4h: f64,
    aligned_count: usize,
    aligned_sum_1h: f64,
    aligned_sum_4h: f64,
}

/// 运行冻结的多棒累计 Delta 增量因子面板，不产生交易或数据库写入。
pub async fn run_taker_delta_factor_panel(
    args: &CrossExchangeBasisPanelArgs,
    database_url: &str,
) -> Result<TakerDeltaFactorPanelReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode cumulative taker-delta universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let first = schedule
        .windows
        .first()
        .context("missing first universe window")?;
    let last = schedule
        .windows
        .last()
        .context("missing last universe window")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for cumulative taker-delta factor panel")?;
    let mut okx = BTreeMap::<String, Vec<CandleItem>>::new();
    for symbol in schedule.union_symbols() {
        okx.insert(
            symbol.clone(),
            load_okx_candles(
                &pool,
                &symbol,
                first.from_ms.saturating_sub(2 * DAY_MS),
                last.to_ms.saturating_add(DAY_MS),
            )
            .await?,
        );
    }
    let (binance, binance_audit) = load_binance_klines(args, &schedule).await?;
    let (observations, paired_points, stages) = build_panel(&schedule, &okx, &binance);
    let split_ms = schedule.windows[6].from_ms;
    let report = build_report(
        &schedule,
        okx.len(),
        binance_audit,
        stages,
        &observations,
        &paired_points,
        split_ms,
    );
    print_report(&report);
    Ok(report)
}

/// 以 4h 决策屏障构造可见因子、固定 outcome 和 time-level 配对差。
fn build_panel(
    schedule: &UniverseSchedule,
    okx: &BTreeMap<String, Vec<CandleItem>>,
    binance: &BTreeMap<String, Vec<BinanceCandle>>,
) -> (
    Vec<FactorObservation>,
    Vec<PairedPoint>,
    TakerDeltaFactorStages,
) {
    let mut observations = Vec::new();
    let mut paired_points = Vec::new();
    let mut stages = TakerDeltaFactorStages::default();
    let (Some(first), Some(last)) = (schedule.windows.first(), schedule.windows.last()) else {
        return (observations, paired_points, stages);
    };
    let mut decision_ts = first.from_ms;
    while decision_ts < last.to_ms {
        let Some(window) = schedule.window_at(decision_ts) else {
            decision_ts = decision_ts.saturating_add(MS_4H);
            continue;
        };
        stages.decision_points += 1;
        let minimum = (window.members.len() as f64 * MIN_FACTOR_COVERAGE).ceil() as usize;
        let mut factors = Vec::new();
        for symbol in &window.members {
            let (Some(okx_candles), Some(binance_candles)) = (okx.get(symbol), binance.get(symbol))
            else {
                continue;
            };
            if let Some(factor) = factor_at(symbol, okx_candles, binance_candles, decision_ts) {
                factors.push(factor);
            }
        }
        if minimum == 0 || factors.len() < minimum {
            stages.coverage_blocked += 1;
            decision_ts = decision_ts.saturating_add(MS_4H);
            continue;
        }
        stages.factor_observations += factors.len();
        let down_medians = visible_medians(&factors, PriceDirection::Down);
        let up_medians = visible_medians(&factors, PriceDirection::Up);
        let mut current = Vec::new();
        for factor in factors {
            let Some(okx_candles) = okx.get(&factor.symbol) else {
                continue;
            };
            let Some((forward_1h, forward_4h)) = forward_returns(okx_candles, decision_ts) else {
                stages.incomplete_outcomes += 1;
                continue;
            };
            let direction = match factor.price_direction {
                PriceDirection::Down => 1.0,
                PriceDirection::Up => -1.0,
            };
            current.push(FactorObservation {
                symbol: factor.symbol,
                decision_ts: factor.decision_ts,
                price_direction: factor.price_direction,
                divergent: factor.divergent,
                absolute_price_move: factor.absolute_price_move,
                relative_volume: factor.relative_volume,
                flow_1h: factor.flow_1h,
                directed_1h: direction * forward_1h,
                directed_4h: direction * forward_4h,
            });
        }
        stages.outcome_observations += current.len();
        if let Some(medians) = down_medians {
            if let Some(point) =
                paired_point_at(decision_ts, PriceDirection::Down, medians, &current)
            {
                stages.paired_long_points += 1;
                paired_points.push(point);
            }
        }
        if let Some(medians) = up_medians {
            if let Some(point) = paired_point_at(decision_ts, PriceDirection::Up, medians, &current)
            {
                stages.paired_short_points += 1;
                paired_points.push(point);
            }
        }
        observations.append(&mut current);
        decision_ts = decision_ts.saturating_add(MS_4H);
    }
    (observations, paired_points, stages)
}

/// 用决策前 84 根 Binance 与最后四根同步 OKX 棒形成可见因子。
fn factor_at(
    symbol: &str,
    okx: &[CandleItem],
    binance: &[BinanceCandle],
    decision_ts: i64,
) -> Option<VisibleFactor> {
    let factor_start = decision_ts.checked_sub(CURRENT_FACTOR_BARS as i64 * MS_15M)?;
    let baseline_start = decision_ts.checked_sub(FACTOR_LOOKBACK_BARS as i64 * MS_15M)?;
    let okx_start = okx
        .binary_search_by_key(&factor_start, |candle| candle.ts)
        .ok()?;
    let binance_start = binance
        .binary_search_by_key(&baseline_start, |candle| candle.ts)
        .ok()?;
    let okx_current = okx.get(okx_start..okx_start + CURRENT_FACTOR_BARS)?;
    let binance_window = binance.get(binance_start..binance_start + FACTOR_LOOKBACK_BARS)?;
    if !continuous_okx(okx_current, factor_start)
        || !continuous_binance(binance_window, baseline_start)
        || binance_window[BASELINE_BARS..]
            .iter()
            .zip(okx_current)
            .any(|(binance_candle, okx_candle)| binance_candle.ts != okx_candle.ts)
    {
        return None;
    }
    let open = okx_current.first()?.o;
    let close = okx_current.last()?.c;
    if open <= 0.0 || close <= 0.0 {
        return None;
    }
    let price_1h = close / open - 1.0;
    let price_direction = if price_1h < 0.0 {
        PriceDirection::Down
    } else if price_1h > 0.0 {
        PriceDirection::Up
    } else {
        return None;
    };
    if binance_window.iter().any(|candle| {
        !candle.quote_volume.is_finite()
            || !candle.taker_buy_quote_volume.is_finite()
            || candle.quote_volume <= 0.0
            || candle.taker_buy_quote_volume < 0.0
            || candle.taker_buy_quote_volume > candle.quote_volume
    }) {
        return None;
    }
    let baseline_total = binance_window[..BASELINE_BARS]
        .iter()
        .map(|candle| candle.quote_volume)
        .sum::<f64>();
    let current_total = binance_window[BASELINE_BARS..]
        .iter()
        .map(|candle| candle.quote_volume)
        .sum::<f64>();
    let signed_delta = binance_window[BASELINE_BARS..]
        .iter()
        .map(|candle| 2.0 * candle.taker_buy_quote_volume - candle.quote_volume)
        .sum::<f64>();
    let baseline_hourly_mean = baseline_total / BASELINE_HOURS as f64;
    if current_total <= 0.0 || baseline_hourly_mean <= 0.0 {
        return None;
    }
    let flow_1h = signed_delta / current_total;
    let relative_volume = current_total / baseline_hourly_mean;
    if !flow_1h.is_finite() || !relative_volume.is_finite() || flow_1h == 0.0 {
        return None;
    }
    let divergent = matches!(
        (price_direction, flow_1h.is_sign_positive()),
        (PriceDirection::Down, true) | (PriceDirection::Up, false)
    );
    Some(VisibleFactor {
        symbol: symbol.to_owned(),
        decision_ts,
        price_direction,
        divergent,
        absolute_price_move: price_1h.abs(),
        relative_volume,
        flow_1h,
    })
}

/// 要求 OKX 因子棒从指定起点严格连续且都已确认。
fn continuous_okx(candles: &[CandleItem], start_ts: i64) -> bool {
    candles.len() == CURRENT_FACTOR_BARS
        && candles.iter().enumerate().all(|(index, candle)| {
            candle.confirm == 1 && candle.ts == start_ts + index as i64 * MS_15M
        })
}

/// 要求 Binance 总量基线和当前因子棒组成严格连续的原生 15m 序列。
fn continuous_binance(candles: &[BinanceCandle], start_ts: i64) -> bool {
    candles.len() == FACTOR_LOOKBACK_BARS
        && candles
            .iter()
            .enumerate()
            .all(|(index, candle)| candle.ts == start_ts + index as i64 * MS_15M)
}

/// 从决策点新开棒的开盘计算严格连续 1h/4h 毛收益。
fn forward_returns(okx: &[CandleItem], decision_ts: i64) -> Option<(f64, f64)> {
    let entry_index = okx
        .binary_search_by_key(&decision_ts, |candle| candle.ts)
        .ok()?;
    let entry = okx.get(entry_index)?;
    if entry.o <= 0.0 || entry.confirm != 1 {
        return None;
    }
    let return_at = |bars: usize| -> Option<f64> {
        let exit_index = entry_index.checked_add(bars.checked_sub(1)?)?;
        let exit = okx.get(exit_index)?;
        if exit.confirm != 1 || exit.ts != decision_ts + (bars as i64 - 1) * MS_15M || exit.c <= 0.0
        {
            return None;
        }
        let value = exit.c / entry.o - 1.0;
        value.is_finite().then_some(value)
    };
    Some((return_at(FORWARD_1H_BARS)?, return_at(FORWARD_4H_BARS)?))
}

/// 只用当前横截面的价格幅度与相对总量计算可见中位数。
fn visible_medians(factors: &[VisibleFactor], direction: PriceDirection) -> Option<(f64, f64)> {
    let mut price = factors
        .iter()
        .filter(|factor| factor.price_direction == direction)
        .map(|factor| factor.absolute_price_move)
        .collect::<Vec<_>>();
    let mut volume = factors
        .iter()
        .filter(|factor| factor.price_direction == direction)
        .map(|factor| factor.relative_volume)
        .collect::<Vec<_>>();
    Some((median(&mut price)?, median(&mut volume)?))
}

/// 计算有限样本中位数，偶数样本取中间两项均值。
fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        Some((values[middle - 1] + values[middle]) / 2.0)
    } else {
        values.get(middle).copied()
    }
}

/// 在 2×2 可见分层内计算背离组减同向组，并等权汇总到单时点。
fn paired_point_at(
    decision_ts: i64,
    direction: PriceDirection,
    medians: (f64, f64),
    observations: &[FactorObservation],
) -> Option<PairedPoint> {
    let mut strata = [StratumAccumulator::default(); 4];
    for observation in observations
        .iter()
        .filter(|observation| observation.price_direction == direction)
    {
        let price_bucket = usize::from(observation.absolute_price_move > medians.0);
        let volume_bucket = usize::from(observation.relative_volume > medians.1);
        let stratum = &mut strata[price_bucket * 2 + volume_bucket];
        if observation.divergent {
            stratum.divergent_count += 1;
            stratum.divergent_sum_1h += observation.directed_1h;
            stratum.divergent_sum_4h += observation.directed_4h;
        } else {
            stratum.aligned_count += 1;
            stratum.aligned_sum_1h += observation.directed_1h;
            stratum.aligned_sum_4h += observation.directed_4h;
        }
    }
    let paired = strata
        .iter()
        .filter(|stratum| stratum.divergent_count > 0 && stratum.aligned_count > 0)
        .map(|stratum| {
            (
                stratum.divergent_sum_1h / stratum.divergent_count as f64
                    - stratum.aligned_sum_1h / stratum.aligned_count as f64,
                stratum.divergent_sum_4h / stratum.divergent_count as f64
                    - stratum.aligned_sum_4h / stratum.aligned_count as f64,
            )
        })
        .collect::<Vec<_>>();
    if paired.is_empty() {
        return None;
    }
    Some(PairedPoint {
        decision_ts,
        price_direction: direction,
        spread_1h: paired.iter().map(|value| value.0).sum::<f64>() / paired.len() as f64,
        spread_4h: paired.iter().map(|value| value.1).sum::<f64>() / paired.len() as f64,
    })
}

/// 汇总四象限 observation 和双方向 time-level 增量门禁。
#[allow(clippy::too_many_arguments)]
fn build_report(
    schedule: &UniverseSchedule,
    symbols: usize,
    binance_audit: BinanceKlineAudit,
    stages: TakerDeltaFactorStages,
    observations: &[FactorObservation],
    paired_points: &[PairedPoint],
    split_ms: i64,
) -> TakerDeltaFactorPanelReport {
    let quadrant = |direction: PriceDirection, divergent: bool| {
        factor_summary(
            &observations
                .iter()
                .filter(|observation| {
                    observation.price_direction == direction && observation.divergent == divergent
                })
                .collect::<Vec<_>>(),
        )
    };
    let paired = |direction: PriceDirection, before_split: Option<bool>| {
        paired_summary(
            &paired_points
                .iter()
                .filter(|point| {
                    point.price_direction == direction
                        && before_split
                            .is_none_or(|before| (point.decision_ts < split_ms) == before)
                })
                .collect::<Vec<_>>(),
        )
    };
    let down_positive = quadrant(PriceDirection::Down, true);
    let down_negative = quadrant(PriceDirection::Down, false);
    let up_negative = quadrant(PriceDirection::Up, true);
    let up_positive = quadrant(PriceDirection::Up, false);
    let long_overall = paired(PriceDirection::Down, None);
    let long_first = paired(PriceDirection::Down, Some(true));
    let long_second = paired(PriceDirection::Down, Some(false));
    let short_overall = paired(PriceDirection::Up, None);
    let short_first = paired(PriceDirection::Up, Some(true));
    let short_second = paired(PriceDirection::Up, Some(false));
    let factor_gate_passed = paired_gate(&long_overall)
        && paired_gate(&short_overall)
        && down_positive
            .mean_directed_4h
            .is_some_and(|value| value >= ECONOMIC_EDGE_RATE)
        && up_negative
            .mean_directed_4h
            .is_some_and(|value| value >= ECONOMIC_EDGE_RATE)
        && positive_4h(&long_first)
        && positive_4h(&long_second)
        && positive_4h(&short_first)
        && positive_4h(&short_second);
    TakerDeltaFactorPanelReport {
        rule_version: RULE_VERSION.to_owned(),
        universe_version: schedule.version.clone(),
        symbols,
        binance_audit,
        stages,
        down_price_positive_delta: down_positive,
        down_price_negative_delta: down_negative,
        up_price_negative_delta: up_negative,
        up_price_positive_delta: up_positive,
        long_paired_overall: long_overall,
        long_paired_first_half: long_first,
        long_paired_second_half: long_second,
        short_paired_overall: short_overall,
        short_paired_first_half: short_first,
        short_paired_second_half: short_second,
        factor_gate_passed,
    }
}

/// 对一个四象限 observation 切片计算毛均值与正收益率。
fn factor_summary(observations: &[&FactorObservation]) -> TakerDeltaFactorSummary {
    TakerDeltaFactorSummary {
        observations: observations.len(),
        mean_directed_1h: mean(observations.iter().map(|value| value.directed_1h)),
        mean_directed_4h: mean(observations.iter().map(|value| value.directed_4h)),
        positive_rate_1h_pct: positive_rate(observations.iter().map(|value| value.directed_1h)),
        positive_rate_4h_pct: positive_rate(observations.iter().map(|value| value.directed_4h)),
    }
}

/// 对 time-level 配对点计算背离组相对同向组的增量均值。
fn paired_summary(points: &[&PairedPoint]) -> TakerDeltaPairedSummary {
    TakerDeltaPairedSummary {
        decision_points: points.len(),
        mean_spread_1h: mean(points.iter().map(|value| value.spread_1h)),
        mean_spread_4h: mean(points.iter().map(|value| value.spread_4h)),
        positive_rate_1h_pct: positive_rate(points.iter().map(|value| value.spread_1h)),
        positive_rate_4h_pct: positive_rate(points.iter().map(|value| value.spread_4h)),
    }
}

/// 计算非空有限序列的算术平均。
fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let values = values.collect::<Vec<_>>();
    (!values.is_empty() && values.iter().all(|value| value.is_finite()))
        .then(|| values.iter().sum::<f64>() / values.len() as f64)
}

/// 计算严格大于零的观察比例，输出百分比。
fn positive_rate(values: impl Iterator<Item = f64>) -> Option<f64> {
    let values = values.collect::<Vec<_>>();
    (!values.is_empty()).then(|| {
        values.iter().filter(|value| **value > 0.0).count() as f64 / values.len() as f64 * 100.0
    })
}

/// 全年度配对增量必须同时满足样本、1h 正向和 4h 经济幅度。
fn paired_gate(summary: &TakerDeltaPairedSummary) -> bool {
    summary.decision_points >= MIN_PAIRED_POINTS
        && summary.mean_spread_1h.is_some_and(|value| value > 0.0)
        && summary
            .mean_spread_4h
            .is_some_and(|value| value >= ECONOMIC_EDGE_RATE)
}

/// 半年度稳定性门只要求 4h 配对差保持正向。
fn positive_4h(summary: &TakerDeltaPairedSummary) -> bool {
    summary.mean_spread_4h.is_some_and(|value| value > 0.0)
}

/// 打印可复核且不包含交易收益措辞的因子面板摘要。
fn print_report(report: &TakerDeltaFactorPanelReport) {
    println!("rule_version={}", report.rule_version);
    println!("universe_version={}", report.universe_version);
    println!("symbols={}", report.symbols);
    println!("binance_audit={:?}", report.binance_audit);
    println!("stages={:?}", report.stages);
    print_factor_summary(
        "down_price_positive_delta",
        &report.down_price_positive_delta,
    );
    print_factor_summary(
        "down_price_negative_delta",
        &report.down_price_negative_delta,
    );
    print_factor_summary("up_price_negative_delta", &report.up_price_negative_delta);
    print_factor_summary("up_price_positive_delta", &report.up_price_positive_delta);
    print_paired_summary("long_paired_overall", &report.long_paired_overall);
    print_paired_summary("long_paired_first_half", &report.long_paired_first_half);
    print_paired_summary("long_paired_second_half", &report.long_paired_second_half);
    print_paired_summary("short_paired_overall", &report.short_paired_overall);
    print_paired_summary("short_paired_first_half", &report.short_paired_first_half);
    print_paired_summary("short_paired_second_half", &report.short_paired_second_half);
    println!("factor_gate_passed={}", report.factor_gate_passed);
}

/// 以百分比显示一个方向象限的固定 forward outcome。
fn print_factor_summary(label: &str, summary: &TakerDeltaFactorSummary) {
    println!(
        "{label}: n={} mean_1h_pct={} mean_4h_pct={} positive_1h_pct={} positive_4h_pct={}",
        summary.observations,
        format_percent(summary.mean_directed_1h),
        format_percent(summary.mean_directed_4h),
        format_number(summary.positive_rate_1h_pct),
        format_number(summary.positive_rate_4h_pct),
    );
}

/// 以百分比显示 time-level 背离减同向增量。
fn print_paired_summary(label: &str, summary: &TakerDeltaPairedSummary) {
    println!(
        "{label}: points={} spread_1h_pct={} spread_4h_pct={} positive_1h_pct={} positive_4h_pct={}",
        summary.decision_points,
        format_percent(summary.mean_spread_1h),
        format_percent(summary.mean_spread_4h),
        format_number(summary.positive_rate_1h_pct),
        format_number(summary.positive_rate_4h_pct),
    );
}

/// 将小数收益转换成百分比字符串。
fn format_percent(value: Option<f64>) -> String {
    value
        .map(|number| format!("{:.6}", number * 100.0))
        .unwrap_or_else(|| "NA".to_owned())
}

/// 将可选数值转换成固定精度字符串。
fn format_number(value: Option<f64>) -> String {
    value
        .map(|number| format!("{number:.6}"))
        .unwrap_or_else(|| "NA".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造严格连续的 Binance 因子窗口。
    fn binance_window(decision_ts: i64, current_taker_share: f64) -> Vec<BinanceCandle> {
        let start = decision_ts - FACTOR_LOOKBACK_BARS as i64 * MS_15M;
        (0..FACTOR_LOOKBACK_BARS)
            .map(|index| BinanceCandle {
                ts: start + index as i64 * MS_15M,
                open: 1.0,
                close: 1.0,
                quote_volume: 100.0,
                taker_buy_quote_volume: if index < BASELINE_BARS {
                    50.0
                } else {
                    100.0 * current_taker_share
                },
            })
            .collect()
    }

    /// 构造包含因子和 forward outcome 的连续 OKX 序列。
    fn okx_window(decision_ts: i64) -> Vec<CandleItem> {
        let start = decision_ts - CURRENT_FACTOR_BARS as i64 * MS_15M;
        (0..CURRENT_FACTOR_BARS + FORWARD_4H_BARS)
            .map(|index| {
                let ts = start + index as i64 * MS_15M;
                CandleItem {
                    ts,
                    o: 100.0,
                    h: 101.0,
                    l: 95.0,
                    c: if index < CURRENT_FACTOR_BARS {
                        99.0 - index as f64
                    } else {
                        100.0 + (index - CURRENT_FACTOR_BARS) as f64
                    },
                    v: 1.0,
                    confirm: 1,
                }
            })
            .collect()
    }

    #[test]
    fn factor_uses_four_completed_bars_and_native_taker_quote_volume() {
        let decision_ts = 10_000_000_000;
        let okx = okx_window(decision_ts);
        let binance = binance_window(decision_ts, 0.70);
        let factor = factor_at("ETH-USDT-SWAP", &okx, &binance, decision_ts).unwrap();

        assert_eq!(factor.price_direction, PriceDirection::Down);
        assert!(factor.divergent);
        assert!((factor.flow_1h - 0.40).abs() < 1e-12);
        assert!((factor.relative_volume - 1.0).abs() < 1e-12);
    }

    #[test]
    fn factor_rejects_a_gap_in_the_visible_binance_window() {
        let decision_ts = 10_000_000_000;
        let okx = okx_window(decision_ts);
        let mut binance = binance_window(decision_ts, 0.70);
        binance[20].ts += MS_15M;

        assert!(factor_at("ETH-USDT-SWAP", &okx, &binance, decision_ts).is_none());
    }

    #[test]
    fn future_outcome_starts_at_decision_open_and_requires_continuity() {
        let decision_ts = 10_000_000_000;
        let okx = okx_window(decision_ts);
        let (forward_1h, forward_4h) = forward_returns(&okx, decision_ts).unwrap();

        assert!((forward_1h - 0.03).abs() < 1e-12);
        assert!((forward_4h - 0.15).abs() < 1e-12);

        let mut broken = okx;
        let entry_index = CURRENT_FACTOR_BARS;
        broken[entry_index + FORWARD_4H_BARS - 1].ts += MS_15M;
        assert!(forward_returns(&broken, decision_ts).is_none());
    }

    #[test]
    fn paired_point_controls_price_and_volume_strata_before_comparison() {
        let decision_ts = 10_000_000_000;
        let make = |divergent: bool, directed_1h: f64, directed_4h: f64| FactorObservation {
            symbol: if divergent { "A" } else { "B" }.to_owned(),
            decision_ts,
            price_direction: PriceDirection::Down,
            divergent,
            absolute_price_move: 0.01,
            relative_volume: 1.0,
            flow_1h: if divergent { 0.2 } else { -0.2 },
            directed_1h,
            directed_4h,
        };
        let observations = vec![make(true, 0.03, 0.05), make(false, 0.01, -0.01)];
        let point = paired_point_at(
            decision_ts,
            PriceDirection::Down,
            (0.02, 2.0),
            &observations,
        )
        .unwrap();

        assert!((point.spread_1h - 0.02).abs() < 1e-12);
        assert!((point.spread_4h - 0.06).abs() < 1e-12);
    }

    #[test]
    fn median_uses_only_the_supplied_visible_cross_section() {
        let mut odd = vec![3.0, 1.0, 2.0];
        let mut even = vec![4.0, 1.0, 3.0, 2.0];

        assert_eq!(median(&mut odd), Some(2.0));
        assert_eq!(median(&mut even), Some(2.5));
    }
}
