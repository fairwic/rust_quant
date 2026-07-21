mod report;

use self::report::{build_report, print_report};
use crate::app::okx_historical_universe::HistoricalUniverseManifest;
use anyhow::{bail, Context, Result};
use rust_quant_strategies::CandleItem;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const MS_15M: i64 = 15 * 60 * 1_000;
const MS_30M: i64 = 30 * 60 * 1_000;
const MS_8H: i64 = 8 * 60 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const BETA_BARS: usize = 7 * 24 * 4;
const RESIDUAL_24H_BARS: usize = 24 * 4;
const RESIDUAL_6H_BARS: usize = 6 * 4;
const ATR_PERIOD: usize = 14;
const ATR_STOP_MULTIPLIER: f64 = 2.0;
const MIN_SCORE: f64 = 1.5;
const MIN_FACTOR_COVERAGE: f64 = 0.80;
const MIN_RISK_PCT: f64 = 0.75;
const MAX_RISK_PCT: f64 = 5.0;
const MOMENTUM_TARGET_R: f64 = 3.0;
const MOMENTUM_MAX_HOLDING_BARS: usize = 48 * 4;
const MOMENTUM_MAX_CONCURRENT: usize = 6;
const MOMENTUM_MAX_SAME_DIRECTION: usize = 4;
const REVERSION_TARGET_R: f64 = 2.0;
const REVERSION_MAX_HOLDING_BARS: usize = 24 * 4;
const REVERSION_MAX_CONCURRENT: usize = 4;
const REVERSION_MAX_SAME_DIRECTION: usize = 3;
const EXECUTION_COST_PER_SIDE: f64 = 0.0008;
const ADVERSE_FUNDING_PER_8H: f64 = 0.0001;
const BENCHMARK: &str = "BTC-USDT-SWAP";

/// 冻结研究只接受历史币池路径，不暴露策略阈值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidualMomentumArgs {
    /// 已通过 point-in-time 完整性审计的当前 live 币池 manifest。
    pub manifest: PathBuf,
}

/// 记录从决策时点到实际成交的因果漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResidualMomentumStageCounts {
    /// 符合 UTC 8 小时节奏且位于币池窗口内的决策时点数。
    pub decision_points: usize,
    /// 因 BTC 或成员完整覆盖不足而整体跳过的决策时点数。
    pub coverage_blocked: usize,
    /// 在所有未阻塞时点成功计算出的币种因子观察数。
    pub factor_observations: usize,
    /// 6h 与 24h 特质收益方向一致的观察数。
    pub persistence_pass: usize,
    /// 6h 与 24h 特质收益已经严格异号的观察数。
    pub reversion_pass: usize,
    /// 同时达到冻结绝对分数门槛的观察数。
    pub score_pass: usize,
    /// 每个决策时点完成确定性 Top1 排序后的候选数。
    pub selected_candidates: usize,
    /// 初始止损距离或入场价格不满足冻结风险合同的候选数。
    pub risk_blocked: usize,
    /// 被同币、总并发或同方向上限阻塞的候选数。
    pub capacity_blocked: usize,
    /// 缺少下一根入场或完整 48h 结算数据的候选数。
    pub incomplete_outcomes: usize,
}

/// 单笔交易保存信号时点因子、冻结初始风险与成本证据。
#[derive(Debug, Clone, PartialEq)]
pub struct ResidualMomentumTrade {
    /// OKX USDT 永续标准标识。
    pub symbol: String,
    /// `long` 跟随正残差，`short` 跟随负残差。
    pub direction: &'static str,
    /// 因子决策时间，Unix 毫秒；此时对应 15m 已完成。
    pub decision_ts: i64,
    /// 下一根 15m 开盘时间，Unix 毫秒。
    pub entry_ts: i64,
    /// 实际退出时间，Unix 毫秒。
    pub exit_ts: i64,
    /// 7 日同步收益 OLS 的 BTC Beta。
    pub beta: f64,
    /// 剔除 alpha 与 BTC Beta 后的 24h 对数特质收益。
    pub residual_24h: f64,
    /// 剔除 alpha 与 BTC Beta 后的 6h 对数特质收益。
    pub residual_6h: f64,
    /// 24h 特质收益除以 7 日估计的日特质波动。
    pub score: f64,
    /// 下一根开盘入场价格。
    pub entry_price: f64,
    /// 入场时冻结且后续不得回写的结构无关 ATR 止损价。
    pub initial_stop: f64,
    /// 固定 3R 目标价。
    pub target_price: f64,
    /// 实际退出价格。
    pub exit_price: f64,
    /// 未扣交易成本的已实现收益，按冻结初始风险归一化。
    pub gross_r: f64,
    /// 标准手续费、滑点与不利资金成本，按初始风险归一化。
    pub cost_r: f64,
    /// 标准成本后的已实现净收益 R。
    pub net_r: f64,
    /// 双倍成本后的已实现净收益 R。
    pub double_cost_net_r: f64,
    /// `stop`、`target` 或 `timeout`。
    pub exit_reason: &'static str,
}

/// 交易级固定 R 汇总；组合逐时权益只有交易级门槛通过后才计算。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResidualMomentumMetrics {
    /// 纳入当前窗口的实际成交数。
    pub trades: usize,
    /// 当前成本口径下累计净 R。
    pub net_sum_r: f64,
    /// 当前成本口径下平均每笔净 R；无交易时为空。
    pub net_expectancy_r: Option<f64>,
    /// 正收益总额除以负收益绝对总额；无负收益时为空。
    pub profit_factor: Option<f64>,
    /// 净收益大于零的交易占比，单位百分比。
    pub win_rate_pct: Option<f64>,
    /// 交易收益样本 Sharpe，仅用于早期筛选，不冒充日权益 Sharpe。
    pub trade_sharpe: Option<f64>,
    /// 按成交顺序累计 R 曲线的最大回撤，单位 R。
    pub max_drawdown_r: f64,
    /// 累计净 R 除以最大回撤 R；无回撤时为空。
    pub recovery_factor: Option<f64>,
}

/// 冻结 V1 的数据覆盖、稳定性、成本和集中度报告。
#[derive(Debug, Clone, PartialEq)]
pub struct ResidualMomentumReport {
    /// 不可变入场规则身份。
    pub rule_version: String,
    /// 历史币池 manifest 身份。
    pub universe_version: String,
    /// 实际加载的 BTC 加成员去重币种数。
    pub symbols: usize,
    /// 因果候选漏斗。
    pub stages: ResidualMomentumStageCounts,
    /// 30 分钟聚类后的有效事件数。
    pub effective_events: usize,
    /// 不含任何成本的反事实指标。
    pub gross_zero_cost: ResidualMomentumMetrics,
    /// 冻结标准成本指标。
    pub overall: ResidualMomentumMetrics,
    /// 冻结双倍成本压力指标。
    pub double_cost: ResidualMomentumMetrics,
    /// 多头标准成本指标。
    pub long: ResidualMomentumMetrics,
    /// 空头标准成本指标。
    pub short: ResidualMomentumMetrics,
    /// 每个币池自然月的标准成本指标。
    pub monthly: Vec<(i64, ResidualMomentumMetrics)>,
    /// 标准成本累计净 R 为正的月份数。
    pub positive_months: usize,
    /// 标准成本净贡献最高的三个盈利币种。
    pub top_three_positive_symbols: Vec<String>,
    /// 移除前三盈利币种后的标准成本累计净 R。
    pub net_r_without_top_three_symbols: f64,
    /// 按退出原因计数。
    pub exit_reasons: BTreeMap<String, usize>,
    /// 是否通过预注册 Discovery 早停条件；不等于职业晋级。
    pub discovery_gate_passed: bool,
    /// 可逐笔审计的实际成交。
    pub trades: Vec<ResidualMomentumTrade>,
}

/// 单个历史币池生效窗口及其 current-live 成员。
#[derive(Debug, Clone, PartialEq, Eq)]
struct UniverseWindow {
    /// 窗口起点，Unix 毫秒且包含。
    from_ms: i64,
    /// 窗口终点，Unix 毫秒且不包含。
    to_ms: i64,
    /// 本月可参与因子横截面的标准合约标识。
    members: BTreeSet<String>,
}

/// 已按时间排序且月份连续的历史币池日程。
#[derive(Debug, Clone, PartialEq, Eq)]
struct UniverseSchedule {
    /// manifest 中冻结的币池版本。
    version: String,
    /// 连续且互不重叠的月度窗口。
    windows: Vec<UniverseWindow>,
}

/// 单个币种完整且已确认的 15m 序列。
#[derive(Debug, Clone)]
struct SymbolSeries {
    /// 按开盘时间严格升序的 K 线。
    candles: Vec<CandleItem>,
}

/// 信号时点可见的 BTC Beta 残差状态。
#[derive(Debug, Clone, Copy, PartialEq)]
struct FactorSnapshot {
    /// 7 日同步收益估计的 BTC Beta。
    beta: f64,
    /// 最近 24h 特质收益。
    residual_24h: f64,
    /// 最近 6h 特质收益。
    residual_6h: f64,
    /// 标准化后的 24h 特质趋势强度。
    score: f64,
    /// 决策 K 线结束时的 ATR(14)。
    atr: f64,
}

/// 每个 8 小时决策时点按绝对分数排序后的唯一候选。
#[derive(Debug, Clone, PartialEq)]
struct Candidate {
    /// 候选合约标识。
    symbol: String,
    /// 已完成决策 K 线的结束时间，Unix 毫秒。
    decision_ts: i64,
    /// 正值做多、负值做空的因子状态。
    factor: FactorSnapshot,
    /// `true` 做多，`false` 做空；由冻结规则决定而非后续行情决定。
    long: bool,
}

/// 只使用入场时可见信息冻结的交易计划。
#[derive(Debug, Clone, PartialEq)]
struct TradePlan {
    /// 被确定性选中的候选。
    candidate: Candidate,
    /// 下一根 15m K 线在序列中的位置。
    entry_index: usize,
    /// 入场价格。
    entry: f64,
    /// 初始止损价格。
    stop: f64,
    /// 固定目标价格。
    target: f64,
    /// 每单位合约价格风险。
    risk: f64,
    /// `true` 做多，`false` 做空。
    long: bool,
}

/// 已接纳持仓只保存容量决策所需身份，不反向影响信号。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivePosition {
    /// 持仓币种。
    symbol: String,
    /// `true` 多头，`false` 空头。
    long: bool,
    /// 该 ResearchBar 交易已结算的退出时间，Unix 毫秒。
    exit_ts: i64,
}

/// 交易计划准备阶段的确定性结果。
enum PlanDecision {
    /// 风险合同有效，可以继续做容量决策。
    Ready(TradePlan),
    /// 初始风险距离不满足冻结范围。
    RiskBlocked,
    /// 缺少下一根连续入场 K 线。
    Incomplete,
}

/// 区分已归档的残差延续与独立残差回归策略身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryRule {
    /// 6h 与 24h 同向时继续跟随 24h 残差。
    MomentumContinuation,
    /// 6h 已与 24h 异号时沿残差回归方向入场。
    MeanReversion,
}

impl EntryRule {
    /// 返回不可变规则版本，用于报告和研究证据区分。
    fn rule_version(self) -> &'static str {
        match self {
            Self::MomentumContinuation => "btc_beta_residual_7d_24h_persistence_8h_v1",
            Self::MeanReversion => "btc_beta_residual_7d_24h_extreme_6h_turn_8h_v1",
        }
    }

    /// 判断 6h 状态是否满足当前策略的预注册确认语义。
    fn condition_passed(self, factor: FactorSnapshot) -> bool {
        if factor.residual_24h == 0.0 || factor.residual_6h == 0.0 {
            return false;
        }
        match self {
            Self::MomentumContinuation => {
                factor.residual_24h.signum() == factor.residual_6h.signum()
            }
            Self::MeanReversion => factor.residual_24h.signum() != factor.residual_6h.signum(),
        }
    }

    /// 将残差方向映射为冻结交易方向。
    fn is_long(self, score: f64) -> bool {
        match self {
            Self::MomentumContinuation => score > 0.0,
            Self::MeanReversion => score < 0.0,
        }
    }

    /// 返回固定止盈倍数。
    fn target_r(self) -> f64 {
        match self {
            Self::MomentumContinuation => MOMENTUM_TARGET_R,
            Self::MeanReversion => REVERSION_TARGET_R,
        }
    }

    /// 返回 ResearchBar 最大持有根数。
    fn max_holding_bars(self) -> usize {
        match self {
            Self::MomentumContinuation => MOMENTUM_MAX_HOLDING_BARS,
            Self::MeanReversion => REVERSION_MAX_HOLDING_BARS,
        }
    }

    /// 返回组合总并发上限。
    fn max_concurrent(self) -> usize {
        match self {
            Self::MomentumContinuation => MOMENTUM_MAX_CONCURRENT,
            Self::MeanReversion => REVERSION_MAX_CONCURRENT,
        }
    }

    /// 返回组合单方向并发上限。
    fn max_same_direction(self) -> usize {
        match self {
            Self::MomentumContinuation => MOMENTUM_MAX_SAME_DIRECTION,
            Self::MeanReversion => REVERSION_MAX_SAME_DIRECTION,
        }
    }
}

/// 解析冻结 V1 参数；未知参数直接失败，避免研究口径静默漂移。
pub fn parse_residual_momentum_args<I>(values: I) -> Result<ResidualMomentumArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut manifest = None;
    while let Some(arg) = values.next() {
        match arg.as_str() {
            "--manifest" => {
                manifest = Some(PathBuf::from(
                    values.next().context("--manifest requires a value")?,
                ));
            }
            "--help" | "-h" => bail!(residual_momentum_usage()),
            _ => bail!("unknown argument: {arg}\n{}", residual_momentum_usage()),
        }
    }
    Ok(ResidualMomentumArgs {
        manifest: manifest.context("--manifest is required")?,
    })
}

/// 返回冻结 V1 的最小命令用法。
pub fn residual_momentum_usage() -> &'static str {
    "Usage: market_beta_residual_momentum_research --manifest PATH"
}

/// 运行冻结 V1 的只读 ResearchBar 回放，不写生产事实或触发交易执行。
pub async fn run_residual_momentum_research(
    args: &ResidualMomentumArgs,
    database_url: &str,
) -> Result<ResidualMomentumReport> {
    run_research(args, database_url, EntryRule::MomentumContinuation).await
}

/// 运行冻结残差回归 V1，不复用或打开残差动量 V1 的 Validation 身份。
pub async fn run_residual_mean_reversion_research(
    args: &ResidualMomentumArgs,
    database_url: &str,
) -> Result<ResidualMomentumReport> {
    run_research(args, database_url, EntryRule::MeanReversion).await
}

/// 复用相同数据与成本机制运行一个显式、不可由 CLI 修改的策略身份。
async fn run_research(
    args: &ResidualMomentumArgs,
    database_url: &str,
    entry_rule: EntryRule,
) -> Result<ResidualMomentumReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode residual-momentum universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let first = schedule
        .windows
        .first()
        .context("missing first residual-momentum window")?;
    let last = schedule
        .windows
        .last()
        .context("missing last residual-momentum window")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for residual-momentum research")?;
    let mut symbols = schedule.union_symbols();
    if !symbols.iter().any(|symbol| symbol == BENCHMARK) {
        symbols.push(BENCHMARK.to_owned());
        symbols.sort();
    }
    let mut series = BTreeMap::<String, SymbolSeries>::new();
    for symbol in symbols {
        series.insert(
            symbol.clone(),
            SymbolSeries {
                candles: load_symbol_candles(
                    &pool,
                    &symbol,
                    first.from_ms.saturating_sub(8 * DAY_MS),
                    last.to_ms.saturating_add(3 * DAY_MS),
                )
                .await?,
            },
        );
    }
    let (candidates, mut stages) = build_candidates(&schedule, &series, entry_rule)?;
    let mut active = Vec::<ActivePosition>::new();
    let mut trades = Vec::<ResidualMomentumTrade>::new();
    for candidate in candidates {
        let symbol_series = series
            .get(&candidate.symbol)
            .with_context(|| format!("missing series for {}", candidate.symbol))?;
        let plan = match prepare_trade(&candidate, &symbol_series.candles, entry_rule) {
            PlanDecision::Ready(plan) => plan,
            PlanDecision::RiskBlocked => {
                stages.risk_blocked += 1;
                continue;
            }
            PlanDecision::Incomplete => {
                stages.incomplete_outcomes += 1;
                continue;
            }
        };
        let entry_ts = symbol_series.candles[plan.entry_index].ts;
        active.retain(|position| position.exit_ts > entry_ts);
        let same_direction = active
            .iter()
            .filter(|position| position.long == plan.long)
            .count();
        if active.len() >= entry_rule.max_concurrent()
            || same_direction >= entry_rule.max_same_direction()
            || active
                .iter()
                .any(|position| position.symbol == candidate.symbol)
        {
            stages.capacity_blocked += 1;
            continue;
        }
        let Some(trade) = settle_plan(&plan, &symbol_series.candles, entry_rule) else {
            stages.incomplete_outcomes += 1;
            continue;
        };
        active.push(ActivePosition {
            symbol: trade.symbol.clone(),
            long: plan.long,
            exit_ts: trade.exit_ts,
        });
        trades.push(trade);
    }
    trades.sort_by(|left, right| {
        left.entry_ts
            .cmp(&right.entry_ts)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    let report = build_report(
        &schedule,
        series.len(),
        stages,
        trades,
        entry_rule.rule_version(),
    );
    print_report(&report);
    Ok(report)
}

impl UniverseSchedule {
    /// 从 current-live crypto-only manifest 构造连续且可审计的月份窗口。
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
            bail!("residual-momentum research requires current-live crypto-only OKX 15m manifest");
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
            bail!("residual-momentum V1 requires exactly twelve contiguous monthly windows");
        }
        Ok(Self {
            version: manifest.universe_version,
            windows,
        })
    }

    /// 返回全部窗口成员的确定性去重并集。
    fn union_symbols(&self) -> Vec<String> {
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

/// 在每个 UTC 8 小时决策点计算完整横截面，并按冻结排序选出唯一候选。
fn build_candidates(
    schedule: &UniverseSchedule,
    series: &BTreeMap<String, SymbolSeries>,
    entry_rule: EntryRule,
) -> Result<(Vec<Candidate>, ResidualMomentumStageCounts)> {
    let benchmark = series.get(BENCHMARK).context("BTC benchmark is missing")?;
    let mut stages = ResidualMomentumStageCounts::default();
    let mut candidates = Vec::new();
    for candle in &benchmark.candles {
        let decision_ts = candle.ts.saturating_add(MS_15M);
        if decision_ts.rem_euclid(MS_8H) != 0 {
            continue;
        }
        let Some(window) = schedule.window_at(decision_ts) else {
            continue;
        };
        stages.decision_points += 1;
        let members = window
            .members
            .iter()
            .filter(|symbol| symbol.as_str() != BENCHMARK)
            .collect::<Vec<_>>();
        let minimum = (members.len() as f64 * MIN_FACTOR_COVERAGE).ceil() as usize;
        let mut snapshots = Vec::<(String, FactorSnapshot)>::new();
        for symbol in members {
            let Some(symbol_series) = series.get(symbol) else {
                continue;
            };
            if let Some(factor) = factor_at(
                &symbol_series.candles,
                &benchmark.candles,
                decision_ts.saturating_sub(MS_15M),
            ) {
                snapshots.push((symbol.clone(), factor));
            }
        }
        if snapshots.len() < minimum || minimum == 0 {
            stages.coverage_blocked += 1;
            continue;
        }
        stages.factor_observations += snapshots.len();
        snapshots.retain(|(_, factor)| {
            let condition_passed = entry_rule.condition_passed(*factor);
            if condition_passed {
                match entry_rule {
                    EntryRule::MomentumContinuation => stages.persistence_pass += 1,
                    EntryRule::MeanReversion => stages.reversion_pass += 1,
                }
            }
            let passed = condition_passed && factor.score.abs() >= MIN_SCORE;
            if passed {
                stages.score_pass += 1;
            }
            passed
        });
        snapshots.sort_by(|left, right| {
            right
                .1
                .score
                .abs()
                .total_cmp(&left.1.score.abs())
                .then_with(|| left.0.cmp(&right.0))
        });
        if let Some((symbol, factor)) = snapshots.into_iter().next() {
            stages.selected_candidates += 1;
            candidates.push(Candidate {
                symbol,
                decision_ts,
                factor,
                long: entry_rule.is_long(factor.score),
            });
        }
    }
    Ok((candidates, stages))
}

/// 只使用同步且连续的历史前缀计算一个信号时点的残差状态。
fn factor_at(
    symbol: &[CandleItem],
    benchmark: &[CandleItem],
    decision_candle_ts: i64,
) -> Option<FactorSnapshot> {
    let symbol_index = symbol
        .binary_search_by_key(&decision_candle_ts, |candle| candle.ts)
        .ok()?;
    let benchmark_index = benchmark
        .binary_search_by_key(&decision_candle_ts, |candle| candle.ts)
        .ok()?;
    if symbol_index < BETA_BARS || benchmark_index < BETA_BARS {
        return None;
    }
    let symbol_start = symbol_index - BETA_BARS;
    let benchmark_start = benchmark_index - BETA_BARS;
    if symbol[symbol_start].ts != benchmark[benchmark_start].ts
        || symbol[symbol_index].ts - symbol[symbol_start].ts != BETA_BARS as i64 * MS_15M
        || benchmark[benchmark_index].ts - benchmark[benchmark_start].ts
            != BETA_BARS as i64 * MS_15M
    {
        return None;
    }
    let mut symbol_returns = Vec::with_capacity(BETA_BARS);
    let mut benchmark_returns = Vec::with_capacity(BETA_BARS);
    for offset in 1..=BETA_BARS {
        let symbol_return = log_return(
            symbol[symbol_start + offset - 1].c,
            symbol[symbol_start + offset].c,
        )?;
        let benchmark_return = log_return(
            benchmark[benchmark_start + offset - 1].c,
            benchmark[benchmark_start + offset].c,
        )?;
        symbol_returns.push(symbol_return);
        benchmark_returns.push(benchmark_return);
    }
    let symbol_mean = mean(&symbol_returns)?;
    let benchmark_mean = mean(&benchmark_returns)?;
    let benchmark_variance_sum = benchmark_returns
        .iter()
        .map(|value| (value - benchmark_mean).powi(2))
        .sum::<f64>();
    if !benchmark_variance_sum.is_finite() || benchmark_variance_sum <= f64::EPSILON {
        return None;
    }
    let covariance_sum = symbol_returns
        .iter()
        .zip(&benchmark_returns)
        .map(|(symbol_value, benchmark_value)| {
            (symbol_value - symbol_mean) * (benchmark_value - benchmark_mean)
        })
        .sum::<f64>();
    let beta = covariance_sum / benchmark_variance_sum;
    let alpha = symbol_mean - beta * benchmark_mean;
    let residuals = symbol_returns
        .iter()
        .zip(&benchmark_returns)
        .map(|(symbol_value, benchmark_value)| symbol_value - alpha - beta * benchmark_value)
        .collect::<Vec<_>>();
    let residual_mean = mean(&residuals)?;
    let variance = residuals
        .iter()
        .map(|value| (value - residual_mean).powi(2))
        .sum::<f64>()
        / (residuals.len().checked_sub(1)? as f64);
    let daily_residual_vol = variance.sqrt() * (RESIDUAL_24H_BARS as f64).sqrt();
    if !beta.is_finite() || !daily_residual_vol.is_finite() || daily_residual_vol <= f64::EPSILON {
        return None;
    }
    let residual_24h = residuals[residuals.len() - RESIDUAL_24H_BARS..]
        .iter()
        .sum::<f64>();
    let residual_6h = residuals[residuals.len() - RESIDUAL_6H_BARS..]
        .iter()
        .sum::<f64>();
    let score = residual_24h / daily_residual_vol;
    let atr = atr_at(symbol, symbol_index)?;
    (residual_24h.is_finite() && residual_6h.is_finite() && score.is_finite()).then_some(
        FactorSnapshot {
            beta,
            residual_24h,
            residual_6h,
            score,
            atr,
        },
    )
}

/// 将候选转换为冻结初始风险计划，不读取入场 K 线之后的价格。
fn prepare_trade(
    candidate: &Candidate,
    candles: &[CandleItem],
    entry_rule: EntryRule,
) -> PlanDecision {
    let Ok(entry_index) = candles.binary_search_by_key(&candidate.decision_ts, |candle| candle.ts)
    else {
        return PlanDecision::Incomplete;
    };
    if entry_index == 0
        || candles[entry_index - 1].ts.saturating_add(MS_15M) != candles[entry_index].ts
    {
        return PlanDecision::Incomplete;
    }
    let entry = candles[entry_index].o;
    let risk = candidate.factor.atr * ATR_STOP_MULTIPLIER;
    let risk_pct = risk / entry * 100.0;
    if !entry.is_finite()
        || !risk.is_finite()
        || entry <= 0.0
        || risk <= 0.0
        || !(MIN_RISK_PCT..=MAX_RISK_PCT).contains(&risk_pct)
    {
        return PlanDecision::RiskBlocked;
    }
    let long = candidate.long;
    let stop = if long { entry - risk } else { entry + risk };
    let target = if long {
        entry + risk * entry_rule.target_r()
    } else {
        entry - risk * entry_rule.target_r()
    };
    if stop <= 0.0 || target <= 0.0 {
        return PlanDecision::RiskBlocked;
    }
    PlanDecision::Ready(TradePlan {
        candidate: candidate.clone(),
        entry_index,
        entry,
        stop,
        target,
        risk,
        long,
    })
}

/// 按保守同棒路径、3R 目标和 48h 上限结算已通过容量门禁的计划。
fn settle_plan(
    plan: &TradePlan,
    candles: &[CandleItem],
    entry_rule: EntryRule,
) -> Option<ResidualMomentumTrade> {
    let end_exclusive = plan
        .entry_index
        .checked_add(entry_rule.max_holding_bars())?;
    if end_exclusive > candles.len() {
        return None;
    }
    let mut exit_index = end_exclusive - 1;
    let mut exit_price = candles[exit_index].c;
    let mut exit_reason = "timeout";
    for index in plan.entry_index..end_exclusive {
        if index > plan.entry_index
            && candles[index - 1].ts.saturating_add(MS_15M) != candles[index].ts
        {
            return None;
        }
        let candle = &candles[index];
        let stop_hit = if plan.long {
            candle.l <= plan.stop
        } else {
            candle.h >= plan.stop
        };
        let target_hit = if plan.long {
            candle.h >= plan.target
        } else {
            candle.l <= plan.target
        };
        if stop_hit {
            exit_index = index;
            exit_price = plan.stop;
            exit_reason = "stop";
            break;
        }
        if target_hit {
            exit_index = index;
            exit_price = plan.target;
            exit_reason = "target";
            break;
        }
    }
    let entry_ts = candles[plan.entry_index].ts;
    let exit_ts = candles[exit_index].ts.saturating_add(MS_15M);
    let gross_price = if plan.long {
        exit_price - plan.entry
    } else {
        plan.entry - exit_price
    };
    let gross_r = gross_price / plan.risk;
    let execution_cost_r = (plan.entry + exit_price) * EXECUTION_COST_PER_SIDE / plan.risk;
    let funding_intervals = (exit_ts.div_euclid(MS_8H) - entry_ts.div_euclid(MS_8H)).max(0);
    let funding_cost_r = plan.entry * ADVERSE_FUNDING_PER_8H * funding_intervals as f64 / plan.risk;
    let cost_r = execution_cost_r + funding_cost_r;
    Some(ResidualMomentumTrade {
        symbol: plan.candidate.symbol.clone(),
        direction: if plan.long { "long" } else { "short" },
        decision_ts: plan.candidate.decision_ts,
        entry_ts,
        exit_ts,
        beta: plan.candidate.factor.beta,
        residual_24h: plan.candidate.factor.residual_24h,
        residual_6h: plan.candidate.factor.residual_6h,
        score: plan.candidate.factor.score,
        entry_price: plan.entry,
        initial_stop: plan.stop,
        target_price: plan.target,
        exit_price,
        gross_r,
        cost_r,
        net_r: gross_r - cost_r,
        double_cost_net_r: gross_r - 2.0 * cost_r,
        exit_reason,
    })
}

/// 从本地 quant_core 读取已确认且严格按时间排序的 15m K 线。
async fn load_symbol_candles(
    pool: &PgPool,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<CandleItem>> {
    if !valid_symbol(symbol) {
        bail!("invalid residual-momentum manifest symbol {symbol}");
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
        .with_context(|| format!("load residual-momentum candles from {table}"))?
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

/// 计算截至指定 K 线的 14 根真实波幅均值。
fn atr_at(candles: &[CandleItem], index: usize) -> Option<f64> {
    if index + 1 < ATR_PERIOD {
        return None;
    }
    let start = index + 1 - ATR_PERIOD;
    let mut total = 0.0;
    for current in start..=index {
        let candle = &candles[current];
        let previous_close = current
            .checked_sub(1)
            .map(|previous| candles[previous].c)
            .unwrap_or(candle.c);
        total += (candle.h - candle.l)
            .max((candle.h - previous_close).abs())
            .max((candle.l - previous_close).abs());
    }
    let atr = total / ATR_PERIOD as f64;
    (atr.is_finite() && atr > 0.0).then_some(atr)
}

/// 计算两个正价格之间的有限对数收益。
fn log_return(previous: f64, current: f64) -> Option<f64> {
    if previous <= 0.0 || current <= 0.0 {
        return None;
    }
    let value = (current / previous).ln();
    value.is_finite().then_some(value)
}

/// 返回非空有限样本的算术平均值。
fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

/// 解析数据库数值并拒绝非有限值。
fn parse_number(value: String) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse residual-momentum candle number {value}"))?;
    if !parsed.is_finite() {
        bail!("non-finite residual-momentum candle number {value}");
    }
    Ok(parsed)
}

/// 限制动态表名只能来自规范的 OKX USDT 永续标识。
fn valid_symbol(symbol: &str) -> bool {
    symbol.ends_with("-USDT-SWAP")
        && symbol
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests;
