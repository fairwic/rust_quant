use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, TimeZone, Utc};
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

#[path = "vegas_cross_asset_portfolio_replay/args.rs"]
mod args;
#[path = "vegas_cross_asset_portfolio_replay/event_clusters.rs"]
mod event_clusters;
#[path = "vegas_cross_asset_portfolio_replay/live_universe.rs"]
mod live_universe;
#[path = "vegas_cross_asset_portfolio_replay/mark_to_market.rs"]
mod mark_to_market;
#[path = "vegas_cross_asset_portfolio_replay/metrics.rs"]
mod metrics;
#[path = "vegas_cross_asset_portfolio_replay/rank_activation.rs"]
mod rank_activation;
#[path = "vegas_cross_asset_portfolio_replay/temporal_validation.rs"]
mod temporal_validation;
#[path = "vegas_cross_asset_portfolio_replay/universe_coverage.rs"]
mod universe_coverage;
use args::{
    market_rank_database_url, parse_args, quant_core_database_url, Args, RankActivationSource,
    DEFAULT_INITIAL_EQUITY,
};
use event_clusters::{
    build_event_cluster_report, load_event_context, EventClusterReport, EventContext,
};
use live_universe::{apply_live_universe, LiveUniverseReport};
use mark_to_market::{
    calculate_mark_to_market_audit, funding_interval_count, CandleMark, EquityPoint,
};
#[cfg(test)]
use mark_to_market::{FOUR_HOURS_MS, FUNDING_INTERVAL_MS};
use metrics::daily_equity_metrics;
use rank_activation::{apply_rank_activation, RankActivationReport};
use temporal_validation::{build_temporal_validation_report, TemporalValidationReport};
use universe_coverage::{load_universe_coverage, UniverseCoverageReport};

const BACKTEST_NAIVE_TIME_ZONE: &str = "Asia/Shanghai";

/// 单币种独立回测产生的一笔完整交易，经标准化后可按共享账户净值重新缩放。
#[derive(Debug, Clone, PartialEq)]
struct CandidateTrade {
    /// 开仓明细 ID，用于同分信号的稳定排序和错误定位。
    detail_id: i64,
    /// 该交易所属的 `back_test_log.id`。
    backtest_id: i64,
    /// 该币种回测实际使用的最小预热 K 线根数，用于区分短、中、长历史配置。
    min_k_line_num: usize,
    /// OKX 永续合约交易对。
    symbol: String,
    /// 开仓方向：`long` 或 `short`。
    side: String,
    /// 实际开仓时间，Unix 毫秒时间戳。
    open_ts: i64,
    /// 完整平仓时间，Unix 毫秒时间戳。
    close_ts: i64,
    /// 独立回测中的实际开仓价格。
    open_price: f64,
    /// 独立回测中的完整平仓价格。
    close_price: f64,
    /// 独立回测按当时账户权益计算出的持仓数量。
    quantity: f64,
    /// 已包含原回测手续费、尚未增加额外滑点的盈亏，单位 U。
    original_profit: f64,
    /// 单币种独立回测在该笔入场前的账户权益，单位 U。
    source_entry_equity: f64,
    /// 从原始毛收益与净收益反推的单边回测手续费率。
    base_fee_rate: f64,
    /// 扣除额外滑点后，相对该币种独立账户入场前权益的收益率。
    normalized_return: f64,
    /// 风控配置的账户风险预算：`max_loss_percent * position_leverage`。
    configured_risk_ratio: Option<f64>,
    /// 策略信号保护价到开仓价的损失金额，占源账户入场权益的比例。
    signal_stop_risk_ratio: Option<f64>,
    /// 信号保护价与最大亏损止损中更紧者，占源账户入场权益的比例。
    initial_stop_risk_ratio: Option<f64>,
    /// 入场 K 线成交量在前序窗口中的因果分位数。
    volume_percentile: f64,
    /// 入场 K 线成交量相对前序基线的倍数。
    relative_volume_ratio: f64,
    /// 入场时点的因果 RSI；旧记录缺失时为 None。
    entry_rsi: Option<f64>,
    /// 入场价相对 Vegas EMA 组的归一化偏离率。
    ema_distance_ratio: f64,
    /// 从入场棒到平仓棒的已确认 4H 行情路径。
    marks: Vec<CandleMark>,
}

/// 已被单账户容量规则接纳、尚未结算的仓位。
#[derive(Debug, Clone, PartialEq)]
struct ActivePosition {
    /// 已被容量规则接纳的原始候选交易。
    trade: CandidateTrade,
    /// 入场时共享账户已结算权益，用于等比例缩放该笔盈亏。
    entry_equity: f64,
}

/// 单账户实际接纳并结算的一笔交易结果。
#[derive(Debug, Clone, PartialEq)]
struct SettledTrade {
    /// 产生组合盈亏的交易对。
    symbol: String,
    /// 组合交易方向：`long` 或 `short`。
    side: String,
    /// 该交易所属回测配置的最小预热 K 线根数。
    min_k_line_num: usize,
    /// 组合结算时间，Unix 毫秒时间戳。
    close_ts: i64,
    /// 按共享账户入场权益缩放后的已实现盈亏，单位 U。
    profit: f64,
    /// 相对共享账户入场权益的单笔收益率。
    normalized_return: f64,
    /// 风控配置的账户风险预算比例。
    configured_risk_ratio: Option<f64>,
    /// 策略信号保护价风险占入场权益的比例。
    signal_stop_risk_ratio: Option<f64>,
    /// 逐笔初始保护价风险占入场权益的比例。
    initial_stop_risk_ratio: Option<f64>,
}

/// 执行价与对应 4H K 线范围不一致的可审计明细。
#[derive(Debug, Clone, Serialize, PartialEq)]
struct PricePathAnomaly {
    /// 开仓明细 ID。
    detail_id: i64,
    /// 异常交易对。
    symbol: String,
    /// `entry` 或 `exit`。
    phase: &'static str,
    /// 执行时间，Unix 毫秒时间戳。
    ts: i64,
    /// 回测记录的执行价格。
    execution_price: f64,
    /// 对应 4H K 线最低价。
    bar_low: f64,
    /// 对应 4H K 线最高价。
    bar_high: f64,
}

/// 单次容量回放结束后的内部状态，集中传递资金曲线和接纳统计。
#[derive(Debug, Clone, PartialEq)]
struct PortfolioSimulation {
    /// 进入容量分配前的完整候选交易数。
    candidate_count: usize,
    /// 被容量规则接纳的交易数。
    accepted: usize,
    /// 仅因并发仓位已满而跳过的交易数。
    skipped: usize,
    /// 回放中实际出现过的最大同时持仓数。
    max_active: usize,
    /// 全部仓位结算后的共享账户权益，单位 U。
    final_equity: f64,
    /// 仅按平仓结算权益计算的最大回撤，使用 0 到 1 的比率。
    max_drawdown: f64,
    /// 按每根 4H 收盘价盯市的最大回撤，使用 0 到 1 的比率。
    close_mark_max_drawdown: f64,
    /// 假设同一 4H 棒内所有仓位同时触及不利极值的保守最大回撤。
    intrabar_conservative_max_drawdown: f64,
    /// 保守盯市曲线中从峰值到谷值的最大绝对回撤，单位 U。
    intrabar_conservative_max_drawdown_amount: f64,
    /// 每个已确认 4H 时点结算后的收盘权益曲线。
    close_equity_curve: Vec<EquityPoint>,
    /// 同时持仓配置风险预算之和的历史最大值。
    max_configured_open_risk_ratio: Option<f64>,
    /// 具备从入场到出场连续 4H K 线路径的接纳交易数。
    fully_covered_positions: usize,
    /// 路径中缺少的预期 4H K 线总数。
    missing_4h_bars: usize,
    /// 入场价格不在对应入场 K 线高低范围内的交易数。
    entry_price_outside_bar_count: usize,
    /// 平仓价格不在对应平仓 K 线高低范围内的交易数。
    exit_price_outside_bar_count: usize,
    /// 执行价与行情路径不一致的逐笔证据。
    price_path_anomalies: Vec<PricePathAnomaly>,
    /// 按共享账户口径结算后的交易明细。
    settled: Vec<SettledTrade>,
    /// 按冻结规则聚类后的有效市场事件审计。
    event_cluster_audit: Option<EventClusterReport>,
}

/// 单个自然年的组合交易统计。
#[derive(Debug, Clone, Serialize, PartialEq)]
struct YearReport {
    /// 按完整平仓时间归属的 UTC 自然年。
    year: i32,
    /// 当年完整结算交易数。
    trades: usize,
    /// 当年正收益交易数。
    wins: usize,
    /// 当年胜率，单位百分比。
    win_rate_pct: f64,
    /// 当年已实现组合盈亏，单位 U。
    profit: f64,
}

/// 单个 UTC 自然月的组合成交与收益，用于识别只依赖少数行情窗口的候选。
#[derive(Debug, Clone, Serialize, PartialEq)]
struct MonthReport {
    /// `YYYY-MM`。
    month: String,
    /// 当月完整结算交易数。
    trades: usize,
    /// 当月正收益交易数。
    wins: usize,
    /// 当月已实现组合盈亏，单位 U。
    profit: f64,
}

/// 按实际开仓方向汇总的组合结果，用于识别只在单边行情有效的候选。
#[derive(Debug, Clone, Serialize, PartialEq)]
struct SideReport {
    /// `long` 或 `short`。
    side: String,
    /// 该方向完整结算交易数。
    trades: usize,
    /// 该方向正收益交易数。
    wins: usize,
    /// 该方向胜率，单位百分比。
    win_rate_pct: f64,
    /// 该方向按共享账户口径结算的盈亏，单位 U。
    profit: f64,
}

/// 按回测预热档位汇总的组合表现，用于判断新上市币是否真实贡献交易和利润。
#[derive(Debug, Clone, Serialize, PartialEq)]
struct WarmupReport {
    /// 该档位配置的 `min_k_line_num`。
    min_k_line_num: usize,
    /// 共享账户实际接纳并结算的交易数。
    trades: usize,
    /// 正收益交易数。
    wins: usize,
    /// 组合胜率，单位百分比。
    win_rate_pct: f64,
    /// 该档位按共享账户口径结算的盈亏，单位 U。
    profit: f64,
}

/// 单账户跨币种回放报告。
#[derive(Debug, Clone, Serialize, PartialEq)]
struct PortfolioReport {
    /// 报告对应的最小回测 ID，包含边界。
    backtest_id_min: i64,
    /// 报告对应的最大回测 ID，包含边界。
    backtest_id_max: i64,
    /// 共享账户起始权益，单位 U。
    initial_equity: f64,
    /// 共享账户最终权益，单位 U。
    final_equity: f64,
    /// 对原回测仓位、收益和风险采用的统一线性缩放倍数。
    risk_scale: f64,
    /// 相对起始权益的复利总收益率，单位百分比。
    total_return_pct: f64,
    /// 所有币种独立回测产生的完整候选交易数。
    candidate_trades: usize,
    /// 共享账户容量规则实际接纳的交易数。
    accepted_trades: usize,
    /// 因并发容量已满而跳过的交易数。
    skipped_by_capacity: usize,
    /// 实际结算过交易的不同币种数量。
    traded_symbols: usize,
    /// 组合正收益交易数。
    wins: usize,
    /// 组合胜率，单位百分比。
    win_rate_pct: f64,
    /// 二项分布 Wilson 95% 胜率区间下界；空样本为 None。
    win_rate_wilson_95_low_pct: Option<f64>,
    /// 二项分布 Wilson 95% 胜率区间上界；空样本为 None。
    win_rate_wilson_95_high_pct: Option<f64>,
    /// 多空方向中交易数最多的一侧占全部交易的比例。
    dominant_side_trade_share_pct: Option<f64>,
    /// 已包含回测手续费及本次压力成本的总正收益除以总负收益绝对值。
    profit_factor: Option<f64>,
    /// 未年度化的交易级 Sharpe；样本不足时为 `None`。
    trade_sharpe: Option<f64>,
    /// UTC 日频权益收益按 `sqrt(365)` 年化的 Sharpe。
    daily_sharpe_sqrt_365: Option<f64>,
    /// 日频 Sharpe 使用的连续日历日观察数。
    daily_equity_observations: usize,
    /// 净利润除以保守盯市最大绝对回撤。
    recovery_factor: Option<f64>,
    /// 按逐笔初始保护价风险归一后的成本后平均期望。
    net_expectancy_r: Option<f64>,
    /// 具备可审计初始保护价风险的已接纳交易数。
    initial_stop_risk_covered_trades: usize,
    /// 逐笔初始保护价风险占入场权益的平均百分比。
    average_initial_stop_risk_pct: Option<f64>,
    /// 逐笔初始保护价风险占入场权益的最大百分比。
    max_initial_stop_risk_pct: Option<f64>,
    /// 策略信号保护价风险占入场权益的平均百分比。
    average_signal_stop_risk_pct: Option<f64>,
    /// 策略信号保护价风险占入场权益的最大百分比。
    max_signal_stop_risk_pct: Option<f64>,
    /// 仅使用平仓结算权益计算的回撤，不包含持仓期间盘中浮亏。
    realized_max_drawdown_pct: f64,
    /// 使用已确认 4H 收盘价逐棒盯市的最大回撤。
    close_mark_max_drawdown_pct: f64,
    /// 使用每根 4H 棒不利高低价估算的保守回撤上界。
    intrabar_conservative_max_drawdown_pct: f64,
    /// 保守盯市曲线最大绝对回撤，单位 U。
    intrabar_conservative_max_drawdown_amount: f64,
    /// 命令配置的最大并发仓位数。
    max_concurrent: usize,
    /// 回放中实际达到的最大同时持仓数。
    max_active_positions: usize,
    /// 已接纳交易的平均配置风险预算，单位账户权益百分比。
    configured_risk_per_trade_pct: Option<f64>,
    /// 已接纳交易配置风险预算的最小值。
    min_configured_risk_per_trade_pct: Option<f64>,
    /// 已接纳交易配置风险预算的最大值。
    max_configured_risk_per_trade_pct: Option<f64>,
    /// 实际同时持仓的配置风险预算之和峰值。
    max_configured_open_risk_pct: Option<f64>,
    /// 具备可审计配置风险预算的已接纳交易数。
    configured_risk_covered_trades: usize,
    /// 在原回测手续费之外额外扣除的单边滑点，单位 bps。
    extra_slippage_bps: f64,
    /// 每个 8 小时结算点扣除的资金费率压力，单位 bps。
    funding_bps_per_8h: f64,
    /// 可选的历史横截面排名激活审计；None 表示本次未应用该代理。
    rank_activation: Option<RankActivationReport>,
    /// 做空入场允许的最大 EMA 偏离率；None 表示未应用该研究过滤器。
    max_short_ema_distance_ratio: Option<f64>,
    /// 因超过做空 EMA 偏离率上限而被过滤的交易数。
    filtered_by_short_ema_distance: usize,
    /// legacy 回测明细中无时区时间所采用的解释时区。
    backtest_naive_timezone: &'static str,
    /// 具备连续 4H K 线路径的接纳交易数。
    fully_covered_positions: usize,
    /// 接纳交易路径中缺少的 4H K 线总数。
    missing_4h_bars: usize,
    /// 入场价格落在对应入场棒范围外的交易数。
    entry_price_outside_bar_count: usize,
    /// 平仓价格落在对应平仓棒范围外的交易数。
    exit_price_outside_bar_count: usize,
    /// 执行价与行情路径不一致的逐笔证据。
    price_path_anomalies: Vec<PricePathAnomaly>,
    /// 正贡献最高的交易对；没有正贡献币种时为 `None`。
    top_positive_symbol: Option<String>,
    /// 最大正贡献币种占所有正贡献币种利润之和的比例。
    top_positive_symbol_profit_share_pct: Option<f64>,
    /// 固定执行集合移除最大正贡献交易后的净利润，单位 U。
    fixed_execution_profit_without_top1_trade: f64,
    /// 固定执行集合移除前三大正贡献交易后的净利润，单位 U。
    fixed_execution_profit_without_top3_trades: f64,
    /// 固定执行集合移除最大正贡献币种后的净利润，单位 U。
    fixed_execution_profit_without_top1_symbol: Option<f64>,
    /// 按完整平仓 UTC 年份汇总的组合结果。
    yearly: Vec<YearReport>,
    /// 按完整平仓 UTC 月份汇总的组合结果。
    monthly: Vec<MonthReport>,
    /// 按实际开仓方向汇总的结果。
    by_side: Vec<SideReport>,
    /// 按 `min_k_line_num` 档位汇总的结果。
    by_min_k_line_num: Vec<WarmupReport>,
    /// 同方向、BTC 状态、12 小时锚点和 90 日相关性口径的事件审计。
    event_cluster_audit: Option<EventClusterReport>,
    /// 历史 Top100 重建所需的本地数据覆盖审计。
    historical_universe_coverage: Option<UniverseCoverageReport>,
    /// current-live-only 月度成交额 Top100 的实际成员过滤审计。
    live_universe_audit: Option<LiveUniverseReport>,
    /// 严格时间样本外和固定参数滚动验证；未请求时仍返回空结构以保留审计口径。
    temporal_validation: Option<TemporalValidationReport>,
}

#[tokio::main]
/// 从 quant_core 只读加载回测交易并输出单账户 JSON 报告，不写策略或订单事实。
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_args(std::env::args().skip(1))?;
    let database_url = quant_core_database_url()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core for Vegas portfolio replay")?;
    let rank_event_pool = if args.rank_activation_min_delta.is_some()
        && args.rank_activation_source == RankActivationSource::MarketRankEvents
    {
        Some(
            PgPoolOptions::new()
                .max_connections(2)
                .connect(&market_rank_database_url()?)
                .await
                .context("connect archived market-rank database for Vegas portfolio replay")?,
        )
    } else {
        None
    };
    let mut trades = load_candidate_trades(&pool, args).await?;
    if let Some(side_filter) = args.side_filter {
        trades.retain(|trade| side_filter.matches(&trade.side));
    }
    let live_universe = apply_live_universe(&pool, &mut trades, args).await?;
    let rank_activation =
        apply_rank_activation(&pool, rank_event_pool.as_ref(), &mut trades, args).await?;
    let filtered_by_short_ema_distance = apply_short_ema_distance_filter(&mut trades, args);
    let event_context = load_event_context(&pool, &trades).await?;
    let universe_coverage = load_universe_coverage(&pool, args).await?;
    let temporal_validation =
        build_temporal_validation_report(&trades, args, Some(&event_context))?;
    let mut report = simulate_portfolio_with_event_context(trades, args, Some(&event_context))?;
    report.rank_activation = rank_activation;
    report.filtered_by_short_ema_distance = filtered_by_short_ema_distance;
    report.historical_universe_coverage = Some(universe_coverage);
    report.live_universe_audit = live_universe;
    report.temporal_validation = Some(temporal_validation);
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// 加载完整开平仓对，并用该币种入场前净值把收益归一化，供共享账户等比例复算。
async fn load_candidate_trades(pool: &PgPool, args: Args) -> Result<Vec<CandidateTrade>> {
    let has_initial_stop_column = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
              FROM information_schema.columns
             WHERE table_schema = 'public'
               AND table_name = 'back_test_detail'
               AND column_name = 'initial_stop_price'
        )
        "#,
    )
    .fetch_one(pool)
    .await
    .context("detect initial_stop_price compatibility")?;
    let initial_stop_expression = if has_initial_stop_column {
        "COALESCE(c.initial_stop_price, (NULLIF(c.stop_loss_update_history, '')::jsonb -> 0 ->> 'new_price')::double precision)"
    } else {
        "(NULLIF(c.stop_loss_update_history, '')::jsonb -> 0 ->> 'new_price')::double precision"
    };
    let query = r#"
        SELECT o.id AS detail_id,
               o.back_test_id,
               (b.strategy_detail::jsonb->>'min_k_line_num')::bigint AS min_k_line_num,
               o.inst_id AS symbol,
               o.option_type AS side,
               (EXTRACT(EPOCH FROM (
                   o.open_position_time AT TIME ZONE 'Asia/Shanghai'
               )) * 1000)::bigint AS open_ts,
               (EXTRACT(EPOCH FROM (
                   c.close_position_time AT TIME ZONE 'Asia/Shanghai'
               )) * 1000)::bigint AS close_ts,
               o.open_price::double precision AS open_price,
               c.close_price::double precision AS close_price,
               o.quantity::double precision AS quantity,
               c.profit_loss::double precision AS profit,
               (
                   (b.risk_config_detail::jsonb->>'max_loss_percent')::double precision
                   * COALESCE(
                       (b.risk_config_detail::jsonb->>'position_leverage')::double precision,
                       1.0
                   )
               ) AS configured_risk_ratio,
               __INITIAL_STOP_EXPRESSION__ AS initial_stop_price,
               COALESCE(
                   (NULLIF(o.signal_value, '')::jsonb #>>
                       '{cross_asset_adaptive_value,volume_percentile}')::double precision,
                   0.0
               ) AS volume_percentile,
               COALESCE(
                   (NULLIF(o.signal_value, '')::jsonb #>>
                       '{cross_asset_adaptive_value,relative_volume_ratio}')::double precision,
                   0.0
               ) AS relative_volume_ratio,
               (NULLIF(o.signal_value, '')::jsonb #>>
                   '{rsi_value,rsi_value}')::double precision AS entry_rsi
               ,COALESCE(
                   (NULLIF(o.signal_value, '')::jsonb #>>
                       '{ema_distance_filter,distance_ratio}')::double precision,
                   0.0
               ) AS ema_distance_ratio
          FROM back_test_detail o
          JOIN back_test_log b
            ON b.id = o.back_test_id
          JOIN back_test_detail c
            ON c.back_test_id = o.back_test_id
           AND c.open_position_time = o.open_position_time
           AND c.option_type = 'close'
           AND c.full_close = 'true'
         WHERE o.back_test_id BETWEEN $1 AND $2
           AND o.option_type IN ('long', 'short')
         ORDER BY o.inst_id, o.open_position_time, o.id
        "#
    .replace("__INITIAL_STOP_EXPRESSION__", initial_stop_expression);
    let rows = sqlx::query(&query)
        .bind(args.backtest_id_min)
        .bind(args.backtest_id_max)
        .fetch_all(pool)
        .await
        .context("load completed Vegas trades")?;

    let mut original_equity_by_symbol = HashMap::<String, f64>::new();
    let mut trades = Vec::with_capacity(rows.len());
    for row in rows {
        let symbol = row.try_get::<String, _>("symbol")?;
        let original_equity = original_equity_by_symbol
            .entry(symbol.clone())
            .or_insert(DEFAULT_INITIAL_EQUITY);
        if !original_equity.is_finite() || *original_equity <= 0.0 {
            bail!("non-positive source equity before trade for {symbol}");
        }
        let open_price = row.try_get::<f64, _>("open_price")?;
        let close_price = row.try_get::<f64, _>("close_price")?;
        let quantity = row.try_get::<f64, _>("quantity")?;
        let original_profit = row.try_get::<f64, _>("profit")?;
        let side = row.try_get::<String, _>("side")?;
        let gross_profit = match side.as_str() {
            "long" => (close_price - open_price) * quantity,
            "short" => (open_price - close_price) * quantity,
            _ => bail!("unsupported trade side: {side}"),
        };
        let fee_notional = quantity * (open_price + close_price);
        if fee_notional <= 0.0 {
            bail!("non-positive fee notional for {symbol}");
        }
        let base_fee_rate = (gross_profit - original_profit) / fee_notional;
        if !base_fee_rate.is_finite() || base_fee_rate < -1e-10 {
            bail!("invalid inferred fee rate for {symbol}: {base_fee_rate}");
        }
        let open_ts = row.try_get::<i64, _>("open_ts")?;
        let close_ts = row.try_get::<i64, _>("close_ts")?;
        let extra_slippage =
            quantity * (open_price + close_price) * args.extra_slippage_bps / 10_000.0;
        let funding_cost = quantity
            * open_price
            * funding_interval_count(open_ts, close_ts) as f64
            * args.funding_bps_per_8h
            / 10_000.0;
        let source_entry_equity = *original_equity;
        let normalized_return = (original_profit - extra_slippage - funding_cost)
            / source_entry_equity
            * args.risk_scale;
        let configured_risk_ratio = validated_positive_ratio(
            row.try_get("configured_risk_ratio")?,
            "configured risk",
            &symbol,
        )?
        .map(|risk| risk * args.risk_scale);
        let signal_stop_risk_ratio = signal_stop_risk_ratio(
            &side,
            open_price,
            row.try_get("initial_stop_price")?,
            quantity,
            source_entry_equity,
            &symbol,
        )?
        .map(|risk| risk * args.risk_scale);
        let initial_stop_risk_ratio =
            effective_initial_stop_risk_ratio(configured_risk_ratio, signal_stop_risk_ratio);
        *original_equity += original_profit;
        trades.push(CandidateTrade {
            detail_id: row.try_get("detail_id")?,
            backtest_id: row.try_get("back_test_id")?,
            min_k_line_num: usize::try_from(row.try_get::<i64, _>("min_k_line_num")?)
                .context("min_k_line_num exceeds usize")?,
            symbol,
            side,
            open_ts,
            close_ts,
            open_price,
            close_price,
            quantity: quantity * args.risk_scale,
            original_profit: original_profit * args.risk_scale,
            source_entry_equity,
            base_fee_rate: base_fee_rate.max(0.0),
            normalized_return,
            configured_risk_ratio,
            signal_stop_risk_ratio,
            initial_stop_risk_ratio,
            volume_percentile: row.try_get("volume_percentile")?,
            relative_volume_ratio: row.try_get("relative_volume_ratio")?,
            entry_rsi: row.try_get("entry_rsi")?,
            ema_distance_ratio: row.try_get("ema_distance_ratio")?,
            marks: Vec::new(),
        });
    }
    attach_candle_marks(pool, &mut trades).await?;
    Ok(trades)
}

fn validated_positive_ratio(value: Option<f64>, label: &str, symbol: &str) -> Result<Option<f64>> {
    match value {
        Some(value) if value.is_finite() && value > 0.0 => Ok(Some(value)),
        Some(value) => bail!("invalid {label} for {symbol}: {value}"),
        None => Ok(None),
    }
}

fn signal_stop_risk_ratio(
    side: &str,
    open_price: f64,
    initial_stop_price: Option<f64>,
    quantity: f64,
    source_entry_equity: f64,
    symbol: &str,
) -> Result<Option<f64>> {
    let Some(stop_price) = initial_stop_price else {
        return Ok(None);
    };
    let is_protective = stop_price.is_finite()
        && stop_price > 0.0
        && match side {
            "long" => stop_price < open_price,
            "short" => stop_price > open_price,
            _ => false,
        };
    if !is_protective {
        bail!(
            "invalid initial stop for {symbol}: side={side} entry={open_price} stop={stop_price}"
        );
    }
    validated_positive_ratio(
        Some((open_price - stop_price).abs() * quantity / source_entry_equity),
        "initial stop risk",
        symbol,
    )
}

fn effective_initial_stop_risk_ratio(
    configured_risk_ratio: Option<f64>,
    signal_stop_risk_ratio: Option<f64>,
) -> Option<f64> {
    match (configured_risk_ratio, signal_stop_risk_ratio) {
        (Some(configured), Some(signal)) => Some(configured.min(signal)),
        (Some(configured), None) => Some(configured),
        (None, Some(signal)) => Some(signal),
        (None, None) => None,
    }
}

/// 在动量激活后剔除已经远离 EMA 的追空信号；多单不受该研究过滤器影响。
fn apply_short_ema_distance_filter(trades: &mut Vec<CandidateTrade>, args: Args) -> usize {
    let Some(max_ratio) = args.max_short_ema_distance_ratio else {
        return 0;
    };
    let before = trades.len();
    trades.retain(|trade| trade.side != "short" || trade.ema_distance_ratio <= max_ratio);
    before - trades.len()
}

/// 按币种批量加载覆盖交易区间的 4H K 线，再切分到每笔交易以避免逐笔查询。
async fn attach_candle_marks(pool: &PgPool, trades: &mut [CandidateTrade]) -> Result<()> {
    let mut ranges = HashMap::<String, (i64, i64)>::new();
    for trade in trades.iter() {
        let range = ranges
            .entry(trade.symbol.clone())
            .or_insert((trade.open_ts, trade.close_ts));
        range.0 = range.0.min(trade.open_ts);
        range.1 = range.1.max(trade.close_ts);
    }

    let mut marks_by_symbol = HashMap::<String, Vec<CandleMark>>::new();
    for (symbol, (min_ts, max_ts)) in ranges {
        let table_name = quoted_4h_candle_table(&symbol)?;
        let query = format!(
            "SELECT ts, h::double precision AS high, l::double precision AS low, \
             c::double precision AS close FROM {table_name} \
             WHERE ts BETWEEN $1 AND $2 AND confirm = '1' ORDER BY ts"
        );
        let rows = sqlx::query(&query)
            .bind(min_ts)
            .bind(max_ts)
            .fetch_all(pool)
            .await
            .with_context(|| format!("load 4H candle path for {symbol}"))?;
        let mut marks = Vec::with_capacity(rows.len());
        for row in rows {
            let mark = CandleMark {
                ts: row.try_get("ts")?,
                high: row.try_get("high")?,
                low: row.try_get("low")?,
                close: row.try_get("close")?,
            };
            if !mark.high.is_finite()
                || !mark.low.is_finite()
                || !mark.close.is_finite()
                || mark.low <= 0.0
                || mark.high < mark.low
            {
                bail!("invalid 4H candle path for {symbol} at {}", mark.ts);
            }
            marks.push(mark);
        }
        marks_by_symbol.insert(symbol, marks);
    }

    for trade in trades {
        let symbol_marks = marks_by_symbol
            .get(&trade.symbol)
            .ok_or_else(|| anyhow!("missing 4H candle series for {}", trade.symbol))?;
        trade.marks = symbol_marks
            .iter()
            .copied()
            .filter(|mark| mark.ts >= trade.open_ts && mark.ts <= trade.close_ts)
            .collect();
    }
    Ok(())
}

/// 将受控交易对转换成 quoted legacy 4H 分表名，禁止任意 SQL 标识符注入。
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

/// 按因果量价强度处理同一时刻的竞争信号，并在共享账户上执行容量受限复利回放。
#[cfg(test)]
fn simulate_portfolio(candidates: Vec<CandidateTrade>, args: Args) -> Result<PortfolioReport> {
    simulate_portfolio_with_event_context(candidates, args, None)
}

/// 在共享账户回放结束后，用预先加载的因果行情上下文生成事件聚类审计。
fn simulate_portfolio_with_event_context(
    mut candidates: Vec<CandidateTrade>,
    args: Args,
    event_context: Option<&EventContext>,
) -> Result<PortfolioReport> {
    for trade in &candidates {
        if trade.close_ts < trade.open_ts
            || !trade.normalized_return.is_finite()
            || !trade.source_entry_equity.is_finite()
            || trade.source_entry_equity <= 0.0
            || trade.marks.windows(2).any(|pair| pair[0].ts >= pair[1].ts)
        {
            bail!("invalid completed trade {}", trade.detail_id);
        }
    }
    candidates.sort_by(compare_entry_priority);
    let candidate_count = candidates.len();
    let mut equity = args.initial_equity;
    let mut peak = equity;
    let mut max_drawdown = 0.0_f64;
    let mut active = Vec::<ActivePosition>::new();
    let mut settled = Vec::<SettledTrade>::new();
    let mut accepted_positions = Vec::<ActivePosition>::new();
    let mut accepted = 0_usize;
    let mut skipped = 0_usize;
    let mut max_active = 0_usize;
    let mut configured_risk_covered = 0_usize;
    let mut max_configured_open_risk_ratio = 0.0_f64;
    let mut index = 0_usize;

    while index < candidates.len() {
        let open_ts = candidates[index].open_ts;
        settle_through(
            &mut active,
            open_ts,
            &mut equity,
            &mut peak,
            &mut max_drawdown,
            &mut settled,
        );
        let group_end = candidates[index..]
            .iter()
            .position(|trade| trade.open_ts != open_ts)
            .map(|offset| index + offset)
            .unwrap_or(candidates.len());
        for trade in &candidates[index..group_end] {
            if active.len() >= args.max_concurrent {
                skipped += 1;
                continue;
            }
            let position = ActivePosition {
                trade: trade.clone(),
                entry_equity: equity,
            };
            accepted_positions.push(position.clone());
            active.push(position);
            accepted += 1;
            configured_risk_covered += usize::from(trade.configured_risk_ratio.is_some());
        }
        max_active = max_active.max(active.len());
        if active
            .iter()
            .all(|position| position.trade.configured_risk_ratio.is_some())
        {
            let open_risk = active
                .iter()
                .filter_map(|position| position.trade.configured_risk_ratio)
                .sum::<f64>();
            max_configured_open_risk_ratio = max_configured_open_risk_ratio.max(open_risk);
        }
        // 同一根 K 线开平的交易也参与当时的容量竞争，但不会占用下一时点的仓位。
        settle_through(
            &mut active,
            open_ts,
            &mut equity,
            &mut peak,
            &mut max_drawdown,
            &mut settled,
        );
        index = group_end;
    }
    settle_through(
        &mut active,
        i64::MAX,
        &mut equity,
        &mut peak,
        &mut max_drawdown,
        &mut settled,
    );
    let mark_to_market = calculate_mark_to_market_audit(&accepted_positions, args)?;
    let event_cluster_audit = event_context
        .map(|context| build_event_cluster_report(&accepted_positions, context))
        .transpose()?;

    build_report(
        args,
        PortfolioSimulation {
            candidate_count,
            accepted,
            skipped,
            max_active,
            final_equity: equity,
            max_drawdown,
            close_mark_max_drawdown: mark_to_market.close_mark_max_drawdown,
            intrabar_conservative_max_drawdown: mark_to_market.intrabar_conservative_max_drawdown,
            intrabar_conservative_max_drawdown_amount: mark_to_market
                .intrabar_conservative_max_drawdown_amount,
            close_equity_curve: mark_to_market.close_equity_curve,
            max_configured_open_risk_ratio: (accepted > 0 && configured_risk_covered == accepted)
                .then_some(max_configured_open_risk_ratio),
            fully_covered_positions: mark_to_market.fully_covered_positions,
            missing_4h_bars: mark_to_market.missing_4h_bars,
            entry_price_outside_bar_count: mark_to_market.entry_price_outside_bar_count,
            exit_price_outside_bar_count: mark_to_market.exit_price_outside_bar_count,
            price_path_anomalies: mark_to_market.price_path_anomalies,
            settled,
            event_cluster_audit,
        },
    )
}

/// 同时到达的信号只按入场时可见指标排序，禁止用最终盈亏或平仓时间挑选赢家。
fn compare_entry_priority(left: &CandidateTrade, right: &CandidateTrade) -> Ordering {
    left.open_ts
        .cmp(&right.open_ts)
        .then_with(|| right.volume_percentile.total_cmp(&left.volume_percentile))
        .then_with(|| {
            right
                .relative_volume_ratio
                .total_cmp(&left.relative_volume_ratio)
        })
        .then_with(|| left.symbol.cmp(&right.symbol))
        .then_with(|| left.detail_id.cmp(&right.detail_id))
}

/// 将截止时点已平仓的仓位按同一平仓时间批量结算，避免同刻结算顺序扭曲回撤。
fn settle_through(
    active: &mut Vec<ActivePosition>,
    cutoff_ts: i64,
    equity: &mut f64,
    peak: &mut f64,
    max_drawdown: &mut f64,
    settled: &mut Vec<SettledTrade>,
) {
    loop {
        let Some(close_ts) = active
            .iter()
            .filter(|position| position.trade.close_ts <= cutoff_ts)
            .map(|position| position.trade.close_ts)
            .min()
        else {
            break;
        };
        let mut remaining = Vec::with_capacity(active.len());
        let mut group_profit = 0.0_f64;
        for position in active.drain(..) {
            if position.trade.close_ts == close_ts {
                let profit = position.entry_equity * position.trade.normalized_return;
                group_profit += profit;
                settled.push(SettledTrade {
                    symbol: position.trade.symbol,
                    side: position.trade.side,
                    min_k_line_num: position.trade.min_k_line_num,
                    close_ts,
                    profit,
                    normalized_return: position.trade.normalized_return,
                    configured_risk_ratio: position.trade.configured_risk_ratio,
                    signal_stop_risk_ratio: position.trade.signal_stop_risk_ratio,
                    initial_stop_risk_ratio: position.trade.initial_stop_risk_ratio,
                });
            } else {
                remaining.push(position);
            }
        }
        *active = remaining;
        *equity += group_profit;
        *peak = peak.max(*equity);
        if *peak > 0.0 {
            *max_drawdown = max_drawdown.max((*peak - *equity) / *peak);
        }
    }
}

/// 将结算明细聚合为组合、年度和集中度指标。
fn build_report(args: Args, simulation: PortfolioSimulation) -> Result<PortfolioReport> {
    if simulation.accepted != simulation.settled.len() {
        bail!("accepted and settled trade counts differ");
    }
    let wins = simulation
        .settled
        .iter()
        .filter(|trade| trade.profit > 0.0)
        .count();
    let gross_profit = simulation
        .settled
        .iter()
        .filter(|trade| trade.profit > 0.0)
        .map(|trade| trade.profit)
        .sum::<f64>();
    let gross_loss = simulation
        .settled
        .iter()
        .filter(|trade| trade.profit < 0.0)
        .map(|trade| trade.profit.abs())
        .sum::<f64>();
    let mut by_year = BTreeMap::<i32, (usize, usize, f64)>::new();
    let mut by_month = BTreeMap::<(i32, u32), (usize, usize, f64)>::new();
    let mut by_symbol = HashMap::<String, f64>::new();
    let mut by_side = BTreeMap::<String, (usize, usize, f64)>::new();
    let mut by_min_k_line_num = BTreeMap::<usize, (usize, usize, f64)>::new();
    for trade in &simulation.settled {
        let close_time = Utc
            .timestamp_millis_opt(trade.close_ts)
            .single()
            .ok_or_else(|| anyhow!("invalid close timestamp: {}", trade.close_ts))?;
        let year = close_time.year();
        let entry = by_year.entry(year).or_default();
        entry.0 += 1;
        entry.1 += usize::from(trade.profit > 0.0);
        entry.2 += trade.profit;
        let month_entry = by_month.entry((year, close_time.month())).or_default();
        month_entry.0 += 1;
        month_entry.1 += usize::from(trade.profit > 0.0);
        month_entry.2 += trade.profit;
        *by_symbol.entry(trade.symbol.clone()).or_default() += trade.profit;
        let side_entry = by_side.entry(trade.side.clone()).or_default();
        side_entry.0 += 1;
        side_entry.1 += usize::from(trade.profit > 0.0);
        side_entry.2 += trade.profit;
        let warmup_entry = by_min_k_line_num.entry(trade.min_k_line_num).or_default();
        warmup_entry.0 += 1;
        warmup_entry.1 += usize::from(trade.profit > 0.0);
        warmup_entry.2 += trade.profit;
    }
    let yearly = by_year
        .into_iter()
        .map(|(year, (trades, wins, profit))| YearReport {
            year,
            trades,
            wins,
            win_rate_pct: percentage(wins as f64, trades as f64),
            profit,
        })
        .collect::<Vec<_>>();
    let monthly = by_month
        .into_iter()
        .map(|((year, month), (trades, wins, profit))| MonthReport {
            month: format!("{year:04}-{month:02}"),
            trades,
            wins,
            profit,
        })
        .collect::<Vec<_>>();
    let side_reports = by_side
        .into_iter()
        .map(|(side, (trades, wins, profit))| SideReport {
            side,
            trades,
            wins,
            win_rate_pct: percentage(wins as f64, trades as f64),
            profit,
        })
        .collect::<Vec<_>>();
    let warmup_reports = by_min_k_line_num
        .into_iter()
        .map(|(min_k_line_num, (trades, wins, profit))| WarmupReport {
            min_k_line_num,
            trades,
            wins,
            win_rate_pct: percentage(wins as f64, trades as f64),
            profit,
        })
        .collect::<Vec<_>>();
    let dominant_side_trade_share_pct = side_reports
        .iter()
        .map(|report| report.trades)
        .max()
        .map(|trades| percentage(trades as f64, simulation.accepted as f64));
    let win_rate_interval = wilson_interval_95(wins, simulation.accepted);
    let positive_symbol_profit = by_symbol
        .values()
        .copied()
        .filter(|profit| *profit > 0.0)
        .sum::<f64>();
    let top_positive = by_symbol
        .iter()
        .filter(|(_, profit)| **profit > 0.0)
        .max_by(|left, right| left.1.total_cmp(right.1));
    let fixed_execution_net_profit = simulation.final_equity - args.initial_equity;
    let mut positive_trade_profits = simulation
        .settled
        .iter()
        .map(|trade| trade.profit)
        .filter(|profit| *profit > 0.0)
        .collect::<Vec<_>>();
    positive_trade_profits.sort_by(|left, right| right.total_cmp(left));
    let top1_trade_profit = positive_trade_profits.first().copied().unwrap_or(0.0);
    let top3_trade_profit = positive_trade_profits.iter().take(3).sum::<f64>();
    let returns = simulation
        .settled
        .iter()
        .map(|trade| trade.normalized_return)
        .collect::<Vec<_>>();
    let configured_risks = simulation
        .settled
        .iter()
        .filter_map(|trade| trade.configured_risk_ratio)
        .collect::<Vec<_>>();
    let configured_risk_complete =
        simulation.accepted > 0 && configured_risks.len() == simulation.accepted;
    let configured_risk_average = configured_risk_complete
        .then(|| configured_risks.iter().sum::<f64>() / configured_risks.len() as f64);
    let configured_risk_min = configured_risk_complete
        .then(|| configured_risks.iter().copied().reduce(f64::min))
        .flatten();
    let configured_risk_max = configured_risk_complete
        .then(|| configured_risks.iter().copied().reduce(f64::max))
        .flatten();
    let initial_stop_risks = simulation
        .settled
        .iter()
        .filter_map(|trade| trade.initial_stop_risk_ratio)
        .collect::<Vec<_>>();
    let signal_stop_risks = simulation
        .settled
        .iter()
        .filter_map(|trade| trade.signal_stop_risk_ratio)
        .collect::<Vec<_>>();
    let initial_stop_risk_complete =
        simulation.accepted > 0 && initial_stop_risks.len() == simulation.accepted;
    let initial_stop_risk_average = initial_stop_risk_complete
        .then(|| initial_stop_risks.iter().sum::<f64>() / initial_stop_risks.len() as f64);
    let initial_stop_risk_max = initial_stop_risk_complete
        .then(|| initial_stop_risks.iter().copied().reduce(f64::max))
        .flatten();
    let signal_stop_risk_average = (!signal_stop_risks.is_empty())
        .then(|| signal_stop_risks.iter().sum::<f64>() / signal_stop_risks.len() as f64);
    let signal_stop_risk_max = signal_stop_risks.iter().copied().reduce(f64::max);
    let net_expectancy_r = initial_stop_risk_complete.then(|| {
        simulation
            .settled
            .iter()
            .map(|trade| {
                trade.normalized_return
                    / trade
                        .initial_stop_risk_ratio
                        .expect("complete initial stop coverage")
            })
            .sum::<f64>()
            / simulation.accepted as f64
    });
    let daily_metrics = daily_equity_metrics(args.initial_equity, &simulation.close_equity_curve);
    let recovery_factor = (simulation.intrabar_conservative_max_drawdown_amount > 0.0).then(|| {
        (simulation.final_equity - args.initial_equity)
            / simulation.intrabar_conservative_max_drawdown_amount
    });

    Ok(PortfolioReport {
        backtest_id_min: args.backtest_id_min,
        backtest_id_max: args.backtest_id_max,
        initial_equity: args.initial_equity,
        final_equity: simulation.final_equity,
        risk_scale: args.risk_scale,
        total_return_pct: percentage(
            simulation.final_equity - args.initial_equity,
            args.initial_equity,
        ),
        candidate_trades: simulation.candidate_count,
        accepted_trades: simulation.accepted,
        skipped_by_capacity: simulation.skipped,
        traded_symbols: by_symbol.len(),
        wins,
        win_rate_pct: percentage(wins as f64, simulation.accepted as f64),
        win_rate_wilson_95_low_pct: win_rate_interval.map(|interval| interval.0),
        win_rate_wilson_95_high_pct: win_rate_interval.map(|interval| interval.1),
        dominant_side_trade_share_pct,
        profit_factor: (gross_loss > 0.0).then_some(gross_profit / gross_loss),
        trade_sharpe: trade_sharpe(&returns),
        daily_sharpe_sqrt_365: daily_metrics.annualized_sharpe_sqrt_365,
        daily_equity_observations: daily_metrics.observations,
        recovery_factor,
        net_expectancy_r,
        initial_stop_risk_covered_trades: initial_stop_risks.len(),
        average_initial_stop_risk_pct: initial_stop_risk_average.map(|value| value * 100.0),
        max_initial_stop_risk_pct: initial_stop_risk_max.map(|value| value * 100.0),
        average_signal_stop_risk_pct: signal_stop_risk_average.map(|value| value * 100.0),
        max_signal_stop_risk_pct: signal_stop_risk_max.map(|value| value * 100.0),
        realized_max_drawdown_pct: simulation.max_drawdown * 100.0,
        close_mark_max_drawdown_pct: simulation.close_mark_max_drawdown * 100.0,
        intrabar_conservative_max_drawdown_pct: simulation.intrabar_conservative_max_drawdown
            * 100.0,
        intrabar_conservative_max_drawdown_amount: simulation
            .intrabar_conservative_max_drawdown_amount,
        max_concurrent: args.max_concurrent,
        max_active_positions: simulation.max_active,
        configured_risk_per_trade_pct: configured_risk_average.map(|value| value * 100.0),
        min_configured_risk_per_trade_pct: configured_risk_min.map(|value| value * 100.0),
        max_configured_risk_per_trade_pct: configured_risk_max.map(|value| value * 100.0),
        max_configured_open_risk_pct: simulation
            .max_configured_open_risk_ratio
            .map(|value| value * 100.0),
        configured_risk_covered_trades: configured_risks.len(),
        extra_slippage_bps: args.extra_slippage_bps,
        funding_bps_per_8h: args.funding_bps_per_8h,
        rank_activation: None,
        max_short_ema_distance_ratio: args.max_short_ema_distance_ratio,
        filtered_by_short_ema_distance: 0,
        backtest_naive_timezone: BACKTEST_NAIVE_TIME_ZONE,
        fully_covered_positions: simulation.fully_covered_positions,
        missing_4h_bars: simulation.missing_4h_bars,
        entry_price_outside_bar_count: simulation.entry_price_outside_bar_count,
        exit_price_outside_bar_count: simulation.exit_price_outside_bar_count,
        price_path_anomalies: simulation.price_path_anomalies,
        top_positive_symbol: top_positive.map(|(symbol, _)| symbol.clone()),
        top_positive_symbol_profit_share_pct: top_positive.and_then(|(_, profit)| {
            (positive_symbol_profit > 0.0).then_some(*profit / positive_symbol_profit * 100.0)
        }),
        fixed_execution_profit_without_top1_trade: fixed_execution_net_profit - top1_trade_profit,
        fixed_execution_profit_without_top3_trades: fixed_execution_net_profit - top3_trade_profit,
        fixed_execution_profit_without_top1_symbol: top_positive
            .map(|(_, profit)| fixed_execution_net_profit - *profit),
        yearly,
        monthly,
        by_side: side_reports,
        by_min_k_line_num: warmup_reports,
        event_cluster_audit: simulation.event_cluster_audit,
        historical_universe_coverage: None,
        live_universe_audit: None,
        temporal_validation: None,
    })
}

/// 计算百分比；空样本返回零，避免报告输出非有限数值。
fn percentage(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator * 100.0
    }
}

/// 小样本胜率使用 Wilson 区间，避免把点估计误当成已验证的真实胜率。
fn wilson_interval_95(wins: usize, trades: usize) -> Option<(f64, f64)> {
    if trades == 0 || wins > trades {
        return None;
    }
    let z = 1.959_963_984_540_054_f64;
    let n = trades as f64;
    let proportion = wins as f64 / n;
    let z_squared = z * z;
    let center = proportion + z_squared / (2.0 * n);
    let margin = z * (proportion * (1.0 - proportion) / n + z_squared / (4.0 * n * n)).sqrt();
    let denominator = 1.0 + z_squared / n;
    Some((
        (center - margin) / denominator * 100.0,
        (center + margin) / denominator * 100.0,
    ))
}

/// 交易级 Sharpe 不做年度化，仅用于同一回放样本之间比较收益分布稳定性。
fn trade_sharpe(returns: &[f64]) -> Option<f64> {
    if returns.len() < 2 {
        return None;
    }
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (returns.len() - 1) as f64;
    (variance > 0.0).then_some(mean / variance.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(max_concurrent: usize) -> Args {
        Args {
            backtest_id_min: 1,
            backtest_id_max: 10,
            max_concurrent,
            initial_equity: 100.0,
            risk_scale: 1.0,
            extra_slippage_bps: 0.0,
            funding_bps_per_8h: 0.0,
            oos_start_ts: None,
            walk_forward_train_months: None,
            walk_forward_test_months: None,
            side_filter: None,
            max_short_ema_distance_ratio: None,
            portfolio_universe: args::PortfolioUniverse::Backtest,
            rank_universe: args::RankUniverse::Backtest,
            rank_activation_source: RankActivationSource::Reconstructed4h,
            rank_activation_min_delta: None,
            rank_activation_max_delta: None,
            rank_activation_lookback_bars: 1,
            rank_activation_min_price_change_pct: None,
            rank_activation_max_price_change_pct: None,
            rank_activation_valid_for_bars: 9,
            rank_activation_min_wait_bars: 1,
            rank_activation_min_rsi: None,
            rank_activation_max_rsi: None,
            rank_passthrough_id_min: None,
            rank_passthrough_id_max: None,
        }
    }

    fn trade(
        detail_id: i64,
        symbol: &str,
        open_ts: i64,
        close_ts: i64,
        normalized_return: f64,
        volume_ratio: f64,
    ) -> CandidateTrade {
        CandidateTrade {
            detail_id,
            backtest_id: detail_id,
            min_k_line_num: 3600,
            symbol: symbol.to_string(),
            side: "short".to_string(),
            open_ts,
            close_ts,
            open_price: 100.0,
            close_price: 100.0,
            quantity: 1.0,
            original_profit: normalized_return * 100.0,
            source_entry_equity: 100.0,
            base_fee_rate: 0.0,
            normalized_return,
            configured_risk_ratio: None,
            signal_stop_risk_ratio: None,
            initial_stop_risk_ratio: None,
            volume_percentile: 0.99,
            relative_volume_ratio: volume_ratio,
            entry_rsi: Some(40.0),
            ema_distance_ratio: 0.03,
            marks: Vec::new(),
        }
    }

    #[test]
    fn capacity_uses_causal_strength_instead_of_future_profit() {
        let candidates = vec![
            trade(1, "HIGH", 0, 10, -0.02, 3.0),
            trade(2, "LOW", 0, 10, 0.10, 2.0),
        ];

        let report = simulate_portfolio(candidates, args(1)).expect("portfolio report");

        assert_eq!(report.accepted_trades, 1);
        assert_eq!(report.skipped_by_capacity, 1);
        assert_eq!(report.wins, 0);
        assert!((report.final_equity - 98.0).abs() < 1e-9);
    }

    #[test]
    fn settled_equity_compounds_before_the_next_entry() {
        let candidates = vec![
            trade(1, "A", 0, 10, 0.10, 2.0),
            trade(2, "B", 10, 20, 0.10, 2.0),
        ];

        let report = simulate_portfolio(candidates, args(1)).expect("portfolio report");

        assert_eq!(report.accepted_trades, 2);
        assert!((report.final_equity - 121.0).abs() < 1e-9);
    }

    #[test]
    fn reports_results_by_min_k_line_num() {
        let mut short_history = trade(1, "NEW", 0, 10, 0.10, 2.0);
        short_history.min_k_line_num = 720;
        let long_history = trade(2, "OLD", 10, 20, -0.05, 2.0);

        let report = simulate_portfolio(vec![short_history, long_history], args(1))
            .expect("portfolio report");

        assert_eq!(
            report.by_min_k_line_num,
            vec![
                WarmupReport {
                    min_k_line_num: 720,
                    trades: 1,
                    wins: 1,
                    win_rate_pct: 100.0,
                    profit: 10.0,
                },
                WarmupReport {
                    min_k_line_num: 3600,
                    trades: 1,
                    wins: 0,
                    win_rate_pct: 0.0,
                    profit: -5.5,
                },
            ]
        );
    }

    #[test]
    fn short_ema_distance_filter_keeps_longs_and_boundary_shorts() {
        let mut filter_args = args(3);
        filter_args.max_short_ema_distance_ratio = Some(0.10);
        let mut far_short = trade(1, "FAR_SHORT", 0, 10, -0.02, 2.0);
        far_short.ema_distance_ratio = 0.11;
        let mut boundary_short = trade(2, "BOUNDARY_SHORT", 0, 10, 0.02, 2.0);
        boundary_short.ema_distance_ratio = 0.10;
        let mut far_long = trade(3, "FAR_LONG", 0, 10, 0.02, 2.0);
        far_long.side = "long".to_string();
        far_long.ema_distance_ratio = 0.20;
        let mut trades = vec![far_short, boundary_short, far_long];

        let filtered = apply_short_ema_distance_filter(&mut trades, filter_args);

        assert_eq!(filtered, 1);
        assert_eq!(
            trades
                .iter()
                .map(|trade| trade.symbol.as_str())
                .collect::<Vec<_>>(),
            vec!["BOUNDARY_SHORT", "FAR_LONG"]
        );
    }

    #[test]
    fn same_bar_trades_share_capacity_then_release_it() {
        let candidates = vec![
            trade(1, "A", 0, 0, 0.01, 4.0),
            trade(2, "B", 0, 0, 0.01, 3.0),
            trade(3, "C", 0, 0, 0.01, 2.0),
            trade(4, "D", 1, 2, 0.01, 2.0),
        ];

        let report = simulate_portfolio(candidates, args(2)).expect("portfolio report");

        assert_eq!(report.accepted_trades, 3);
        assert_eq!(report.skipped_by_capacity, 1);
        assert_eq!(report.max_active_positions, 2);
    }

    #[test]
    fn four_hour_marks_reveal_drawdown_hidden_by_profitable_settlement() {
        let mut candidate = trade(1, "A", 0, FOUR_HOURS_MS * 2, 0.10, 2.0);
        candidate.side = "long".to_string();
        candidate.open_price = 100.0;
        candidate.close_price = 110.0;
        candidate.marks = vec![
            CandleMark {
                ts: 0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
            },
            CandleMark {
                ts: FOUR_HOURS_MS,
                high: 102.0,
                low: 80.0,
                close: 90.0,
            },
            CandleMark {
                ts: FOUR_HOURS_MS * 2,
                high: 112.0,
                low: 89.0,
                close: 110.0,
            },
        ];

        let report = simulate_portfolio(vec![candidate], args(1)).expect("portfolio report");

        assert_eq!(report.realized_max_drawdown_pct, 0.0);
        assert!((report.close_mark_max_drawdown_pct - 10.0).abs() < 1e-9);
        assert!((report.intrabar_conservative_max_drawdown_pct - 20.0).abs() < 1e-9);
        assert!(
            report.intrabar_conservative_max_drawdown_pct >= report.close_mark_max_drawdown_pct
        );
        assert_eq!(report.fully_covered_positions, 1);
        assert_eq!(report.missing_4h_bars, 0);
    }

    #[test]
    fn funding_stress_is_charged_at_each_crossed_eight_hour_boundary() {
        let mut stressed_args = args(1);
        stressed_args.funding_bps_per_8h = 1.0;
        let mut candidate = trade(1, "A", 0, FUNDING_INTERVAL_MS * 2, 0.10, 2.0);
        candidate.open_price = 100.0;
        candidate.close_price = 110.0;
        candidate.normalized_return = 0.10 - 0.0002;

        let report = simulate_portfolio(vec![candidate], stressed_args).expect("portfolio report");

        assert!((report.final_equity - 109.98).abs() < 1e-9);
        assert_eq!(funding_interval_count(0, FUNDING_INTERVAL_MS * 2), 2);
    }

    #[test]
    fn execution_prices_outside_their_candle_ranges_are_reported() {
        let mut candidate = trade(1, "A", 0, FOUR_HOURS_MS, 0.01, 2.0);
        candidate.open_price = 100.0;
        candidate.close_price = 120.0;
        candidate.marks = vec![
            CandleMark {
                ts: 0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
            },
            CandleMark {
                ts: FOUR_HOURS_MS,
                high: 110.0,
                low: 95.0,
                close: 105.0,
            },
        ];

        let report = simulate_portfolio(vec![candidate], args(1)).expect("portfolio report");

        assert_eq!(report.entry_price_outside_bar_count, 0);
        assert_eq!(report.exit_price_outside_bar_count, 1);
    }

    #[test]
    fn wilson_interval_keeps_small_samples_from_overstating_win_rate() {
        let interval = wilson_interval_95(14, 20).expect("non-empty interval");

        assert!((interval.0 - 48.10).abs() < 0.01);
        assert!((interval.1 - 85.45).abs() < 0.01);
    }

    #[test]
    fn report_uses_trade_configured_risk_instead_of_a_constant() {
        let mut candidate = trade(1, "A", 0, 10, 0.04, 2.0);
        candidate.configured_risk_ratio = Some(0.02);
        candidate.initial_stop_risk_ratio = Some(0.02);

        let report = simulate_portfolio(vec![candidate], args(1)).expect("portfolio report");

        assert_eq!(report.configured_risk_per_trade_pct, Some(2.0));
        assert_eq!(report.max_configured_open_risk_pct, Some(2.0));
        assert_eq!(report.initial_stop_risk_covered_trades, 1);
        assert_eq!(report.net_expectancy_r, Some(2.0));
    }

    #[test]
    fn effective_initial_risk_uses_the_tighter_protective_stop() {
        assert_eq!(
            effective_initial_stop_risk_ratio(Some(0.02), Some(0.10)),
            Some(0.02)
        );
        assert_eq!(
            effective_initial_stop_risk_ratio(Some(0.02), Some(0.01)),
            Some(0.01)
        );
    }

    #[test]
    fn daily_sharpe_is_annualized_from_calendar_day_equity() {
        let day = 24 * 60 * 60 * 1_000;
        let points = vec![
            mark_to_market::EquityPoint {
                ts: 0,
                equity: 101.0,
            },
            mark_to_market::EquityPoint {
                ts: day * 2,
                equity: 102.01,
            },
        ];

        let metrics = metrics::daily_equity_metrics(100.0, &points);

        assert_eq!(metrics.observations, 3);
        assert!(metrics.annualized_sharpe_sqrt_365.is_some());
    }
}
