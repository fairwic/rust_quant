//! 15 分钟动量衰竭 V2 长影线 cohort 的 Bollinger(20,2.5) L1 无标签覆盖扫描。
//!
//! 本模块只读信号时点已经完成的 K 线，不执行成交或读取任何结果标签。

pub mod confirmed_source_extreme_relimit;
pub mod first_retest_band_reentry;
pub mod first_retest_source_extreme_reclaim;
pub mod middle_partial_exit_l2;
pub mod recent_fast_ema_lead;
pub mod single_bar_ema_12_144_576_alignment;
pub mod source_extreme_reclaim_l2;

use super::args::market_momentum_exhaustion_reversal_v2_research_args;
use super::computed_candles::{bollinger_bands_from_closes, FAST_MOMENTUM_BOLLINGER_PERIOD};
use super::filtered_volume_rsi_ema_macd::filtered_volume_rsi_ema_macd_signal;
use super::{
    config_from_env_and_args, load_backtest_data, BacktestDataSet, ComputedCandle,
    MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection, MS_15M,
};
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::BTreeMap;
use std::path::Path;

/// 本轮 L1 只研究 V2 长影线候选增加 Bollinger 外轨触碰，不改变其他规则。
pub const RESEARCH_CANDIDATE_KEY: &str = "market_momentum_bollinger_wick_reversion_15m_v1";
/// 信号时点规则身份；后续过滤和退出研究必须使用新的版本标识。
pub const RESEARCH_RULE_VERSION: &str = "l1_filtered_vol2p5_net96x8_wick60_bb20x2p5_outer_touch_v1";
/// 冻结总体标准差宽度，避免误用现有通用指标的 2.0 倍实现。
pub const BOLLINGER_STDDEV_MULTIPLIER: f64 = 2.5;
/// V2 当前评估窗口起点，L1 仅做同窗口无标签覆盖诊断。
pub const EVALUATION_START_MS: i64 = 1_751_328_000_000;
/// V2 当前评估窗口终点，L1 不把该窗口表述为样本外证据。
pub const EVALUATION_END_MS: i64 = 1_784_470_500_000;
/// current-live Top60 的冻结抽样身份。
pub const SAMPLE_SEED: &str = "top60_v36_direct_kline_20260721";
/// V2 的周成交额 P90 至少需要信号前连续 672 根 K 线。
const REQUIRED_WARMUP_CANDLES: usize = 672;
/// 同方向候选在 60 分钟内视为同一市场事件，避免把共振币种当成独立样本。
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;

/// 机器报告的冻结输入身份，便于后续拒绝混用其他策略或布林参数。
#[derive(Debug, Clone, Serialize)]
pub struct L1ResearchIdentity {
    /// 当前研究等级；L1 禁止读取结果标签。
    pub level: &'static str,
    /// 新研究能力的独立候选键，不覆盖 V2。
    pub candidate_key: &'static str,
    /// 本轮唯一规则版本。
    pub rule_version: &'static str,
    /// 提供量比、96 根趋势和长影线定义的冻结来源。
    pub source_strategy_key: &'static str,
    /// 本轮唯一变量。
    pub only_variable: &'static str,
    /// 布林带收盘价周期。
    pub bollinger_period: usize,
    /// 布林带总体标准差倍数。
    pub bollinger_stddev_multiplier: f64,
    /// 因果计算时点。
    pub signal_time_policy: &'static str,
    /// 明确禁止本报告读取的标签。
    pub label_boundary: &'static str,
}

/// 当前本地样本的价格与成交额覆盖，缺口成员不会进入候选统计。
#[derive(Debug, Clone, Serialize)]
pub struct L1Coverage {
    /// 预期的冻结 Top60 成员数。
    pub expected_symbol_count: usize,
    /// 数据加载器实际返回的成员数。
    pub returned_symbol_count: usize,
    /// 同时具备完整预热和评价窗口的成员数。
    pub eligible_symbol_count: usize,
    /// 因 K 线缺口被排除的成员。
    pub excluded_symbols: Vec<L1ExcludedSymbol>,
    /// 评价窗口两端均包含。
    pub evaluation_start_ms: i64,
    /// 评价窗口两端均包含。
    pub evaluation_end_ms: i64,
    /// 每个完整成员的评价 K 线根数。
    pub expected_evaluation_candles_per_symbol: usize,
    /// 价格和 `vol_ccy` 身份指纹，不包含运行时间与输出路径。
    pub dataset_fingerprint_sha256: String,
    /// current-live Top60 的固有限制。
    pub universe_limitation: &'static str,
}

/// 被排除成员的可审计缺口，避免把部分币池误称完整 Top60。
#[derive(Debug, Clone, Serialize)]
pub struct L1ExcludedSymbol {
    /// OKX 永续合约标识。
    pub symbol: String,
    /// 预热加评价窗口应有的 K 线数。
    pub expected_candles: usize,
    /// 实际位于该窗口内的 K 线数。
    pub loaded_candles: usize,
    /// 缺失或重复导致的净缺口。
    pub missing_candles: usize,
    /// 缺失 `vol_ccy` 的 K 线数；信号门禁会逐根失败关闭。
    pub missing_volume_ccy_candles: usize,
    /// 排除原因。
    pub reason: &'static str,
}

/// 一条只含信号时点特征的候选记录，故意不包含成交后行情或盈亏字段。
#[derive(Debug, Clone, Serialize)]
pub struct L1Candidate {
    /// 交易对。
    pub symbol: String,
    /// 信号 K 的开始时间。
    pub signal_ts_ms: i64,
    /// 便于检查月份覆盖的 UTC 月份。
    pub signal_month_utc: String,
    /// `long` 或 `short`。
    pub direction: &'static str,
    /// V2 原始方向长影线触发标签。
    pub source_trigger: String,
    /// 过滤后量比。
    pub filtered_volume_ratio: f64,
    /// 过滤量比基线保留根数。
    pub filtered_volume_retained_candles: usize,
    /// 信号 K 的 `vol_ccy`。
    pub current_volume_ccy: f64,
    /// 信号前 672 根 `vol_ccy` 的 nearest-rank P90。
    pub weekly_volume_ccy_p90: f64,
    /// 信号前 96 根的有符号净移动百分比。
    pub prior_96_net_move_pct: f64,
    /// 信号 K 实体占完整振幅比例。
    pub body_range_ratio: f64,
    /// 对应方向影线占完整振幅比例。
    pub directional_wick_range_ratio: f64,
    /// 反方向影线占完整振幅比例。
    pub opposite_wick_range_ratio: f64,
    /// 信号 K 完成后的 20 期中轨。
    pub bollinger_middle: f64,
    /// 信号 K 完成后的 20 期 2.5 倍上轨。
    pub bollinger_upper: f64,
    /// 信号 K 完成后的 20 期 2.5 倍下轨。
    pub bollinger_lower: f64,
    /// 做多取 low<=lower，做空取 high>=upper。
    pub touches_directional_outer_band: bool,
    /// 外轨越界距离除以半带宽；负数表示尚未触轨。
    pub outer_excursion_half_band_ratio: f64,
    /// 收盘到中轨距离除以方向半带宽，只供下一轮分布预注册，不参与本轮筛选。
    pub close_middle_distance_half_band_ratio: f64,
    /// V2 冻结的实际成交价反方向初始止损距离。
    pub source_initial_stop_atr: f64,
    /// V2 冻结的影线极值限价有效根数。
    pub source_limit_valid_candles: usize,
}

/// L1 只评价覆盖和分散性，不能把这些统计解释为收益优势。
#[derive(Debug, Clone, Serialize)]
pub struct L1CandidateSummary {
    /// V2 中已经满足 60% 方向长影线的原始 setup 数。
    pub source_directional_wick_setups: usize,
    /// 新增外轨触碰后保留的 setup 数。
    pub outer_band_touch_setups: usize,
    /// 外轨触碰相对源长影线 setup 的保留比例。
    pub retention_pct: f64,
    /// 源 setup 多空分布。
    pub source_by_direction: BTreeMap<&'static str, usize>,
    /// 触轨 setup 多空分布。
    pub touches_by_direction: BTreeMap<&'static str, usize>,
    /// 触轨 setup 的币种覆盖。
    pub touches_by_symbol: BTreeMap<String, usize>,
    /// 触轨 setup 的月份覆盖。
    pub touches_by_month_utc: BTreeMap<String, usize>,
    /// 按方向与 60 分钟单链归并后的有效事件数。
    pub effective_market_events: usize,
    /// 触轨 setup 收盘距中轨的无标签分布，不能据此回看盈亏选阈值。
    pub close_middle_distance_distribution: BTreeMap<&'static str, usize>,
    /// 全量信号扫描的失败关闭计数，用于发现数据或基线门禁异常。
    pub source_blockers: BTreeMap<String, usize>,
}

/// 预注册覆盖门槛的逐项结果；目标图表审计因本轮未提供样本而保持待办。
#[derive(Debug, Clone, Serialize)]
pub struct L1Decision {
    /// `stop` 或 `coverage_pass_target_audit_pending`。
    pub status: &'static str,
    /// 预注册的覆盖门槛。
    pub gates: BTreeMap<&'static str, bool>,
    /// 当前停止或继续边界。
    pub reason: String,
    /// 本轮没有读取任何成交后结果。
    pub outcome_evaluation_performed: bool,
    /// 代表性目标图表是否已由用户提供并审计。
    pub target_chart_audit_completed: bool,
}

/// 完整 L1 机器产物；候选账本保留全部 setup 以支持独立复核。
#[derive(Debug, Clone, Serialize)]
pub struct L1Report {
    /// 报告 schema，后续字段变化必须显式升级。
    pub schema_version: &'static str,
    /// 生成时间不参与数据指纹。
    pub generated_at_utc: String,
    /// 冻结研究身份。
    pub identity: L1ResearchIdentity,
    /// 数据覆盖与局限。
    pub coverage: L1Coverage,
    /// 无标签候选汇总。
    pub summary: L1CandidateSummary,
    /// 预注册停止条件结果。
    pub decision: L1Decision,
    /// V2 方向长影线源账本，`touches_directional_outer_band` 表示唯一变量结果。
    pub candidates: Vec<L1Candidate>,
}

/// 用冻结 V2 参数读取本地 quant_core，生成且写出一份无标签 L1 报告。
pub async fn run_l1_scan(output: &Path) -> Result<L1Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for Bollinger wick L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_l1_report(&data, &config.args)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 构造与 V2 相同的只读 Top60 窗口，禁止调用者从命令行混入其他研究变量。
fn frozen_l1_args() -> Result<MarketVelocityEventBacktestArgs> {
    let mut args = market_momentum_exhaustion_reversal_v2_research_args()?;
    args.sample_limit = 60;
    args.sample_seed = SAMPLE_SEED.to_owned();
    args.event_start_ms = Some(EVALUATION_START_MS);
    args.event_end_ms = Some(EVALUATION_END_MS);
    args.save_backtest_detail = false;
    if args.entry_min_volume_ratio != 2.5 || args.entry_period != FAST_MOMENTUM_BOLLINGER_PERIOD {
        bail!("V2 frozen baseline identity drifted before Bollinger L1 scan");
    }
    Ok(args)
}

/// 从已加载行情构建机器报告；该函数只读取信号 K 及其之前的数据。
fn build_l1_report(
    data: &BacktestDataSet,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<L1Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(REQUIRED_WARMUP_CANDLES as i64 * MS_15M)
        .context("L1 warmup start overflow")?;
    let expected_window_candles = inclusive_candle_count(warmup_start_ms, EVALUATION_END_MS)?;
    let expected_evaluation_candles =
        inclusive_candle_count(EVALUATION_START_MS, EVALUATION_END_MS)?;
    let mut excluded_symbols = Vec::new();
    let mut eligible: Vec<(&str, &[ComputedCandle], usize, usize)> = Vec::new();
    for pair in &data.pairs {
        let candles = data
            .candles_15m_computed
            .get(&pair.symbol)
            .with_context(|| format!("missing computed candles for {}", pair.symbol))?;
        match complete_window_bounds(candles, warmup_start_ms, EVALUATION_END_MS) {
            Some((start_idx, end_idx)) => {
                eligible.push((pair.symbol.as_str(), candles.as_slice(), start_idx, end_idx))
            }
            None => excluded_symbols.push(excluded_symbol(
                &pair.symbol,
                candles,
                warmup_start_ms,
                EVALUATION_END_MS,
                expected_window_candles,
            )),
        }
    }
    eligible.sort_by(|left, right| left.0.cmp(right.0));
    excluded_symbols.sort_by(|left, right| left.symbol.cmp(&right.symbol));

    let dataset_fingerprint_sha256 = dataset_fingerprint(&eligible);
    let mut source_blockers = BTreeMap::new();
    let mut candidates = Vec::new();
    for (symbol, candles, _, _) in &eligible {
        candidates.extend(scan_symbol(symbol, candles, args, &mut source_blockers)?);
    }
    candidates.sort_by(|left, right| {
        (left.signal_ts_ms, left.direction, left.symbol.as_str()).cmp(&(
            right.signal_ts_ms,
            right.direction,
            right.symbol.as_str(),
        ))
    });
    let summary = summarize_candidates(&candidates, source_blockers);
    let decision = decide_l1(&summary);

    Ok(L1Report {
        schema_version: "momentum_bollinger_wick_l1_v1",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: L1ResearchIdentity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: RESEARCH_CANDIDATE_KEY,
            rule_version: RESEARCH_RULE_VERSION,
            source_strategy_key: "market_momentum_exhaustion_reversal_15m_v2_directional_wick_cohort",
            only_variable: "signal candle directional extreme touches Bollinger(20,2.5) outer band",
            bollinger_period: FAST_MOMENTUM_BOLLINGER_PERIOD,
            bollinger_stddev_multiplier: BOLLINGER_STDDEV_MULTIPLIER,
            signal_time_policy: "bands use the signal candle and previous 19 completed closes; any order can activate only after that signal candle closes",
            label_boundary: "candidate construction reads no fill, MFE, MAE, exit, PnL, R, win, or future candle fields",
        },
        coverage: L1Coverage {
            expected_symbol_count: 60,
            returned_symbol_count: data.pairs.len(),
            eligible_symbol_count: eligible.len(),
            excluded_symbols,
            evaluation_start_ms: EVALUATION_START_MS,
            evaluation_end_ms: EVALUATION_END_MS,
            expected_evaluation_candles_per_symbol: expected_evaluation_candles,
            dataset_fingerprint_sha256,
            universe_limitation: "current-live Top60 is a partial diagnostic with survivorship bias; it is not a point-in-time historical universe or OOS proof",
        },
        summary,
        decision,
        candidates,
    })
}

/// 扫描单币种 V2 方向长影线 setup，并只附加 20/2.5 外轨触碰这一项特征。
fn scan_symbol(
    symbol: &str,
    candles: &[ComputedCandle],
    args: &MarketVelocityEventBacktestArgs,
    source_blockers: &mut BTreeMap<String, usize>,
) -> Result<Vec<L1Candidate>> {
    let mut candidates = Vec::new();
    for (idx, latest) in candles.iter().enumerate() {
        if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&latest.candle.ts) {
            continue;
        }
        let signal = match filtered_volume_rsi_ema_macd_signal(candles, idx + 1, args) {
            Ok(signal) => signal,
            Err(reason) => {
                *source_blockers.entry(reason.to_owned()).or_default() += 1;
                continue;
            }
        };
        let Some(anchor) = signal.evidence.anchor_entry.as_ref() else {
            continue;
        };
        if anchor.activation_mode != "directional_wick_limit_12_candles" {
            continue;
        }
        let start = idx
            .checked_add(1)
            .and_then(|end| end.checked_sub(FAST_MOMENTUM_BOLLINGER_PERIOD))
            .context("directional wick candidate lacks Bollinger warmup")?;
        let closes = candles[start..=idx]
            .iter()
            .map(|candle| candle.candle.close)
            .collect::<Vec<_>>();
        let bands = bollinger_bands_from_closes(&closes, BOLLINGER_STDDEV_MULTIPLIER)
            .context("invalid Bollinger(20,2.5) input")?;
        let (direction, touches, excursion, half_band) = match signal.direction {
            MarketVelocityTradeDirection::Long => {
                let half_band = bands.middle - bands.lower;
                (
                    "long",
                    latest.candle.low <= bands.lower,
                    bands.lower - latest.candle.low,
                    half_band,
                )
            }
            MarketVelocityTradeDirection::Short => {
                let half_band = bands.upper - bands.middle;
                (
                    "short",
                    latest.candle.high >= bands.upper,
                    latest.candle.high - bands.upper,
                    half_band,
                )
            }
            MarketVelocityTradeDirection::Both => continue,
        };
        if !half_band.is_finite() || half_band <= 0.0 {
            continue;
        }
        let isolated = signal
            .evidence
            .isolated_family
            .as_ref()
            .context("V2 signal missing isolated-family evidence")?;
        let current_volume_ccy = signal
            .evidence
            .current_volume_ccy
            .context("V2 signal missing current vol_ccy")?;
        let weekly_volume_ccy_p90 = signal
            .evidence
            .weekly_volume_ccy_p90
            .context("V2 signal missing weekly vol_ccy P90")?;
        let signal_month_utc = Utc
            .timestamp_millis_opt(latest.candle.ts)
            .single()
            .context("invalid signal timestamp")?
            .format("%Y-%m")
            .to_string();
        candidates.push(L1Candidate {
            symbol: symbol.to_owned(),
            signal_ts_ms: latest.candle.ts,
            signal_month_utc,
            direction,
            source_trigger: signal.trigger,
            filtered_volume_ratio: signal.evidence.filtered_volume_ratio,
            filtered_volume_retained_candles: signal.evidence.filtered_volume_retained_candles,
            current_volume_ccy,
            weekly_volume_ccy_p90,
            prior_96_net_move_pct: isolated
                .prior_96_net_move_pct
                .context("V2 signal missing prior 96-bar move")?,
            body_range_ratio: anchor.pivot_body_range_ratio,
            directional_wick_range_ratio: anchor.pivot_directional_wick_range_ratio,
            opposite_wick_range_ratio: anchor.pivot_opposite_wick_range_ratio,
            bollinger_middle: bands.middle,
            bollinger_upper: bands.upper,
            bollinger_lower: bands.lower,
            touches_directional_outer_band: touches,
            outer_excursion_half_band_ratio: excursion / half_band,
            close_middle_distance_half_band_ratio: (latest.candle.close - bands.middle).abs()
                / half_band,
            source_initial_stop_atr: 1.5,
            source_limit_valid_candles: 12,
        });
    }
    Ok(candidates)
}

/// 汇总候选覆盖；所有分组字段都来自信号时点账本。
fn summarize_candidates(
    candidates: &[L1Candidate],
    source_blockers: BTreeMap<String, usize>,
) -> L1CandidateSummary {
    let mut source_by_direction = BTreeMap::new();
    let mut touches_by_direction = BTreeMap::new();
    let mut touches_by_symbol = BTreeMap::new();
    let mut touches_by_month_utc = BTreeMap::new();
    let mut close_middle_distance_distribution = BTreeMap::from([
        ("le_0_10_half_band", 0),
        ("gt_0_10_le_0_25", 0),
        ("gt_0_25_le_0_50", 0),
        ("gt_0_50_le_1_00", 0),
        ("gt_1_00", 0),
    ]);
    for candidate in candidates {
        *source_by_direction.entry(candidate.direction).or_default() += 1;
        if !candidate.touches_directional_outer_band {
            continue;
        }
        *touches_by_direction.entry(candidate.direction).or_default() += 1;
        *touches_by_symbol
            .entry(candidate.symbol.clone())
            .or_default() += 1;
        *touches_by_month_utc
            .entry(candidate.signal_month_utc.clone())
            .or_default() += 1;
        let bucket = match candidate.close_middle_distance_half_band_ratio {
            value if value <= 0.10 => "le_0_10_half_band",
            value if value <= 0.25 => "gt_0_10_le_0_25",
            value if value <= 0.50 => "gt_0_25_le_0_50",
            value if value <= 1.00 => "gt_0_50_le_1_00",
            _ => "gt_1_00",
        };
        *close_middle_distance_distribution
            .entry(bucket)
            .or_default() += 1;
    }
    let outer_band_touch_setups = candidates
        .iter()
        .filter(|candidate| candidate.touches_directional_outer_band)
        .count();
    let retention_pct = percentage(outer_band_touch_setups, candidates.len());
    L1CandidateSummary {
        source_directional_wick_setups: candidates.len(),
        outer_band_touch_setups,
        retention_pct,
        source_by_direction,
        touches_by_direction,
        touches_by_symbol,
        touches_by_month_utc,
        effective_market_events: effective_market_event_count(candidates),
        close_middle_distance_distribution,
        source_blockers,
    }
}

/// 应用查看结果前冻结的覆盖门槛；未提供目标图表时最多进入待审计状态。
fn decide_l1(summary: &L1CandidateSummary) -> L1Decision {
    let long_count = summary
        .touches_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_count = summary
        .touches_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let mut gates = BTreeMap::new();
    gates.insert(
        "touch_setups_at_least_30",
        summary.outer_band_touch_setups >= 30,
    );
    gates.insert(
        "retention_between_10_and_60_pct",
        (10.0..=60.0).contains(&summary.retention_pct),
    );
    gates.insert(
        "effective_events_at_least_15",
        summary.effective_market_events >= 15,
    );
    gates.insert("symbols_at_least_8", summary.touches_by_symbol.len() >= 8);
    gates.insert("months_at_least_6", summary.touches_by_month_utc.len() >= 6);
    gates.insert(
        "both_directions_at_least_5",
        long_count >= 5 && short_count >= 5,
    );
    let coverage_passed = gates.values().all(|passed| *passed);
    L1Decision {
        status: if coverage_passed {
            "coverage_pass_target_audit_pending"
        } else {
            "stop"
        },
        gates,
        reason: if coverage_passed {
            "无标签覆盖与分散性达到预注册门槛；用户尚未提供目标图表，不能进入结果回放或宣称有效。"
                .to_owned()
        } else {
            "至少一项预注册覆盖门槛未通过；按 L1 停止条件不进入结果回放。".to_owned()
        },
        outcome_evaluation_performed: false,
        target_chart_audit_completed: false,
    }
}

/// 按方向把相邻不超过 60 分钟的触轨 setup 单链归并为有效市场事件。
fn effective_market_event_count(candidates: &[L1Candidate]) -> usize {
    let mut last_by_direction: BTreeMap<&str, i64> = BTreeMap::new();
    let mut count = 0;
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.touches_directional_outer_band)
    {
        let starts_new = last_by_direction
            .get(candidate.direction)
            .is_none_or(|previous| candidate.signal_ts_ms - *previous > EVENT_CLUSTER_WINDOW_MS);
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(candidate.direction, candidate.signal_ts_ms);
    }
    count
}

/// 校验预热到评价终点的每个 15m 时间戳，禁止跨缺口延长指标窗口。
fn complete_window_bounds(
    candles: &[ComputedCandle],
    start_ms: i64,
    end_ms: i64,
) -> Option<(usize, usize)> {
    let start_idx = candles
        .binary_search_by_key(&start_ms, |candle| candle.candle.ts)
        .ok()?;
    let end_idx = candles
        .binary_search_by_key(&end_ms, |candle| candle.candle.ts)
        .ok()?;
    let window = candles.get(start_idx..=end_idx)?;
    let expected = inclusive_candle_count(start_ms, end_ms).ok()?;
    if window.len() != expected
        || window
            .iter()
            .enumerate()
            .any(|(offset, candle)| candle.candle.ts != start_ms + offset as i64 * MS_15M)
    {
        return None;
    }
    Some((start_idx, end_idx))
}

/// 生成排除证据；`vol_ccy` 缺失单独报告但价格时间缺口才排除整个成员。
fn excluded_symbol(
    symbol: &str,
    candles: &[ComputedCandle],
    start_ms: i64,
    end_ms: i64,
    expected_candles: usize,
) -> L1ExcludedSymbol {
    let window = candles
        .iter()
        .filter(|candle| (start_ms..=end_ms).contains(&candle.candle.ts))
        .collect::<Vec<_>>();
    let loaded_candles = window.len();
    L1ExcludedSymbol {
        symbol: symbol.to_owned(),
        expected_candles,
        loaded_candles,
        missing_candles: expected_candles.saturating_sub(loaded_candles),
        missing_volume_ccy_candles: window
            .iter()
            .filter(|candle| candle.volume_ccy.is_none())
            .count(),
        reason: "incomplete_or_non_contiguous_15m_warmup_and_evaluation_window",
    }
}

/// 计算两端包含且必须与 15m 对齐的 K 线数量。
fn inclusive_candle_count(start_ms: i64, end_ms: i64) -> Result<usize> {
    let span = end_ms
        .checked_sub(start_ms)
        .context("candle window end precedes start")?;
    if span < 0 || span % MS_15M != 0 {
        bail!("candle window is not aligned to 15m boundaries");
    }
    usize::try_from(span / MS_15M + 1).context("candle window count overflows usize")
}

/// 对完整成员的信号可见行情生成稳定指纹，避免同名报告混用不同数据。
fn dataset_fingerprint(eligible: &[(&str, &[ComputedCandle], usize, usize)]) -> String {
    let mut hasher = Sha256::new();
    for (symbol, candles, start_idx, end_idx) in eligible {
        hash_bytes(&mut hasher, symbol.as_bytes());
        for candle in &candles[*start_idx..=*end_idx] {
            hasher.update(candle.candle.ts.to_le_bytes());
            hasher.update(candle.candle.open.to_bits().to_le_bytes());
            hasher.update(candle.candle.high.to_bits().to_le_bytes());
            hasher.update(candle.candle.low.to_bits().to_le_bytes());
            hasher.update(candle.candle.close.to_bits().to_le_bytes());
            hasher.update(candle.candle.volume.to_bits().to_le_bytes());
            match candle.volume_ccy {
                Some(value) => {
                    hasher.update([1]);
                    hasher.update(value.to_bits().to_le_bytes());
                }
                None => hasher.update([0]),
            }
        }
    }
    hex::encode(hasher.finalize())
}

/// 用长度前缀消除相邻文本拼接的指纹歧义。
fn hash_bytes(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

/// 返回稳定百分比；空分母按零处理，供停止条件直接判定。
fn percentage(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::momentum_exhaustion_reversal_v1::MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES;
    use crate::app::market_velocity_event_backtest::{
        BacktestCandle, MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION,
    };

    /// 构造已经具备 V2 预热的中性 K 线，测试只修改信号时点字段。
    fn candle(idx: usize) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: idx as i64 * MS_15M,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 10.0,
            },
            volume_ccy: Some(100.0),
            sma: Some(100.0),
            ema: Some(100.0),
            ema12: Some(100.0),
            ema144: Some(100.0),
            ema169: Some(100.0),
            ema696: Some(100.0),
            previous_volume_avg: Some(10.0),
            previous_range_avg: Some(2.0),
            rsi14: Some(50.0),
            atr14: Some(2.0),
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: Some(0.0),
            macd_signal_line: Some(0.0),
            macd_histogram: Some(0.0),
        }
    }

    /// 构造长下影做多并让 low 明确越过 20/2.5 下轨。
    fn long_setup() -> (Vec<ComputedCandle>, MarketVelocityEventBacktestArgs) {
        let mut candles = (0..750).map(candle).collect::<Vec<_>>();
        let latest_idx = candles.len() - 1;
        for (idx, candle) in candles.iter_mut().enumerate() {
            candle.candle.ts = EVALUATION_START_MS - (latest_idx - idx) as i64 * MS_15M;
        }
        let history_start = latest_idx - MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES;
        candles[history_start].candle.open = 100.0;
        candles[latest_idx - 1].candle.close = 91.0;
        candles[latest_idx].candle = BacktestCandle {
            ts: EVALUATION_START_MS,
            open: 90.0,
            high: 96.0,
            low: 70.0,
            close: 94.0,
            volume: 25.0,
        };
        candles[latest_idx].volume_ccy = Some(200.0);
        let args = frozen_l1_args().expect("frozen args");
        (candles, args)
    }

    /// 构造长上影做空并让 high 明确越过 20/2.5 上轨，验证镜像语义。
    fn short_setup() -> (Vec<ComputedCandle>, MarketVelocityEventBacktestArgs) {
        let mut candles = (0..750).map(candle).collect::<Vec<_>>();
        let latest_idx = candles.len() - 1;
        for (idx, candle) in candles.iter_mut().enumerate() {
            candle.candle.ts = EVALUATION_START_MS - (latest_idx - idx) as i64 * MS_15M;
        }
        let history_start = latest_idx - MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES;
        candles[history_start].candle.open = 100.0;
        candles[latest_idx - 1].candle.close = 109.0;
        candles[latest_idx].candle = BacktestCandle {
            ts: EVALUATION_START_MS,
            open: 110.0,
            high: 130.0,
            low: 104.0,
            close: 106.0,
            volume: 25.0,
        };
        candles[latest_idx].volume_ccy = Some(200.0);
        let args = frozen_l1_args().expect("frozen args");
        (candles, args)
    }

    /// 布林与 EMA 只能扩展既有 V2 候选，不能回退到通用 RSI/EMA/MACD 方向分支。
    #[test]
    fn frozen_source_dispatches_to_existing_momentum_v2_entry_rule() {
        let args = frozen_l1_args().expect("frozen args");

        assert_eq!(
            args.paper_outcome_entry_rule_version,
            MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION
        );
    }

    /// 方向长下影必须映射为下轨触碰做多，并使用用户指定的 2.5 倍宽度。
    #[test]
    fn lower_wick_touch_maps_to_long_and_uses_two_point_five_width() {
        let (candles, args) = long_setup();
        let mut blockers = BTreeMap::new();
        let candidates =
            scan_symbol("TEST-USDT-SWAP", &candles, &args, &mut blockers).expect("scan long setup");

        assert_eq!(candidates.len(), 1, "blockers={blockers:?}");
        assert_eq!(candidates[0].direction, "long");
        assert!(candidates[0].touches_directional_outer_band);
        assert_eq!(candidates[0].source_limit_valid_candles, 12);
        let closes = candles[candles.len() - FAST_MOMENTUM_BOLLINGER_PERIOD..]
            .iter()
            .map(|candle| candle.candle.close)
            .collect::<Vec<_>>();
        let expected = bollinger_bands_from_closes(&closes, 2.5).expect("bands");
        assert_eq!(candidates[0].bollinger_lower, expected.lower);
    }

    /// “反之镜像”必须映射为长上影触碰上轨做空，不能复用用户原文的重复空单笔误。
    #[test]
    fn upper_wick_touch_maps_to_short_mirror() {
        let (candles, args) = short_setup();
        let mut blockers = BTreeMap::new();
        let candidates = scan_symbol("TEST-USDT-SWAP", &candles, &args, &mut blockers)
            .expect("scan short setup");

        assert_eq!(candidates.len(), 1, "blockers={blockers:?}");
        assert_eq!(candidates[0].direction, "short");
        assert!(candidates[0].touches_directional_outer_band);
    }

    /// 候选 JSON 必须只含信号可见特征，不得出现任何成交后结果键。
    #[test]
    fn report_schema_contains_no_post_signal_result_fields() {
        let (candles, args) = long_setup();
        let mut blockers = BTreeMap::new();
        let candidates =
            scan_symbol("TEST-USDT-SWAP", &candles, &args, &mut blockers).expect("scan long setup");
        let json = serde_json::to_value(&candidates).expect("candidate json");
        let object = json[0].as_object().expect("candidate object");

        for forbidden in ["fill", "mfe", "mae", "exit", "pnl", "r", "win"] {
            assert!(!object.contains_key(forbidden));
        }
    }

    /// 即使窗口两端存在，中间缺 K 也必须把整个成员排除。
    #[test]
    fn incomplete_window_is_rejected_even_when_endpoints_exist() {
        let mut candles = (0..5).map(candle).collect::<Vec<_>>();
        candles.remove(2);

        assert!(complete_window_bounds(&candles, 0, 4 * MS_15M).is_none());
    }

    /// 同方向一小时内的跨币共振只计一个事件，反方向仍必须独立计数。
    #[test]
    fn clustered_events_are_direction_sensitive_and_chain_within_one_hour() {
        let (candles, args) = long_setup();
        let mut blockers = BTreeMap::new();
        let base = scan_symbol("A-USDT-SWAP", &candles, &args, &mut blockers)
            .expect("base candidate")
            .pop()
            .expect("one candidate");
        let mut second = base.clone();
        second.symbol = "B-USDT-SWAP".to_owned();
        second.signal_ts_ms += EVENT_CLUSTER_WINDOW_MS;
        let mut third = second.clone();
        third.symbol = "C-USDT-SWAP".to_owned();
        third.signal_ts_ms += EVENT_CLUSTER_WINDOW_MS;
        let mut opposite = third.clone();
        opposite.direction = "short";

        assert_eq!(
            effective_market_event_count(&[base, second, third, opposite]),
            2
        );
    }
}
