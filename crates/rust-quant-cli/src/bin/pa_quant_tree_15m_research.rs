use anyhow::{Context, Result};
use rust_quant_analytics::pa_quant_tree::{
    bootstrap_shared_market_mean, calculate_metrics, default_historical_cost_model,
    default_portfolio_risk_policy, replay_shared_portfolio, simulate_pa_abc_counterfactual,
    simulate_pa_candidate_history_with_funding, train_challenger, validate_temporal_purge,
    BootstrapConfig, BootstrapMeanEstimate, ChallengerTrainingResult, HistoricalFundingRatePoint,
    HistoricalPaSimulation, ModelFamily, PerformanceMetrics, PortfolioReplay, ResearchDataset,
    SharedMarketTimeBlock, WalkForwardPlan,
};
use rust_quant_common::{utils::function::sha256, CandleItem};
use rust_quant_infrastructure::repositories::ShardedExternalMarketSnapshotRepository;
use rust_quant_market::models::CandlesEntity;
use rust_quant_services::market::get_confirmed_candles_for_backtest;
use rust_quant_strategies::implementations::pa_quant_tree::{RuntimeManifest, RuntimeModel};
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use std::collections::BTreeMap;

#[path = "pa_quant_tree_15m_research/abc_report.rs"]
mod abc_report;
#[path = "pa_quant_tree_15m_research/oof.rs"]
mod oof;
#[path = "pa_quant_tree_15m_research/source_identity.rs"]
mod source_identity;

use oof::{selected_model_oof_diagnostic, SelectedModelOofDiagnostic};

const SAMPLE_LIMIT: usize = 50_000;
const BASELINE_TRAINING_PROTOCOL_VERSION: &str = "pa-training-v4-trend-quality-features";
const FOLLOWTHROUGH_TRAINING_PROTOCOL_VERSION: &str = "pa-training-v5-trend-followthrough";
const SELECTED_OOF_BASELINE_PROTOCOL_VERSION: &str = "pa-evaluation-v6-selected-oof-baseline";
const SELECTED_OOF_FOLLOWTHROUGH_PROTOCOL_VERSION: &str =
    "pa-evaluation-v6-selected-oof-followthrough";
const ABC_COUNTERFACTUAL_PROTOCOL_VERSION: &str = "pa-diagnostic-v7-abc-counterfactual";
const FUNDING_SOURCE: &str = "hyperliquid";
const FUNDING_METRIC_TYPE: &str = "funding_rate";
const SYMBOLS: [&str; 4] = [
    "BTC-USDT-SWAP",
    "ETH-USDT-SWAP",
    "SOL-USDT-SWAP",
    "BCH-USDT-SWAP",
];

/// 研究入口允许的候选家族；默认 baseline 保持 v4 输出兼容。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateFamily {
    /// v1 趋势与区间候选，用于复现冻结的 v4 证据。
    Baseline,
    /// v5 趋势 setup、下一棒确认、再下一棒开盘候选。
    Followthrough,
}

impl CandidateFamily {
    /// 返回该候选家族唯一的训练协议版本。
    fn training_protocol_version(self) -> &'static str {
        match self {
            Self::Baseline => BASELINE_TRAINING_PROTOCOL_VERSION,
            Self::Followthrough => FOLLOWTHROUGH_TRAINING_PROTOCOL_VERSION,
        }
    }
}

/// 研究统计协议；legacy 保持历史报告兼容，新协议必须显式启用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvaluationProtocol {
    /// 原 v4/v5 统计输出，不增加入选模型 OOF 聚合字段。
    Legacy,
    /// 对入选家族的折外保留路径生成独立聚合统计。
    SelectedOofV6,
    /// 唯一一次同 setup A/B/C 机制诊断，不创建新策略能力。
    AbcCounterfactualV7,
}

impl EvaluationProtocol {
    /// 将候选家族与评估方式绑定为唯一协议身份，避免新统计覆盖 legacy 证据。
    fn training_protocol_version(self, candidate_family: CandidateFamily) -> &'static str {
        match (self, candidate_family) {
            (Self::Legacy, family) => family.training_protocol_version(),
            (Self::SelectedOofV6, CandidateFamily::Baseline) => {
                SELECTED_OOF_BASELINE_PROTOCOL_VERSION
            }
            (Self::SelectedOofV6, CandidateFamily::Followthrough) => {
                SELECTED_OOF_FOLLOWTHROUGH_PROTOCOL_VERSION
            }
            (Self::AbcCounterfactualV7, _) => ABC_COUNTERFACTUAL_PROTOCOL_VERSION,
        }
    }
}

/// 只读研究命令参数，周期与候选家族共同隔离策略证据。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResearchArgs {
    /// 单批次唯一 K 线周期，仅允许 15m 或 1h。
    timeframe: String,
    /// 本批次候选事件合同。
    candidate_family: CandidateFamily,
    /// 统计输出协议；默认 legacy 以保持冻结报告可复现。
    evaluation_protocol: EvaluationProtocol,
}

/// 单个市场的训练期候选统计。
#[derive(Debug, Serialize)]
struct SymbolResearchSummary {
    /// 交易对。
    symbol: String,
    /// 公共窗口内已确认 K 线数量。
    candles: usize,
    /// 生成的结构候选数。
    candidates: usize,
    /// 完成成本后结算的候选数。
    settled: usize,
    /// 下一棒风险计划无效的候选数。
    invalid_risk_plans: usize,
    /// 样本末尾仍未退出的候选数。
    unresolved: usize,
    /// 单市场基础成本指标。
    metrics: PerformanceMetrics,
    /// 单市场两倍成本压力指标。
    double_cost_metrics: PerformanceMetrics,
}

/// 一个策略在四市场公共训练窗口上的只读报告。
#[derive(Debug, Serialize)]
struct StrategyResearchReport {
    /// 策略标识。
    strategy_key: String,
    /// 不可变研究版本。
    version: String,
    /// M0 运行时 manifest 哈希。
    manifest_hash: String,
    /// 每个市场的候选统计。
    symbols: Vec<SymbolResearchSummary>,
    /// 合并候选的基础成本指标；不等同于共享组合结果。
    pooled_metrics: PerformanceMetrics,
    /// 合并候选的两倍成本压力指标。
    pooled_double_cost_metrics: PerformanceMetrics,
    /// 同币种隔离和风险预算后的共享组合回放。
    portfolio: PortfolioReplay,
    /// 进入 ResearchDataset 的时间点一致样本数。
    research_observations: usize,
    /// 本次训练期采用的 purged walk-forward 配置。
    walk_forward_plan: WalkForwardPlan,
    /// M0-M4 的训练期验证分数和 one-standard-error 选择结果。
    model_tournament: ChallengerTrainingResult,
    /// 入选模型在全部训练数据重拟合后的压力诊断；不得当作 OOS。
    selected_model_training_diagnostic: SelectedModelTrainingDiagnostic,
    /// 入选家族的折外路径统计；legacy 协议省略该字段以保持历史 JSON 稳定。
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_model_oof_diagnostic: Option<SelectedModelOofDiagnostic>,
    /// 按自然日共享市场块执行的成本后平均 R bootstrap。
    bootstrap_mean: BootstrapMeanEstimate,
    /// 预注册成本/R 阈值的训练期诊断；只用于生成下一轮假设。
    cost_gate_diagnostics: Vec<CostGateDiagnostic>,
}

/// 一个最大往返成本 R 门槛的训练期诊断结果。
#[derive(Debug, Serialize)]
struct CostGateDiagnostic {
    /// 允许的最大往返成本，单位为 R。
    max_cost_r: f64,
    /// 门槛后保留的候选数。
    kept_trades: usize,
    /// 门槛后基础成本指标。
    metrics: PerformanceMetrics,
    /// 门槛后两倍成本压力指标。
    double_cost_metrics: PerformanceMetrics,
}

/// 入选模型在单市场完整训练样本上的过滤结果。
#[derive(Debug, Serialize)]
struct SelectedModelSymbolDiagnostic {
    /// Core 统一交易对标识。
    symbol: String,
    /// 入选模型保留的训练候选数。
    kept_trades: usize,
    /// 保留候选的基础成本指标。
    metrics: PerformanceMetrics,
    /// 保留候选的两倍成本指标。
    double_cost_metrics: PerformanceMetrics,
}

/// 入选模型使用全部训练数据重拟合后的诊断，不具备独立验证含义。
#[derive(Debug, Serialize)]
struct SelectedModelTrainingDiagnostic {
    /// one-standard-error 选择的模型家族。
    family: ModelFamily,
    /// false 明确阻止调用方把该诊断解释成 OOS。
    independently_validated: bool,
    /// 全训练样本上被重拟合模型保留的候选数。
    kept_trades: usize,
    /// 全训练样本过滤后的基础成本指标。
    metrics: PerformanceMetrics,
    /// 全训练样本过滤后的两倍成本指标。
    double_cost_metrics: PerformanceMetrics,
    /// 过滤后使用共享风险预算重放的组合结果。
    portfolio: PortfolioReplay,
    /// 分市场过滤诊断，用于识别单币种依赖。
    symbols: Vec<SelectedModelSymbolDiagnostic>,
}

/// 单市场代理资金费率覆盖摘要；必须覆盖公共小时桶且无缺口。
#[derive(Debug, Serialize)]
struct FundingCoverageSummary {
    /// Core 统一交易对标识。
    symbol: String,
    /// 公共窗口内的资金费率结算点数量。
    rows: usize,
    /// 最早结算时间，Unix 毫秒时间戳。
    first_ts: i64,
    /// 最晚结算时间，Unix 毫秒时间戳。
    last_ts: i64,
    /// 公共小时桶缺口数；正式研究要求为0。
    missing_hour_buckets: usize,
}

/// PA 训练期研究输出；该结构没有 OOS 或 Promote 状态。
#[derive(Debug, Serialize)]
struct PaResearchReport {
    /// 本批次唯一 K 线周期；15m 与 1h 不能混合。
    timeframe: String,
    /// 离线模型目标、阈值选择和验证门槛的不可变协议版本。
    training_protocol_version: String,
    /// true 表示报告只用于训练期探索，不能作为 OOS。
    training_only: bool,
    /// true 表示已计入资金费率；是否为真实交易所事实由 funding_cost_is_proxy 区分。
    funding_cost_included: bool,
    /// 资金费率来源；本批次固定使用 Hyperliquid。
    funding_cost_source: String,
    /// true 表示资金费率是跨交易所保守代理，不是 OKX 实际结算事实。
    funding_cost_is_proxy: bool,
    /// 代理资金费率的分市场连续性证据。
    funding_coverage: Vec<FundingCoverageSummary>,
    /// 始终为 false；本 CLI 不执行晋级。
    promotion_eligible: bool,
    /// 禁止晋级的明确原因。
    promotion_blocker: String,
    /// K 线与资金费率代理联合生成的训练数据指纹。
    dataset_fingerprint: String,
    /// 仅 K 线序列的数据指纹。
    candle_dataset_fingerprint: String,
    /// 仅资金费率代理序列的数据指纹。
    funding_dataset_fingerprint: String,
    /// 四市场公共窗口起点，Unix 毫秒时间戳。
    common_start_ts: i64,
    /// 四市场公共窗口终点，Unix 毫秒时间戳。
    common_end_ts: i64,
    /// 各策略的研究报告。
    strategies: Vec<StrategyResearchReport>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();
    let args = parse_research_args(std::env::args().skip(1))?;
    require_quant_core_database_url()?;
    let timeframe = args.timeframe;
    let candidate_family = args.candidate_family;
    let evaluation_protocol = args.evaluation_protocol;
    let training_protocol_version = evaluation_protocol.training_protocol_version(candidate_family);

    let mut loaded = Vec::with_capacity(SYMBOLS.len());
    for symbol in SYMBOLS {
        loaded.push((symbol, load_candles(symbol, &timeframe).await?));
    }
    let common_start_ts = loaded
        .iter()
        .filter_map(|(_, candles)| candles.first().map(|candle| candle.ts))
        .max()
        .context("PA research cannot determine common start")?;
    let common_end_ts = loaded
        .iter()
        .filter_map(|(_, candles)| candles.last().map(|candle| candle.ts))
        .min()
        .context("PA research cannot determine common end")?;
    let common: Vec<_> = loaded
        .into_iter()
        .map(|(symbol, candles)| {
            let selected = candles
                .into_iter()
                .filter(|candle| candle.ts >= common_start_ts && candle.ts <= common_end_ts)
                .collect::<Vec<_>>();
            anyhow::ensure!(
                selected.len() >= 1_000,
                "{symbol} has fewer than 1000 common-window candles"
            );
            Ok((symbol, selected))
        })
        .collect::<Result<_>>()?;
    let candle_dataset_fingerprint = candle_dataset_fingerprint(&common)?;
    let (funding_rates, funding_coverage) =
        load_funding_proxy(&common, common_start_ts, common_end_ts).await?;
    let funding_dataset_fingerprint = funding_dataset_fingerprint(&funding_rates)?;
    let dataset_fingerprint = combined_dataset_fingerprint(
        training_protocol_version,
        &candle_dataset_fingerprint,
        &funding_dataset_fingerprint,
    );

    if evaluation_protocol == EvaluationProtocol::AbcCounterfactualV7 {
        let simulations = common
            .iter()
            .map(|(symbol, candles)| {
                let symbol_funding = funding_rates
                    .get(*symbol)
                    .with_context(|| format!("missing funding proxy for {symbol}"))?;
                simulate_pa_abc_counterfactual(
                    symbol,
                    candles,
                    &default_historical_cost_model(),
                    symbol_funding,
                )
                .map_err(anyhow::Error::msg)
            })
            .collect::<Result<Vec<_>>>()?;
        let repo_root = source_identity::discover_repo_root(&std::env::current_dir()?)?;
        let source_identity = source_identity::detect_source_identity(&repo_root)?;
        let report = abc_report::build_abc_diagnostic_report(
            source_identity,
            dataset_fingerprint,
            candle_dataset_fingerprint,
            funding_dataset_fingerprint,
            common_start_ts,
            common_end_ts,
            &simulations,
        )?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    let mut strategies = Vec::new();
    for strategy_key in strategy_keys(&timeframe, candidate_family)? {
        strategies.push(run_strategy(
            strategy_key,
            &dataset_fingerprint,
            &common,
            &funding_rates,
            evaluation_protocol,
        )?);
    }
    let report = PaResearchReport {
        timeframe,
        training_protocol_version: training_protocol_version.to_owned(),
        training_only: true,
        funding_cost_included: true,
        funding_cost_source: FUNDING_SOURCE.to_owned(),
        funding_cost_is_proxy: true,
        funding_coverage,
        promotion_eligible: false,
        promotion_blocker: "funding_cost_proxy_and_sealed_oos_not_opened".to_owned(),
        dataset_fingerprint,
        candle_dataset_fingerprint,
        funding_dataset_fingerprint,
        common_start_ts,
        common_end_ts,
        strategies,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// 仅允许计划内周期和候选家族，缺省值保持既有 v4 命令兼容。
fn parse_research_args<I, S>(args: I) -> Result<ResearchArgs>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut timeframe = "15m".to_owned();
    let mut candidate_family = CandidateFamily::Baseline;
    let mut evaluation_protocol = EvaluationProtocol::Legacy;
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--timeframe" => {
                timeframe = args
                    .next()
                    .context("--timeframe requires 15m or 1h")?
                    .to_ascii_lowercase();
            }
            other if other.starts_with("--timeframe=") => {
                timeframe = other
                    .split_once('=')
                    .map(|(_, value)| value.to_ascii_lowercase())
                    .unwrap_or_default();
            }
            "--candidate-family" => {
                candidate_family = parse_candidate_family(
                    &args
                        .next()
                        .context("--candidate-family requires baseline or followthrough")?,
                )?;
            }
            other if other.starts_with("--candidate-family=") => {
                candidate_family = parse_candidate_family(
                    other
                        .split_once('=')
                        .map(|(_, value)| value)
                        .unwrap_or_default(),
                )?;
            }
            "--evaluation-protocol" => {
                evaluation_protocol = parse_evaluation_protocol(
                    &args
                        .next()
                    .context(
                        "--evaluation-protocol requires legacy, selected-oof-v6 or abc-counterfactual-v7",
                    )?,
                )?;
            }
            other if other.starts_with("--evaluation-protocol=") => {
                evaluation_protocol = parse_evaluation_protocol(
                    other
                        .split_once('=')
                        .map(|(_, value)| value)
                        .unwrap_or_default(),
                )?;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: pa_quant_tree_15m_research [--timeframe 15m|1h] \
                     [--candidate-family baseline|followthrough] \
                     [--evaluation-protocol legacy|selected-oof-v6|abc-counterfactual-v7]"
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }
    if !matches!(timeframe.as_str(), "15m" | "1h") {
        anyhow::bail!("PA research supports only 15m or 1h");
    }
    if evaluation_protocol == EvaluationProtocol::AbcCounterfactualV7
        && (timeframe != "15m" || candidate_family != CandidateFamily::Baseline)
    {
        anyhow::bail!("A/B/C counterfactual is limited to the 15m baseline trend setup");
    }
    Ok(ResearchArgs {
        timeframe,
        candidate_family,
        evaluation_protocol,
    })
}

/// 解析显式评估协议；未知值不得静默回退到 legacy。
fn parse_evaluation_protocol(value: &str) -> Result<EvaluationProtocol> {
    match value.to_ascii_lowercase().as_str() {
        "legacy" => Ok(EvaluationProtocol::Legacy),
        "selected-oof-v6" => Ok(EvaluationProtocol::SelectedOofV6),
        "abc-counterfactual-v7" => Ok(EvaluationProtocol::AbcCounterfactualV7),
        _ => anyhow::bail!(
            "PA research supports only legacy, selected-oof-v6 or abc-counterfactual-v7 evaluation"
        ),
    }
}

/// 将外部候选家族参数解析为冻结枚举，拒绝隐式新能力。
fn parse_candidate_family(value: &str) -> Result<CandidateFamily> {
    match value.to_ascii_lowercase().as_str() {
        "baseline" => Ok(CandidateFamily::Baseline),
        "followthrough" => Ok(CandidateFamily::Followthrough),
        _ => anyhow::bail!("PA research supports only baseline or followthrough candidate family"),
    }
}

/// 将研究周期映射到互相隔离的策略标识，避免 15m 与 1h 证据混用。
fn strategy_keys(timeframe: &str, candidate_family: CandidateFamily) -> Result<Vec<&'static str>> {
    match (timeframe, candidate_family) {
        ("15m", CandidateFamily::Baseline) => Ok(vec!["pa_trend_15m", "pa_range_15m"]),
        ("1h", CandidateFamily::Baseline) => Ok(vec!["pa_trend_1h", "pa_range_1h"]),
        ("15m", CandidateFamily::Followthrough) => Ok(vec!["pa_trend_followthrough_15m"]),
        ("1h", CandidateFamily::Followthrough) => Ok(vec!["pa_trend_followthrough_1h"]),
        _ => anyhow::bail!("PA research supports only 15m or 1h"),
    }
}

/// 仅允许 Core 专用变量，避免误连 quant_web 的通用 DATABASE_URL。
fn require_quant_core_database_url() -> Result<()> {
    quant_core_database_url().map(|_| ())
}

/// 返回 Core 专用连接串，不允许回退到可能指向 quant_web 的 DATABASE_URL。
fn quant_core_database_url() -> Result<String> {
    std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("pa_quant_tree_15m_research requires QUANT_CORE_DATABASE_URL")
}

/// 在同一公共 K 线窗口内执行单一策略研究，确保策略版本与数据指纹绑定。
fn run_strategy(
    strategy_key: &str,
    dataset_fingerprint: &str,
    common: &[(&str, Vec<CandleItem>)],
    funding_rates: &BTreeMap<String, Vec<HistoricalFundingRatePoint>>,
    evaluation_protocol: EvaluationProtocol,
) -> Result<StrategyResearchReport> {
    let manifest = RuntimeManifest {
        strategy_key: strategy_key.to_owned(),
        version: "1.0.0-research".to_owned(),
        feature_registry_version: "pa-feature-registry-v2".to_owned(),
        dataset_fingerprint: dataset_fingerprint.to_owned(),
        code_revision: option_env!("GIT_COMMIT_SHA")
            .unwrap_or("working-tree")
            .to_owned(),
        model: RuntimeModel::FixedRules { rules: vec![] },
    };
    let cost_model = default_historical_cost_model();
    let simulations = common
        .iter()
        .map(|(symbol, candles)| {
            let symbol_funding = funding_rates
                .get(*symbol)
                .with_context(|| format!("missing funding proxy for {symbol}"))?;
            simulate_pa_candidate_history_with_funding(
                symbol,
                candles,
                &manifest,
                &cost_model,
                symbol_funding,
            )
            .map_err(anyhow::Error::msg)
        })
        .collect::<Result<Vec<_>>>()?;
    let observations: Vec<_> = simulations
        .iter()
        .flat_map(|simulation| {
            simulation
                .trades
                .iter()
                .map(|trade| trade.observation.clone())
        })
        .collect();
    anyhow::ensure!(
        observations.len() >= 100,
        "{strategy_key} has only {} settled candidates; at least 100 are required",
        observations.len()
    );
    let research_dataset = ResearchDataset::new(observations).map_err(anyhow::Error::msg)?;
    let walk_forward_plan = training_walk_forward_plan(&research_dataset)?;
    let model_tournament =
        train_challenger(&research_dataset, &walk_forward_plan).map_err(anyhow::Error::msg)?;
    let selected_model_training_diagnostic =
        selected_model_training_diagnostic(&simulations, &model_tournament)?;
    let selected_model_oof_diagnostic = match evaluation_protocol {
        EvaluationProtocol::Legacy => None,
        EvaluationProtocol::SelectedOofV6 => Some(selected_model_oof_diagnostic(
            &simulations,
            &model_tournament,
        )?),
        EvaluationProtocol::AbcCounterfactualV7 => {
            anyhow::bail!("A/B/C diagnostics must use the dedicated counterfactual report path")
        }
    };
    let bootstrap_mean = bootstrap_shared_market_mean(
        &shared_daily_blocks(&research_dataset),
        &BootstrapConfig {
            block_size: 7,
            resamples: 1_000,
            seed: 20_260_715,
        },
    )
    .map_err(anyhow::Error::msg)?;
    let portfolio_inputs = simulations
        .iter()
        .flat_map(|simulation| {
            simulation
                .trades
                .iter()
                .map(|trade| trade.portfolio_trade.clone())
        })
        .collect::<Vec<_>>();
    let portfolio = replay_shared_portfolio(&portfolio_inputs, &default_portfolio_risk_policy())
        .map_err(anyhow::Error::msg)?;
    let pooled_net_r = simulations
        .iter()
        .flat_map(|simulation| {
            simulation
                .trades
                .iter()
                .map(|trade| trade.observation.net_r)
        })
        .collect::<Vec<_>>();
    let pooled_double_cost = simulations
        .iter()
        .flat_map(|simulation| {
            simulation
                .trades
                .iter()
                .map(|trade| trade.double_cost_net_r)
        })
        .collect::<Vec<_>>();
    let symbols = simulations.iter().map(symbol_summary).collect();
    let cost_gate_diagnostics = cost_gate_diagnostics(&simulations);
    Ok(StrategyResearchReport {
        strategy_key: strategy_key.to_owned(),
        version: manifest.version.clone(),
        manifest_hash: manifest.manifest_hash().map_err(anyhow::Error::msg)?,
        symbols,
        pooled_metrics: calculate_metrics(&pooled_net_r),
        pooled_double_cost_metrics: calculate_metrics(&pooled_double_cost),
        portfolio,
        research_observations: research_dataset.observations.len(),
        walk_forward_plan,
        model_tournament,
        selected_model_training_diagnostic,
        selected_model_oof_diagnostic,
        bootstrap_mean,
        cost_gate_diagnostics,
    })
}

/// 计算入选模型在全训练数据重拟合后的成本与组合压力诊断，结果仅用于失败筛查。
fn selected_model_training_diagnostic(
    simulations: &[HistoricalPaSimulation],
    tournament: &ChallengerTrainingResult,
) -> Result<SelectedModelTrainingDiagnostic> {
    let selected = tournament
        .entries
        .get(tournament.selected_index)
        .context("selected model index is invalid")?;
    let mut all_base = Vec::new();
    let mut all_double = Vec::new();
    let mut portfolio_inputs = Vec::new();
    let mut symbols = Vec::with_capacity(simulations.len());
    for simulation in simulations {
        let kept = simulation
            .trades
            .iter()
            .filter(|trade| selected.model.evaluate(&trade.observation.features).keep)
            .collect::<Vec<_>>();
        let base = kept
            .iter()
            .map(|trade| trade.observation.net_r)
            .collect::<Vec<_>>();
        let double = kept
            .iter()
            .map(|trade| trade.double_cost_net_r)
            .collect::<Vec<_>>();
        all_base.extend(base.iter().copied());
        all_double.extend(double.iter().copied());
        portfolio_inputs.extend(kept.iter().map(|trade| trade.portfolio_trade.clone()));
        symbols.push(SelectedModelSymbolDiagnostic {
            symbol: simulation.symbol.clone(),
            kept_trades: kept.len(),
            metrics: calculate_metrics(&base),
            double_cost_metrics: calculate_metrics(&double),
        });
    }
    let portfolio = replay_shared_portfolio(&portfolio_inputs, &default_portfolio_risk_policy())
        .map_err(anyhow::Error::msg)?;
    Ok(SelectedModelTrainingDiagnostic {
        family: selected.family,
        independently_validated: false,
        kept_trades: all_base.len(),
        metrics: calculate_metrics(&all_base),
        double_cost_metrics: calculate_metrics(&all_double),
        portfolio,
        symbols,
    })
}

/// 扫描固定成本/R 门槛并保留全部结果，避免只汇报最优阈值。
fn cost_gate_diagnostics(simulations: &[HistoricalPaSimulation]) -> Vec<CostGateDiagnostic> {
    [0.10, 0.20, 0.30, 0.50]
        .into_iter()
        .map(|max_cost_r| {
            let kept = simulations
                .iter()
                .flat_map(|simulation| simulation.trades.iter())
                .filter(|trade| trade.gross_r - trade.observation.net_r <= max_cost_r)
                .collect::<Vec<_>>();
            CostGateDiagnostic {
                max_cost_r,
                kept_trades: kept.len(),
                metrics: calculate_metrics(
                    &kept
                        .iter()
                        .map(|trade| trade.observation.net_r)
                        .collect::<Vec<_>>(),
                ),
                double_cost_metrics: calculate_metrics(
                    &kept
                        .iter()
                        .map(|trade| trade.double_cost_net_r)
                        .collect::<Vec<_>>(),
                ),
            }
        })
        .collect()
}

/// 同一自然日内的跨币种候选整体进入一个 block，避免低估市场共振风险。
fn shared_daily_blocks(dataset: &ResearchDataset) -> Vec<SharedMarketTimeBlock> {
    const DAY_MS: i64 = 86_400_000;
    let mut days = BTreeMap::<i64, Vec<f64>>::new();
    for observation in &dataset.observations {
        let day_start = observation.signal_ts.div_euclid(DAY_MS) * DAY_MS;
        days.entry(day_start).or_default().push(observation.net_r);
    }
    days.into_iter()
        .map(|(start_ts, net_r)| SharedMarketTimeBlock { start_ts, net_r })
        .collect()
}

/// 训练期按样本比例构建固定切分，不查看任何验证结果后调整窗口。
fn training_walk_forward_plan(dataset: &ResearchDataset) -> Result<WalkForwardPlan> {
    let observations = dataset.observations.len();
    anyhow::ensure!(
        observations >= 100,
        "at least 100 settled candidates are required"
    );
    let min_train_size = (observations / 2).max(60);
    let validation_size = (observations / 10).max(30);
    // purge 只根据候选持仓期限扩大，不读取 R 标签或验证指标。
    for purge_size in (observations / 20).max(10)..=(observations / 3) {
        let plan = WalkForwardPlan {
            min_train_size,
            validation_size,
            purge_size,
            max_windows: 5,
        };
        let windows = plan.build(dataset).map_err(anyhow::Error::msg)?;
        if !windows.is_empty()
            && windows
                .iter()
                .all(|window| validate_temporal_purge(dataset, window).is_ok())
        {
            return Ok(plan);
        }
    }
    anyhow::bail!("no leakage-free walk-forward window remains after temporal purge")
}

/// 将单市场结算结果压缩为报告指标，保留无效风险计划与未结算数量。
fn symbol_summary(simulation: &HistoricalPaSimulation) -> SymbolResearchSummary {
    let net_r = simulation
        .trades
        .iter()
        .map(|trade| trade.observation.net_r)
        .collect::<Vec<_>>();
    let double_cost = simulation
        .trades
        .iter()
        .map(|trade| trade.double_cost_net_r)
        .collect::<Vec<_>>();
    SymbolResearchSummary {
        symbol: simulation.symbol.clone(),
        candles: simulation.candle_count,
        candidates: simulation.candidate_count,
        settled: simulation.trades.len(),
        invalid_risk_plans: simulation.invalid_risk_plan_count,
        unresolved: simulation.unresolved_count,
        metrics: calculate_metrics(&net_r),
        double_cost_metrics: calculate_metrics(&double_cost),
    }
}

/// 从分源 Core 快照加载连续的 Hyperliquid 小时费率，作为保守跨交易所成本代理。
async fn load_funding_proxy(
    common: &[(&str, Vec<CandleItem>)],
    start_ts: i64,
    end_ts: i64,
) -> Result<(
    BTreeMap<String, Vec<HistoricalFundingRatePoint>>,
    Vec<FundingCoverageSummary>,
)> {
    const HOUR_MS: i64 = 3_600_000;
    let database_url = quant_core_database_url()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core for PA funding proxy")?;
    let repository = ShardedExternalMarketSnapshotRepository::new(pool);
    let start_hour = start_ts.div_euclid(HOUR_MS);
    let end_hour = end_ts.div_euclid(HOUR_MS);
    let mut by_symbol = BTreeMap::new();
    let mut coverage = Vec::with_capacity(common.len());

    for (symbol, _) in common {
        let rows = repository
            .find_range_existing(
                FUNDING_SOURCE,
                symbol,
                FUNDING_METRIC_TYPE,
                start_ts,
                end_ts.saturating_add(999),
                Some(20_000),
            )
            .await
            .with_context(|| format!("load funding proxy failed: {symbol}"))?;
        let mut hourly = BTreeMap::<i64, HistoricalFundingRatePoint>::new();
        for row in rows {
            let rate = row
                .funding_rate
                .with_context(|| format!("funding proxy is missing rate: {symbol}"))?;
            let hour = row.metric_time.div_euclid(HOUR_MS);
            anyhow::ensure!(
                hourly
                    .insert(
                        hour,
                        HistoricalFundingRatePoint {
                            funding_time: row.metric_time,
                            rate,
                        },
                    )
                    .is_none(),
                "funding proxy has duplicate hour bucket: {symbol} {hour}"
            );
        }
        let missing_hour_buckets = (start_hour..=end_hour)
            .filter(|hour| !hourly.contains_key(hour))
            .count();
        anyhow::ensure!(
            missing_hour_buckets == 0,
            "funding proxy has {missing_hour_buckets} missing hour buckets: {symbol}"
        );
        let points = hourly.into_values().collect::<Vec<_>>();
        let first_ts = points
            .first()
            .map(|point| point.funding_time)
            .context("funding proxy has no first timestamp")?;
        let last_ts = points
            .last()
            .map(|point| point.funding_time)
            .context("funding proxy has no last timestamp")?;
        coverage.push(FundingCoverageSummary {
            symbol: (*symbol).to_owned(),
            rows: points.len(),
            first_ts,
            last_ts,
            missing_hour_buckets,
        });
        by_symbol.insert((*symbol).to_owned(), points);
    }
    Ok((by_symbol, coverage))
}

/// 从 Core 事实库读取指定周期的确认 K 线，并按时间去重后返回。
async fn load_candles(symbol: &str, period: &str) -> Result<Vec<CandleItem>> {
    let entities = get_confirmed_candles_for_backtest(symbol, period, SAMPLE_LIMIT, None)
        .await
        .with_context(|| format!("load confirmed candles failed: {symbol} {period}"))?;
    let mut candles = entities
        .iter()
        .map(|entity| candle_entity_to_item(entity, symbol, period))
        .collect::<Result<Vec<_>>>()?;
    candles.sort_unstable_by_key(|candle| candle.ts);
    candles.dedup_by_key(|candle| candle.ts);
    Ok(candles)
}

/// 将数据库字符串字段转换为研究用 K 线，解析错误保留市场、周期和时间证据。
fn candle_entity_to_item(entity: &CandlesEntity, symbol: &str, period: &str) -> Result<CandleItem> {
    Ok(CandleItem {
        ts: entity.ts,
        o: parse_number(&entity.o, "open", entity.ts, symbol, period)?,
        h: parse_number(&entity.h, "high", entity.ts, symbol, period)?,
        l: parse_number(&entity.l, "low", entity.ts, symbol, period)?,
        c: parse_number(&entity.c, "close", entity.ts, symbol, period)?,
        v: parse_number(&entity.vol_ccy, "volume", entity.ts, symbol, period)?,
        confirm: entity.confirm.parse::<i32>().unwrap_or(0),
    })
}

/// 解析单个行情数值，并在失败时保留可定位原始记录的上下文。
fn parse_number(value: &str, field: &str, ts: i64, symbol: &str, period: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .with_context(|| format!("invalid {field}: symbol={symbol} period={period} ts={ts}"))
}

/// 对按市场和时间排序后的实际 K 线序列生成稳定指纹，隔离不同研究批次。
fn candle_dataset_fingerprint(common: &[(&str, Vec<CandleItem>)]) -> Result<String> {
    let canonical = serde_json::to_string(common)?;
    Ok(format!("sha256:{}", sha256(&canonical)))
}

/// 对分市场、按小时排序后的资金费率代理序列生成稳定指纹。
fn funding_dataset_fingerprint(
    funding_rates: &BTreeMap<String, Vec<HistoricalFundingRatePoint>>,
) -> Result<String> {
    let canonical = serde_json::to_string(funding_rates)?;
    Ok(format!("sha256:{}", sha256(&canonical)))
}

/// 将 K 线、资金费率和训练协议绑定为联合数据指纹，防止成本标签变化后沿用旧证据。
fn combined_dataset_fingerprint(
    training_protocol_version: &str,
    candle_fingerprint: &str,
    funding_fingerprint: &str,
) -> String {
    let canonical =
        format!("{training_protocol_version}|{candle_fingerprint}|{funding_fingerprint}");
    format!("sha256:{}", sha256(&canonical))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeframe_argument_selects_independent_strategy_keys() {
        assert_eq!(
            parse_research_args(Vec::<String>::new()).unwrap(),
            ResearchArgs {
                timeframe: "15m".to_owned(),
                candidate_family: CandidateFamily::Baseline,
                evaluation_protocol: EvaluationProtocol::Legacy,
            }
        );
        assert_eq!(
            parse_research_args(["--timeframe", "1H"])
                .unwrap()
                .timeframe,
            "1h"
        );
        assert_eq!(
            strategy_keys("1h", CandidateFamily::Baseline).unwrap(),
            ["pa_trend_1h", "pa_range_1h"]
        );
        assert!(parse_research_args(["--timeframe", "4h"]).is_err());
    }

    #[test]
    fn followthrough_argument_selects_only_new_strategy_key_and_protocol() {
        let args =
            parse_research_args(["--timeframe=15m", "--candidate-family=followthrough"]).unwrap();

        assert_eq!(args.candidate_family, CandidateFamily::Followthrough);
        assert_eq!(
            args.candidate_family.training_protocol_version(),
            FOLLOWTHROUGH_TRAINING_PROTOCOL_VERSION
        );
        assert_eq!(
            strategy_keys(&args.timeframe, args.candidate_family).unwrap(),
            ["pa_trend_followthrough_15m"]
        );
    }

    #[test]
    fn selected_oof_protocol_requires_explicit_argument_and_new_identity() {
        let legacy = parse_research_args(Vec::<String>::new()).unwrap();
        let corrected = parse_research_args(["--evaluation-protocol=selected-oof-v6"]).unwrap();

        assert_eq!(legacy.evaluation_protocol, EvaluationProtocol::Legacy);
        assert_eq!(
            corrected.evaluation_protocol,
            EvaluationProtocol::SelectedOofV6
        );
        assert_eq!(
            corrected
                .evaluation_protocol
                .training_protocol_version(corrected.candidate_family),
            SELECTED_OOF_BASELINE_PROTOCOL_VERSION
        );
    }

    #[test]
    fn abc_protocol_is_explicit_and_limited_to_15m_baseline_trend_setup() {
        let legacy = parse_research_args(Vec::<String>::new()).unwrap();
        let abc = parse_research_args(["--evaluation-protocol=abc-counterfactual-v7"]).unwrap();

        assert_eq!(legacy.evaluation_protocol, EvaluationProtocol::Legacy);
        assert_eq!(
            abc.evaluation_protocol,
            EvaluationProtocol::AbcCounterfactualV7
        );
        assert_eq!(abc.timeframe, "15m");
        assert_eq!(abc.candidate_family, CandidateFamily::Baseline);
        assert!(parse_research_args([
            "--timeframe=1h",
            "--evaluation-protocol=abc-counterfactual-v7"
        ])
        .is_err());
        assert!(parse_research_args([
            "--candidate-family=followthrough",
            "--evaluation-protocol=abc-counterfactual-v7"
        ])
        .is_err());
    }
}
