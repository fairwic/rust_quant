//! 用已同步到 `quant_core` 的最新已确认 15m K 线回放冻结的严格横盘突破候选。
//!
//! 该入口只接受显式币种和时间窗口，不选择阈值、不注册 Paper/Live，也不改变
//! 已冻结的 V23 `acceptance retention >= 0.75` 或其 V28 单棒守稳候选与执行成本合同。

use anyhow::{bail, Context, Result};
use chrono::Utc;
use rust_quant_cli::app::tradingview_velocity_parity::{
    load_research_window_symbols_from_quant_core, replay_with_strict_visual_breakout_variant,
    verify_v20_pine_source, Direction, EntryIntent, ParityRuleVersion, ReplayConfig, ReplayReport,
    ResearchWindowSymbolCandles, SignalFamily, StrictVisualBreakoutResearchVariant, Trade,
};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const CANDLE_INTERVAL_MS: i64 = 15 * 60 * 1_000;
const WARMUP_DAYS: i64 = 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const EVENT_CLUSTER_MS: i64 = 60 * 60 * 1_000;
const ENTRY_MAKER_FEE_BPS: f64 = 2.0;
const ENTRY_SLIPPAGE_BPS: f64 = 1.0;
const EXIT_TAKER_FEE_BPS: f64 = 5.0;
const EXIT_SLIPPAGE_BPS: f64 = 1.0;
const RETENTION_THRESHOLD: f64 = 0.75;

/// 当前窗口回放只允许改变数据边界；策略、成本和预热长度保持冻结。
struct Args {
    /// 显式 OKX USDT 永续成员，保持调用方给出的顺序。
    symbols: Vec<String>,
    /// 样本延伸首根 K 线开盘时间，Unix 毫秒。
    evaluation_start_ms: i64,
    /// 样本延伸上界，Unix 毫秒且不包含该时点。
    evaluation_end_exclusive_ms: i64,
    /// 原子发布的 JSON 报告路径。
    output: PathBuf,
    /// 只允许基线 V23 或当前预注册的单变量候选。
    variant: StrictVisualBreakoutResearchVariant,
}

impl Args {
    /// 解析显式研究身份，拒绝任何阈值或成本开关。
    fn parse(values: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut symbols = None;
        let mut evaluation_start_ms = None;
        let mut evaluation_end_exclusive_ms = None;
        let mut output = None;
        let mut variant = StrictVisualBreakoutResearchVariant::V23AcceptanceRetention75CostReplay;
        let mut values = values.into_iter();
        while let Some(argument) = values.next() {
            match argument.as_str() {
                "--symbols" => {
                    symbols = Some(parse_symbols(
                        &values.next().context("--symbols requires a comma list")?,
                    )?)
                }
                "--evaluation-start-ms" => {
                    evaluation_start_ms = Some(parse_i64(
                        &values
                            .next()
                            .context("--evaluation-start-ms requires Unix milliseconds")?,
                        "--evaluation-start-ms",
                    )?)
                }
                "--evaluation-end-exclusive-ms" => {
                    evaluation_end_exclusive_ms = Some(parse_i64(
                        &values
                            .next()
                            .context("--evaluation-end-exclusive-ms requires Unix milliseconds")?,
                        "--evaluation-end-exclusive-ms",
                    )?)
                }
                "--output" => {
                    output = Some(PathBuf::from(
                        values.next().context("--output requires a file path")?,
                    ))
                }
                "--variant" => variant = match values
                    .next()
                    .context(
                        "--variant requires v23, v28, v29, v30, v31, v32, v33, v34, v35, v36, v40, v41, v42, or v43",
                    )?
                    .as_str()
                {
                    "v23" => {
                        StrictVisualBreakoutResearchVariant::V23AcceptanceRetention75CostReplay
                    }
                    "v28" => {
                        StrictVisualBreakoutResearchVariant::V28AcceptanceRetention75NextBarHold
                    }
                    "v29" => StrictVisualBreakoutResearchVariant::V29BoundaryP96P04,
                    "v30" => StrictVisualBreakoutResearchVariant::V30PriorValidRangeClearance,
                    "v31" => StrictVisualBreakoutResearchVariant::V31InternalImpulseBodyGuard,
                    "v32" => StrictVisualBreakoutResearchVariant::V32PriorThreeWickDirectChase,
                    "v33" => StrictVisualBreakoutResearchVariant::V33OpposingConfirmationWickGuard,
                    "v34" => StrictVisualBreakoutResearchVariant::V34VolumeProfitProtection,
                    "v35" => StrictVisualBreakoutResearchVariant::V35RemainingNetReward50,
                    "v36" => StrictVisualBreakoutResearchVariant::V36PreRangeExtremeTradeBoundary96,
                    "v40" => {
                        StrictVisualBreakoutResearchVariant::V40VisualParentExecutionBoundary
                    }
                    "v41" => StrictVisualBreakoutResearchVariant::V41NestedChildRange,
                    "v42" => {
                        StrictVisualBreakoutResearchVariant::V42ContainedInternalBodyExemption
                    }
                    "v43" => StrictVisualBreakoutResearchVariant::V43BalancedParentUpgrade,
                    other => bail!("unsupported latest replay variant: {other}"),
                },
                "--help" | "-h" => {
                    println!(
                    "Usage: strict_visual_breakout_latest_window_replay --symbols BTC-USDT-SWAP,ETH-USDT-SWAP --evaluation-start-ms <ms> --evaluation-end-exclusive-ms <ms> --variant v23|v28|v29|v30|v31|v32|v33|v34|v35|v36|v40|v41|v42|v43 --output <json>"
                    );
                    std::process::exit(0);
                }
                other => bail!("unsupported argument: {other}"),
            }
        }
        Ok(Self {
            symbols: symbols.context("missing --symbols")?,
            evaluation_start_ms: evaluation_start_ms.context("missing --evaluation-start-ms")?,
            evaluation_end_exclusive_ms: evaluation_end_exclusive_ms
                .context("missing --evaluation-end-exclusive-ms")?,
            output: output.context("missing --output")?,
            variant,
        })
    }
}

/// 数据覆盖不足的成员；它不会进入信号或收益统计。
#[derive(Debug, Serialize)]
struct ExcludedSymbol {
    /// OKX 交易对。
    symbol: String,
    /// 60 天预热应有的 K 线数。
    warmup_expected: usize,
    /// 60 天预热实际已确认 K 线数。
    warmup_loaded: usize,
    /// 评价窗口应有的 K 线数。
    evaluation_expected: usize,
    /// 评价窗口实际已确认 K 线数。
    evaluation_loaded: usize,
    /// 成员被排除的可审计原因。
    reason: &'static str,
}

/// 一笔严格横盘实际平仓交易的最小统计记录。
#[derive(Debug, Clone)]
struct StrictTradeObservation {
    /// 交易所属 OKX 交易对。
    symbol: String,
    /// 多头或空头方向。
    direction: Direction,
    /// 信号完成棒时间，Unix 毫秒。
    signal_time_ms: i64,
    /// 实际下一根开盘成交时间，Unix 毫秒。
    entry_time_ms: i64,
    /// 实际平仓时间，Unix 毫秒。
    exit_time_ms: i64,
    /// 扣除冻结成本后的 R。
    net_r: f64,
}

/// 严格横盘家族的成本后指标；全部收益字段都以初始止损风险 R 为单位。
#[derive(Debug, Serialize)]
struct StrictVisualMetrics {
    /// 已平仓交易数。
    trades: usize,
    /// 成本后 R 为正的交易数。
    wins: usize,
    /// 成本后 R 为负的交易数。
    losses: usize,
    /// 成本后 R 为零的交易数。
    flats: usize,
    /// 胜率，百分比。
    win_rate_percent: f64,
    /// 全部交易成本后净 R。
    net_r: f64,
    /// 单笔平均成本后净 R。
    average_net_r: f64,
    /// 正 R 总和。
    gross_profit_r: f64,
    /// 负 R 绝对值总和。
    gross_loss_r: f64,
    /// `gross_profit_r / gross_loss_r`；没有亏损分母时为 `None`。
    profit_factor_r: Option<f64>,
    /// `true` 表示存在盈利且没有亏损，PF 为正无穷。
    profit_factor_r_is_infinite: bool,
    /// 按实际平仓时序累计 R 的最大回撤。
    chronological_max_drawdown_r: f64,
    /// 交易覆盖的不同币种数。
    distinct_symbols: usize,
    /// 多头已平仓交易数。
    long_trades: usize,
    /// 空头已平仓交易数。
    short_trades: usize,
    /// 按方向分离、相邻信号不超过 60 分钟的事件簇数量。
    direction_hour_event_clusters: usize,
}

/// 一笔头部正收益交易，用于审计收益是否集中在少数样本。
#[derive(Debug, Clone, Serialize)]
struct TradeContribution {
    /// OKX 交易对。
    symbol: String,
    /// 交易方向。
    direction: Direction,
    /// 信号时间，Unix 毫秒。
    signal_time_ms: i64,
    /// 入场时间，Unix 毫秒。
    entry_time_ms: i64,
    /// 成本后净 R。
    net_r: f64,
}

/// 删除指定头部贡献后的剩余 R 指标。
#[derive(Debug, Serialize)]
struct RemovalMetrics {
    /// 剩余交易数。
    trades: usize,
    /// 剩余成本后净 R。
    net_r: f64,
    /// 剩余单笔平均净 R。
    average_net_r: f64,
    /// 剩余交易 Profit Factor；无亏损分母时为 `None`。
    profit_factor_r: Option<f64>,
    /// `true` 表示剩余交易有盈利且没有亏损。
    profit_factor_r_is_infinite: bool,
}

/// 正收益交易和币种的集中度审计。
#[derive(Debug, Serialize)]
struct StrictVisualConcentration {
    /// 全部正收益交易净 R 总和。
    gross_positive_trade_r: f64,
    /// 最大单笔正收益交易。
    top1_trade: Option<TradeContribution>,
    /// 最大五笔正收益交易。
    top5_trades: Vec<TradeContribution>,
    /// Top1 占全部正收益 R 的百分比。
    top1_positive_r_share_percent: Option<f64>,
    /// Top5 占全部正收益 R 的百分比。
    top5_positive_r_share_percent: Option<f64>,
    /// 移除 Top5 正收益交易后的指标。
    after_removing_top5_trades: RemovalMetrics,
    /// 聚合净 R 最大的正收益币种。
    best_positive_symbol: Option<String>,
    /// 最佳币种占全部正收益币种净 R 的百分比。
    best_symbol_positive_r_share_percent: Option<f64>,
    /// 移除最佳正收益币种及其全部交易后的指标。
    after_removing_best_symbol: RemovalMetrics,
}

/// 用户核图所需的一笔完整成交生命周期。
#[derive(Debug, Serialize)]
struct LatestTradeDetail {
    /// OKX 交易对。
    symbol: String,
    /// 交易方向。
    direction: Direction,
    /// 信号完成棒时间，Unix 毫秒。
    signal_time_ms: i64,
    /// 下一根开盘实际成交时间，Unix 毫秒。
    entry_time_ms: i64,
    /// 平仓时间，Unix 毫秒。
    exit_time_ms: i64,
    /// 实际成交价。
    entry_price: f64,
    /// 实际平仓价。
    exit_price: f64,
    /// 入场时冻结的实际初始止损。
    initial_stop: f64,
    /// 由信号意图和实际成交价还原的冻结目标价。
    target_price: Option<f64>,
    /// 信号完成时 ATR14。
    signal_atr: f64,
    /// 信号时冻结的横盘高度。
    range_height: Option<f64>,
    /// 信号时冻结的横盘长度。
    range_length_bars: Option<usize>,
    /// 本版本固定要求的首次接受保留率下限。
    minimum_acceptance_retention_ratio: f64,
    /// 平仓原因。
    exit_reason: rust_quant_cli::app::tradingview_velocity_parity::ExitReason,
    /// 未扣成本毛 R。
    gross_r: f64,
    /// 手续费与滑点折算的 R 损耗。
    cost_r: f64,
    /// 扣除成本后的净 R。
    net_r: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse(std::env::args().skip(1))?;
    verify_v20_pine_source()?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(WARMUP_DAYS * DAY_MS)
        .context("calculating 60-day warmup start")?;
    let replay_end_ms = args
        .evaluation_end_exclusive_ms
        .checked_sub(CANDLE_INTERVAL_MS)
        .context("calculating inclusive replay end")?;
    let loaded = load_research_window_symbols_from_quant_core(
        &args.symbols,
        warmup_start_ms,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
    )
    .await?;
    let dataset_fingerprint_sha256 = dataset_fingerprint(
        &loaded,
        warmup_start_ms,
        args.evaluation_start_ms,
        args.evaluation_end_exclusive_ms,
    );

    let mut included = Vec::new();
    let mut excluded = Vec::new();
    for symbol in loaded {
        if symbol.warmup_coverage.is_complete && symbol.evaluation_coverage.is_complete {
            included.push(symbol);
            continue;
        }
        excluded.push(ExcludedSymbol {
            symbol: symbol.symbol,
            warmup_expected: symbol.warmup_coverage.expected_candle_count,
            warmup_loaded: symbol.warmup_coverage.loaded_candle_count,
            evaluation_expected: symbol.evaluation_coverage.expected_candle_count,
            evaluation_loaded: symbol.evaluation_coverage.loaded_candle_count,
            reason: match (
                symbol.warmup_coverage.is_complete,
                symbol.evaluation_coverage.is_complete,
            ) {
                (false, false) => "warmup_and_evaluation_incomplete",
                (false, true) => "warmup_incomplete",
                (true, false) => "evaluation_incomplete",
                (true, true) => unreachable!("complete member is included above"),
            },
        });
    }
    if included.is_empty() {
        bail!("no symbol has complete 60-day warmup and evaluation coverage");
    }

    let variant = args.variant;
    let mut per_symbol = Vec::with_capacity(included.len());
    for symbol in included {
        let config = ReplayConfig {
            fee_bps_per_side: ENTRY_MAKER_FEE_BPS,
            slippage_bps_per_side: ENTRY_SLIPPAGE_BPS,
            exit_fee_bps_per_side: Some(EXIT_TAKER_FEE_BPS),
            exit_slippage_bps_per_side: Some(EXIT_SLIPPAGE_BPS),
            ..ReplayConfig::current_pine_v20(
                symbol.symbol,
                symbol.tick_size,
                args.evaluation_start_ms,
                replay_end_ms,
            )
        };
        per_symbol.push(replay_with_strict_visual_breakout_variant(
            &symbol.candles,
            config,
            variant,
        ));
    }

    let observations = strict_trade_observations(&per_symbol);
    let metrics = strict_visual_metrics(&observations);
    let concentration = strict_visual_concentration(&observations);
    let latest_twenty = latest_twenty_btc_eth_first(&per_symbol)?;
    let report = json!({
        "report_schema_version": 1,
        "report_scope": "research_only_latest_confirmed_window_cost_replay",
        "generated_at_ms": Utc::now().timestamp_millis(),
        "strategy_version": variant.strategy_version(ParityRuleVersion::CandidateV20),
        "strict_visual_breakout_research_variant": variant.slug(),
        "rule_version": "candidate_v20",
        "dataset_fingerprint_sha256": dataset_fingerprint_sha256,
        "universe": {
            "kind": "frozen_20260723_top60_plus_eth_explicit_extension",
            "survivorship_bias": true,
            "requested_symbols": args.symbols,
            "requested_symbol_count": args.symbols.len(),
            "included_symbol_count": per_symbol.len(),
            "excluded_symbols": excluded,
        },
        "market_data": {
            "source": "quant_core per-symbol confirmed OKX 15m tables",
            "volume_field": "vol_ccy",
            "warmup_days": WARMUP_DAYS,
            "warmup_start_ms": warmup_start_ms,
            "evaluation_start_ms": args.evaluation_start_ms,
            "evaluation_end_exclusive_ms": args.evaluation_end_exclusive_ms,
            "replay_last_candle_open_ms": replay_end_ms,
        },
        "execution_cost_contract": {
            "entry_fee_bps": ENTRY_MAKER_FEE_BPS,
            "entry_slippage_bps": ENTRY_SLIPPAGE_BPS,
            "exit_fee_bps": EXIT_TAKER_FEE_BPS,
            "exit_slippage_bps": EXIT_SLIPPAGE_BPS,
        },
        "strict_visual_metrics": metrics,
        "strict_visual_concentration": concentration,
        "latest_twenty_btc_eth_first": latest_twenty,
        "per_symbol": per_symbol,
        "interpretation_limits": [
            "This is a refreshed-data L2 diagnostic over a current-live survivorship-biased panel, not a point-in-time universe or L3 promotion result.",
            "Only the preregistered acceptance retention >= 0.75 subset is replayed; no neighboring threshold is searched.",
            "Metrics aggregate independent fixed-one-unit symbol replays and are not a shared-capital capacity-constrained portfolio.",
            "Chronological R drawdown orders closed trades by exit time; concurrent positions do not share a capital budget.",
            "Latest twenty prioritizes all available BTC/ETH strict-family trades, then fills remaining rows by actual entry time descending."
        ],
    });
    write_json_atomically(&args.output, &report)?;
    println!(
        "strict_visual_breakout_latest_window_replay: requested={} included={} excluded={} strict_trades={} output={}",
        args.symbols.len(),
        report["universe"]["included_symbol_count"],
        report["universe"]["excluded_symbols"]
            .as_array()
            .map_or(0, Vec::len),
        report["strict_visual_metrics"]["trades"],
        args.output.display(),
    );
    Ok(())
}

/// 解析并验证显式币种，保持顺序以便结果可稳定复现。
fn parse_symbols(raw: &str) -> Result<Vec<String>> {
    let symbols = raw
        .split(',')
        .map(str::trim)
        .filter(|symbol| !symbol.is_empty())
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>();
    if symbols.is_empty() {
        bail!("--symbols cannot be empty");
    }
    let unique = symbols.iter().collect::<BTreeSet<_>>();
    if unique.len() != symbols.len() {
        bail!("--symbols contains duplicate members");
    }
    Ok(symbols)
}

/// 解析 Unix 毫秒参数并保留参数名上下文。
fn parse_i64(raw: &str, name: &str) -> Result<i64> {
    raw.parse::<i64>()
        .with_context(|| format!("{name} must be an integer Unix millisecond timestamp"))
}

/// 数据指纹绑定成员顺序、窗口和全部已确认 OHLCV 输入，防止结果与行情快照脱离。
fn dataset_fingerprint(
    symbols: &[ResearchWindowSymbolCandles],
    warmup_start_ms: i64,
    evaluation_start_ms: i64,
    evaluation_end_exclusive_ms: i64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(warmup_start_ms.to_le_bytes());
    hasher.update(evaluation_start_ms.to_le_bytes());
    hasher.update(evaluation_end_exclusive_ms.to_le_bytes());
    for symbol in symbols {
        hasher.update(symbol.symbol.as_bytes());
        hasher.update(symbol.tick_size.to_bits().to_le_bytes());
        for candle in &symbol.candles {
            hasher.update(candle.timestamp_ms.to_le_bytes());
            hasher.update(candle.open.to_bits().to_le_bytes());
            hasher.update(candle.high.to_bits().to_le_bytes());
            hasher.update(candle.low.to_bits().to_le_bytes());
            hasher.update(candle.close.to_bits().to_le_bytes());
            hasher.update(candle.volume.to_bits().to_le_bytes());
        }
    }
    hex::encode(hasher.finalize())
}

/// 只抽取实际平仓且携带严格横盘家族的交易，避免其他 V20 家族污染指标。
fn strict_trade_observations(reports: &[ReplayReport]) -> Vec<StrictTradeObservation> {
    reports
        .iter()
        .flat_map(|report| {
            report
                .trades
                .iter()
                .filter(|trade| is_strict_visual_trade(trade))
                .map(|trade| StrictTradeObservation {
                    symbol: report.symbol.clone(),
                    direction: trade.direction,
                    signal_time_ms: trade.signal_time_ms,
                    entry_time_ms: trade.entry_time_ms,
                    exit_time_ms: trade.exit_time_ms,
                    net_r: trade.net_r,
                })
        })
        .collect()
}

/// 判断交易是否由严格视觉横盘突破多头或空头家族触发。
fn is_strict_visual_trade(trade: &Trade) -> bool {
    trade.families.iter().any(|family| {
        matches!(
            family,
            SignalFamily::StrictVisualConsolidationBreakLong
                | SignalFamily::StrictVisualConsolidationBreakShort
        )
    })
}

/// 按成本后 R 计算严格横盘家族的描述性 L2 指标。
fn strict_visual_metrics(trades: &[StrictTradeObservation]) -> StrictVisualMetrics {
    let wins = trades.iter().filter(|trade| trade.net_r > 0.0).count();
    let losses = trades.iter().filter(|trade| trade.net_r < 0.0).count();
    let gross_profit_r = trades.iter().map(|trade| trade.net_r.max(0.0)).sum();
    let gross_loss_r = trades.iter().map(|trade| (-trade.net_r).max(0.0)).sum();
    let net_r = trades.iter().map(|trade| trade.net_r).sum();
    StrictVisualMetrics {
        trades: trades.len(),
        wins,
        losses,
        flats: trades.len().saturating_sub(wins + losses),
        win_rate_percent: ratio_percent(wins as f64, trades.len() as f64).unwrap_or(0.0),
        net_r,
        average_net_r: if trades.is_empty() {
            0.0
        } else {
            net_r / trades.len() as f64
        },
        gross_profit_r,
        gross_loss_r,
        profit_factor_r: profit_factor(gross_profit_r, gross_loss_r),
        profit_factor_r_is_infinite: gross_profit_r > 0.0 && gross_loss_r == 0.0,
        chronological_max_drawdown_r: chronological_max_drawdown_r(trades),
        distinct_symbols: trades
            .iter()
            .map(|trade| trade.symbol.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        long_trades: trades
            .iter()
            .filter(|trade| trade.direction == Direction::Long)
            .count(),
        short_trades: trades
            .iter()
            .filter(|trade| trade.direction == Direction::Short)
            .count(),
        direction_hour_event_clusters: direction_hour_event_clusters(trades),
    }
}

/// 审计头部正收益交易与币种，避免被少数偶然样本掩盖。
fn strict_visual_concentration(trades: &[StrictTradeObservation]) -> StrictVisualConcentration {
    let mut positive = trades
        .iter()
        .filter(|trade| trade.net_r > 0.0)
        .cloned()
        .collect::<Vec<_>>();
    positive.sort_by(|left, right| {
        right
            .net_r
            .total_cmp(&left.net_r)
            .then_with(|| left.symbol.cmp(&right.symbol))
            .then_with(|| left.signal_time_ms.cmp(&right.signal_time_ms))
    });
    let gross_positive_trade_r = positive.iter().map(|trade| trade.net_r).sum::<f64>();
    let top5 = positive.iter().take(5).cloned().collect::<Vec<_>>();
    let top5_keys = top5
        .iter()
        .map(trade_key)
        .collect::<BTreeSet<(String, i64, i64, i64)>>();
    let top5_r = top5.iter().map(|trade| trade.net_r).sum::<f64>();

    let mut by_symbol = BTreeMap::<String, f64>::new();
    for trade in trades {
        *by_symbol.entry(trade.symbol.clone()).or_default() += trade.net_r;
    }
    let mut positive_symbols = by_symbol
        .into_iter()
        .filter(|(_, net_r)| *net_r > 0.0)
        .collect::<Vec<_>>();
    positive_symbols.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    let gross_positive_symbol_r = positive_symbols.iter().map(|(_, net_r)| net_r).sum::<f64>();
    let best_positive_symbol = positive_symbols.first().map(|(symbol, _)| symbol.clone());

    StrictVisualConcentration {
        gross_positive_trade_r,
        top1_trade: positive.first().map(trade_contribution),
        top5_trades: top5.iter().map(trade_contribution).collect(),
        top1_positive_r_share_percent: positive
            .first()
            .and_then(|trade| ratio_percent(trade.net_r, gross_positive_trade_r)),
        top5_positive_r_share_percent: ratio_percent(top5_r, gross_positive_trade_r),
        after_removing_top5_trades: removal_metrics(
            &trades
                .iter()
                .filter(|trade| !top5_keys.contains(&trade_key(trade)))
                .cloned()
                .collect::<Vec<_>>(),
        ),
        best_symbol_positive_r_share_percent: positive_symbols
            .first()
            .and_then(|(_, net_r)| ratio_percent(*net_r, gross_positive_symbol_r)),
        after_removing_best_symbol: removal_metrics(
            &trades
                .iter()
                .filter(|trade| {
                    best_positive_symbol
                        .as_ref()
                        .is_none_or(|symbol| &trade.symbol != symbol)
                })
                .cloned()
                .collect::<Vec<_>>(),
        ),
        best_positive_symbol,
    }
}

/// 将一笔统计记录转换为稳定去重键。
fn trade_key(trade: &StrictTradeObservation) -> (String, i64, i64, i64) {
    (
        trade.symbol.clone(),
        trade.signal_time_ms,
        trade.entry_time_ms,
        trade.exit_time_ms,
    )
}

/// 将头部交易转换为可序列化贡献记录。
fn trade_contribution(trade: &StrictTradeObservation) -> TradeContribution {
    TradeContribution {
        symbol: trade.symbol.clone(),
        direction: trade.direction,
        signal_time_ms: trade.signal_time_ms,
        entry_time_ms: trade.entry_time_ms,
        net_r: trade.net_r,
    }
}

/// 计算任意剩余子集的净 R、EV 与 PF。
fn removal_metrics(trades: &[StrictTradeObservation]) -> RemovalMetrics {
    let gross_profit_r = trades.iter().map(|trade| trade.net_r.max(0.0)).sum();
    let gross_loss_r = trades.iter().map(|trade| (-trade.net_r).max(0.0)).sum();
    let net_r = trades.iter().map(|trade| trade.net_r).sum();
    RemovalMetrics {
        trades: trades.len(),
        net_r,
        average_net_r: if trades.is_empty() {
            0.0
        } else {
            net_r / trades.len() as f64
        },
        profit_factor_r: profit_factor(gross_profit_r, gross_loss_r),
        profit_factor_r_is_infinite: gross_profit_r > 0.0 && gross_loss_r == 0.0,
    }
}

/// PF 只有在存在负 R 分母时才返回有限数值。
fn profit_factor(gross_profit_r: f64, gross_loss_r: f64) -> Option<f64> {
    (gross_loss_r > 0.0).then_some(gross_profit_r / gross_loss_r)
}

/// 百分比计算拒绝零分母，避免把无样本写成 0% 贡献。
fn ratio_percent(numerator: f64, denominator: f64) -> Option<f64> {
    (denominator > 0.0).then_some(numerator / denominator * 100.0)
}

/// 按平仓时间稳定排序后计算累计 R 权益的峰谷回撤。
fn chronological_max_drawdown_r(trades: &[StrictTradeObservation]) -> f64 {
    let mut ordered = trades.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.exit_time_ms
            .cmp(&right.exit_time_ms)
            .then_with(|| left.symbol.cmp(&right.symbol))
            .then_with(|| left.signal_time_ms.cmp(&right.signal_time_ms))
    });
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for trade in ordered {
        equity += trade.net_r;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    max_drawdown
}

/// 多空分开后按相邻信号一小时以内 single-linkage 统计有效市场事件。
fn direction_hour_event_clusters(trades: &[StrictTradeObservation]) -> usize {
    [Direction::Long, Direction::Short]
        .into_iter()
        .map(|direction| {
            let mut times = trades
                .iter()
                .filter(|trade| trade.direction == direction)
                .map(|trade| trade.signal_time_ms)
                .collect::<Vec<_>>();
            times.sort_unstable();
            times
                .iter()
                .enumerate()
                .filter(|(index, time)| {
                    *index == 0 || **time - times[*index - 1] > EVENT_CLUSTER_MS
                })
                .count()
        })
        .sum()
}

/// 先列出 BTC/ETH，再按实际开仓时间倒序用其他币种补足 20 笔。
fn latest_twenty_btc_eth_first(reports: &[ReplayReport]) -> Result<Vec<LatestTradeDetail>> {
    let mut btc_eth = Vec::new();
    let mut others = Vec::new();
    for report in reports {
        for trade in report
            .trades
            .iter()
            .filter(|trade| is_strict_visual_trade(trade))
        {
            let detail = latest_trade_detail(report, trade)?;
            if matches!(report.symbol.as_str(), "BTC-USDT-SWAP" | "ETH-USDT-SWAP") {
                btc_eth.push(detail);
            } else {
                others.push(detail);
            }
        }
    }
    let order = |left: &LatestTradeDetail, right: &LatestTradeDetail| {
        right
            .entry_time_ms
            .cmp(&left.entry_time_ms)
            .then_with(|| left.symbol.cmp(&right.symbol))
    };
    btc_eth.sort_by(order);
    others.sort_by(order);
    btc_eth.extend(others);
    btc_eth.truncate(20);
    Ok(btc_eth)
}

/// 将实际交易与同一信号时点冻结的意图对账，补齐目标、区间和成本 R。
fn latest_trade_detail(report: &ReplayReport, trade: &Trade) -> Result<LatestTradeDetail> {
    let intent = report
        .entry_candidates
        .iter()
        .find(|intent| {
            intent.signal_time_ms == trade.signal_time_ms
                && intent.direction == trade.direction
                && is_strict_visual_intent(intent)
        })
        .with_context(|| {
            format!(
                "missing strict entry intent for {} at {}",
                report.symbol, trade.signal_time_ms
            )
        })?;
    let gross_r = trade.gross_pnl / trade.initial_risk;
    Ok(LatestTradeDetail {
        symbol: report.symbol.clone(),
        direction: trade.direction,
        signal_time_ms: trade.signal_time_ms,
        entry_time_ms: trade.entry_time_ms,
        exit_time_ms: trade.exit_time_ms,
        entry_price: trade.entry_price,
        exit_price: trade.exit_price,
        initial_stop: trade.initial_stop,
        target_price: actual_target_from_intent(intent, trade.entry_price, report.tick_size),
        signal_atr: intent.signal_atr,
        range_height: intent.strict_visual_range_height,
        range_length_bars: intent.strict_visual_range_length_bars,
        minimum_acceptance_retention_ratio: RETENTION_THRESHOLD,
        exit_reason: trade.exit_reason,
        gross_r,
        cost_r: gross_r - trade.net_r,
        net_r: trade.net_r,
    })
}

/// 判断冻结意图是否属于严格横盘突破家族。
fn is_strict_visual_intent(intent: &EntryIntent) -> bool {
    intent.families.iter().any(|family| {
        matches!(
            family,
            SignalFamily::StrictVisualConsolidationBreakLong
                | SignalFamily::StrictVisualConsolidationBreakShort
        )
    })
}

/// 结构目标优先使用绝对价，tick 目标则按实际成交价和方向还原。
fn actual_target_from_intent(
    intent: &EntryIntent,
    entry_price: f64,
    tick_size: f64,
) -> Option<f64> {
    intent.target_price.or_else(|| {
        intent.target_ticks.map(|ticks| match intent.direction {
            Direction::Long => entry_price + ticks as f64 * tick_size,
            Direction::Short => entry_price - ticks as f64 * tick_size,
        })
    })
}

/// 报告先写同目录临时文件再原子替换，避免中断后留下半份 JSON。
fn write_json_atomically(path: &Path, report: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating output directory {}", parent.display()))?;
    }
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let mut bytes = serde_json::to_vec_pretty(report).context("serializing replay report")?;
    bytes.push(b'\n');
    std::fs::write(&temporary, bytes)
        .with_context(|| format!("writing temporary report {}", temporary.display()))?;
    std::fs::rename(&temporary, path).with_context(|| {
        format!(
            "publishing replay report {} -> {}",
            temporary.display(),
            path.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造仅用于纯统计测试的严格横盘观察记录。
    fn observation(
        symbol: &str,
        signal_time_ms: i64,
        exit_time_ms: i64,
        net_r: f64,
    ) -> StrictTradeObservation {
        StrictTradeObservation {
            symbol: symbol.to_owned(),
            direction: Direction::Long,
            signal_time_ms,
            entry_time_ms: signal_time_ms + CANDLE_INTERVAL_MS,
            exit_time_ms,
            net_r,
        }
    }

    #[test]
    fn metrics_use_net_r_and_real_exit_order() {
        let trades = vec![
            observation("BTC-USDT-SWAP", 0, 3, 2.0),
            observation("ETH-USDT-SWAP", 1, 4, -1.0),
            observation("BTC-USDT-SWAP", 2, 5, -0.5),
        ];

        let metrics = strict_visual_metrics(&trades);

        assert_eq!(metrics.trades, 3);
        assert!((metrics.net_r - 0.5).abs() < 1e-12);
        assert!((metrics.average_net_r - 1.0 / 6.0).abs() < 1e-12);
        assert!((metrics.profit_factor_r.unwrap() - 4.0 / 3.0).abs() < 1e-12);
        assert!((metrics.chronological_max_drawdown_r - 1.5).abs() < 1e-12);
    }

    #[test]
    fn event_clusters_separate_direction_and_link_adjacent_signals() {
        let mut trades = vec![
            observation("BTC-USDT-SWAP", 0, 1, 1.0),
            observation("ETH-USDT-SWAP", EVENT_CLUSTER_MS, 2, 1.0),
            observation("BTC-USDT-SWAP", EVENT_CLUSTER_MS * 2 + 1, 3, 1.0),
        ];
        trades[2].direction = Direction::Short;

        assert_eq!(direction_hour_event_clusters(&trades), 2);
    }

    #[test]
    fn args_accept_v40_visual_parent_execution_boundary() {
        let args = Args::parse(
            [
                "--symbols",
                "BTC-USDT-SWAP",
                "--evaluation-start-ms",
                "1",
                "--evaluation-end-exclusive-ms",
                "2",
                "--variant",
                "v40",
                "--output",
                "/tmp/v40.json",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .expect("V40 should be accepted by the fixed-window L2 runner");

        assert_eq!(
            args.variant,
            StrictVisualBreakoutResearchVariant::V40VisualParentExecutionBoundary
        );
    }

    #[test]
    fn args_accept_v43_balanced_parent_upgrade() {
        let args = Args::parse(
            [
                "--symbols",
                "BTC-USDT-SWAP",
                "--evaluation-start-ms",
                "1",
                "--evaluation-end-exclusive-ms",
                "2",
                "--variant",
                "v43",
                "--output",
                "/tmp/v43.json",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .expect("V43 should be accepted by the fixed-window L2 runner");

        assert_eq!(
            args.variant,
            StrictVisualBreakoutResearchVariant::V43BalancedParentUpgrade
        );
    }
}
