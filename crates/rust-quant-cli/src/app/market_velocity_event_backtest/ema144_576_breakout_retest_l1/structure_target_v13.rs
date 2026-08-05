//! V13：保持 V12 入场与稳定成交额面板，只把固定目标改为信号前已确认的结构目标。
//!
//! 本模块只生成信号时可见的 L1 几何账本，不读取成交、退出或收益字段。

use super::super::{
    config_from_env_and_args, load_backtest_data, BacktestDataSet, ComputedCandle, MS_15M,
};
use super::frozen_l1_args;
use super::persistent_dynamic_retest_v2::{build_v6_l1_report, V6_CANDIDATE_KEY, V6_RULE_VERSION};
use super::reexpansion_volume_rank_stable_panel_v12::{V12_CANDIDATE_KEY, V12_RULE_VERSION};
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// V13 独立候选键；V12 和生产版本继续保持冻结。
pub const V13_CANDIDATE_KEY: &str =
    "market_momentum_15m_ema144_576_stable_panel_structure_target_v13";
/// V13 只描述因果结构目标几何，不包含成交后结果。
pub const V13_RULE_VERSION: &str =
    "l1_v12_latest_confirmed_fractal2_structure_target_lookback96_v13";

const EXPECTED_V12_L1_SHA256: &str =
    "201114b4ae1e519793f2988f000e00b5751c05fffc710ad0146e28340eb14dd1";
const EXPECTED_V12_CANDIDATE_LEDGER_SHA256: &str =
    "e597abb7cb3eac6318a32556101bfbe91440d1f78cdaf7901d171ecf881c0cb4";
const EXPECTED_DATASET_FINGERPRINT_SHA256: &str =
    "67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997";
const EXPECTED_V12_CANDIDATES: usize = 48_048;
const EXPECTED_V6_CANDIDATES: usize = 54_837;
const PIVOT_LEFT_BARS: usize = 2;
const PIVOT_RIGHT_BARS: usize = 2;
const STRUCTURE_LOOKBACK_BARS: usize = 96;
const STOP_LOSS_PCT: f64 = 0.04;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;

#[derive(Debug, Deserialize)]
struct AuthorizationIdentity {
    /// V12 候选键。
    candidate_key: String,
    /// V12 L1 规则版本。
    rule_version: String,
    /// V12 继承的 V6 候选键。
    source_v6_candidate_key: String,
    /// V12 继承的 V6 L1 规则。
    source_v6_rule_version: String,
    /// V12 继承的 V11 排名规则。
    source_v11_rule_version: String,
}

#[derive(Debug, Deserialize)]
struct AuthorizationSummary {
    /// V12 过滤前的 V6 候选数。
    source_candidate_count: usize,
    /// V12 最终授权候选数。
    candidate_count: usize,
    /// V12 全候选账本哈希。
    candidate_ledger_sha256: String,
}

#[derive(Debug, Deserialize)]
struct AuthorizationDecision {
    /// V12 L1 覆盖结论。
    status: String,
    /// 必须为 false，防止 L1 授权混入 outcome。
    outcome_evaluation_performed: bool,
}

#[derive(Debug, Deserialize)]
struct AuthorizationTargetAudit {
    /// true 表示对应用户图在 V12 定义内。
    matched: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct AuthorizationCandidate {
    /// OKX 永续合约。
    symbol: String,
    /// `long` 或 `short`。
    direction: String,
    /// V12 回踩信号 K 的 Unix 毫秒时间戳。
    signal_ts_ms: i64,
    /// 信号所属 UTC 月份。
    signal_month_utc: String,
    /// V12 重扩张 K 的 Unix 毫秒时间戳。
    reexpanded_ts_ms: i64,
    /// 稳定面板排名事件 ID。
    rank_event_id: i64,
    /// 排名事件的 Unix 毫秒时间戳。
    rank_event_ts_ms: i64,
    /// 触碰前冻结的 EMA144。
    anchor_ema144: f64,
    /// 触碰前冻结的 ATR14。
    anchor_atr14: f64,
    /// V12 计划限价。
    touch_zone_boundary: f64,
}

#[derive(Debug, Deserialize)]
struct AuthorizationReport {
    /// V12 策略身份。
    identity: AuthorizationIdentity,
    /// V12 使用的 V6 源文件 SHA-256。
    source_v6_l1_report_sha256: String,
    /// V12 使用的 V11 源文件 SHA-256。
    source_v11_l1_report_sha256: String,
    /// 冻结行情指纹。
    source_v6_dataset_fingerprint_sha256: String,
    /// Top60 实际返回成员数。
    returned_symbol_count: usize,
    /// 完成预热的本地成员数。
    eligible_symbol_count: usize,
    /// V12 候选覆盖摘要。
    summary: AuthorizationSummary,
    /// 三张用户图的定义审计。
    target_audits: Vec<AuthorizationTargetAudit>,
    /// V12 无标签门禁结论。
    decision: AuthorizationDecision,
    /// V12 全量候选。
    candidates: Vec<AuthorizationCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructureDirection {
    Long,
    Short,
}

impl StructureDirection {
    fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "long" => Ok(Self::Long),
            "short" => Ok(Self::Short),
            _ => Err("candidate_direction_invalid"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct StructureBar {
    /// K 线开盘对应的 Unix 毫秒时间戳。
    ts_ms: i64,
    /// K 线最高价。
    high: f64,
    /// K 线最低价。
    low: f64,
}

#[derive(Debug, Clone, Copy)]
struct StructureTarget {
    /// 摆动中心 K 的 Unix 毫秒时间戳。
    pivot_ts_ms: i64,
    /// 右侧第二根 K 收盘后，目标首次可用的 Unix 毫秒时间戳。
    confirmed_at_ms: i64,
    /// 摆动中心距信号 K 的棒数。
    pivot_age_bars: usize,
    /// 摆动高或摆动低的绝对目标价。
    price: f64,
    /// 相对计划限价和 4% 初始风险的目标倍数。
    r_at_limit: f64,
}

/// 一条具有信号时可见结构目标的 V13 候选。
#[derive(Debug, Clone, Serialize)]
pub struct V13Candidate {
    /// OKX 永续合约。
    pub symbol: String,
    /// `long` 或 `short`。
    pub direction: &'static str,
    /// 回踩信号 K 的 Unix 毫秒时间戳。
    pub signal_ts_ms: i64,
    /// 信号所属 UTC 月份。
    pub signal_month_utc: String,
    /// V12 重扩张 K 的 Unix 毫秒时间戳。
    pub reexpanded_ts_ms: i64,
    /// V12 稳定面板排名事件 ID。
    pub rank_event_id: i64,
    /// V12 排名事件的 Unix 毫秒时间戳。
    pub rank_event_ts_ms: i64,
    /// 触碰前冻结的 EMA144。
    pub anchor_ema144: f64,
    /// 触碰前冻结的 ATR14。
    pub anchor_atr14: f64,
    /// V12 计划限价。
    pub touch_zone_boundary: f64,
    /// 摆动中心 K 的 Unix 毫秒时间戳。
    pub structure_pivot_ts_ms: i64,
    /// 结构目标在信号前首次确认可用的 Unix 毫秒时间戳。
    pub structure_confirmed_at_ms: i64,
    /// 摆动中心距离信号 K 的 15m 棒数。
    pub structure_pivot_age_bars: usize,
    /// 已确认摆动高或摆动低的绝对目标价。
    pub structure_target_price: f64,
    /// 目标距离除以计划限价 4% 风险，不参与 L1 筛选。
    pub structure_target_r_at_limit: f64,
}

/// V13 从 V12 候选到有效结构目标的逐层计数。
#[derive(Debug, Default, Serialize)]
pub struct V13Stages {
    /// V12 授权候选数。
    pub source_v12_candidates: usize,
    /// 找到对应本地 15m 行情的候选数。
    pub symbol_candles_found: usize,
    /// 找到精确信号 K 的候选数。
    pub signal_candle_found: usize,
    /// 限价或方向字段无效的候选数。
    pub invalid_candidate_geometry: usize,
    /// 前 96 根已完成 K 内没有盈利侧已确认结构的候选数。
    pub no_directional_structure_target: usize,
    /// 最终具有因果结构目标的候选数。
    pub final_candidates: usize,
}

/// 三张用户图在 V13 结构目标定义下的覆盖审计。
#[derive(Debug, Serialize)]
pub struct V13TargetAudit {
    /// 用户样本稳定名称。
    pub name: &'static str,
    /// 用户样本交易对。
    pub symbol: &'static str,
    /// 用户样本方向。
    pub direction: &'static str,
    /// 审计窗口起点，Unix 毫秒。
    pub start_ms: i64,
    /// 审计窗口终点，Unix 毫秒。
    pub end_ms: i64,
    /// 窗口内匹配的 V13 信号时间。
    pub matched_signal_timestamps_ms: Vec<i64>,
    /// true 表示窗口内至少有一个 V13 候选。
    pub matched: bool,
}

/// V13 因果摆动目标的冻结参数。
#[derive(Debug, Serialize)]
pub struct V13StructureContract {
    /// 摆动中心左侧比较棒数。
    pub pivot_left_bars: usize,
    /// 摆动中心右侧确认棒数。
    pub pivot_right_bars: usize,
    /// 只搜索信号前多少根已完成 15m K。
    pub lookback_completed_bars: usize,
    /// true 表示相等高低点不构成摆动点。
    pub comparisons_are_strict: bool,
    /// 多头目标定义。
    pub long_target_policy: &'static str,
    /// 空头目标定义。
    pub short_target_policy: &'static str,
    /// 目标 R 的风险分母。
    pub diagnostic_initial_stop_pct: f64,
    /// 明确禁止 L1 用目标 R 阈值筛选。
    pub target_r_filter: &'static str,
}

/// V13 候选覆盖、结构距离分布与可复现账本身份。
#[derive(Debug, Serialize)]
pub struct V13Summary {
    /// V12 源候选数。
    pub source_candidate_count: usize,
    /// 成功构造结构目标的候选数。
    pub candidate_count: usize,
    /// 有效结构目标占 V12 候选的比例。
    pub valid_target_coverage_pct: f64,
    /// 多空候选分布。
    pub by_direction: BTreeMap<&'static str, usize>,
    /// 各币种候选数。
    pub by_symbol: BTreeMap<String, usize>,
    /// 各 UTC 月份候选数。
    pub by_month_utc: BTreeMap<String, usize>,
    /// 一小时同方向连续链归并后的事件数。
    pub effective_market_events: usize,
    /// 最小结构目标 R。
    pub target_r_at_limit_min: Option<f64>,
    /// 最近秩口径的结构目标 R 中位数。
    pub target_r_at_limit_p50: Option<f64>,
    /// 最近秩口径的结构目标 R 第 90 百分位。
    pub target_r_at_limit_p90: Option<f64>,
    /// 最大结构目标 R。
    pub target_r_at_limit_max: Option<f64>,
    /// 全候选账本 SHA-256。
    pub candidate_ledger_sha256: String,
    /// 逐层阻塞计数。
    pub stages: V13Stages,
}

/// V13 单变量身份与 Research-only 边界。
#[derive(Debug, Serialize)]
pub struct V13Identity {
    /// 当前只属于无 outcome 的 L1。
    pub level: &'static str,
    /// V13 独立候选键。
    pub candidate_key: &'static str,
    /// V13 L1 规则版本。
    pub rule_version: &'static str,
    /// 冻结的 V12 源候选键。
    pub source_v12_candidate_key: &'static str,
    /// 冻结的 V12 L1 规则。
    pub source_v12_rule_version: &'static str,
    /// 本轮唯一变量。
    pub only_variable: &'static str,
    /// L1 禁止读取的后验字段。
    pub label_boundary: &'static str,
    /// 不影响运行态的边界。
    pub runtime_boundary: &'static str,
}

/// V13 预注册 L1 门禁结论。
#[derive(Debug, Serialize)]
pub struct V13Decision {
    /// 覆盖通过、定义不匹配或覆盖停止。
    pub status: &'static str,
    /// 每项冻结门禁结果。
    pub gates: BTreeMap<&'static str, bool>,
    /// 停止或进入 L2 预注册的原因。
    pub reason: String,
    /// L1 必须为 false。
    pub outcome_evaluation_performed: bool,
}

/// V13 L1 机器报告；候选字段均在信号开始前可确定。
#[derive(Debug, Serialize)]
pub struct V13Report {
    /// 报告 schema 身份。
    pub schema_version: &'static str,
    /// 报告生成时间，不参与策略身份。
    pub generated_at_utc: String,
    /// V13 策略身份。
    pub identity: V13Identity,
    /// V12 授权文件 SHA-256。
    pub source_v12_l1_report_sha256: String,
    /// 重载行情指纹。
    pub dataset_fingerprint_sha256: String,
    /// Top60 实际返回成员数。
    pub returned_symbol_count: usize,
    /// 完成预热的本地成员数。
    pub eligible_symbol_count: usize,
    /// 结构目标冻结合同。
    pub structure_contract: V13StructureContract,
    /// 候选覆盖与目标距离分布。
    pub summary: V13Summary,
    /// 三张用户图覆盖。
    pub target_audits: Vec<V13TargetAudit>,
    /// L1 门禁结论。
    pub decision: V13Decision,
    /// 全量信号时结构目标账本。
    pub candidates: Vec<V13Candidate>,
}

/// 校验冻结 V12 授权，重载相同行情并写出不含 outcome 的 V13 结构目标账本。
pub async fn run_v13_l1(v12_source: &Path, output: &Path) -> Result<V13Report> {
    let (authorization, authorization_sha256) = load_v12_authorization(v12_source)?;
    let config = config_from_env_and_args(frozen_l1_args()?)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for EMA144/576 V13 L1")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_report(&data, authorization, authorization_sha256)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("创建 EMA144/576 V13 L1 报告目录失败：{}", parent.display())
        })?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 EMA144/576 V13 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 完整 SHA、V12 身份、候选哈希和 3/3 目标共同授权结构目标扫描。
fn load_v12_authorization(source: &Path) -> Result<(AuthorizationReport, String)> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取 EMA144/576 V12 L1 授权失败：{}", source.display()))?;
    let sha256 = sha256_hex(&bytes);
    if sha256 != EXPECTED_V12_L1_SHA256 {
        bail!("V12 L1 authorization SHA mismatch");
    }
    let report: AuthorizationReport =
        serde_json::from_slice(&bytes).context("解析 EMA144/576 V12 L1 授权失败")?;
    if report.identity.candidate_key != V12_CANDIDATE_KEY
        || report.identity.rule_version != V12_RULE_VERSION
        || report.identity.source_v6_candidate_key != V6_CANDIDATE_KEY
        || report.identity.source_v6_rule_version != V6_RULE_VERSION
        || report.identity.source_v11_rule_version
            != "l1_v6_reexpansion_same_candle_volume_rank_nonworse_v11"
        || report.source_v6_l1_report_sha256
            != "a69b9cafb83ea55601bc35eaf13a821c0a5fb5080f4d256632457ab3e6f974da"
        || report.source_v11_l1_report_sha256
            != "02bbb99a7337c5213c25e5d503268d13c49bd4fe84d4aeeb61d69da6677104dd"
        || report.source_v6_dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256
        || report.returned_symbol_count != 60
        || report.eligible_symbol_count != 44
        || report.summary.source_candidate_count != EXPECTED_V6_CANDIDATES
        || report.summary.candidate_count != EXPECTED_V12_CANDIDATES
        || report.summary.candidate_ledger_sha256 != EXPECTED_V12_CANDIDATE_LEDGER_SHA256
        || report.target_audits.len() != 3
        || !report.target_audits.iter().all(|target| target.matched)
        || report.decision.status != "coverage_pass_ready_for_l2_prereg"
        || report.decision.outcome_evaluation_performed
        || report.candidates.len() != EXPECTED_V12_CANDIDATES
    {
        bail!("V12 L1 authorization identity or coverage gate mismatch");
    }
    let unique_ids = report
        .candidates
        .iter()
        .map(candidate_id)
        .collect::<BTreeSet<_>>();
    if unique_ids.len() != EXPECTED_V12_CANDIDATES {
        bail!("V12 L1 authorization contains duplicate candidate identities");
    }
    Ok((report, sha256))
}

fn build_report(
    data: &BacktestDataSet,
    authorization: AuthorizationReport,
    authorization_sha256: String,
) -> Result<V13Report> {
    let rebuilt_v6 = build_v6_l1_report(data)?;
    if rebuilt_v6.coverage.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256
        || rebuilt_v6.summary.candidate_count != EXPECTED_V6_CANDIDATES
    {
        bail!("V13 reloaded V6 dataset or candidate identity mismatch");
    }

    let mut stages = V13Stages {
        source_v12_candidates: authorization.candidates.len(),
        ..V13Stages::default()
    };
    // 同一币种的 15m 结构序列只构建一次；V12 同币种候选很多，逐候选复制会无意义放大扫描成本。
    let structure_bars = data
        .candles_15m_computed
        .iter()
        .map(|(symbol, candles)| {
            (
                symbol.as_str(),
                candles.iter().map(StructureBar::from).collect::<Vec<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut candidates = Vec::with_capacity(authorization.candidates.len());
    for source in authorization.candidates {
        let direction = match StructureDirection::parse(&source.direction) {
            Ok(value) => value,
            Err(_) => {
                stages.invalid_candidate_geometry += 1;
                continue;
            }
        };
        if !source.touch_zone_boundary.is_finite() || source.touch_zone_boundary <= 0.0 {
            stages.invalid_candidate_geometry += 1;
            continue;
        }
        let Some(bars) = structure_bars.get(source.symbol.as_str()) else {
            continue;
        };
        stages.symbol_candles_found += 1;
        if bars
            .binary_search_by_key(&source.signal_ts_ms, |bar| bar.ts_ms)
            .is_err()
        {
            continue;
        }
        stages.signal_candle_found += 1;
        let Some(target) = find_structure_target(
            bars,
            source.signal_ts_ms,
            source.touch_zone_boundary,
            direction,
        ) else {
            stages.no_directional_structure_target += 1;
            continue;
        };
        candidates.push(V13Candidate {
            symbol: source.symbol,
            direction: direction.label(),
            signal_ts_ms: source.signal_ts_ms,
            signal_month_utc: source.signal_month_utc,
            reexpanded_ts_ms: source.reexpanded_ts_ms,
            rank_event_id: source.rank_event_id,
            rank_event_ts_ms: source.rank_event_ts_ms,
            anchor_ema144: source.anchor_ema144,
            anchor_atr14: source.anchor_atr14,
            touch_zone_boundary: source.touch_zone_boundary,
            structure_pivot_ts_ms: target.pivot_ts_ms,
            structure_confirmed_at_ms: target.confirmed_at_ms,
            structure_pivot_age_bars: target.pivot_age_bars,
            structure_target_price: target.price,
            structure_target_r_at_limit: target.r_at_limit,
        });
    }
    candidates.sort_by(|left, right| {
        (left.signal_ts_ms, left.symbol.as_str(), left.direction).cmp(&(
            right.signal_ts_ms,
            right.symbol.as_str(),
            right.direction,
        ))
    });
    stages.final_candidates = candidates.len();

    let by_direction = counts_by_static(candidates.iter().map(|candidate| candidate.direction));
    let by_symbol = counts_by(candidates.iter().map(|candidate| candidate.symbol.clone()));
    let by_month_utc = counts_by(
        candidates
            .iter()
            .map(|candidate| candidate.signal_month_utc.clone()),
    );
    let target_rs = candidates
        .iter()
        .map(|candidate| candidate.structure_target_r_at_limit)
        .collect::<Vec<_>>();
    let summary = V13Summary {
        source_candidate_count: EXPECTED_V12_CANDIDATES,
        candidate_count: candidates.len(),
        valid_target_coverage_pct: candidates.len() as f64 / EXPECTED_V12_CANDIDATES as f64 * 100.0,
        by_direction,
        by_symbol,
        by_month_utc,
        effective_market_events: effective_market_event_count(&candidates),
        target_r_at_limit_min: target_rs.iter().copied().min_by(f64::total_cmp),
        target_r_at_limit_p50: nearest_rank(&target_rs, 0.50),
        target_r_at_limit_p90: nearest_rank(&target_rs, 0.90),
        target_r_at_limit_max: target_rs.iter().copied().max_by(f64::total_cmp),
        candidate_ledger_sha256: hash_candidates(&candidates),
        stages,
    };
    let target_audits = audit_targets(&candidates);
    let decision = decide(&summary, &target_audits, &candidates);
    Ok(V13Report {
        schema_version: "market_momentum_15m_ema144_576_stable_panel_structure_target_l1_v13",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V13Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V13_CANDIDATE_KEY,
            rule_version: V13_RULE_VERSION,
            source_v12_candidate_key: V12_CANDIDATE_KEY,
            source_v12_rule_version: V12_RULE_VERSION,
            only_variable: "replace the fixed 0.52R target with the latest signal-time confirmed 2-left 2-right directional swing target within 96 completed 15m candles",
            label_boundary: "uses only V12 signal-time fields and candles completed before the signal candle; no fill, post-signal candle, exit, MFE, MAE, final R, win, loss, or PnL",
            runtime_boundary: "research-only V13 L1; V12, paper, readonly shadow, live, compose, Pine, and production preset remain unchanged",
        },
        source_v12_l1_report_sha256: authorization_sha256,
        dataset_fingerprint_sha256: rebuilt_v6.coverage.dataset_fingerprint_sha256,
        returned_symbol_count: authorization.returned_symbol_count,
        eligible_symbol_count: authorization.eligible_symbol_count,
        structure_contract: V13StructureContract {
            pivot_left_bars: PIVOT_LEFT_BARS,
            pivot_right_bars: PIVOT_RIGHT_BARS,
            lookback_completed_bars: STRUCTURE_LOOKBACK_BARS,
            comparisons_are_strict: true,
            long_target_policy: "latest confirmed swing high strictly above the planned limit",
            short_target_policy: "latest confirmed swing low strictly below the planned limit",
            diagnostic_initial_stop_pct: STOP_LOSS_PCT,
            target_r_filter: "none; signal-time target R distribution is diagnostic only",
        },
        summary,
        target_audits,
        decision,
        candidates,
    })
}

impl From<&ComputedCandle> for StructureBar {
    fn from(value: &ComputedCandle) -> Self {
        Self {
            ts_ms: value.candle.ts,
            high: value.candle.high,
            low: value.candle.low,
        }
    }
}

/// 最近结构必须在信号开始时已经由右侧两根 K 确认，避免把信号 K 当成历史证据。
fn find_structure_target(
    bars: &[StructureBar],
    signal_ts_ms: i64,
    limit_price: f64,
    direction: StructureDirection,
) -> Option<StructureTarget> {
    if !limit_price.is_finite() || limit_price <= 0.0 {
        return None;
    }
    let signal_idx = bars
        .binary_search_by_key(&signal_ts_ms, |bar| bar.ts_ms)
        .ok()?;
    // `p+2` 必须在信号前一根或更早；其收盘边界才能不晚于信号 K 开始。
    let pivot_end = signal_idx.checked_sub(PIVOT_RIGHT_BARS + 1)?;
    let pivot_start = signal_idx
        .saturating_sub(STRUCTURE_LOOKBACK_BARS)
        .max(PIVOT_LEFT_BARS);
    if pivot_start > pivot_end {
        return None;
    }
    for pivot_idx in (pivot_start..=pivot_end).rev() {
        let price = match direction {
            StructureDirection::Long if is_strict_pivot_high(bars, pivot_idx) => {
                bars[pivot_idx].high
            }
            StructureDirection::Short if is_strict_pivot_low(bars, pivot_idx) => {
                bars[pivot_idx].low
            }
            _ => continue,
        };
        let directional_distance = match direction {
            StructureDirection::Long => price - limit_price,
            StructureDirection::Short => limit_price - price,
        };
        if !directional_distance.is_finite() || directional_distance <= 0.0 {
            continue;
        }
        let confirmed_at_ms = bars[pivot_idx + PIVOT_RIGHT_BARS]
            .ts_ms
            .saturating_add(MS_15M);
        if confirmed_at_ms > signal_ts_ms {
            continue;
        }
        return Some(StructureTarget {
            pivot_ts_ms: bars[pivot_idx].ts_ms,
            confirmed_at_ms,
            pivot_age_bars: signal_idx - pivot_idx,
            price,
            r_at_limit: directional_distance / (limit_price * STOP_LOSS_PCT),
        });
    }
    None
}

fn is_strict_pivot_high(bars: &[StructureBar], pivot_idx: usize) -> bool {
    valid_pivot_window(bars, pivot_idx)
        && (1..=PIVOT_LEFT_BARS).all(|offset| bars[pivot_idx].high > bars[pivot_idx - offset].high)
        && (1..=PIVOT_RIGHT_BARS).all(|offset| bars[pivot_idx].high > bars[pivot_idx + offset].high)
}

fn is_strict_pivot_low(bars: &[StructureBar], pivot_idx: usize) -> bool {
    valid_pivot_window(bars, pivot_idx)
        && (1..=PIVOT_LEFT_BARS).all(|offset| bars[pivot_idx].low < bars[pivot_idx - offset].low)
        && (1..=PIVOT_RIGHT_BARS).all(|offset| bars[pivot_idx].low < bars[pivot_idx + offset].low)
}

fn valid_pivot_window(bars: &[StructureBar], pivot_idx: usize) -> bool {
    pivot_idx >= PIVOT_LEFT_BARS
        && pivot_idx + PIVOT_RIGHT_BARS < bars.len()
        && bars[pivot_idx - PIVOT_LEFT_BARS..=pivot_idx + PIVOT_RIGHT_BARS]
            .iter()
            .all(|bar| {
                bar.high.is_finite()
                    && bar.low.is_finite()
                    && bar.high > 0.0
                    && bar.low > 0.0
                    && bar.high >= bar.low
            })
}

fn audit_targets(candidates: &[V13Candidate]) -> Vec<V13TargetAudit> {
    [
        (
            "nmr_2026_07_01_user_chart",
            "NMR-USDT-SWAP",
            1_782_835_200_000,
            1_782_878_400_000,
        ),
        (
            "btc_2026_07_02_user_chart",
            "BTC-USDT-SWAP",
            1_782_943_200_000,
            1_782_964_800_000,
        ),
        (
            "btc_2026_07_12_user_chart",
            "BTC-USDT-SWAP",
            1_783_828_800_000,
            1_783_850_400_000,
        ),
    ]
    .into_iter()
    .map(|(name, symbol, start_ms, end_ms)| {
        let matched_signal_timestamps_ms = candidates
            .iter()
            .filter(|candidate| {
                candidate.symbol == symbol
                    && candidate.direction == "long"
                    && (start_ms..=end_ms).contains(&candidate.signal_ts_ms)
            })
            .map(|candidate| candidate.signal_ts_ms)
            .collect::<Vec<_>>();
        V13TargetAudit {
            name,
            symbol,
            direction: "long",
            start_ms,
            end_ms,
            matched: !matched_signal_timestamps_ms.is_empty(),
            matched_signal_timestamps_ms,
        }
    })
    .collect()
}

fn decide(
    summary: &V13Summary,
    targets: &[V13TargetAudit],
    candidates: &[V13Candidate],
) -> V13Decision {
    let long_count = summary
        .by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_count = summary
        .by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let targets_match = targets.len() == 3 && targets.iter().all(|target| target.matched);
    let causal_targets = candidates.iter().all(|candidate| {
        candidate.structure_confirmed_at_ms <= candidate.signal_ts_ms
            && candidate.structure_pivot_age_bars >= PIVOT_RIGHT_BARS + 1
            && candidate.structure_pivot_age_bars <= STRUCTURE_LOOKBACK_BARS
            && candidate.structure_target_r_at_limit.is_finite()
            && candidate.structure_target_r_at_limit > 0.0
    });
    let coverage_in_range = (50.0..=100.0).contains(&summary.valid_target_coverage_pct);
    let mut gates = BTreeMap::new();
    gates.insert("all_three_user_targets_match", targets_match);
    gates.insert(
        "valid_structure_target_coverage_50_to_100_pct",
        coverage_in_range,
    );
    gates.insert(
        "all_targets_confirmed_before_signal_and_profitable_side",
        causal_targets,
    );
    gates.insert("candidates_at_least_30", summary.candidate_count >= 30);
    gates.insert(
        "both_directions_at_least_10",
        long_count >= 10 && short_count >= 10,
    );
    gates.insert("symbols_at_least_8", summary.by_symbol.len() >= 8);
    gates.insert("utc_months_at_least_6", summary.by_month_utc.len() >= 6);
    gates.insert(
        "effective_events_at_least_15",
        summary.effective_market_events >= 15,
    );
    let passed = gates.values().all(|gate| *gate);
    V13Decision {
        status: if passed {
            "coverage_pass_ready_for_l2_prereg"
        } else if !targets_match {
            "rejected_definition_mismatch"
        } else {
            "stop_coverage_gate_failed"
        },
        gates,
        reason: if passed {
            "V13 在不读取 outcome 的前提下保留三张目标，并形成广泛、双向且因果可确认的结构目标账本；下一步只能先冻结 L2 成本回放清单。".to_owned()
        } else {
            "V13 至少一项预注册目标、因果性或覆盖门禁失败；按规则停止，不读取成交后结果。"
                .to_owned()
        },
        outcome_evaluation_performed: false,
    }
}

fn effective_market_event_count(candidates: &[V13Candidate]) -> usize {
    let mut ordered = candidates
        .iter()
        .map(|candidate| (candidate.signal_ts_ms, candidate.direction))
        .collect::<Vec<_>>();
    ordered.sort_unstable();
    let mut last_by_direction = BTreeMap::new();
    let mut count = 0;
    for (ts, direction) in ordered {
        if last_by_direction
            .get(direction)
            .is_none_or(|last| ts.saturating_sub(*last) > EVENT_CLUSTER_WINDOW_MS)
        {
            count += 1;
        }
        last_by_direction.insert(direction, ts);
    }
    count
}

fn nearest_rank(values: &[f64], quantile: f64) -> Option<f64> {
    if values.is_empty() || !(0.0..=1.0).contains(&quantile) {
        return None;
    }
    let mut ordered = values.to_vec();
    ordered.sort_by(f64::total_cmp);
    let rank = (quantile * ordered.len() as f64).ceil().max(1.0) as usize;
    ordered.get(rank - 1).copied()
}

fn counts_by(values: impl Iterator<Item = String>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    counts
}

fn counts_by_static(values: impl Iterator<Item = &'static str>) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    counts
}

fn candidate_id(candidate: &AuthorizationCandidate) -> String {
    format!(
        "{}:{}:{}",
        candidate.symbol, candidate.signal_ts_ms, candidate.direction
    )
}

fn hash_candidates(candidates: &[V13Candidate]) -> String {
    let mut hasher = Sha256::new();
    for candidate in candidates {
        hash_bytes(&mut hasher, candidate.symbol.as_bytes());
        hash_bytes(&mut hasher, candidate.direction.as_bytes());
        hasher.update(candidate.signal_ts_ms.to_le_bytes());
        hasher.update(candidate.reexpanded_ts_ms.to_le_bytes());
        hasher.update(candidate.rank_event_id.to_le_bytes());
        hasher.update(candidate.rank_event_ts_ms.to_le_bytes());
        hasher.update(candidate.touch_zone_boundary.to_bits().to_le_bytes());
        hasher.update(candidate.structure_pivot_ts_ms.to_le_bytes());
        hasher.update(candidate.structure_confirmed_at_ms.to_le_bytes());
        hasher.update(candidate.structure_pivot_age_bars.to_le_bytes());
        hasher.update(candidate.structure_target_price.to_bits().to_le_bytes());
        hasher.update(
            candidate
                .structure_target_r_at_limit
                .to_bits()
                .to_le_bytes(),
        );
    }
    hex::encode(hasher.finalize())
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_pivot_is_usable_only_after_right_confirmation_closes() {
        let bars = bars(&[10.0, 11.0, 12.0, 11.0, 10.0, 13.0, 20.0, 12.0, 11.0, 10.0]);
        let target = find_structure_target(&bars, 9 * MS_15M, 15.0, StructureDirection::Long)
            .expect("pivot at index 6 is confirmed exactly when signal starts");
        assert_eq!(target.pivot_ts_ms, 6 * MS_15M);
        assert_eq!(target.confirmed_at_ms, 9 * MS_15M);
        assert_eq!(target.pivot_age_bars, 3);
        assert_eq!(target.price, 21.0);

        let earlier_signal =
            find_structure_target(&bars, 8 * MS_15M, 15.0, StructureDirection::Long);
        assert!(earlier_signal.is_none());
    }

    #[test]
    fn short_target_mirrors_latest_confirmed_profitable_side_pivot() {
        let bars = bars(&[20.0, 19.0, 18.0, 19.0, 20.0, 17.0, 10.0, 18.0, 19.0, 20.0]);
        let target = find_structure_target(&bars, 9 * MS_15M, 15.0, StructureDirection::Short)
            .expect("confirmed short structure target");
        assert_eq!(target.pivot_ts_ms, 6 * MS_15M);
        assert_eq!(target.price, 9.0);
        assert!((target.r_at_limit - 10.0).abs() < 1e-12);
    }

    fn bars(centers: &[f64]) -> Vec<StructureBar> {
        centers
            .iter()
            .enumerate()
            .map(|(idx, center)| StructureBar {
                ts_ms: idx as i64 * MS_15M,
                high: center + 1.0,
                low: center - 1.0,
            })
            .collect()
    }
}
