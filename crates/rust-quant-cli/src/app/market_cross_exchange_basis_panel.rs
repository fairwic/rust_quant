mod binance_aggtrades;
mod binance_bvol;
mod binance_funding;
mod binance_klines;
mod binance_liquidation;
mod binance_positioning;
mod bvol_relative;
mod funding_carry;
mod large_trade_absorption;
mod liquidation_relative;
mod positioning_spread;
mod premium_recovery;
mod report;
mod taker_delta_factor_panel;
mod taker_delta_reversal;

pub use bvol_relative::{
    parse_bvol_relative_panel_args, run_bvol_relative_momentum_panel, BvolRelativePanelArgs,
    BvolRelativePanelReport, BvolRelativeStages, BvolRelativeSummary,
};
pub use funding_carry::{
    run_cross_sectional_funding_carry_panel, run_cross_sectional_funding_carry_panel_v2,
    CrossSectionalFundingCarryReport, FundingCarryStages, FundingCarrySummary,
};
pub use large_trade_absorption::{
    parse_large_trade_panel_args, run_large_trade_absorption_panel, LargeTradePanelArgs,
    LargeTradePanelReport, LargeTradeStages, LargeTradeSummary,
};
pub use liquidation_relative::{
    parse_liquidation_relative_panel_args, run_liquidation_relative_panel,
    LiquidationRelativePanelArgs, LiquidationRelativePanelReport, LiquidationRelativeStages,
    LiquidationRelativeSummary,
};
pub use positioning_spread::{
    run_top_trader_positioning_spread_panel, run_top_trader_vs_crowd_spread_panel,
    TopTraderPositioningReport, TopTraderPositioningStages, TopTraderPositioningSummary,
};
pub use premium_recovery::{
    run_premium_discount_recovery_panel, PremiumRecoveryPanelReport, PremiumRecoveryStages,
    PremiumRecoverySummary,
};
pub use taker_delta_factor_panel::{
    run_taker_delta_factor_panel, TakerDeltaFactorPanelReport, TakerDeltaFactorStages,
    TakerDeltaFactorSummary, TakerDeltaPairedSummary,
};
pub use taker_delta_reversal::{
    run_taker_delta_reversal_research, TakerDeltaDirection, TakerDeltaMetrics,
    TakerDeltaResearchReport, TakerDeltaStages, TakerDeltaTrade,
};

use self::binance_klines::{load_binance_klines, BinanceCandle, BinanceKlineAudit};
use self::report::{
    build_dislocation_report, build_report, print_dislocation_report, print_report,
};
use crate::app::okx_historical_universe::HistoricalUniverseManifest;
use anyhow::{anyhow, bail, Context, Result};
use rust_quant_strategies::CandleItem;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MS_15M: i64 = 15 * 60 * 1_000;
const MS_4H: i64 = 4 * 60 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const BASIS_BARS: usize = 7 * 24 * 4;
const FORWARD_1H_BARS: usize = 4;
const FORWARD_4H_BARS: usize = 16;
const FORWARD_24H_BARS: usize = 96;
const MIN_FACTOR_COVERAGE: f64 = 0.80;
const EXTREME_Z: f64 = 2.0;
const EXECUTABLE_DEVIATION: f64 = 0.0050;
const CONTROL_DEVIATION: f64 = 0.0032;
const RULE_VERSION: &str = "okx_binance_7d_basis_zscore_top1_4h_v1";
const DISLOCATION_RULE_VERSION: &str = "okx_binance_7d_basis_first_cross_50bps_15m_v1";
const DEFAULT_BINANCE_REST_BASE: &str = "https://fapi.binance.com";
const DEFAULT_BINANCE_DATA_BASE: &str = "https://data.binance.vision";

/// 冻结因子面板只允许数据位置与下载并发，不暴露研究阈值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossExchangeBasisPanelArgs {
    /// current-live crypto-only 的十二个月 OKX 历史币池 manifest。
    pub manifest: PathBuf,
    /// Binance 官方月包的本地可复用缓存目录。
    pub cache_dir: PathBuf,
    /// 官方月包最大并发下载数。
    pub download_concurrency: usize,
    /// Binance 当前合约元数据 API 根地址。
    pub binance_rest_base: String,
    /// Binance 官方公开历史数据根地址。
    pub binance_data_base: String,
}

/// 记录跨交易所因子从同步覆盖到固定 outcome 的完整漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrossExchangeBasisStages {
    /// 十二个月内的 UTC 4 小时决策时点数。
    pub decision_points: usize,
    /// 完整同步因子低于当月成员 80% 的时点数。
    pub coverage_blocked: usize,
    /// 未阻塞时点中成功计算的 7 日基差观察数。
    pub factor_observations: usize,
    /// 完成确定性最大绝对 z-score 排序后的时点数。
    pub selected_candidates: usize,
    /// 选中项达到 `abs(z) >= 2` 的时点数。
    pub extreme_candidates: usize,
    /// 选中项未达到极端阈值的对照时点数。
    pub control_candidates: usize,
    /// 缺少下一共同开盘或 24h 同步 outcome 的选中项数。
    pub incomplete_outcomes: usize,
}

/// 一个预注册分组的毛配对收益与命中率。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CrossExchangeBasisSummary {
    /// 当前分组的有效观察数。
    pub observations: usize,
    /// 下一共同开盘至 1h 的平均方向配对收益。
    pub mean_forward_1h: Option<f64>,
    /// 下一共同开盘至 4h 的平均方向配对收益。
    pub mean_forward_4h: Option<f64>,
    /// 下一共同开盘至 24h 的平均方向配对收益。
    pub mean_forward_24h: Option<f64>,
    /// 1h 配对收益为正的比例，单位百分比。
    pub positive_rate_1h_pct: Option<f64>,
    /// 4h 配对收益为正的比例，单位百分比。
    pub positive_rate_4h_pct: Option<f64>,
    /// 24h 配对收益为正的比例，单位百分比。
    pub positive_rate_24h_pct: Option<f64>,
}

/// 因子面板完整报告；通过前不得解释为可执行策略回测。
#[derive(Debug, Clone, PartialEq)]
pub struct CrossExchangeBasisPanelReport {
    /// 冻结因子与候选规则身份。
    pub rule_version: String,
    /// 历史币池版本。
    pub universe_version: String,
    /// OKX 币池唯一成员数。
    pub okx_symbols: usize,
    /// Binance 当前合约映射和官方文件审计。
    pub binance_audit: BinanceKlineAudit,
    /// 因果候选与 outcome 漏斗。
    pub stages: CrossExchangeBasisStages,
    /// 全窗口极端组。
    pub extreme_overall: CrossExchangeBasisSummary,
    /// 全窗口非极端对照组。
    pub control_overall: CrossExchangeBasisSummary,
    /// 前六个月极端组。
    pub extreme_discovery: CrossExchangeBasisSummary,
    /// 前六个月非极端对照组。
    pub control_discovery: CrossExchangeBasisSummary,
    /// 后六个月极端组。
    pub extreme_validation: CrossExchangeBasisSummary,
    /// 后六个月非极端对照组。
    pub control_validation: CrossExchangeBasisSummary,
    /// OKX 相对昂贵、short OKX / long Binance 的极端组。
    pub extreme_positive_z: CrossExchangeBasisSummary,
    /// OKX 相对便宜、long OKX / short Binance 的极端组。
    pub extreme_negative_z: CrossExchangeBasisSummary,
    /// 是否通过全部预注册边际价值门槛。
    pub factor_gate_passed: bool,
}

/// 记录首次越过经济幅度阈值的覆盖、候选与 outcome 漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrossExchangeDislocationStages {
    /// 十二个月内的每个 15m 决策时点数。
    pub decision_points: usize,
    /// 完整同步因子低于当月成员 80% 的时点数。
    pub coverage_blocked: usize,
    /// 未阻塞时点中可计算当前和上一偏离的因子观察数。
    pub factor_observations: usize,
    /// 全横截面首次越过 50bps 的可执行成员观察数。
    pub executable_crossings: usize,
    /// 全横截面首次越过 32bps、但低于 50bps 的对照成员观察数。
    pub control_crossings: usize,
    /// 每个时点按绝对偏离排序后选出的候选数。
    pub selected_candidates: usize,
    /// 选中候选属于 50bps 可执行越界的数量。
    pub selected_executable: usize,
    /// 选中候选属于近成本对照越界的数量。
    pub selected_control: usize,
    /// 缺少下一共同开盘或 24h 同步 outcome 的选中项数。
    pub incomplete_outcomes: usize,
}

/// 首次越过 50bps 因子面板的完整稳定性和经济幅度报告。
#[derive(Debug, Clone, PartialEq)]
pub struct CrossExchangeDislocationPanelReport {
    /// 冻结首次越界规则身份。
    pub rule_version: String,
    /// 历史币池版本。
    pub universe_version: String,
    /// OKX 币池唯一成员数。
    pub okx_symbols: usize,
    /// Binance 当前合约映射和官方文件审计。
    pub binance_audit: BinanceKlineAudit,
    /// 首次越界因果漏斗。
    pub stages: CrossExchangeDislocationStages,
    /// 50bps 可执行组的 4h 全市场事件聚类数。
    pub effective_events_4h: usize,
    /// 全窗口 50bps 可执行组。
    pub executable_overall: CrossExchangeBasisSummary,
    /// 全窗口 32～50bps 近成本对照组。
    pub control_overall: CrossExchangeBasisSummary,
    /// 前六个月 50bps 可执行组。
    pub executable_discovery: CrossExchangeBasisSummary,
    /// 前六个月近成本对照组。
    pub control_discovery: CrossExchangeBasisSummary,
    /// 后六个月 50bps 可执行组。
    pub executable_validation: CrossExchangeBasisSummary,
    /// 后六个月近成本对照组。
    pub control_validation: CrossExchangeBasisSummary,
    /// OKX 相对昂贵的 50bps 首次越界组。
    pub executable_positive_deviation: CrossExchangeBasisSummary,
    /// OKX 相对便宜的 50bps 首次越界组。
    pub executable_negative_deviation: CrossExchangeBasisSummary,
    /// 是否通过全部预注册经济幅度与频率门槛。
    pub factor_gate_passed: bool,
}

/// 单个历史币池的生效区间和 current-live 成员。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UniverseWindow {
    /// 窗口起点，Unix 毫秒且包含。
    pub from_ms: i64,
    /// 窗口终点，Unix 毫秒且不包含。
    pub to_ms: i64,
    /// 本月可参与横截面的 OKX 合约标识。
    pub members: BTreeSet<String>,
}

/// 已按时间排序且月份连续的 point-in-time 币池。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UniverseSchedule {
    /// manifest 中冻结的币池版本。
    pub version: String,
    /// 连续十二个月窗口。
    pub windows: Vec<UniverseWindow>,
}

/// 信号时点可见的 7 日跨交易所基差状态。
#[derive(Debug, Clone, Copy, PartialEq)]
struct BasisFactor {
    /// 最新对数基差减 7 日均值后的标准分数。
    z_score: f64,
}

/// 同时保存当前和上一 15m 相对 7 日均值的对数基差偏离。
#[derive(Debug, Clone, Copy, PartialEq)]
struct BasisDislocationFactor {
    /// 当前完成 K 线的基差减当前 trailing 672 均值。
    current_deviation: f64,
    /// 上一完成 K 线的基差减上一 trailing 672 均值。
    previous_deviation: f64,
}

/// 固定候选及其后验双腿 outcome。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct BasisObservation {
    /// OKX 合约标识；Binance 映射保留在数据审计中。
    pub symbol: String,
    /// 因子决策时间；两交易所上一根 15m 均已完成。
    pub decision_ts: i64,
    /// 冻结 7 日基差 z-score。
    pub z_score: f64,
    /// 下一共同开盘至 1h 的方向配对收益。
    pub forward_1h: f64,
    /// 下一共同开盘至 4h 的方向配对收益。
    pub forward_4h: f64,
    /// 下一共同开盘至 24h 的方向配对收益。
    pub forward_24h: f64,
}

/// 绑定首次越界类型、方向和固定期限双腿 outcome。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DislocationObservation {
    /// OKX 合约标识。
    pub symbol: String,
    /// 因子决策时间。
    pub decision_ts: i64,
    /// 当前对数基差相对 7 日均值的偏离。
    pub deviation: f64,
    /// `true` 表示首次越过 50bps；`false` 表示 32～50bps 对照。
    pub executable: bool,
    /// 下一共同开盘至 1h 的方向配对收益。
    pub forward_1h: f64,
    /// 下一共同开盘至 4h 的方向配对收益。
    pub forward_4h: f64,
    /// 下一共同开盘至 24h 的方向配对收益。
    pub forward_24h: f64,
}

/// 因子预计算后按决策时点参与确定性横截面排序的首次越界。
#[derive(Debug, Clone, PartialEq)]
struct CrossingCandidate {
    /// OKX 合约标识。
    symbol: String,
    /// 当前相对 7 日均值的对数基差偏离。
    deviation: f64,
    /// 是否首次越过 50bps 可执行阈值。
    executable: bool,
}

/// 解析冻结面板参数；未知参数直接失败。
pub fn parse_cross_exchange_basis_panel_args<I>(values: I) -> Result<CrossExchangeBasisPanelArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut manifest = None;
    let mut cache_dir = None;
    let mut download_concurrency = 16usize;
    let mut binance_rest_base = DEFAULT_BINANCE_REST_BASE.to_owned();
    let mut binance_data_base = DEFAULT_BINANCE_DATA_BASE.to_owned();
    while let Some(arg) = values.next() {
        let value = |values: &mut I::IntoIter| {
            values
                .next()
                .ok_or_else(|| anyhow!("{arg} requires a value"))
        };
        match arg.as_str() {
            "--manifest" => manifest = Some(PathBuf::from(value(&mut values)?)),
            "--cache-dir" => cache_dir = Some(PathBuf::from(value(&mut values)?)),
            "--download-concurrency" => {
                download_concurrency = value(&mut values)?
                    .parse()
                    .context("parse --download-concurrency")?
            }
            "--binance-rest-base" => {
                binance_rest_base = value(&mut values)?.trim_end_matches('/').to_owned()
            }
            "--binance-data-base" => {
                binance_data_base = value(&mut values)?.trim_end_matches('/').to_owned()
            }
            "--help" | "-h" => bail!(cross_exchange_basis_panel_usage()),
            _ => bail!(
                "unknown argument: {arg}\n{}",
                cross_exchange_basis_panel_usage()
            ),
        }
    }
    if !(1..=32).contains(&download_concurrency) {
        bail!("--download-concurrency must be between 1 and 32");
    }
    Ok(CrossExchangeBasisPanelArgs {
        manifest: manifest.context("--manifest is required")?,
        cache_dir: cache_dir.context("--cache-dir is required")?,
        download_concurrency,
        binance_rest_base,
        binance_data_base,
    })
}

/// 返回冻结因子面板的最小命令用法。
pub fn cross_exchange_basis_panel_usage() -> &'static str {
    "Usage: market_cross_exchange_basis_panel --manifest PATH --cache-dir PATH [--download-concurrency 16]"
}

/// 运行冻结因子面板并打印一次性边际价值结果。
pub async fn run_cross_exchange_basis_panel(
    args: &CrossExchangeBasisPanelArgs,
    database_url: &str,
) -> Result<CrossExchangeBasisPanelReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode cross-exchange basis universe manifest")?;
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
        .context("connect quant_core for cross-exchange basis panel")?;
    let mut okx = BTreeMap::<String, Vec<CandleItem>>::new();
    for symbol in schedule.union_symbols() {
        okx.insert(
            symbol.clone(),
            load_okx_candles(
                &pool,
                &symbol,
                first.from_ms.saturating_sub(8 * DAY_MS),
                last.to_ms.saturating_add(2 * DAY_MS),
            )
            .await?,
        );
    }
    let (binance, binance_audit) = load_binance_klines(args, &schedule).await?;
    let (observations, stages) = build_observations(&schedule, &okx, &binance);
    let report = build_report(&schedule, okx.len(), binance_audit, stages, &observations);
    print_report(&report);
    Ok(report)
}

/// 运行冻结的 50bps 首次越界因子面板，不复用 z-score 面板身份。
pub async fn run_cross_exchange_dislocation_panel(
    args: &CrossExchangeBasisPanelArgs,
    database_url: &str,
) -> Result<CrossExchangeDislocationPanelReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode executable basis dislocation universe manifest")?;
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
        .context("connect quant_core for executable basis dislocation panel")?;
    let mut okx = BTreeMap::<String, Vec<CandleItem>>::new();
    for symbol in schedule.union_symbols() {
        okx.insert(
            symbol.clone(),
            load_okx_candles(
                &pool,
                &symbol,
                first.from_ms.saturating_sub(8 * DAY_MS),
                last.to_ms.saturating_add(2 * DAY_MS),
            )
            .await?,
        );
    }
    let (binance, binance_audit) = load_binance_klines(args, &schedule).await?;
    let (observations, stages) = build_dislocation_observations(&schedule, &okx, &binance);
    let report =
        build_dislocation_report(&schedule, okx.len(), binance_audit, stages, &observations);
    print_dislocation_report(&report);
    Ok(report)
}

impl UniverseSchedule {
    /// 从 current-live crypto-only manifest 构造连续十二个月窗口。
    fn from_manifest(manifest: HistoricalUniverseManifest) -> Result<Self> {
        if manifest.schema_version != 1
            || manifest.exchange != "okx"
            || manifest.market_type != "perpetual_swap"
            || manifest.quote_currency != "USDT"
            || manifest.timeframe != "15m"
            || !manifest
                .selection_rule
                .starts_with("current-live OKX USDT swaps only")
            || !manifest
                .source
                .classification_boundary
                .contains("instCategory=1")
        {
            bail!("cross-exchange basis panel requires current-live crypto-only OKX 15m manifest");
        }
        let mut windows = manifest
            .months
            .into_iter()
            .map(|month| UniverseWindow {
                from_ms: month.effective_from_ms,
                to_ms: month.effective_to_ms,
                members: month
                    .members
                    .into_iter()
                    .map(|member| member.symbol.to_ascii_uppercase())
                    .collect(),
            })
            .collect::<Vec<_>>();
        windows.sort_by_key(|window| window.from_ms);
        if windows.len() != 12
            || windows.iter().any(|window| {
                window.from_ms >= window.to_ms
                    || window.members.is_empty()
                    || window.members.iter().any(|symbol| !valid_symbol(symbol))
            })
            || windows
                .windows(2)
                .any(|pair| pair[0].to_ms != pair[1].from_ms)
        {
            bail!("cross-exchange basis V1 requires twelve contiguous monthly windows");
        }
        Ok(Self {
            version: manifest.universe_version,
            windows,
        })
    }

    /// 返回全部窗口成员的确定性去重并集。
    pub(super) fn union_symbols(&self) -> Vec<String> {
        self.windows
            .iter()
            .flat_map(|window| window.members.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// 查找指定决策时间所属的历史币池窗口。
    fn window_at(&self, ts: i64) -> Option<&UniverseWindow> {
        self.windows
            .iter()
            .find(|window| ts >= window.from_ms && ts < window.to_ms)
    }
}

/// 每个 UTC 4 小时时点选择绝对基差 z-score 最大的一项并固定 outcome。
fn build_observations(
    schedule: &UniverseSchedule,
    okx: &BTreeMap<String, Vec<CandleItem>>,
    binance: &BTreeMap<String, Vec<BinanceCandle>>,
) -> (Vec<BasisObservation>, CrossExchangeBasisStages) {
    let mut stages = CrossExchangeBasisStages::default();
    let mut observations = Vec::new();
    let Some(first) = schedule.windows.first() else {
        return (observations, stages);
    };
    let Some(last) = schedule.windows.last() else {
        return (observations, stages);
    };
    let mut decision_ts = first.from_ms;
    while decision_ts < last.to_ms {
        let Some(window) = schedule.window_at(decision_ts) else {
            decision_ts = decision_ts.saturating_add(MS_4H);
            continue;
        };
        stages.decision_points += 1;
        let minimum = (window.members.len() as f64 * MIN_FACTOR_COVERAGE).ceil() as usize;
        let mut factors = Vec::<(String, BasisFactor)>::new();
        for symbol in &window.members {
            let (Some(okx_candles), Some(binance_candles)) = (okx.get(symbol), binance.get(symbol))
            else {
                continue;
            };
            if let Some(factor) = basis_factor_at(
                okx_candles,
                binance_candles,
                decision_ts.saturating_sub(MS_15M),
            ) {
                factors.push((symbol.clone(), factor));
            }
        }
        if minimum == 0 || factors.len() < minimum {
            stages.coverage_blocked += 1;
            decision_ts = decision_ts.saturating_add(MS_4H);
            continue;
        }
        stages.factor_observations += factors.len();
        factors.sort_by(|left, right| {
            right
                .1
                .z_score
                .abs()
                .total_cmp(&left.1.z_score.abs())
                .then_with(|| left.0.cmp(&right.0))
        });
        let Some((symbol, factor)) = factors.into_iter().next() else {
            decision_ts = decision_ts.saturating_add(MS_4H);
            continue;
        };
        stages.selected_candidates += 1;
        if factor.z_score.abs() >= EXTREME_Z {
            stages.extreme_candidates += 1;
        } else {
            stages.control_candidates += 1;
        }
        let outcome = paired_forward_returns(
            &okx[&symbol],
            &binance[&symbol],
            decision_ts,
            factor.z_score,
        );
        if let Some((forward_1h, forward_4h, forward_24h)) = outcome {
            observations.push(BasisObservation {
                symbol,
                decision_ts,
                z_score: factor.z_score,
                forward_1h,
                forward_4h,
                forward_24h,
            });
        } else {
            stages.incomplete_outcomes += 1;
        }
        decision_ts = decision_ts.saturating_add(MS_4H);
    }
    (observations, stages)
}

/// 线性预计算全部同步因子，并只在首次跨越经济阈值时参与 15m 横截面选择。
fn build_dislocation_observations(
    schedule: &UniverseSchedule,
    okx: &BTreeMap<String, Vec<CandleItem>>,
    binance: &BTreeMap<String, Vec<BinanceCandle>>,
) -> (Vec<DislocationObservation>, CrossExchangeDislocationStages) {
    let mut coverage_by_time = BTreeMap::<i64, usize>::new();
    let mut crossings_by_time = BTreeMap::<i64, Vec<CrossingCandidate>>::new();
    for symbol in schedule.union_symbols() {
        let (Some(okx_candles), Some(binance_candles)) = (okx.get(&symbol), binance.get(&symbol))
        else {
            continue;
        };
        for (decision_ts, factor) in dislocation_factors(okx_candles, binance_candles) {
            if !schedule
                .window_at(decision_ts)
                .is_some_and(|window| window.members.contains(&symbol))
            {
                continue;
            }
            *coverage_by_time.entry(decision_ts).or_default() += 1;
            let current = factor.current_deviation.abs();
            let previous = factor.previous_deviation.abs();
            let executable = current >= EXECUTABLE_DEVIATION && previous < EXECUTABLE_DEVIATION;
            let control = (CONTROL_DEVIATION..EXECUTABLE_DEVIATION).contains(&current)
                && previous < CONTROL_DEVIATION;
            if executable || control {
                crossings_by_time
                    .entry(decision_ts)
                    .or_default()
                    .push(CrossingCandidate {
                        symbol: symbol.clone(),
                        deviation: factor.current_deviation,
                        executable,
                    });
            }
        }
    }
    let mut stages = CrossExchangeDislocationStages::default();
    let mut observations = Vec::new();
    let Some(first) = schedule.windows.first() else {
        return (observations, stages);
    };
    let Some(last) = schedule.windows.last() else {
        return (observations, stages);
    };
    let mut decision_ts = first.from_ms;
    while decision_ts < last.to_ms {
        let Some(window) = schedule.window_at(decision_ts) else {
            decision_ts = decision_ts.saturating_add(MS_15M);
            continue;
        };
        stages.decision_points += 1;
        let coverage = coverage_by_time.get(&decision_ts).copied().unwrap_or(0);
        let minimum = (window.members.len() as f64 * MIN_FACTOR_COVERAGE).ceil() as usize;
        if minimum == 0 || coverage < minimum {
            stages.coverage_blocked += 1;
            decision_ts = decision_ts.saturating_add(MS_15M);
            continue;
        }
        stages.factor_observations += coverage;
        let mut candidates = crossings_by_time.remove(&decision_ts).unwrap_or_default();
        stages.executable_crossings += candidates
            .iter()
            .filter(|candidate| candidate.executable)
            .count();
        stages.control_crossings += candidates
            .iter()
            .filter(|candidate| !candidate.executable)
            .count();
        candidates.sort_by(|left, right| {
            right
                .deviation
                .abs()
                .total_cmp(&left.deviation.abs())
                .then_with(|| left.symbol.cmp(&right.symbol))
        });
        if let Some(candidate) = candidates.into_iter().next() {
            stages.selected_candidates += 1;
            if candidate.executable {
                stages.selected_executable += 1;
            } else {
                stages.selected_control += 1;
            }
            let outcome = paired_forward_returns(
                &okx[&candidate.symbol],
                &binance[&candidate.symbol],
                decision_ts,
                candidate.deviation,
            );
            if let Some((forward_1h, forward_4h, forward_24h)) = outcome {
                observations.push(DislocationObservation {
                    symbol: candidate.symbol,
                    decision_ts,
                    deviation: candidate.deviation,
                    executable: candidate.executable,
                    forward_1h,
                    forward_4h,
                    forward_24h,
                });
            } else {
                stages.incomplete_outcomes += 1;
            }
        }
        decision_ts = decision_ts.saturating_add(MS_15M);
    }
    (observations, stages)
}

/// 将同步序列按连续片段线性转换为当前和上一 7 日均值偏离。
fn dislocation_factors(
    okx: &[CandleItem],
    binance: &[BinanceCandle],
) -> Vec<(i64, BasisDislocationFactor)> {
    let mut aligned = Vec::<(i64, f64)>::new();
    let (mut okx_index, mut binance_index) = (0usize, 0usize);
    while okx_index < okx.len() && binance_index < binance.len() {
        let okx_candle = &okx[okx_index];
        let binance_candle = &binance[binance_index];
        match okx_candle.ts.cmp(&binance_candle.ts) {
            std::cmp::Ordering::Less => okx_index += 1,
            std::cmp::Ordering::Greater => binance_index += 1,
            std::cmp::Ordering::Equal => {
                if okx_candle.c > 0.0 && binance_candle.close > 0.0 {
                    let basis = (okx_candle.c / binance_candle.close).ln();
                    if basis.is_finite() {
                        aligned.push((okx_candle.ts, basis));
                    }
                }
                okx_index += 1;
                binance_index += 1;
            }
        }
    }
    let mut factors = Vec::new();
    let mut segment_start = 0usize;
    while segment_start < aligned.len() {
        let mut segment_end = segment_start + 1;
        while segment_end < aligned.len()
            && aligned[segment_end - 1].0.saturating_add(MS_15M) == aligned[segment_end].0
        {
            segment_end += 1;
        }
        let segment = &aligned[segment_start..segment_end];
        if segment.len() > BASIS_BARS {
            let mut prefix = Vec::with_capacity(segment.len() + 1);
            prefix.push(0.0);
            for (_, value) in segment {
                prefix.push(prefix.last().copied().unwrap_or(0.0) + value);
            }
            for index in BASIS_BARS..segment.len() {
                let previous_mean =
                    (prefix[index] - prefix[index - BASIS_BARS]) / BASIS_BARS as f64;
                let current_mean =
                    (prefix[index + 1] - prefix[index + 1 - BASIS_BARS]) / BASIS_BARS as f64;
                let previous_deviation = segment[index - 1].1 - previous_mean;
                let current_deviation = segment[index].1 - current_mean;
                if previous_deviation.is_finite() && current_deviation.is_finite() {
                    factors.push((
                        segment[index].0.saturating_add(MS_15M),
                        BasisDislocationFactor {
                            current_deviation,
                            previous_deviation,
                        },
                    ));
                }
            }
        }
        segment_start = segment_end;
    }
    factors
}

/// 只使用同步、连续、已完成的 7 日价格计算基差 z-score。
fn basis_factor_at(
    okx: &[CandleItem],
    binance: &[BinanceCandle],
    decision_candle_ts: i64,
) -> Option<BasisFactor> {
    let okx_index = okx
        .binary_search_by_key(&decision_candle_ts, |candle| candle.ts)
        .ok()?;
    let binance_index = binance
        .binary_search_by_key(&decision_candle_ts, |candle| candle.ts)
        .ok()?;
    if okx_index + 1 < BASIS_BARS || binance_index + 1 < BASIS_BARS {
        return None;
    }
    let okx_start = okx_index + 1 - BASIS_BARS;
    let binance_start = binance_index + 1 - BASIS_BARS;
    let mut basis = Vec::with_capacity(BASIS_BARS);
    for offset in 0..BASIS_BARS {
        let okx_candle = &okx[okx_start + offset];
        let binance_candle = &binance[binance_start + offset];
        if okx_candle.ts != binance_candle.ts
            || (offset > 0
                && (okx[okx_start + offset - 1].ts.saturating_add(MS_15M) != okx_candle.ts
                    || binance[binance_start + offset - 1]
                        .ts
                        .saturating_add(MS_15M)
                        != binance_candle.ts))
            || okx_candle.c <= 0.0
            || binance_candle.close <= 0.0
        {
            return None;
        }
        let value = (okx_candle.c / binance_candle.close).ln();
        if !value.is_finite() {
            return None;
        }
        basis.push(value);
    }
    let mean = basis.iter().sum::<f64>() / basis.len() as f64;
    let variance = basis
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / basis.len().checked_sub(1)? as f64;
    let standard_deviation = variance.sqrt();
    if !standard_deviation.is_finite() || standard_deviation <= f64::EPSILON {
        return None;
    }
    let z_score = (basis.last()? - mean) / standard_deviation;
    z_score.is_finite().then_some(BasisFactor { z_score })
}

/// 从下一共同开盘计算固定 1h、4h 和 24h 的双腿方向收益。
fn paired_forward_returns(
    okx: &[CandleItem],
    binance: &[BinanceCandle],
    decision_ts: i64,
    z_score: f64,
) -> Option<(f64, f64, f64)> {
    if z_score == 0.0 || !z_score.is_finite() {
        return None;
    }
    let okx_entry_index = okx
        .binary_search_by_key(&decision_ts, |candle| candle.ts)
        .ok()?;
    let binance_entry_index = binance
        .binary_search_by_key(&decision_ts, |candle| candle.ts)
        .ok()?;
    let okx_entry = okx.get(okx_entry_index)?.o;
    let binance_entry = binance.get(binance_entry_index)?.open;
    if okx_entry <= 0.0 || binance_entry <= 0.0 {
        return None;
    }
    let direction = if z_score < 0.0 { 1.0 } else { -1.0 };
    let paired_at = |bars: usize| -> Option<f64> {
        let okx_exit_index = okx_entry_index.checked_add(bars.checked_sub(1)?)?;
        let binance_exit_index = binance_entry_index.checked_add(bars.checked_sub(1)?)?;
        let okx_exit = okx.get(okx_exit_index)?;
        let binance_exit = binance.get(binance_exit_index)?;
        if okx[okx_entry_index].ts != binance[binance_entry_index].ts
            || okx_exit.ts != binance_exit.ts
            || okx_exit.ts - okx[okx_entry_index].ts != (bars as i64 - 1) * MS_15M
            || binance_exit.ts - binance[binance_entry_index].ts != (bars as i64 - 1) * MS_15M
            || okx_exit.c <= 0.0
            || binance_exit.close <= 0.0
        {
            return None;
        }
        let okx_return = okx_exit.c / okx_entry - 1.0;
        let binance_return = binance_exit.close / binance_entry - 1.0;
        let paired = direction * (okx_return - binance_return);
        paired.is_finite().then_some(paired)
    };
    Some((
        paired_at(FORWARD_1H_BARS)?,
        paired_at(FORWARD_4H_BARS)?,
        paired_at(FORWARD_24H_BARS)?,
    ))
}

/// 从本地 quant_core 读取已确认且严格排序的 OKX 15m K 线。
async fn load_okx_candles(
    pool: &PgPool,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<CandleItem>> {
    if !valid_symbol(symbol) {
        bail!("invalid cross-exchange basis manifest symbol {symbol}");
    }
    let table = format!("{}_candles_15m", symbol.to_ascii_lowercase());
    let query = format!(
        "SELECT ts, o, h, l, c, vol FROM \"{table}\" WHERE confirm = '1' AND ts >= $1 AND ts < $2 ORDER BY ts"
    );
    sqlx::query(&query)
        .bind(start_ms)
        .bind(end_ms)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load cross-exchange basis candles from {table}"))?
        .into_iter()
        .map(|row| {
            Ok(CandleItem {
                ts: row.get("ts"),
                o: parse_number(row.get::<String, _>("o"))?,
                h: parse_number(row.get::<String, _>("h"))?,
                l: parse_number(row.get::<String, _>("l"))?,
                c: parse_number(row.get::<String, _>("c"))?,
                v: parse_number(row.get::<String, _>("vol"))?,
                confirm: 1,
            })
        })
        .collect()
}

/// 解析数据库数值并拒绝非有限值。
fn parse_number(value: String) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse cross-exchange basis candle number {value}"))?;
    if !parsed.is_finite() {
        bail!("non-finite cross-exchange basis candle number {value}");
    }
    Ok(parsed)
}

/// 限制动态表名只能来自规范 OKX USDT 永续标识。
fn valid_symbol(symbol: &str) -> bool {
    symbol.ends_with("-USDT-SWAP")
        && symbol
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests;
