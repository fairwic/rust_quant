use anyhow::{bail, Context, Result};
use serde::Serialize;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::{BTreeMap, HashMap};

const STRATEGY_KEY: &str = "vegas_bear_regime_failed_compressed_breakdown_reclaim_4h_research";
const VERSION: &str = "xasset_4h_top100_v66_bear_failed_compressed_reclaim_20260721";
const MACD_STRATEGY_KEY: &str = "vegas_btc_positive_macd_failed_breakdown_reclaim_4h_research";
const MACD_VERSION: &str = "xasset_4h_top100_v67_btc_positive_macd_reclaim_20260721";
const BACKTEST_ID_MIN: i64 = 15_620;
const BACKTEST_ID_MAX: i64 = 15_719;
const FOUR_HOURS_MS: i64 = 4 * 60 * 60 * 1_000;
const FUNDING_INTERVAL_MS: i64 = 8 * 60 * 60 * 1_000;
const WATCH_BARS: usize = 12;
const MACD_WATCH_BARS: usize = 2;
const HOLD_BARS: usize = 12;
const TARGET_R: f64 = 2.0;
const BASE_FEE_RATE_PER_SIDE: f64 = 0.0007;
const STANDARD_EXTRA_SLIPPAGE_BPS: f64 = 5.0;
const STANDARD_FUNDING_BPS_PER_8H: f64 = 1.0;
const DOUBLE_EXTRA_SLIPPAGE_BPS: f64 = 10.0;
const DOUBLE_FUNDING_BPS_PER_8H: f64 = 2.0;
const OOS_START_MS: i64 = 1_767_225_600_000;

/// BTC 长周期状态只由信号前已经完成的 4H 收盘与两条 EMA 决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BtcRegime {
    Bull,
    Bear,
    Neutral,
}

/// V63 压缩跌破信号提供失败回收观察所需的冻结事实，不读取原交易结局。
#[derive(Debug, Clone, PartialEq)]
struct CompressedBreakdownSeed {
    detail_id: i64,
    symbol: String,
    signal_ts: i64,
    short_entry_price: f64,
    frozen_short_stop: f64,
}

/// 一根已确认的本地 4H K 线；时间戳是 K 线起点。
#[derive(Debug, Clone, PartialEq)]
struct Candle {
    ts: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

/// 单笔失败跌破回收多头保留完整因果路径和两档成本结果。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReclaimTrade {
    pub seed_detail_id: i64,
    pub symbol: String,
    pub seed_signal_ts: i64,
    pub confirmation_ts: i64,
    pub entry_ts: i64,
    pub exit_ts: i64,
    pub entry_price: f64,
    pub initial_stop: f64,
    pub target_price: f64,
    pub exit_price: f64,
    pub exit_reason: &'static str,
    pub gross_r: f64,
    pub standard_net_r: f64,
    pub double_cost_net_r: f64,
}

/// 从原始信号到完整成交的漏斗，明确区分没有确认与样本尾部不完整。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct StageCounts {
    pub compressed_seeds: usize,
    pub btc_regime_missing: usize,
    pub non_bear_seeds: usize,
    pub bear_seeds: usize,
    pub same_symbol_state_blocked: usize,
    pub no_reclaim_confirmation: usize,
    pub invalid_confirmation_risk: usize,
    pub incomplete_outcome: usize,
    pub completed_trades: usize,
}

/// 一个固定时间切片的交易级成本后指标。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SliceSummary {
    pub label: String,
    pub trades: usize,
    pub wins: usize,
    pub win_rate_pct: f64,
    pub net_expectancy_r: Option<f64>,
    pub profit_factor: Option<f64>,
    pub total_net_r: f64,
    pub max_drawdown_r: f64,
    pub recovery_factor: Option<f64>,
    pub trade_sharpe: Option<f64>,
}

/// V66 独立 setup 报告；只有门禁通过才值得进入组合层回放。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResearchReport {
    pub strategy_key: &'static str,
    pub version: &'static str,
    pub source_backtest_id_min: i64,
    pub source_backtest_id_max: i64,
    pub watch_bars: usize,
    pub hold_bars: usize,
    pub target_r: f64,
    pub stage_counts: StageCounts,
    pub standard_cost: SliceSummary,
    pub double_cost: SliceSummary,
    pub in_sample: SliceSummary,
    pub out_of_sample: SliceSummary,
    pub yearly: Vec<SliceSummary>,
    pub walk_forward: Vec<SliceSummary>,
    pub historical_gate_pass: bool,
    pub blockers: Vec<String>,
    pub trades: Vec<ReclaimTrade>,
}

/// V67 的市场级 MACD 漏斗，单独报告正柱种子与本币中点收回数量。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct MacdStageCounts {
    pub compressed_seeds: usize,
    pub btc_macd_missing: usize,
    pub non_positive_histogram: usize,
    pub positive_histogram_seeds: usize,
    pub same_symbol_state_blocked: usize,
    pub no_midpoint_reclaim: usize,
    pub invalid_confirmation_risk: usize,
    pub incomplete_outcome: usize,
    pub completed_trades: usize,
}

/// V67 独立回收多头的固定成本与时间切片报告。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MacdResearchReport {
    pub strategy_key: &'static str,
    pub version: &'static str,
    pub source_backtest_id_min: i64,
    pub source_backtest_id_max: i64,
    pub watch_bars: usize,
    pub hold_bars: usize,
    pub target_r: f64,
    pub stage_counts: MacdStageCounts,
    pub standard_cost: SliceSummary,
    pub double_cost: SliceSummary,
    pub in_sample: SliceSummary,
    pub out_of_sample: SliceSummary,
    pub yearly: Vec<SliceSummary>,
    pub walk_forward: Vec<SliceSummary>,
    pub historical_gate_pass: bool,
    pub blockers: Vec<String>,
    pub trades: Vec<ReclaimTrade>,
}

/// 单个种子的终态；未确认、非法风险和样本尾部不能混入已结算交易指标。
enum SeedOutcome {
    NoConfirmation { busy_until: i64 },
    InvalidRisk { busy_until: i64 },
    Incomplete,
    Trade(ReclaimTrade),
}

/// 执行冻结研究、输出 JSON 报告；函数只读取 quant_core 行情与回测审计表。
pub async fn run_research(database_url: &str) -> Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for V66 reclaim research")?;
    let seeds = load_compressed_breakdown_seeds(&pool).await?;
    let max_signal_ts = seeds.iter().map(|seed| seed.signal_ts).max().unwrap_or(0);
    let btc_regimes = load_btc_regimes(&pool, max_signal_ts).await?;
    let mut stage_counts = StageCounts {
        compressed_seeds: seeds.len(),
        ..StageCounts::default()
    };
    let mut busy_until_by_symbol = HashMap::<String, i64>::new();
    let mut trades = Vec::new();

    for seed in seeds {
        let Some(regime) = regime_before(&btc_regimes, seed.signal_ts) else {
            stage_counts.btc_regime_missing += 1;
            continue;
        };
        if regime != BtcRegime::Bear {
            stage_counts.non_bear_seeds += 1;
            continue;
        }
        stage_counts.bear_seeds += 1;
        let seed_decision_ts = seed.signal_ts + FOUR_HOURS_MS;
        if busy_until_by_symbol
            .get(&seed.symbol)
            .is_some_and(|busy_until| seed_decision_ts < *busy_until)
        {
            stage_counts.same_symbol_state_blocked += 1;
            continue;
        }
        let candles = load_seed_candles(&pool, &seed).await?;
        match evaluate_seed(&seed, &candles)? {
            SeedOutcome::NoConfirmation { busy_until } => {
                stage_counts.no_reclaim_confirmation += 1;
                busy_until_by_symbol.insert(seed.symbol, busy_until);
            }
            SeedOutcome::InvalidRisk { busy_until } => {
                stage_counts.invalid_confirmation_risk += 1;
                busy_until_by_symbol.insert(seed.symbol, busy_until);
            }
            SeedOutcome::Incomplete => {
                stage_counts.incomplete_outcome += 1;
                busy_until_by_symbol.insert(seed.symbol, i64::MAX);
            }
            SeedOutcome::Trade(trade) => {
                busy_until_by_symbol.insert(seed.symbol, trade.exit_ts);
                trades.push(trade);
            }
        }
    }
    stage_counts.completed_trades = trades.len();
    let report = build_report(stage_counts, trades);
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// 执行 V67 BTC 正 MACD + 本币实体中点收回研究，只输出本地 JSON 证据。
pub async fn run_btc_positive_macd_research(database_url: &str) -> Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for V67 BTC MACD reclaim research")?;
    let seeds = load_compressed_breakdown_seeds(&pool).await?;
    let max_signal_ts = seeds.iter().map(|seed| seed.signal_ts).max().unwrap_or(0);
    let btc_histograms = load_btc_macd_histograms(&pool, max_signal_ts).await?;
    let mut stage_counts = MacdStageCounts {
        compressed_seeds: seeds.len(),
        ..MacdStageCounts::default()
    };
    let mut busy_until_by_symbol = HashMap::<String, i64>::new();
    let mut trades = Vec::new();
    for seed in seeds {
        let Some(histogram) = value_before(&btc_histograms, seed.signal_ts) else {
            stage_counts.btc_macd_missing += 1;
            continue;
        };
        if histogram <= 0.0 {
            stage_counts.non_positive_histogram += 1;
            continue;
        }
        stage_counts.positive_histogram_seeds += 1;
        let seed_decision_ts = seed.signal_ts + FOUR_HOURS_MS;
        if busy_until_by_symbol
            .get(&seed.symbol)
            .is_some_and(|busy_until| seed_decision_ts < *busy_until)
        {
            stage_counts.same_symbol_state_blocked += 1;
            continue;
        }
        let candles = load_signal_and_future_candles(&pool, &seed, MACD_WATCH_BARS).await?;
        match evaluate_macd_midpoint_seed(&seed, &candles)? {
            SeedOutcome::NoConfirmation { busy_until } => {
                stage_counts.no_midpoint_reclaim += 1;
                busy_until_by_symbol.insert(seed.symbol, busy_until);
            }
            SeedOutcome::InvalidRisk { busy_until } => {
                stage_counts.invalid_confirmation_risk += 1;
                busy_until_by_symbol.insert(seed.symbol, busy_until);
            }
            SeedOutcome::Incomplete => {
                stage_counts.incomplete_outcome += 1;
                busy_until_by_symbol.insert(seed.symbol, i64::MAX);
            }
            SeedOutcome::Trade(trade) => {
                busy_until_by_symbol.insert(seed.symbol, trade.exit_ts);
                trades.push(trade);
            }
        }
    }
    stage_counts.completed_trades = trades.len();
    let report = build_macd_report(stage_counts, trades);
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// 从 V63 审计标签读取压缩跌破种子；查询不连接原交易的平仓与盈亏字段。
async fn load_compressed_breakdown_seeds(pool: &PgPool) -> Result<Vec<CompressedBreakdownSeed>> {
    let rows = sqlx::query(
        r#"
        SELECT o.id AS detail_id,
               o.inst_id AS symbol,
               (EXTRACT(EPOCH FROM (
                   o.open_position_time AT TIME ZONE 'Asia/Shanghai'
               )) * 1000)::bigint AS signal_ts,
               o.open_price::double precision AS short_entry_price,
               o.initial_stop_price AS frozen_short_stop
          FROM back_test_detail o
          JOIN LATERAL (
              SELECT adjustments
                FROM dynamic_config_log log
               WHERE log.backtest_id = o.back_test_id
                 AND log.kline_time = o.open_position_time
               ORDER BY log.id DESC
               LIMIT 1
          ) audit ON TRUE
         WHERE o.back_test_id BETWEEN $1 AND $2
           AND o.option_type = 'short'
           AND audit.adjustments ? 'COMPRESSED_RANGE_BREAKOUT_SHORT'
         ORDER BY signal_ts, o.id
        "#,
    )
    .bind(BACKTEST_ID_MIN)
    .bind(BACKTEST_ID_MAX)
    .fetch_all(pool)
    .await
    .context("load frozen V63 compressed breakdown seeds")?;
    rows.into_iter()
        .map(|row| {
            let seed = CompressedBreakdownSeed {
                detail_id: row.try_get("detail_id")?,
                symbol: row.try_get("symbol")?,
                signal_ts: row.try_get("signal_ts")?,
                short_entry_price: row.try_get("short_entry_price")?,
                frozen_short_stop: row.try_get("frozen_short_stop")?,
            };
            if !seed.short_entry_price.is_finite()
                || !seed.frozen_short_stop.is_finite()
                || seed.frozen_short_stop <= seed.short_entry_price
            {
                bail!("invalid frozen compressed short seed {}", seed.detail_id);
            }
            Ok(seed)
        })
        .collect()
}

/// 复用组合回放的 EMA288/338 定义，只保留信号时间之前可见的 BTC 状态。
async fn load_btc_regimes(pool: &PgPool, max_signal_ts: i64) -> Result<BTreeMap<i64, BtcRegime>> {
    let rows = sqlx::query(
        r#"
        SELECT ts, c::double precision AS close
          FROM "btc-usdt-swap_candles_4h"
         WHERE ts < $1 AND confirm = '1'
         ORDER BY ts
        "#,
    )
    .bind(max_signal_ts)
    .fetch_all(pool)
    .await
    .context("load causal BTC 4H regime candles for V66")?;
    let mut ema288 = None;
    let mut ema338 = None;
    let mut regimes = BTreeMap::new();
    for (index, row) in rows.into_iter().enumerate() {
        let ts: i64 = row.try_get("ts")?;
        let close: f64 = row.try_get("close")?;
        if !close.is_finite() || close <= 0.0 {
            bail!("invalid BTC close at {ts}");
        }
        ema288 = Some(update_ema(ema288, close, 288));
        ema338 = Some(update_ema(ema338, close, 338));
        if index + 1 < 338 {
            continue;
        }
        let regime = classify_btc_regime(close, ema288.unwrap(), ema338.unwrap());
        regimes.insert(ts, regime);
    }
    Ok(regimes)
}

/// 计算标准 `12/26/9` BTC MACD 柱；调用方仍须严格取信号之前的最后一个值。
async fn load_btc_macd_histograms(pool: &PgPool, max_signal_ts: i64) -> Result<BTreeMap<i64, f64>> {
    let rows = sqlx::query(
        r#"
        SELECT ts, c::double precision AS close
          FROM "btc-usdt-swap_candles_4h"
         WHERE ts < $1 AND confirm = '1'
         ORDER BY ts
        "#,
    )
    .bind(max_signal_ts)
    .fetch_all(pool)
    .await
    .context("load causal BTC candles for V67 MACD")?;
    let mut ema12 = None;
    let mut ema26 = None;
    let mut signal9 = None;
    let mut histograms = BTreeMap::new();
    for (index, row) in rows.into_iter().enumerate() {
        let ts: i64 = row.try_get("ts")?;
        let close: f64 = row.try_get("close")?;
        if !close.is_finite() || close <= 0.0 {
            bail!("invalid BTC close at {ts}");
        }
        ema12 = Some(update_ema(ema12, close, 12));
        ema26 = Some(update_ema(ema26, close, 26));
        let line = ema12.unwrap() - ema26.unwrap();
        signal9 = Some(update_ema(signal9, line, 9));
        if index + 1 >= 35 {
            histograms.insert(ts, line - signal9.unwrap());
        }
    }
    Ok(histograms)
}

/// 把 BTC 收盘相对两条长 EMA 的位置映射为不带收益信息的市场状态。
fn classify_btc_regime(close: f64, ema288: f64, ema338: f64) -> BtcRegime {
    if close > ema288 && close > ema338 {
        BtcRegime::Bull
    } else if close < ema288 && close < ema338 {
        BtcRegime::Bear
    } else {
        BtcRegime::Neutral
    }
}

/// 按标准递推公式更新单条 EMA，首个有效收盘作为初始化值。
fn update_ema(previous: Option<f64>, value: f64, period: usize) -> f64 {
    let alpha = 2.0 / (period as f64 + 1.0);
    previous.map_or(value, |ema| alpha * value + (1.0 - alpha) * ema)
}

/// 严格排除信号同一根 BTC K 线，避免用尚未完成的市场状态决定入场。
fn regime_before(regimes: &BTreeMap<i64, BtcRegime>, signal_ts: i64) -> Option<BtcRegime> {
    regimes
        .range(..signal_ts)
        .next_back()
        .map(|(_, regime)| *regime)
}

/// 读取严格早于信号的最后一个连续指标值，避免同棒 MACD 泄漏。
fn value_before(values: &BTreeMap<i64, f64>, signal_ts: i64) -> Option<f64> {
    values
        .range(..signal_ts)
        .next_back()
        .map(|(_, value)| *value)
}

/// 一次性读取观察期和最大持有期所需的确认 K 线，不读取未确认尾部。
async fn load_seed_candles(pool: &PgPool, seed: &CompressedBreakdownSeed) -> Result<Vec<Candle>> {
    let table = quoted_4h_candle_table(&seed.symbol)?;
    let end_ts = seed.signal_ts + ((WATCH_BARS + HOLD_BARS + 1) as i64 * FOUR_HOURS_MS);
    let query = format!(
        "SELECT ts, o::double precision AS open, h::double precision AS high, \
                l::double precision AS low, c::double precision AS close \
           FROM {table} WHERE ts > $1 AND ts <= $2 AND confirm = '1' ORDER BY ts"
    );
    sqlx::query(&query)
        .bind(seed.signal_ts)
        .bind(end_ts)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load V66 outcome candles for {}", seed.symbol))?
        .into_iter()
        .map(|row| {
            let candle = Candle {
                ts: row.try_get("ts")?,
                open: row.try_get("open")?,
                high: row.try_get("high")?,
                low: row.try_get("low")?,
                close: row.try_get("close")?,
            };
            validate_candle(&candle)?;
            Ok(candle)
        })
        .collect()
}

/// 为 V67 同时读取原信号棒、两根确认窗口和后续固定持有路径。
async fn load_signal_and_future_candles(
    pool: &PgPool,
    seed: &CompressedBreakdownSeed,
    watch_bars: usize,
) -> Result<Vec<Candle>> {
    let table = quoted_4h_candle_table(&seed.symbol)?;
    let end_ts = seed.signal_ts + ((watch_bars + HOLD_BARS) as i64 * FOUR_HOURS_MS);
    let query = format!(
        "SELECT ts, o::double precision AS open, h::double precision AS high, \
                l::double precision AS low, c::double precision AS close \
           FROM {table} WHERE ts >= $1 AND ts <= $2 AND confirm = '1' ORDER BY ts"
    );
    sqlx::query(&query)
        .bind(seed.signal_ts)
        .bind(end_ts)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load V67 signal and outcome candles for {}", seed.symbol))?
        .into_iter()
        .map(|row| {
            let candle = Candle {
                ts: row.try_get("ts")?,
                open: row.try_get("open")?,
                high: row.try_get("high")?,
                low: row.try_get("low")?,
                close: row.try_get("close")?,
            };
            validate_candle(&candle)?;
            Ok(candle)
        })
        .collect()
}

/// 对动态表名做白名单校验并返回带引号的 4H 表名。
fn quoted_4h_candle_table(symbol: &str) -> Result<String> {
    let normalized = symbol.to_ascii_lowercase();
    if normalized.is_empty()
        || !normalized
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_'))
    {
        bail!("invalid candle symbol: {symbol}");
    }
    Ok(format!("\"{normalized}_candles_4h\""))
}

/// 拒绝非有限价格和 OHLC 边界冲突，避免异常 K 线制造虚假触发。
fn validate_candle(candle: &Candle) -> Result<()> {
    if [candle.open, candle.high, candle.low, candle.close]
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
        || candle.low > candle.open.min(candle.close)
        || candle.high < candle.open.max(candle.close)
    {
        bail!("invalid 4H candle at {}", candle.ts);
    }
    Ok(())
}

/// 按冻结 12/12 根窗口模拟单个种子；确认 K 线完成后才建立风险，不反查原空头结局。
fn evaluate_seed(seed: &CompressedBreakdownSeed, candles: &[Candle]) -> Result<SeedOutcome> {
    let confirmation = candles
        .iter()
        .take(WATCH_BARS)
        .find(|candle| candle.close > seed.frozen_short_stop);
    let Some(confirmation) = confirmation else {
        return if candles.len() < WATCH_BARS {
            Ok(SeedOutcome::Incomplete)
        } else {
            Ok(SeedOutcome::NoConfirmation {
                busy_until: candles[WATCH_BARS - 1].ts + FOUR_HOURS_MS,
            })
        };
    };
    let entry_price = confirmation.close;
    let initial_stop = confirmation.low;
    let risk = entry_price - initial_stop;
    if !risk.is_finite() || risk <= 0.0 {
        return Ok(SeedOutcome::InvalidRisk {
            busy_until: confirmation.ts + FOUR_HOURS_MS,
        });
    }
    let target_price = entry_price + TARGET_R * risk;
    let future = candles
        .iter()
        .filter(|candle| candle.ts > confirmation.ts)
        .take(HOLD_BARS)
        .collect::<Vec<_>>();
    let mut exit = None;
    for candle in &future {
        // 4H 内部路径未知时先判止损，避免同根双触发产生乐观偏差。
        if candle.low <= initial_stop {
            exit = Some((candle.ts, initial_stop, "stop"));
            break;
        }
        if candle.high >= target_price {
            exit = Some((candle.ts, target_price, "target_2r"));
            break;
        }
    }
    let (exit_bar_ts, exit_price, exit_reason) = if let Some(exit) = exit {
        exit
    } else if future.len() == HOLD_BARS {
        let last = future[HOLD_BARS - 1];
        (last.ts, last.close, "time_12_bars")
    } else {
        return Ok(SeedOutcome::Incomplete);
    };
    let entry_ts = confirmation.ts + FOUR_HOURS_MS;
    let exit_ts = exit_bar_ts + FOUR_HOURS_MS;
    let gross_r = (exit_price - entry_price) / risk;
    Ok(SeedOutcome::Trade(ReclaimTrade {
        seed_detail_id: seed.detail_id,
        symbol: seed.symbol.clone(),
        seed_signal_ts: seed.signal_ts,
        confirmation_ts: confirmation.ts + FOUR_HOURS_MS,
        entry_ts,
        exit_ts,
        entry_price,
        initial_stop,
        target_price,
        exit_price,
        exit_reason,
        gross_r,
        standard_net_r: net_r_after_costs(
            entry_price,
            exit_price,
            risk,
            entry_ts,
            exit_ts,
            STANDARD_EXTRA_SLIPPAGE_BPS,
            STANDARD_FUNDING_BPS_PER_8H,
        ),
        double_cost_net_r: net_r_after_costs(
            entry_price,
            exit_price,
            risk,
            entry_ts,
            exit_ts,
            DOUBLE_EXTRA_SLIPPAGE_BPS,
            DOUBLE_FUNDING_BPS_PER_8H,
        ),
    }))
}

/// V67 只在两根内出现阳线收回原信号实体中点时做多，随后沿用保守 2R 结算。
fn evaluate_macd_midpoint_seed(
    seed: &CompressedBreakdownSeed,
    candles: &[Candle],
) -> Result<SeedOutcome> {
    let Some(signal) = candles.first().filter(|candle| candle.ts == seed.signal_ts) else {
        return Ok(SeedOutcome::Incomplete);
    };
    let midpoint = (signal.open + signal.close) / 2.0;
    let confirmation = candles
        .iter()
        .skip(1)
        .take(MACD_WATCH_BARS)
        .find(|candle| candle.close > candle.open && candle.close > midpoint);
    let Some(confirmation) = confirmation else {
        return if candles.len() < MACD_WATCH_BARS + 1 {
            Ok(SeedOutcome::Incomplete)
        } else {
            Ok(SeedOutcome::NoConfirmation {
                busy_until: candles[MACD_WATCH_BARS].ts + FOUR_HOURS_MS,
            })
        };
    };
    let entry_price = confirmation.close;
    let initial_stop = signal.low.min(confirmation.low);
    let risk = entry_price - initial_stop;
    if !risk.is_finite() || risk <= 0.0 {
        return Ok(SeedOutcome::InvalidRisk {
            busy_until: confirmation.ts + FOUR_HOURS_MS,
        });
    }
    let target_price = entry_price + TARGET_R * risk;
    let future = candles
        .iter()
        .filter(|candle| candle.ts > confirmation.ts)
        .take(HOLD_BARS)
        .collect::<Vec<_>>();
    let mut exit = None;
    for candle in &future {
        if candle.low <= initial_stop {
            exit = Some((candle.ts, initial_stop, "stop"));
            break;
        }
        if candle.high >= target_price {
            exit = Some((candle.ts, target_price, "target_2r"));
            break;
        }
    }
    let (exit_bar_ts, exit_price, exit_reason) = if let Some(exit) = exit {
        exit
    } else if future.len() == HOLD_BARS {
        let last = future[HOLD_BARS - 1];
        (last.ts, last.close, "time_12_bars")
    } else {
        return Ok(SeedOutcome::Incomplete);
    };
    let entry_ts = confirmation.ts + FOUR_HOURS_MS;
    let exit_ts = exit_bar_ts + FOUR_HOURS_MS;
    let gross_r = (exit_price - entry_price) / risk;
    Ok(SeedOutcome::Trade(ReclaimTrade {
        seed_detail_id: seed.detail_id,
        symbol: seed.symbol.clone(),
        seed_signal_ts: seed.signal_ts,
        confirmation_ts: entry_ts,
        entry_ts,
        exit_ts,
        entry_price,
        initial_stop,
        target_price,
        exit_price,
        exit_reason,
        gross_r,
        standard_net_r: net_r_after_costs(
            entry_price,
            exit_price,
            risk,
            entry_ts,
            exit_ts,
            STANDARD_EXTRA_SLIPPAGE_BPS,
            STANDARD_FUNDING_BPS_PER_8H,
        ),
        double_cost_net_r: net_r_after_costs(
            entry_price,
            exit_price,
            risk,
            entry_ts,
            exit_ts,
            DOUBLE_EXTRA_SLIPPAGE_BPS,
            DOUBLE_FUNDING_BPS_PER_8H,
        ),
    }))
}

/// 在固定初始风险上扣除双边基础费、额外滑点和跨过的 8H 不利资金成本。
fn net_r_after_costs(
    entry: f64,
    exit: f64,
    risk: f64,
    entry_ts: i64,
    exit_ts: i64,
    extra_slippage_bps: f64,
    funding_bps_per_8h: f64,
) -> f64 {
    let execution_rate = BASE_FEE_RATE_PER_SIDE + extra_slippage_bps / 10_000.0;
    let execution_cost = (entry + exit) * execution_rate;
    let funding_intervals = (exit_ts.div_euclid(FUNDING_INTERVAL_MS)
        - entry_ts.div_euclid(FUNDING_INTERVAL_MS))
    .max(0) as f64;
    let funding_cost = entry * funding_intervals * funding_bps_per_8h / 10_000.0;
    (exit - entry - execution_cost - funding_cost) / risk
}

/// 构建标准/压力、时间切片与预登记门禁报告，不根据结果重新选择参数。
fn build_report(stage_counts: StageCounts, trades: Vec<ReclaimTrade>) -> ResearchReport {
    let standard_cost = summarize("all_standard", &trades, |trade| trade.standard_net_r);
    let double_cost = summarize("all_double_cost", &trades, |trade| trade.double_cost_net_r);
    let in_sample_trades = trades
        .iter()
        .filter(|trade| trade.entry_ts < OOS_START_MS)
        .cloned()
        .collect::<Vec<_>>();
    let out_of_sample_trades = trades
        .iter()
        .filter(|trade| trade.entry_ts >= OOS_START_MS)
        .cloned()
        .collect::<Vec<_>>();
    let in_sample = summarize("in_sample", &in_sample_trades, |trade| trade.standard_net_r);
    let out_of_sample = summarize("out_of_sample", &out_of_sample_trades, |trade| {
        trade.standard_net_r
    });
    let yearly = [2024, 2025, 2026]
        .into_iter()
        .map(|year| {
            let start = year_start_ms(year);
            let end = year_start_ms(year + 1);
            let scoped = trades
                .iter()
                .filter(|trade| start <= trade.entry_ts && trade.entry_ts < end)
                .cloned()
                .collect::<Vec<_>>();
            summarize(&year.to_string(), &scoped, |trade| trade.standard_net_r)
        })
        .collect();
    let walk_forward = walk_forward_windows()
        .into_iter()
        .map(|(label, start, end)| {
            let scoped = trades
                .iter()
                .filter(|trade| start <= trade.entry_ts && trade.entry_ts < end)
                .cloned()
                .collect::<Vec<_>>();
            summarize(label, &scoped, |trade| trade.standard_net_r)
        })
        .collect();
    let mut blockers = Vec::new();
    if standard_cost.trades < 20 {
        blockers.push(format!(
            "completed trades {} < frozen minimum 20",
            standard_cost.trades
        ));
    }
    if standard_cost.net_expectancy_r.unwrap_or(f64::NEG_INFINITY) < 0.6 {
        blockers.push("standard net expectancy below 0.6R".to_string());
    }
    if !profit_factor_passes(&standard_cost, 2.2) {
        blockers.push("standard profit factor below 2.2".to_string());
    }
    if in_sample.total_net_r <= 0.0 || out_of_sample.total_net_r <= 0.0 {
        blockers.push("in-sample and OOS must both have positive net R".to_string());
    }
    ResearchReport {
        strategy_key: STRATEGY_KEY,
        version: VERSION,
        source_backtest_id_min: BACKTEST_ID_MIN,
        source_backtest_id_max: BACKTEST_ID_MAX,
        watch_bars: WATCH_BARS,
        hold_bars: HOLD_BARS,
        target_r: TARGET_R,
        stage_counts,
        standard_cost,
        double_cost,
        in_sample,
        out_of_sample,
        yearly,
        walk_forward,
        historical_gate_pass: blockers.is_empty(),
        blockers,
        trades,
    }
}

/// 构建 V67 的独立历史门禁；失败后不进入冲突空头替换组合。
fn build_macd_report(
    stage_counts: MacdStageCounts,
    trades: Vec<ReclaimTrade>,
) -> MacdResearchReport {
    let standard_cost = summarize("all_standard", &trades, |trade| trade.standard_net_r);
    let double_cost = summarize("all_double_cost", &trades, |trade| trade.double_cost_net_r);
    let in_sample_trades = trades
        .iter()
        .filter(|trade| trade.entry_ts < OOS_START_MS)
        .cloned()
        .collect::<Vec<_>>();
    let out_of_sample_trades = trades
        .iter()
        .filter(|trade| trade.entry_ts >= OOS_START_MS)
        .cloned()
        .collect::<Vec<_>>();
    let in_sample = summarize("in_sample", &in_sample_trades, |trade| trade.standard_net_r);
    let out_of_sample = summarize("out_of_sample", &out_of_sample_trades, |trade| {
        trade.standard_net_r
    });
    let yearly = [2024, 2025, 2026]
        .into_iter()
        .map(|year| {
            let start = year_start_ms(year);
            let end = year_start_ms(year + 1);
            let scoped = trades
                .iter()
                .filter(|trade| start <= trade.entry_ts && trade.entry_ts < end)
                .cloned()
                .collect::<Vec<_>>();
            summarize(&year.to_string(), &scoped, |trade| trade.standard_net_r)
        })
        .collect();
    let walk_forward = walk_forward_windows()
        .into_iter()
        .map(|(label, start, end)| {
            let scoped = trades
                .iter()
                .filter(|trade| start <= trade.entry_ts && trade.entry_ts < end)
                .cloned()
                .collect::<Vec<_>>();
            summarize(label, &scoped, |trade| trade.standard_net_r)
        })
        .collect();
    let mut blockers = Vec::new();
    if standard_cost.trades < 20 {
        blockers.push(format!(
            "completed trades {} < frozen minimum 20",
            standard_cost.trades
        ));
    }
    if standard_cost.net_expectancy_r.unwrap_or(f64::NEG_INFINITY) < 0.6 {
        blockers.push("standard net expectancy below 0.6R".to_string());
    }
    if !profit_factor_passes(&standard_cost, 2.2) {
        blockers.push("standard profit factor below 2.2".to_string());
    }
    if in_sample.total_net_r <= 0.0 || out_of_sample.total_net_r <= 0.0 {
        blockers.push("in-sample and OOS must both have positive net R".to_string());
    }
    MacdResearchReport {
        strategy_key: MACD_STRATEGY_KEY,
        version: MACD_VERSION,
        source_backtest_id_min: BACKTEST_ID_MIN,
        source_backtest_id_max: BACKTEST_ID_MAX,
        watch_bars: MACD_WATCH_BARS,
        hold_bars: HOLD_BARS,
        target_r: TARGET_R,
        stage_counts,
        standard_cost,
        double_cost,
        in_sample,
        out_of_sample,
        yearly,
        walk_forward,
        historical_gate_pass: blockers.is_empty(),
        blockers,
        trades,
    }
}

/// 按传入成本口径计算交易级 EV、PF、回撤、Recovery 和 Sharpe。
fn summarize<F>(label: &str, trades: &[ReclaimTrade], value: F) -> SliceSummary
where
    F: Fn(&ReclaimTrade) -> f64,
{
    let values = trades.iter().map(value).collect::<Vec<_>>();
    let wins = values.iter().filter(|result| **result > 0.0).count();
    let total_net_r = values.iter().sum::<f64>();
    let gross_profit = values.iter().filter(|result| **result > 0.0).sum::<f64>();
    let gross_loss = values
        .iter()
        .filter(|result| **result < 0.0)
        .map(|result| result.abs())
        .sum::<f64>();
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for result in &values {
        equity += result;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    let mean = (!values.is_empty()).then(|| total_net_r / values.len() as f64);
    let trade_sharpe = mean.and_then(|mean| {
        if values.len() < 2 {
            return None;
        }
        let variance = values
            .iter()
            .map(|result| (result - mean).powi(2))
            .sum::<f64>()
            / (values.len() - 1) as f64;
        (variance > 0.0).then(|| mean / variance.sqrt() * (values.len() as f64).sqrt())
    });
    SliceSummary {
        label: label.to_string(),
        trades: values.len(),
        wins,
        win_rate_pct: if values.is_empty() {
            0.0
        } else {
            wins as f64 / values.len() as f64 * 100.0
        },
        net_expectancy_r: mean,
        profit_factor: (gross_loss > 0.0).then(|| gross_profit / gross_loss),
        total_net_r,
        max_drawdown_r: max_drawdown,
        recovery_factor: (max_drawdown > 0.0).then(|| total_net_r / max_drawdown),
        trade_sharpe,
    }
}

/// 无亏损且存在成交时视为 PF 门禁通过，否则使用有限 PF 比较。
fn profit_factor_passes(summary: &SliceSummary, threshold: f64) -> bool {
    summary
        .profit_factor
        .is_some_and(|profit_factor| profit_factor >= threshold)
        || (summary.trades > 0 && summary.wins == summary.trades)
}

/// 返回 UTC 年初时间戳，用于冻结年度切片。
fn year_start_ms(year: i32) -> i64 {
    chrono::NaiveDate::from_ymd_opt(year, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis()
}

/// 复用 V63 的 12 个月训练、3 个月固定参数观察窗口。
fn walk_forward_windows() -> [(&'static str, i64, i64); 7] {
    [
        ("walk_forward_test_1", 1_735_689_600_000, 1_743_465_600_000),
        ("walk_forward_test_2", 1_743_465_600_000, 1_751_328_000_000),
        ("walk_forward_test_3", 1_751_328_000_000, 1_759_276_800_000),
        ("walk_forward_test_4", 1_759_276_800_000, 1_767_225_600_000),
        ("walk_forward_test_5", 1_767_225_600_000, 1_775_001_600_000),
        ("walk_forward_test_6", 1_775_001_600_000, 1_782_864_000_000),
        ("walk_forward_test_7", 1_782_864_000_000, 1_790_812_800_000),
    ]
}

#[cfg(test)]
mod tests;
