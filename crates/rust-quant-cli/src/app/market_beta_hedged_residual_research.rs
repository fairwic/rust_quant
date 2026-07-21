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
const MS_4H: i64 = 4 * 60 * 60 * 1_000;
const MS_8H: i64 = 8 * 60 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const BETA_BARS: usize = 7 * 24 * 4;
const RESIDUAL_24H_BARS: usize = 24 * 4;
const RESIDUAL_6H_BARS: usize = 6 * 4;
const RISK_HORIZON_BARS: f64 = 4.0 * 4.0;
const RISK_SIGMA_MULTIPLIER: f64 = 2.0;
const MIN_SCORE: f64 = 1.5;
const MIN_BETA: f64 = 0.25;
const MAX_BETA: f64 = 3.0;
const MIN_FACTOR_COVERAGE: f64 = 0.80;
const MIN_RISK_RETURN: f64 = 0.005;
const MAX_RISK_RETURN: f64 = 0.05;
const TARGET_R: f64 = 2.0;
const MAX_HOLDING_BARS: usize = 24 * 4;
const MAX_CONCURRENT: usize = 4;
const MAX_SAME_DIRECTION: usize = 3;
const EXECUTION_COST_PER_FILL: f64 = 0.0008;
const ADVERSE_FUNDING_PER_8H: f64 = 0.0001;
const BENCHMARK: &str = "BTC-USDT-SWAP";
const RULE_VERSION: &str = "btc_beta_hedged_residual_7d_24h_extreme_6h_turn_4h_v1";

/// 冻结研究入口只接受历史币池路径，不允许从命令行调整策略阈值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetaHedgedResidualArgs {
    /// 已完成 current-live crypto-only 审计的 12 个月币池 manifest。
    pub manifest: PathBuf,
}

/// 记录双腿候选从信号到实际接纳的完整因果漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BetaHedgedResidualStageCounts {
    /// 位于研究窗口的 UTC 4 小时决策时点数。
    pub decision_points: usize,
    /// 因 BTC 或横截面完整因子覆盖低于 80% 而阻塞的时点数。
    pub coverage_blocked: usize,
    /// 在未阻塞时点成功计算出的同步残差观察数。
    pub factor_observations: usize,
    /// 24h 与 6h 残差严格异号的观察数。
    pub reversion_pass: usize,
    /// 在回归确认后达到冻结绝对分数门槛的观察数。
    pub score_pass: usize,
    /// 在分数门禁后满足可执行 Beta 名义范围的观察数。
    pub beta_pass: usize,
    /// 每个时点完成确定性 Top1 排序后的候选数。
    pub selected_candidates: usize,
    /// 初始残差风险不在冻结范围内的候选数。
    pub risk_blocked: usize,
    /// 被同币、总并发或同方向并发上限阻塞的候选数。
    pub capacity_blocked: usize,
    /// 缺少共同入场、触发或下一共同退出开盘的候选数。
    pub incomplete_outcomes: usize,
}

/// 保存一组币种腿加 BTC 对冲腿的信号、成交、风险和成本证据。
#[derive(Debug, Clone, PartialEq)]
pub struct BetaHedgedResidualTrade {
    /// 非 BTC 的 OKX USDT 永续标识。
    pub symbol: String,
    /// `long_residual` 或 `short_residual`，不是裸币种方向。
    pub direction: &'static str,
    /// 因子决策时间；此时对应的 15m K 线已经完成。
    pub decision_ts: i64,
    /// 两腿共同入场开盘时间。
    pub entry_ts: i64,
    /// 两腿共同退出开盘时间。
    pub exit_ts: i64,
    /// 触发止损、目标或超时的完成 K 线结束时间。
    pub trigger_ts: i64,
    /// 信号时冻结且持仓期不再平衡的 BTC 名义 Beta。
    pub beta: f64,
    /// 信号时 24h 对数残差。
    pub residual_24h: f64,
    /// 信号时 6h 对数残差。
    pub residual_6h: f64,
    /// 24h 残差除以 7 日估计日残差波动。
    pub score: f64,
    /// 以币种腿 1 单位名义计的初始价差风险收益率。
    pub initial_risk_return: f64,
    /// 币种腿共同入场开盘价。
    pub symbol_entry_price: f64,
    /// BTC 对冲腿共同入场开盘价。
    pub btc_entry_price: f64,
    /// 币种腿实际退出开盘价。
    pub symbol_exit_price: f64,
    /// BTC 对冲腿实际退出开盘价。
    pub btc_exit_price: f64,
    /// 触发判断时的价差 R；实际退出可以因下一开盘跳空而不同。
    pub trigger_spread_r: f64,
    /// 四次成交与不利资金成本之前的实际双腿价差 R。
    pub gross_r: f64,
    /// 标准四次成交与不利资金成本，按初始残差风险归一化。
    pub cost_r: f64,
    /// 标准成本后的净 R。
    pub net_r: f64,
    /// 双倍成本压力后的净 R。
    pub double_cost_net_r: f64,
    /// `stop`、`target` 或 `timeout`。
    pub exit_reason: &'static str,
}

/// 交易级固定 R 汇总；职业门禁通过前不构造组合资金曲线。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BetaHedgedResidualMetrics {
    /// 当前切片的实际成交组数。
    pub trades: usize,
    /// 当前成本口径下的累计净 R。
    pub net_sum_r: f64,
    /// 当前成本口径下的平均每笔净 R。
    pub net_expectancy_r: Option<f64>,
    /// 正收益总额除以负收益绝对总额。
    pub profit_factor: Option<f64>,
    /// 净收益大于零的交易占比，单位百分比。
    pub win_rate_pct: Option<f64>,
    /// 交易收益样本 Sharpe，仅用于交易级早停。
    pub trade_sharpe: Option<f64>,
    /// 按成交顺序累计 R 曲线的最大回撤。
    pub max_drawdown_r: f64,
    /// 累计净 R 除以最大回撤 R。
    pub recovery_factor: Option<f64>,
}

/// 汇总冻结双腿策略的数据覆盖、成本、稳定性与集中度证据。
#[derive(Debug, Clone, PartialEq)]
pub struct BetaHedgedResidualReport {
    /// 不可变入场和双腿执行规则身份。
    pub rule_version: String,
    /// 历史币池 manifest 身份。
    pub universe_version: String,
    /// 实际加载的 BTC 加成员去重币种数。
    pub symbols: usize,
    /// 候选和成交漏斗。
    pub stages: BetaHedgedResidualStageCounts,
    /// 30 分钟触发聚类后的有效事件数。
    pub effective_events: usize,
    /// 不含成本的反事实指标。
    pub gross_zero_cost: BetaHedgedResidualMetrics,
    /// 冻结标准成本指标。
    pub overall: BetaHedgedResidualMetrics,
    /// 冻结双倍成本压力指标。
    pub double_cost: BetaHedgedResidualMetrics,
    /// 做多残差的标准成本指标。
    pub long_residual: BetaHedgedResidualMetrics,
    /// 做空残差的标准成本指标。
    pub short_residual: BetaHedgedResidualMetrics,
    /// 每个币池自然月的标准成本指标。
    pub monthly: Vec<(i64, BetaHedgedResidualMetrics)>,
    /// 标准成本累计净 R 为正的月份数。
    pub positive_months: usize,
    /// 标准成本净贡献最高的三个盈利币种。
    pub top_three_positive_symbols: Vec<String>,
    /// 移除前三盈利币种后的标准成本累计净 R。
    pub net_r_without_top_three_symbols: f64,
    /// 按退出原因计数。
    pub exit_reasons: BTreeMap<String, usize>,
    /// 所有交易的平均冻结 Beta。
    pub average_beta: Option<f64>,
    /// 所有交易的平均双腿总名义倍数 `1 + beta`。
    pub average_gross_notional: Option<f64>,
    /// 是否通过预注册 Discovery 早停条件；不等于职业晋级。
    pub discovery_gate_passed: bool,
    /// 可逐组审计的真实回放成交。
    pub trades: Vec<BetaHedgedResidualTrade>,
}

/// 单个历史币池的生效区间与 current-live 成员。
#[derive(Debug, Clone, PartialEq, Eq)]
struct UniverseWindow {
    /// 窗口起点，Unix 毫秒且包含。
    from_ms: i64,
    /// 窗口终点，Unix 毫秒且不包含。
    to_ms: i64,
    /// 本月可参与横截面的标准 OKX 合约标识。
    members: BTreeSet<String>,
}

/// 已按时间排序且月份连续的历史币池日程。
#[derive(Debug, Clone, PartialEq, Eq)]
struct UniverseSchedule {
    /// manifest 中冻结的币池版本。
    version: String,
    /// 连续且互不重叠的十二个月窗口。
    windows: Vec<UniverseWindow>,
}

/// 单个币种完整且已确认的 15m 序列。
#[derive(Debug, Clone)]
struct SymbolSeries {
    /// 按开盘时间严格升序的 K 线。
    candles: Vec<CandleItem>,
}

/// 信号时点可见且用于整笔双腿交易的冻结残差状态。
#[derive(Debug, Clone, Copy, PartialEq)]
struct FactorSnapshot {
    /// 7 日同步收益 OLS 的 BTC Beta。
    beta: f64,
    /// 最近 24h 特质收益。
    residual_24h: f64,
    /// 最近 6h 特质收益。
    residual_6h: f64,
    /// 标准化后的 24h 残差极端分数。
    score: f64,
    /// 7 日 15m 残差样本标准差。
    residual_std_15m: f64,
}

/// 每个 4 小时决策时点按绝对分数排序后的唯一候选。
#[derive(Debug, Clone, PartialEq)]
struct Candidate {
    /// 非 BTC 候选合约。
    symbol: String,
    /// 已完成决策 K 线的结束时间。
    decision_ts: i64,
    /// 冻结因子状态。
    factor: FactorSnapshot,
    /// `true` 做多残差，`false` 做空残差。
    long_residual: bool,
}

/// 只使用入场时可见信息冻结的双腿交易计划。
#[derive(Debug, Clone, PartialEq)]
struct PairPlan {
    /// 被确定性选中的候选。
    candidate: Candidate,
    /// 币种腿下一共同开盘在序列中的位置。
    symbol_entry_index: usize,
    /// BTC 腿下一共同开盘在序列中的位置。
    btc_entry_index: usize,
    /// 币种腿入场开盘价。
    symbol_entry: f64,
    /// BTC 腿入场开盘价。
    btc_entry: f64,
    /// 冻结初始残差风险收益率。
    risk_return: f64,
    /// `true` 做多残差，`false` 做空残差。
    long_residual: bool,
}

/// 已接纳双腿持仓只保存容量决策所需身份。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivePair {
    /// 非 BTC 币种腿标识。
    symbol: String,
    /// 冻结残差方向。
    long_residual: bool,
    /// 两腿共同退出开盘时间。
    exit_ts: i64,
}

/// 区分可成交、风险阻塞与共同 K 线缺失。
enum PlanDecision {
    /// 风险和共同入场合同有效，可继续做容量判断。
    Ready(PairPlan),
    /// 初始残差风险不在冻结范围。
    RiskBlocked,
    /// 缺少同步下一根共同开盘。
    Incomplete,
}

/// 解析冻结参数；未知参数直接失败，避免研究口径漂移。
pub fn parse_beta_hedged_residual_args<I>(values: I) -> Result<BetaHedgedResidualArgs>
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
            "--help" | "-h" => bail!(beta_hedged_residual_usage()),
            _ => bail!("unknown argument: {arg}\n{}", beta_hedged_residual_usage()),
        }
    }
    Ok(BetaHedgedResidualArgs {
        manifest: manifest.context("--manifest is required")?,
    })
}

/// 返回冻结研究入口的最小命令用法。
pub fn beta_hedged_residual_usage() -> &'static str {
    "Usage: market_beta_hedged_residual_research --manifest PATH"
}

/// 运行只读双腿 ResearchBar 回放，不写交易事实或触发执行。
pub async fn run_beta_hedged_residual_research(
    args: &BetaHedgedResidualArgs,
    database_url: &str,
) -> Result<BetaHedgedResidualReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode beta-hedged residual universe manifest")?;
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
        .context("connect quant_core for beta-hedged residual research")?;
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
                    last.to_ms.saturating_add(2 * DAY_MS),
                )
                .await?,
            },
        );
    }
    let (candidates, mut stages) = build_candidates(&schedule, &series)?;
    let benchmark = series.get(BENCHMARK).context("BTC benchmark is missing")?;
    let mut active = Vec::<ActivePair>::new();
    let mut trades = Vec::<BetaHedgedResidualTrade>::new();
    for candidate in candidates {
        let symbol_series = series
            .get(&candidate.symbol)
            .with_context(|| format!("missing series for {}", candidate.symbol))?;
        let plan = match prepare_pair(&candidate, &symbol_series.candles, &benchmark.candles) {
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
        let entry_ts = symbol_series.candles[plan.symbol_entry_index].ts;
        active.retain(|position| position.exit_ts > entry_ts);
        let same_direction = active
            .iter()
            .filter(|position| position.long_residual == plan.long_residual)
            .count();
        if active.len() >= MAX_CONCURRENT
            || same_direction >= MAX_SAME_DIRECTION
            || active
                .iter()
                .any(|position| position.symbol == candidate.symbol)
        {
            stages.capacity_blocked += 1;
            continue;
        }
        let Some(trade) = settle_pair(&plan, &symbol_series.candles, &benchmark.candles) else {
            stages.incomplete_outcomes += 1;
            continue;
        };
        active.push(ActivePair {
            symbol: trade.symbol.clone(),
            long_residual: plan.long_residual,
            exit_ts: trade.exit_ts,
        });
        trades.push(trade);
    }
    trades.sort_by(|left, right| {
        left.entry_ts
            .cmp(&right.entry_ts)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    let report = build_report(&schedule, series.len(), stages, trades);
    print_report(&report);
    Ok(report)
}

impl UniverseSchedule {
    /// 从 current-live crypto-only manifest 构造连续的十二个月窗口。
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
            bail!(
                "beta-hedged residual research requires current-live crypto-only OKX 15m manifest"
            );
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
            bail!("beta-hedged residual V1 requires twelve contiguous monthly windows");
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

/// 在每个 UTC 4 小时决策点构造完整横截面并选择唯一候选。
fn build_candidates(
    schedule: &UniverseSchedule,
    series: &BTreeMap<String, SymbolSeries>,
) -> Result<(Vec<Candidate>, BetaHedgedResidualStageCounts)> {
    let benchmark = series.get(BENCHMARK).context("BTC benchmark is missing")?;
    let mut stages = BetaHedgedResidualStageCounts::default();
    let mut candidates = Vec::new();
    for candle in &benchmark.candles {
        let decision_ts = candle.ts.saturating_add(MS_15M);
        if decision_ts.rem_euclid(MS_4H) != 0 {
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
        if minimum == 0 || snapshots.len() < minimum {
            stages.coverage_blocked += 1;
            continue;
        }
        stages.factor_observations += snapshots.len();
        snapshots.retain(|(_, factor)| {
            let reversion = factor.residual_24h != 0.0
                && factor.residual_6h != 0.0
                && factor.residual_24h.signum() != factor.residual_6h.signum();
            if reversion {
                stages.reversion_pass += 1;
            }
            let score_pass = reversion && factor.score.abs() >= MIN_SCORE;
            if score_pass {
                stages.score_pass += 1;
            }
            let beta_pass = score_pass && (MIN_BETA..=MAX_BETA).contains(&factor.beta);
            if beta_pass {
                stages.beta_pass += 1;
            }
            beta_pass
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
                long_residual: factor.score < 0.0,
            });
        }
    }
    Ok((candidates, stages))
}

/// 只用同步且连续的历史前缀估计 OLS Beta 和残差状态。
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
    let mut symbol_returns = Vec::with_capacity(BETA_BARS);
    let mut benchmark_returns = Vec::with_capacity(BETA_BARS);
    for offset in 1..=BETA_BARS {
        let symbol_previous = &symbol[symbol_start + offset - 1];
        let symbol_current = &symbol[symbol_start + offset];
        let benchmark_previous = &benchmark[benchmark_start + offset - 1];
        let benchmark_current = &benchmark[benchmark_start + offset];
        if symbol_previous.ts != benchmark_previous.ts
            || symbol_current.ts != benchmark_current.ts
            || symbol_previous.ts.saturating_add(MS_15M) != symbol_current.ts
            || benchmark_previous.ts.saturating_add(MS_15M) != benchmark_current.ts
        {
            return None;
        }
        symbol_returns.push(log_return(symbol_previous.c, symbol_current.c)?);
        benchmark_returns.push(log_return(benchmark_previous.c, benchmark_current.c)?);
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
    let residual_variance = residuals
        .iter()
        .map(|value| (value - residual_mean).powi(2))
        .sum::<f64>()
        / residuals.len().checked_sub(1)? as f64;
    let residual_std_15m = residual_variance.sqrt();
    let daily_residual_vol = residual_std_15m * (RESIDUAL_24H_BARS as f64).sqrt();
    if !beta.is_finite()
        || !residual_std_15m.is_finite()
        || !daily_residual_vol.is_finite()
        || residual_std_15m <= f64::EPSILON
    {
        return None;
    }
    let residual_24h = residuals[residuals.len() - RESIDUAL_24H_BARS..]
        .iter()
        .sum::<f64>();
    let residual_6h = residuals[residuals.len() - RESIDUAL_6H_BARS..]
        .iter()
        .sum::<f64>();
    let score = residual_24h / daily_residual_vol;
    (residual_24h.is_finite() && residual_6h.is_finite() && score.is_finite()).then_some(
        FactorSnapshot {
            beta,
            residual_24h,
            residual_6h,
            score,
            residual_std_15m,
        },
    )
}

/// 将候选转换为同步双腿计划，不读取共同入场开盘之后的价格。
fn prepare_pair(
    candidate: &Candidate,
    symbol: &[CandleItem],
    benchmark: &[CandleItem],
) -> PlanDecision {
    let Ok(symbol_entry_index) =
        symbol.binary_search_by_key(&candidate.decision_ts, |candle| candle.ts)
    else {
        return PlanDecision::Incomplete;
    };
    let Ok(btc_entry_index) =
        benchmark.binary_search_by_key(&candidate.decision_ts, |candle| candle.ts)
    else {
        return PlanDecision::Incomplete;
    };
    if symbol_entry_index == 0
        || btc_entry_index == 0
        || symbol[symbol_entry_index - 1].ts.saturating_add(MS_15M) != symbol[symbol_entry_index].ts
        || benchmark[btc_entry_index - 1].ts.saturating_add(MS_15M) != benchmark[btc_entry_index].ts
        || symbol[symbol_entry_index].ts != benchmark[btc_entry_index].ts
    {
        return PlanDecision::Incomplete;
    }
    let symbol_entry = symbol[symbol_entry_index].o;
    let btc_entry = benchmark[btc_entry_index].o;
    let risk_return =
        candidate.factor.residual_std_15m * RISK_HORIZON_BARS.sqrt() * RISK_SIGMA_MULTIPLIER;
    if !symbol_entry.is_finite()
        || !btc_entry.is_finite()
        || symbol_entry <= 0.0
        || btc_entry <= 0.0
        || !risk_return.is_finite()
        || !(MIN_RISK_RETURN..=MAX_RISK_RETURN).contains(&risk_return)
    {
        return PlanDecision::RiskBlocked;
    }
    PlanDecision::Ready(PairPlan {
        candidate: candidate.clone(),
        symbol_entry_index,
        btc_entry_index,
        symbol_entry,
        btc_entry,
        risk_return,
        long_residual: candidate.long_residual,
    })
}

/// 用共同完成收盘触发，并在下一共同开盘结算真实双腿价差。
fn settle_pair(
    plan: &PairPlan,
    symbol: &[CandleItem],
    benchmark: &[CandleItem],
) -> Option<BetaHedgedResidualTrade> {
    let mut trigger_symbol_index = plan.symbol_entry_index;
    let mut trigger_btc_index = plan.btc_entry_index;
    let mut trigger_spread_r = 0.0;
    let mut exit_reason = "timeout";
    for offset in 0..MAX_HOLDING_BARS {
        let symbol_index = plan.symbol_entry_index.checked_add(offset)?;
        let btc_index = plan.btc_entry_index.checked_add(offset)?;
        let symbol_candle = symbol.get(symbol_index)?;
        let btc_candle = benchmark.get(btc_index)?;
        if symbol_candle.ts != btc_candle.ts
            || (offset > 0
                && (symbol[symbol_index - 1].ts.saturating_add(MS_15M) != symbol_candle.ts
                    || benchmark[btc_index - 1].ts.saturating_add(MS_15M) != btc_candle.ts))
        {
            return None;
        }
        let marked_return = pair_return(
            plan.long_residual,
            plan.candidate.factor.beta,
            plan.symbol_entry,
            symbol_candle.c,
            plan.btc_entry,
            btc_candle.c,
        )?;
        trigger_spread_r = marked_return / plan.risk_return;
        if trigger_spread_r <= -1.0 {
            trigger_symbol_index = symbol_index;
            trigger_btc_index = btc_index;
            exit_reason = "stop";
            break;
        }
        if trigger_spread_r >= TARGET_R {
            trigger_symbol_index = symbol_index;
            trigger_btc_index = btc_index;
            exit_reason = "target";
            break;
        }
        trigger_symbol_index = symbol_index;
        trigger_btc_index = btc_index;
    }
    let symbol_exit_index = trigger_symbol_index.checked_add(1)?;
    let btc_exit_index = trigger_btc_index.checked_add(1)?;
    let symbol_exit_candle = symbol.get(symbol_exit_index)?;
    let btc_exit_candle = benchmark.get(btc_exit_index)?;
    if symbol[trigger_symbol_index].ts.saturating_add(MS_15M) != symbol_exit_candle.ts
        || benchmark[trigger_btc_index].ts.saturating_add(MS_15M) != btc_exit_candle.ts
        || symbol_exit_candle.ts != btc_exit_candle.ts
    {
        return None;
    }
    let symbol_exit = symbol_exit_candle.o;
    let btc_exit = btc_exit_candle.o;
    let gross_return = pair_return(
        plan.long_residual,
        plan.candidate.factor.beta,
        plan.symbol_entry,
        symbol_exit,
        plan.btc_entry,
        btc_exit,
    )?;
    let gross_r = gross_return / plan.risk_return;
    let entry_ts = symbol[plan.symbol_entry_index].ts;
    let exit_ts = symbol_exit_candle.ts;
    let gross_notional = 1.0 + plan.candidate.factor.beta;
    let execution_cost_return = 2.0 * EXECUTION_COST_PER_FILL * gross_notional;
    let funding_intervals = (exit_ts.div_euclid(MS_8H) - entry_ts.div_euclid(MS_8H)).max(0);
    let funding_cost_return = ADVERSE_FUNDING_PER_8H * gross_notional * funding_intervals as f64;
    let cost_r = (execution_cost_return + funding_cost_return) / plan.risk_return;
    Some(BetaHedgedResidualTrade {
        symbol: plan.candidate.symbol.clone(),
        direction: if plan.long_residual {
            "long_residual"
        } else {
            "short_residual"
        },
        decision_ts: plan.candidate.decision_ts,
        entry_ts,
        exit_ts,
        trigger_ts: symbol[trigger_symbol_index].ts.saturating_add(MS_15M),
        beta: plan.candidate.factor.beta,
        residual_24h: plan.candidate.factor.residual_24h,
        residual_6h: plan.candidate.factor.residual_6h,
        score: plan.candidate.factor.score,
        initial_risk_return: plan.risk_return,
        symbol_entry_price: plan.symbol_entry,
        btc_entry_price: plan.btc_entry,
        symbol_exit_price: symbol_exit,
        btc_exit_price: btc_exit,
        trigger_spread_r,
        gross_r,
        cost_r,
        net_r: gross_r - cost_r,
        double_cost_net_r: gross_r - 2.0 * cost_r,
        exit_reason,
    })
}

/// 计算固定 Beta 双腿相对收益；方向正值表示做多残差。
fn pair_return(
    long_residual: bool,
    beta: f64,
    symbol_entry: f64,
    symbol_price: f64,
    btc_entry: f64,
    btc_price: f64,
) -> Option<f64> {
    if !(beta.is_finite()
        && symbol_entry.is_finite()
        && symbol_price.is_finite()
        && btc_entry.is_finite()
        && btc_price.is_finite())
        || beta <= 0.0
        || symbol_entry <= 0.0
        || symbol_price <= 0.0
        || btc_entry <= 0.0
        || btc_price <= 0.0
    {
        return None;
    }
    let spread = symbol_price / symbol_entry - 1.0 - beta * (btc_price / btc_entry - 1.0);
    let directed = if long_residual { spread } else { -spread };
    directed.is_finite().then_some(directed)
}

/// 从本地 quant_core 读取已确认且严格按时间排序的 15m K 线。
async fn load_symbol_candles(
    pool: &PgPool,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<CandleItem>> {
    if !valid_symbol(symbol) {
        bail!("invalid beta-hedged residual manifest symbol {symbol}");
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
        .with_context(|| format!("load beta-hedged residual candles from {table}"))?
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
        .with_context(|| format!("parse beta-hedged residual candle number {value}"))?;
    if !parsed.is_finite() {
        bail!("non-finite beta-hedged residual candle number {value}");
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
