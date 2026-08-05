//! Bollinger 长影 setup 首次重测收回带内的 L1 无标签扫描。
//!
//! 来源长影 K 只负责建立 setup。新信号必须等待后续首次重测完成，并在该根 K 收盘确认
//! 价格重新进入当时的 Bollinger(20,2.5)；最早只能在下一根 K 开盘激活。

use super::super::computed_candles::{bollinger_bands_from_closes, FAST_MOMENTUM_BOLLINGER_PERIOD};
use super::{
    build_l1_report, frozen_l1_args, L1Candidate, L1Coverage, BOLLINGER_STDDEV_MULTIPLIER,
    RESEARCH_CANDIDATE_KEY as SOURCE_CANDIDATE_KEY, RESEARCH_RULE_VERSION as SOURCE_RULE_VERSION,
};
use crate::app::market_velocity_event_backtest::{
    config_from_env_and_args, load_backtest_data, BacktestDataSet, ComputedCandle, MS_15M,
};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

/// 入场语义改变后使用独立候选键，避免覆盖既有布林长影被动限价版本。
pub const FIRST_RETEST_REENTRY_CANDIDATE_KEY: &str =
    "market_momentum_bollinger_wick_reentry_confirmation_15m_v1";
/// 本轮唯一规则版本：首次重测收回 20/2.5 带内后，下一根开盘才允许激活。
pub const FIRST_RETEST_REENTRY_RULE_VERSION: &str =
    "l1_first_retest_close_inside_bb20x2p5_next_open_v1";
/// 保持来源 V2 的挂单观察窗口，避免同时调节确认规则与有效期。
const RETEST_VALID_CANDLES: usize = 12;
/// 同方向确认信号相邻不超过一小时视为同一市场事件。
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
/// 上一轮固定的十个失败 setup，仅用于检查新定义是否解释目标样本，不参与阈值选择。
const TARGET_SAMPLES: [(&str, i64); 10] = [
    ("AGLD-USDT-SWAP", 1_783_444_500_000),
    ("YFI-USDT-SWAP", 1_783_328_400_000),
    ("NMR-USDT-SWAP", 1_782_962_100_000),
    ("ORDI-USDT-SWAP", 1_782_738_900_000),
    ("SATS-USDT-SWAP", 1_782_647_100_000),
    ("WIF-USDT-SWAP", 1_782_522_900_000),
    ("EIGEN-USDT-SWAP", 1_781_521_200_000),
    ("BABY-USDT-SWAP", 1_781_520_300_000),
    ("OP-USDT-SWAP", 1_781_332_200_000),
    ("CVX-USDT-SWAP", 1_781_216_100_000),
];

/// L1 报告的独立版本、因果时点与单变量边界。
#[derive(Debug, Clone, Serialize)]
pub struct FirstRetestReentryIdentity {
    /// 当前研究等级；L1 不读取确认后结果。
    pub level: &'static str,
    /// 入场语义改变后的独立候选策略键。
    pub candidate_key: &'static str,
    /// 本轮首次重测确认规则版本。
    pub rule_version: &'static str,
    /// 提供量比、长影线和首次外轨触碰的来源候选键。
    pub source_candidate_key: &'static str,
    /// 来源候选的冻结规则版本。
    pub source_rule_version: &'static str,
    /// 本批唯一变化的入场语义。
    pub only_variable: &'static str,
    /// 首次重测的方向极值与有效期合同。
    pub first_retest_policy: &'static str,
    /// 重测 K 收盘如何判定重新进入方向外轨。
    pub close_confirmation_policy: &'static str,
    /// 确认完成后允许激活的最早时点。
    pub activation_policy: &'static str,
    /// 重测使用的布林收盘周期。
    pub bollinger_period: usize,
    /// 重测使用的总体标准差倍数。
    pub bollinger_stddev_multiplier: f64,
    /// 来源 setup 后最多观察的完整 15m K 数。
    pub retest_valid_candles: usize,
    /// 明确禁止读取的结果标签。
    pub label_boundary: &'static str,
}

/// 单条来源 setup 的首次重测与确认状态，不含成交价格或持仓结果。
#[derive(Debug, Clone, Serialize)]
pub struct FirstRetestReentryCandidate {
    /// OKX 永续合约标识。
    pub symbol: String,
    /// 来源长影 setup K 的开始时间，Unix 毫秒。
    pub setup_ts_ms: i64,
    /// 来源 setup 所在 UTC 月份。
    pub setup_month_utc: String,
    /// `long` 或 `short`，完全继承来源 V2 方向。
    pub direction: &'static str,
    /// 来源 V2 的长影触发标签。
    pub source_trigger: String,
    /// 来源 setup 的方向极值；做空取 high，做多取 low。
    pub source_extreme_price: f64,
    /// 来源信号时过滤量比，只供账本审计，不参与本轮确认。
    pub filtered_volume_ratio: f64,
    /// 来源信号前 96 根有符号净移动百分比。
    pub prior_96_net_move_pct: f64,
    /// 来源方向影线占完整振幅比例。
    pub directional_wick_range_ratio: f64,
    /// 首次触及来源极值的后续 K 时间；未触及时为 `None`。
    pub first_retest_ts_ms: Option<i64>,
    /// 首次重测距来源 setup 的 15m K 根数，从 1 开始。
    pub first_retest_offset_bars: Option<usize>,
    /// 首次重测 K 的收盘价；没有重测时为 `None`。
    pub first_retest_close: Option<f64>,
    /// 首次重测 K 完成后的 20/2.5 中轨。
    pub retest_bollinger_middle: Option<f64>,
    /// 首次重测 K 完成后的 20/2.5 上轨。
    pub retest_bollinger_upper: Option<f64>,
    /// 首次重测 K 完成后的 20/2.5 下轨。
    pub retest_bollinger_lower: Option<f64>,
    /// `true` 表示做空收于上轨内或做多收于下轨内；未重测时为空。
    pub close_inside_directional_band: Option<bool>,
    /// 只记录收盘是否越过来源极值，不参与本轮筛选，防止暗中叠加第二变量。
    pub close_beyond_source_extreme: Option<bool>,
    /// 新确认信号时间；只有首次重测收回带内时存在。
    pub confirmation_signal_ts_ms: Option<i64>,
    /// 最早允许入场的下一根 K 开始时间；本报告不读取该 K 的价格。
    pub earliest_entry_ts_ms: Option<i64>,
    /// 首次重测确认、突破接受、来源替换或超时等终态。
    pub status: &'static str,
}

/// 用户固定目标 setup 的定义命中结果。
#[derive(Debug, Clone, Serialize)]
pub struct FirstRetestTargetAudit {
    /// 目标交易对。
    pub symbol: &'static str,
    /// 目标来源 setup 时间，Unix 毫秒。
    pub setup_ts_ms: i64,
    /// 是否在来源账本中找到目标 setup。
    pub source_found: bool,
    /// 目标首次重测时间；未触及时为空。
    pub first_retest_ts_ms: Option<i64>,
    /// 目标候选终态；来源缺失时为空。
    pub status: Option<&'static str>,
    /// 是否因首次重测收于方向外轨之外而拒绝。
    pub rejected_by_close_acceptance: bool,
}

/// L1 无标签覆盖、方向分布与目标样本汇总。
#[derive(Debug, Clone, Serialize)]
pub struct FirstRetestReentrySummary {
    /// 来源 Bollinger 外轨长影 setup 总数。
    pub base_touch_setups: usize,
    /// 12 根内实际发生首次方向极值重测的 setup 数。
    pub first_retest_setups: usize,
    /// 首次重测收回方向外轨内并形成新确认信号的数量。
    pub confirmed_setups: usize,
    /// 首次重测收在方向外轨之外并立即失效的数量。
    pub rejected_close_acceptance_setups: usize,
    /// 确认数占首次重测数的比例。
    pub confirmation_retention_pct_of_first_retests: f64,
    /// 突破接受拒绝数占首次重测数的比例。
    pub rejection_impact_pct_of_first_retests: f64,
    /// 确认信号的多空分布。
    pub confirmed_by_direction: BTreeMap<&'static str, usize>,
    /// 突破接受拒绝的多空分布。
    pub rejected_by_direction: BTreeMap<&'static str, usize>,
    /// 确认信号覆盖的币种数。
    pub confirmed_symbol_count: usize,
    /// 确认信号覆盖的 UTC 月份数。
    pub confirmed_month_count: usize,
    /// 按确认时间、方向和一小时窗口归并的有效市场事件数。
    pub confirmed_effective_market_events: usize,
    /// 未重测、被新 setup 替换或 forward 数据不足的终态计数。
    pub blockers: BTreeMap<&'static str, usize>,
    /// 十个固定目标中实际被突破接受规则拒绝的数量。
    pub target_rejected_count: usize,
}

/// 查看任何结果标签前冻结的 L1 门禁结论。
#[derive(Debug, Clone, Serialize)]
pub struct FirstRetestReentryDecision {
    /// `stop` 或 `coverage_pass_l2_ready`。
    pub status: &'static str,
    /// 每项预注册覆盖门槛的结果。
    pub gates: BTreeMap<&'static str, bool>,
    /// 停止或允许进入 L2 的证据边界。
    pub reason: String,
    /// L1 必须始终为 `false`。
    pub outcome_evaluation_performed: bool,
}

/// 只读 L1 扫描的阶段耗时，单位毫秒。
#[derive(Debug, Clone, Default, Serialize)]
pub struct FirstRetestPhaseTimingsMs {
    /// 连接并加载本地行情所用毫秒数。
    pub data_load: u128,
    /// 重建冻结来源 L1 账本所用毫秒数。
    pub source_l1_build: u128,
    /// 首次重测扫描、汇总和目标审计所用毫秒数。
    pub variant_scan: u128,
}

/// 首次重测收回带内的完整 L1 机器报告。
#[derive(Debug, Clone, Serialize)]
pub struct FirstRetestReentryReport {
    /// 报告 schema；字段语义变化时必须升级。
    pub schema_version: &'static str,
    /// 报告生成时间，不参与数据身份。
    pub generated_at_utc: String,
    /// 独立候选及因果边界。
    pub identity: FirstRetestReentryIdentity,
    /// 来源外轨候选账本的稳定 SHA-256。
    pub source_candidate_ledger_sha256: String,
    /// 来源行情身份与成员局限。
    pub coverage: L1Coverage,
    /// 各阶段执行耗时，单位毫秒。
    pub phase_timings_ms: FirstRetestPhaseTimingsMs,
    /// 无标签覆盖汇总。
    pub summary: FirstRetestReentrySummary,
    /// 固定十个目标 setup 的定义审计。
    pub target_sample_audit: Vec<FirstRetestTargetAudit>,
    /// L1 停止或升级门禁。
    pub decision: FirstRetestReentryDecision,
    /// 全部来源外轨 setup 的首次重测终态账本。
    pub candidates: Vec<FirstRetestReentryCandidate>,
}

/// 读取本地 `quant_core`，执行首次重测收回带内的 Research-only L1 扫描。
pub async fn run_first_retest_band_reentry_l1_scan(
    output: &Path,
) -> Result<FirstRetestReentryReport> {
    let data_started = Instant::now();
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for first-retest Bollinger L1")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let data_load = data_started.elapsed().as_millis();

    let source_started = Instant::now();
    let source_report = build_l1_report(&data, &config.args)?;
    let source_l1_build = source_started.elapsed().as_millis();

    let variant_started = Instant::now();
    let mut report = build_first_retest_report(&data, source_report)?;
    report.phase_timings_ms = FirstRetestPhaseTimingsMs {
        data_load,
        source_l1_build,
        variant_scan: variant_started.elapsed().as_millis(),
    };
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建首次重测 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入首次重测 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 从冻结来源账本构建新确认信号；只读取确认时已经完成的 K 线。
fn build_first_retest_report(
    data: &BacktestDataSet,
    source_report: super::L1Report,
) -> Result<FirstRetestReentryReport> {
    let source_candidates = source_report
        .candidates
        .iter()
        .filter(|candidate| candidate.touches_directional_outer_band)
        .collect::<Vec<_>>();
    let source_candidate_ledger_sha256 = candidate_ledger_sha256(&source_candidates)?;
    let replacement_times = replacement_times_by_symbol(&source_candidates);
    let mut candidates = Vec::with_capacity(source_candidates.len());
    for source in source_candidates {
        let replacements = replacement_times
            .get(source.symbol.as_str())
            .context("source symbol missing replacement timeline")?;
        let candles = data
            .candles_15m_computed
            .get(&source.symbol)
            .with_context(|| format!("missing computed candles for {}", source.symbol))?;
        candidates.push(resolve_first_retest(candles, source, replacements)?);
    }
    candidates.sort_by(|left, right| {
        (left.setup_ts_ms, left.direction, left.symbol.as_str()).cmp(&(
            right.setup_ts_ms,
            right.direction,
            right.symbol.as_str(),
        ))
    });
    let target_sample_audit = audit_target_samples(&candidates);
    let summary = summarize_candidates(&candidates, &target_sample_audit);
    let decision = decide_l1(&summary, &target_sample_audit);

    Ok(FirstRetestReentryReport {
        schema_version: "momentum_bollinger_first_retest_reentry_l1_v1",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: FirstRetestReentryIdentity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: FIRST_RETEST_REENTRY_CANDIDATE_KEY,
            rule_version: FIRST_RETEST_REENTRY_RULE_VERSION,
            source_candidate_key: SOURCE_CANDIDATE_KEY,
            source_rule_version: SOURCE_RULE_VERSION,
            only_variable: "replace passive first extreme touch fill with first-retest close-back-inside Bollinger(20,2.5) confirmation and next-bar activation",
            first_retest_policy: "inspect offsets 1..=12; short first high>=source high, long first low<=source low; first touch is terminal and an untouched pending setup is replaced by a newer source setup",
            close_confirmation_policy: "short confirms when completed retest close<=current upper band; long confirms when close>=current lower band; the retest close is included in the 20-close band",
            activation_policy: "confirmation signal exists only at retest close; earliest entry timestamp is the next 15m candle and this L1 report does not read its price",
            bollinger_period: FAST_MOMENTUM_BOLLINGER_PERIOD,
            bollinger_stddev_multiplier: BOLLINGER_STDDEV_MULTIPLIER,
            retest_valid_candles: RETEST_VALID_CANDLES,
            label_boundary: "reads source setup and completed pre-entry retest candles only; no entry price, stop/target path, MFE, MAE, exit, PnL, R, win, or loss fields",
        },
        source_candidate_ledger_sha256,
        coverage: source_report.coverage,
        phase_timings_ms: FirstRetestPhaseTimingsMs::default(),
        summary,
        target_sample_audit,
        decision,
        candidates,
    })
}

/// 为每个币种冻结来源 setup 时间线，保持旧挂单“先触及、后替换”的同棒顺序。
fn replacement_times_by_symbol(sources: &[&L1Candidate]) -> BTreeMap<String, BTreeSet<i64>> {
    let mut replacement_times = BTreeMap::new();
    for source in sources {
        replacement_times
            .entry(source.symbol.clone())
            .or_insert_with(BTreeSet::new)
            .insert(source.signal_ts_ms);
    }
    replacement_times
}

/// 解析一个来源 setup 的首次重测终态；首次触及后不允许等待更有利的第二次收回。
fn resolve_first_retest(
    candles: &[ComputedCandle],
    source: &L1Candidate,
    replacement_times: &BTreeSet<i64>,
) -> Result<FirstRetestReentryCandidate> {
    let setup_idx = candles
        .binary_search_by_key(&source.signal_ts_ms, |candle| candle.candle.ts)
        .map_err(|_| anyhow::anyhow!("missing setup candle for {}", source.symbol))?;
    let setup = candles
        .get(setup_idx)
        .context("setup index out of bounds")?;
    let source_extreme_price = match source.direction {
        "short" => setup.candle.high,
        "long" => setup.candle.low,
        other => anyhow::bail!("invalid source direction: {other}"),
    };

    for offset in 1..=RETEST_VALID_CANDLES {
        let Some(retest_idx) = setup_idx.checked_add(offset) else {
            return Ok(candidate_without_retest(
                source,
                source_extreme_price,
                "forward_index_overflow",
            ));
        };
        let Some(retest) = candles.get(retest_idx) else {
            return Ok(candidate_without_retest(
                source,
                source_extreme_price,
                "forward_data_incomplete",
            ));
        };
        if directional_extreme_touched(retest, source_extreme_price, source.direction)? {
            let Some((middle, upper, lower)) = bands_at(candles, retest_idx) else {
                return Ok(candidate_without_retest(
                    source,
                    source_extreme_price,
                    "retest_bollinger_not_ready",
                ));
            };
            let close_inside = match source.direction {
                "short" => retest.candle.close <= upper,
                "long" => retest.candle.close >= lower,
                _ => unreachable!("direction validated before scan"),
            };
            let close_beyond_source_extreme = match source.direction {
                "short" => retest.candle.close > source_extreme_price,
                "long" => retest.candle.close < source_extreme_price,
                _ => unreachable!("direction validated before scan"),
            };
            let earliest_entry_ts_ms = close_inside
                .then(|| retest.candle.ts.checked_add(MS_15M))
                .flatten();
            return Ok(candidate_with_retest(
                source,
                source_extreme_price,
                offset,
                retest,
                middle,
                upper,
                lower,
                close_inside,
                close_beyond_source_extreme,
                earliest_entry_ts_ms,
            ));
        }
        // 来源新 setup 在本根盘中未触及旧极值后，才于收盘替换旧候选。
        if replacement_times.contains(&retest.candle.ts) {
            return Ok(candidate_without_retest(
                source,
                source_extreme_price,
                "pending_replaced_by_new_setup",
            ));
        }
    }
    Ok(candidate_without_retest(
        source,
        source_extreme_price,
        "first_retest_not_touched_within_12_candles",
    ))
}

/// 构造尚未发生首次重测的终态，避免用零值冒充真实确认行情。
fn candidate_without_retest(
    source: &L1Candidate,
    source_extreme_price: f64,
    status: &'static str,
) -> FirstRetestReentryCandidate {
    FirstRetestReentryCandidate {
        symbol: source.symbol.clone(),
        setup_ts_ms: source.signal_ts_ms,
        setup_month_utc: source.signal_month_utc.clone(),
        direction: source.direction,
        source_trigger: source.source_trigger.clone(),
        source_extreme_price,
        filtered_volume_ratio: source.filtered_volume_ratio,
        prior_96_net_move_pct: source.prior_96_net_move_pct,
        directional_wick_range_ratio: source.directional_wick_range_ratio,
        first_retest_ts_ms: None,
        first_retest_offset_bars: None,
        first_retest_close: None,
        retest_bollinger_middle: None,
        retest_bollinger_upper: None,
        retest_bollinger_lower: None,
        close_inside_directional_band: None,
        close_beyond_source_extreme: None,
        confirmation_signal_ts_ms: None,
        earliest_entry_ts_ms: None,
        status,
    }
}

/// 构造首次重测终态；只有收回带内时才暴露确认与最早入场时间。
#[allow(clippy::too_many_arguments)]
fn candidate_with_retest(
    source: &L1Candidate,
    source_extreme_price: f64,
    offset: usize,
    retest: &ComputedCandle,
    middle: f64,
    upper: f64,
    lower: f64,
    close_inside: bool,
    close_beyond_source_extreme: bool,
    earliest_entry_ts_ms: Option<i64>,
) -> FirstRetestReentryCandidate {
    let confirmation_signal_ts_ms = close_inside.then_some(retest.candle.ts);
    FirstRetestReentryCandidate {
        symbol: source.symbol.clone(),
        setup_ts_ms: source.signal_ts_ms,
        setup_month_utc: source.signal_month_utc.clone(),
        direction: source.direction,
        source_trigger: source.source_trigger.clone(),
        source_extreme_price,
        filtered_volume_ratio: source.filtered_volume_ratio,
        prior_96_net_move_pct: source.prior_96_net_move_pct,
        directional_wick_range_ratio: source.directional_wick_range_ratio,
        first_retest_ts_ms: Some(retest.candle.ts),
        first_retest_offset_bars: Some(offset),
        first_retest_close: Some(retest.candle.close),
        retest_bollinger_middle: Some(middle),
        retest_bollinger_upper: Some(upper),
        retest_bollinger_lower: Some(lower),
        close_inside_directional_band: Some(close_inside),
        close_beyond_source_extreme: Some(close_beyond_source_extreme),
        confirmation_signal_ts_ms,
        earliest_entry_ts_ms,
        status: if close_inside {
            "confirmed_close_inside_directional_band"
        } else {
            "rejected_close_outside_directional_band"
        },
    }
}

/// 按来源方向判断首次重测是否触及冻结极值。
fn directional_extreme_touched(
    candle: &ComputedCandle,
    source_extreme_price: f64,
    direction: &str,
) -> Result<bool> {
    match direction {
        "short" => Ok(candle.candle.high >= source_extreme_price),
        "long" => Ok(candle.candle.low <= source_extreme_price),
        other => anyhow::bail!("invalid source direction: {other}"),
    }
}

/// 使用包含重测收盘的最近 20 个 close 重算 2.5 倍布林带。
fn bands_at(candles: &[ComputedCandle], idx: usize) -> Option<(f64, f64, f64)> {
    let start = idx
        .checked_add(1)?
        .checked_sub(FAST_MOMENTUM_BOLLINGER_PERIOD)?;
    let closes = candles
        .get(start..=idx)?
        .iter()
        .map(|candle| candle.candle.close)
        .collect::<Vec<_>>();
    let bands = bollinger_bands_from_closes(&closes, BOLLINGER_STDDEV_MULTIPLIER)?;
    Some((bands.middle, bands.upper, bands.lower))
}

/// 汇总覆盖和终态；所有字段最晚在确认 K 收盘时可见。
fn summarize_candidates(
    candidates: &[FirstRetestReentryCandidate],
    targets: &[FirstRetestTargetAudit],
) -> FirstRetestReentrySummary {
    let first_retest_setups = candidates
        .iter()
        .filter(|candidate| candidate.first_retest_ts_ms.is_some())
        .count();
    let confirmed = candidates
        .iter()
        .filter(|candidate| candidate.confirmation_signal_ts_ms.is_some())
        .collect::<Vec<_>>();
    let rejected = candidates
        .iter()
        .filter(|candidate| candidate.status == "rejected_close_outside_directional_band")
        .collect::<Vec<_>>();
    let mut confirmed_by_direction = BTreeMap::new();
    let mut rejected_by_direction = BTreeMap::new();
    let mut symbols = BTreeSet::new();
    let mut months = BTreeSet::new();
    for candidate in &confirmed {
        *confirmed_by_direction
            .entry(candidate.direction)
            .or_default() += 1;
        symbols.insert(candidate.symbol.as_str());
        if let Some(ts) = candidate.confirmation_signal_ts_ms {
            if let Some(month) = Utc.timestamp_millis_opt(ts).single() {
                months.insert(month.format("%Y-%m").to_string());
            }
        }
    }
    for candidate in &rejected {
        *rejected_by_direction
            .entry(candidate.direction)
            .or_default() += 1;
    }
    let mut blockers = BTreeMap::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.first_retest_ts_ms.is_none())
    {
        *blockers.entry(candidate.status).or_default() += 1;
    }
    FirstRetestReentrySummary {
        base_touch_setups: candidates.len(),
        first_retest_setups,
        confirmed_setups: confirmed.len(),
        rejected_close_acceptance_setups: rejected.len(),
        confirmation_retention_pct_of_first_retests: percentage(
            confirmed.len(),
            first_retest_setups,
        ),
        rejection_impact_pct_of_first_retests: percentage(rejected.len(), first_retest_setups),
        confirmed_by_direction,
        rejected_by_direction,
        confirmed_symbol_count: symbols.len(),
        confirmed_month_count: months.len(),
        confirmed_effective_market_events: effective_market_event_count(&confirmed),
        blockers,
        target_rejected_count: targets
            .iter()
            .filter(|target| target.rejected_by_close_acceptance)
            .count(),
    }
}

/// 固定目标样本只核对定义结果，不能用于运行后修改布林倍数或确认边界。
fn audit_target_samples(candidates: &[FirstRetestReentryCandidate]) -> Vec<FirstRetestTargetAudit> {
    TARGET_SAMPLES
        .iter()
        .map(|(symbol, setup_ts_ms)| {
            let candidate = candidates.iter().find(|candidate| {
                candidate.symbol == *symbol && candidate.setup_ts_ms == *setup_ts_ms
            });
            FirstRetestTargetAudit {
                symbol,
                setup_ts_ms: *setup_ts_ms,
                source_found: candidate.is_some(),
                first_retest_ts_ms: candidate.and_then(|item| item.first_retest_ts_ms),
                status: candidate.map(|item| item.status),
                rejected_by_close_acceptance: candidate
                    .is_some_and(|item| item.status == "rejected_close_outside_directional_band"),
            }
        })
        .collect()
}

/// 应用查看结果前冻结的覆盖、分散性和目标样本门禁。
fn decide_l1(
    summary: &FirstRetestReentrySummary,
    targets: &[FirstRetestTargetAudit],
) -> FirstRetestReentryDecision {
    let long_count = summary
        .confirmed_by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_count = summary
        .confirmed_by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let mut gates = BTreeMap::new();
    gates.insert(
        "first_retests_at_least_30",
        summary.first_retest_setups >= 30,
    );
    gates.insert(
        "confirmed_setups_at_least_30",
        summary.confirmed_setups >= 30,
    );
    gates.insert(
        "rejected_close_acceptance_at_least_20",
        summary.rejected_close_acceptance_setups >= 20,
    );
    gates.insert(
        "confirmation_retention_between_20_and_80_pct",
        (20.0..=80.0).contains(&summary.confirmation_retention_pct_of_first_retests),
    );
    gates.insert(
        "both_directions_confirmed_at_least_5",
        long_count >= 5 && short_count >= 5,
    );
    gates.insert(
        "confirmed_symbols_at_least_8",
        summary.confirmed_symbol_count >= 8,
    );
    gates.insert(
        "confirmed_months_at_least_6",
        summary.confirmed_month_count >= 6,
    );
    gates.insert(
        "confirmed_effective_events_at_least_15",
        summary.confirmed_effective_market_events >= 15,
    );
    gates.insert(
        "all_10_target_sources_found",
        targets.iter().all(|target| target.source_found),
    );
    gates.insert(
        "all_10_target_first_retests_resolved",
        targets
            .iter()
            .all(|target| target.first_retest_ts_ms.is_some()),
    );
    gates.insert(
        "target_rejected_at_least_5_of_10",
        summary.target_rejected_count >= 5,
    );
    let passed = gates.values().all(|value| *value);
    FirstRetestReentryDecision {
        status: if passed {
            "coverage_pass_l2_ready"
        } else {
            "stop"
        },
        gates,
        reason: if passed {
            "首次重测收回带内的无标签覆盖、分散性与目标样本达到预注册门槛；允许另行进入 L2 成本后回放。"
                .to_owned()
        } else {
            "至少一项预注册 L1 门禁失败；不得读取确认后的盈亏或进入 L2。".to_owned()
        },
        outcome_evaluation_performed: false,
    }
}

/// 按确认信号时间和方向归并一小时内的跨币共振。
fn effective_market_event_count(candidates: &[&FirstRetestReentryCandidate]) -> usize {
    let mut ordered = candidates.to_vec();
    ordered.sort_by_key(|candidate| {
        (
            candidate.confirmation_signal_ts_ms.unwrap_or(i64::MAX),
            candidate.direction,
            candidate.symbol.as_str(),
        )
    });
    let mut last_by_direction = BTreeMap::new();
    let mut count = 0;
    for candidate in ordered {
        let Some(ts) = candidate.confirmation_signal_ts_ms else {
            continue;
        };
        let starts_new = last_by_direction
            .get(candidate.direction)
            .is_none_or(|previous| ts - *previous > EVENT_CLUSTER_WINDOW_MS);
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(candidate.direction, ts);
    }
    count
}

/// 对来源外轨候选的纯信号账本生成稳定身份。
fn candidate_ledger_sha256(candidates: &[&L1Candidate]) -> Result<String> {
    let serialized = serde_json::to_vec(candidates)?;
    let mut hasher = Sha256::new();
    hasher.update(serialized);
    Ok(hex::encode(hasher.finalize()))
}

/// 返回稳定百分比；空分母按零处理，直接触发覆盖门禁失败。
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
    use crate::app::market_velocity_event_backtest::BacktestCandle;

    /// 构造连续完整 K 线；测试会单独覆盖 setup 与首次重测价格。
    fn candles() -> Vec<ComputedCandle> {
        (0..40)
            .map(|idx| ComputedCandle {
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
            })
            .collect()
    }

    /// 构造来源外轨 setup；本模块只消费该账本，不重新定义原始方向。
    fn source(direction: &'static str) -> L1Candidate {
        L1Candidate {
            symbol: "TEST-USDT-SWAP".to_owned(),
            signal_ts_ms: 19 * MS_15M,
            signal_month_utc: "1970-01".to_owned(),
            direction,
            source_trigger: "test_directional_wick".to_owned(),
            filtered_volume_ratio: 3.0,
            filtered_volume_retained_candles: 9,
            current_volume_ccy: 300.0,
            weekly_volume_ccy_p90: 100.0,
            prior_96_net_move_pct: if direction == "short" { 10.0 } else { -10.0 },
            body_range_ratio: 0.2,
            directional_wick_range_ratio: 0.7,
            opposite_wick_range_ratio: 0.1,
            bollinger_middle: 100.0,
            bollinger_upper: 102.0,
            bollinger_lower: 98.0,
            touches_directional_outer_band: true,
            outer_excursion_half_band_ratio: 1.0,
            close_middle_distance_half_band_ratio: 0.5,
            source_initial_stop_atr: 1.5,
            source_limit_valid_candles: 12,
        }
    }

    /// 做空首次重测收回当根上轨内，只能在重测收盘后的下一根激活。
    #[test]
    fn short_first_retest_inside_band_confirms_next_bar() {
        let mut series = candles();
        series[19].candle.high = 105.0;
        series[20].candle.high = 105.0;
        series[20].candle.close = 99.0;
        let candidate = resolve_first_retest(&series, &source("short"), &BTreeSet::new())
            .expect("resolve short confirmation");

        assert_eq!(candidate.status, "confirmed_close_inside_directional_band");
        assert_eq!(candidate.first_retest_offset_bars, Some(1));
        assert_eq!(candidate.confirmation_signal_ts_ms, Some(20 * MS_15M));
        assert_eq!(candidate.earliest_entry_ts_ms, Some(21 * MS_15M));
    }

    /// 做空首次重测收于当根上轨外即失效，后续更有利的收回不能补造信号。
    #[test]
    fn first_retest_outside_band_is_terminal() {
        let mut series = candles();
        series[19].candle.high = 105.0;
        series[20].candle.high = 110.0;
        series[20].candle.close = 110.0;
        series[21].candle.high = 105.0;
        series[21].candle.close = 100.0;
        let candidate = resolve_first_retest(&series, &source("short"), &BTreeSet::new())
            .expect("resolve rejected first retest");

        assert_eq!(candidate.status, "rejected_close_outside_directional_band");
        assert_eq!(candidate.first_retest_ts_ms, Some(20 * MS_15M));
        assert_eq!(candidate.confirmation_signal_ts_ms, None);
    }

    /// 多单镜像要求首次下探后收回当根下轨内。
    #[test]
    fn long_first_retest_inside_band_confirms() {
        let mut series = candles();
        series[19].candle.low = 95.0;
        series[20].candle.low = 95.0;
        series[20].candle.close = 101.0;
        let candidate = resolve_first_retest(&series, &source("long"), &BTreeSet::new())
            .expect("resolve long confirmation");

        assert_eq!(candidate.status, "confirmed_close_inside_directional_band");
        assert_eq!(candidate.close_inside_directional_band, Some(true));
    }

    /// 旧候选盘中未触及极值时，新来源 setup 才能在收盘替换它。
    #[test]
    fn newer_setup_replaces_untouched_pending_candidate() {
        let mut series = candles();
        series[19].candle.high = 105.0;
        series[20].candle.high = 104.0;
        series[21].candle.high = 105.0;
        let replacement_ts = series[20].candle.ts;
        let candidate =
            resolve_first_retest(&series, &source("short"), &BTreeSet::from([replacement_ts]))
                .expect("resolve replacement");

        assert_eq!(candidate.status, "pending_replaced_by_new_setup");
        assert_eq!(candidate.first_retest_ts_ms, None);
    }

    /// 候选账本不得混入成交价、退出或盈亏结果字段。
    #[test]
    fn candidate_schema_contains_no_outcome_fields() {
        let candidate = candidate_without_retest(&source("short"), 105.0, "test");
        let json = serde_json::to_value(candidate).expect("candidate json");
        let object = json.as_object().expect("candidate object");

        for forbidden in [
            "entry_price",
            "fill",
            "mfe",
            "mae",
            "exit",
            "pnl",
            "r",
            "win",
            "loss",
        ] {
            assert!(
                !object.contains_key(forbidden),
                "unexpected field: {forbidden}"
            );
        }
    }
}
