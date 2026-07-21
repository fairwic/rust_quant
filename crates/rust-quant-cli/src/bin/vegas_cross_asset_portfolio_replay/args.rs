use anyhow::{anyhow, bail, Context, Result};
use chrono::DateTime;

pub(super) const DEFAULT_INITIAL_EQUITY: f64 = 100.0;
const DEFAULT_MAX_CONCURRENT: usize = 3;

/// 本地组合研究允许纳入的开仓方向；不指定时保留全部方向。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum TradeSideFilter {
    Long,
    Short,
}

impl TradeSideFilter {
    pub(super) fn matches(self, side: &str) -> bool {
        matches!(
            (self, side),
            (TradeSideFilter::Long, "long") | (TradeSideFilter::Short, "short")
        )
    }
}

/// 历史排名代理使用的横截面币池。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum RankUniverse {
    /// 只在当前回测 ID 范围覆盖的交易对之间排名，保留旧研究口径。
    Backtest,
    /// 在 quant_core 已保存的全部 4H 交易对之间排名，更接近生产全市场扫描。
    AllAvailable4h,
}

/// 动量激活事件来源；默认使用本地 4H K 线重建，显式配置后可读取归档雷达事件。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum RankActivationSource {
    Reconstructed4h,
    MarketRankEvents,
}

/// 排名激活时允许的价格冲击方向；默认 Any 保留既有绝对涨跌幅语义。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum RankPriceDirection {
    /// 上涨或下跌都可触发，幅度按绝对值比较。
    Any,
    /// 只接受正向上涨冲击。
    Up,
    /// 只接受负向下跌冲击。
    Down,
}

impl RankPriceDirection {
    /// 返回报告与归档事件查询共用的稳定标签。
    pub(super) fn report_label(self) -> &'static str {
        match self {
            RankPriceDirection::Any => "any",
            RankPriceDirection::Up => "up",
            RankPriceDirection::Down => "down",
        }
    }
}

/// 组合回放使用的币池口径；它只决定候选交易是否属于当月币池，不改变信号参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PortfolioUniverse {
    /// 保留回测 ID 范围内的全部候选交易。
    Backtest,
    /// 当前仍 live 的 USDT 永续中，按月使用前 30 个完整 UTC 日成交额 Top100。
    CurrentLiveMonthlyTop100,
}

impl RankActivationSource {
    pub(super) fn report_label(self) -> &'static str {
        match self {
            RankActivationSource::Reconstructed4h => {
                "confirmed_4h_cross_sectional_rolling_quote_volume_rank"
            }
            RankActivationSource::MarketRankEvents => "market_rank_events_24h_rank_velocity",
        }
    }
}

impl RankUniverse {
    pub(super) fn report_label(self) -> &'static str {
        match self {
            RankUniverse::Backtest => "backtest_symbols",
            RankUniverse::AllAvailable4h => "all_available_4h_symbols",
        }
    }
}

/// 单账户回放命令参数；排名激活参数为空时保持原有回放行为。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Args {
    /// 纳入回放的最小 `back_test_log.id`，包含边界。
    pub(super) backtest_id_min: i64,
    /// 纳入回放的最大 `back_test_log.id`，包含边界。
    pub(super) backtest_id_max: i64,
    /// 单账户允许同时持有的最大仓位数量。
    pub(super) max_concurrent: usize,
    /// 共享账户起始权益，单位 U。
    pub(super) initial_equity: f64,
    /// 对原回测仓位、收益和风险做统一线性缩放；1.0 表示保持原风险。
    pub(super) risk_scale: f64,
    /// 在原回测手续费之外额外扣除的单边滑点，单位 bps。
    pub(super) extra_slippage_bps: f64,
    /// 每个 8 小时结算点按绝对成本扣除的资金费率压力，单位 bps。
    pub(super) funding_bps_per_8h: f64,
    /// 严格样本外起点，使用 UTC 毫秒时间戳；只按入场时点划分，避免平仓后信息泄漏。
    pub(super) oos_start_ts: Option<i64>,
    /// 固定策略滚动验证的训练窗口月数；不在窗口内重新拟合参数。
    pub(super) walk_forward_train_months: Option<u32>,
    /// 固定策略滚动验证的测试窗口月数。
    pub(super) walk_forward_test_months: Option<u32>,
    /// 仅用于本地组合研究的方向过滤；None 表示多空都纳入。
    pub(super) side_filter: Option<TradeSideFilter>,
    /// 做空入场允许的最大 EMA 偏离率；None 表示不增加该研究过滤器。
    pub(super) max_short_ema_distance_ratio: Option<f64>,
    /// 双倍成本压力下允许的最大预估执行成本，单位初始风险 R。
    pub(super) max_projected_double_execution_cost_r: Option<f64>,
    /// 组合候选交易使用的币池过滤口径。
    pub(super) portfolio_universe: PortfolioUniverse,
    /// 历史排名代理的横截面币池，默认只使用回测范围内的交易对。
    pub(super) rank_universe: RankUniverse,
    /// 动量激活事件来源；默认使用确认 4H K 线重建。
    pub(super) rank_activation_source: RankActivationSource,
    /// 4H 横截面成交额排名至少前进的名次数；None 表示不启用历史排名代理。
    pub(super) rank_activation_min_delta: Option<i32>,
    /// 允许的最大排名跃升；None 表示不排除极端排名变化。
    pub(super) rank_activation_max_delta: Option<i32>,
    /// 当前排名与多少根已确认 4H K 线前的排名比较，默认 1。
    pub(super) rank_activation_lookback_bars: usize,
    /// 排名对比周期内绝对价格涨跌幅下界，单位百分比；None 表示不限制。
    pub(super) rank_activation_min_price_change_pct: Option<f64>,
    /// 排名对比周期内绝对价格涨跌幅上界，单位百分比；None 表示不限制。
    pub(super) rank_activation_max_price_change_pct: Option<f64>,
    /// 排名对比周期要求的价格冲击方向；Any 表示沿用绝对涨跌幅。
    pub(super) rank_activation_price_direction: RankPriceDirection,
    /// 排名事件发生后允许 Vegas 入场的最长 4H K 线根数，默认 9。
    pub(super) rank_activation_valid_for_bars: usize,
    /// 排名事件后至少等待的完整 4H K 线根数，默认 1。
    pub(super) rank_activation_min_wait_bars: usize,
    /// 排名激活后允许入场的 RSI 下界，包含边界；None 表示不限制下界。
    pub(super) rank_activation_min_rsi: Option<f64>,
    /// 排名激活后允许入场的 RSI 上界，不包含边界；None 表示不限制上界。
    pub(super) rank_activation_max_rsi: Option<f64>,
    /// 显式合并并保留质量基线中的完整入场路径，其余候选才应用排名激活；None 表示不启用。
    pub(super) rank_passthrough_id_min: Option<i64>,
    /// 质量基线的最大回测 ID，和最小 ID 必须同时提供。
    pub(super) rank_passthrough_id_max: Option<i64>,
}

/// 只接受 Core 专用连接变量并校验目标库名，避免研究命令误读 quant_web。
pub(super) fn quant_core_database_url() -> Result<String> {
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("vegas_cross_asset_portfolio_replay requires QUANT_CORE_DATABASE_URL")?;
    let database_name = database_url
        .split('?')
        .next()
        .unwrap_or(&database_url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    if !database_name.eq_ignore_ascii_case("quant_core") {
        bail!("Vegas portfolio replay only accepts a quant_core database URL");
    }
    Ok(database_url)
}

/// 读取显式归档雷达库连接；禁止回退到 Web 库，且不复用 Core 默认连接以免伪装历史覆盖。
pub(super) fn market_rank_database_url() -> Result<String> {
    let database_url = std::env::var("MARKET_RANK_DATABASE_URL")
        .context("market-rank-events activation requires MARKET_RANK_DATABASE_URL")?;
    let database_name = database_url
        .split('?')
        .next()
        .unwrap_or(&database_url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    if database_name.is_empty() || database_name.eq_ignore_ascii_case("quant_web") {
        bail!("MARKET_RANK_DATABASE_URL must target a Core rank-event database");
    }
    Ok(database_url)
}

/// 解析显式回测区间、账户容量和可选的历史横截面排名激活条件。
pub(super) fn parse_args(values: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut args = Args {
        backtest_id_min: 0,
        backtest_id_max: 0,
        max_concurrent: DEFAULT_MAX_CONCURRENT,
        initial_equity: DEFAULT_INITIAL_EQUITY,
        risk_scale: 1.0,
        extra_slippage_bps: 0.0,
        funding_bps_per_8h: 0.0,
        oos_start_ts: None,
        walk_forward_train_months: None,
        walk_forward_test_months: None,
        side_filter: None,
        max_short_ema_distance_ratio: None,
        max_projected_double_execution_cost_r: None,
        portfolio_universe: PortfolioUniverse::Backtest,
        rank_universe: RankUniverse::Backtest,
        rank_activation_source: RankActivationSource::Reconstructed4h,
        rank_activation_min_delta: None,
        rank_activation_max_delta: None,
        rank_activation_lookback_bars: 1,
        rank_activation_min_price_change_pct: None,
        rank_activation_max_price_change_pct: None,
        rank_activation_price_direction: RankPriceDirection::Any,
        rank_activation_valid_for_bars: 9,
        rank_activation_min_wait_bars: 1,
        rank_activation_min_rsi: None,
        rank_activation_max_rsi: None,
        rank_passthrough_id_min: None,
        rank_passthrough_id_max: None,
    };
    let mut values = values.into_iter();
    while let Some(name) = values.next() {
        if name == "--help" || name == "-h" {
            println!(
                "Usage: vegas_cross_asset_portfolio_replay --backtest-id-min ID --backtest-id-max ID [--max-concurrent 3] [--initial-equity 100] [--risk-scale 1.0] [--extra-slippage-bps 0] [--funding-bps-per-8h 0] [--oos-start RFC3339] [--walk-forward-train-months 12 --walk-forward-test-months 3] [--side long|short] [--max-short-ema-distance-ratio 0.10] [--max-projected-double-execution-cost-r 0.20] [--portfolio-universe backtest|current-live-monthly-top100] [--rank-universe backtest|all-4h] [--rank-activation-source reconstructed-4h|market-rank-events] [--rank-activation-min-delta 18] [--rank-activation-max-delta 42] [--rank-activation-lookback-bars 1] [--rank-activation-min-price-change-pct 5] [--rank-activation-max-price-change-pct 10] [--rank-activation-price-direction any|up|down] [--rank-activation-valid-for-bars 9] [--rank-activation-min-wait-bars 1] [--rank-activation-min-rsi 25] [--rank-activation-max-rsi 55] [--rank-passthrough-id-min ID --rank-passthrough-id-max ID]"
            );
            std::process::exit(0);
        }
        let value = values
            .next()
            .ok_or_else(|| anyhow!("{name} requires a value"))?;
        match name.as_str() {
            "--backtest-id-min" => args.backtest_id_min = parse_value(&name, &value)?,
            "--backtest-id-max" => args.backtest_id_max = parse_value(&name, &value)?,
            "--max-concurrent" => args.max_concurrent = parse_value(&name, &value)?,
            "--initial-equity" => args.initial_equity = parse_value(&name, &value)?,
            "--risk-scale" => args.risk_scale = parse_value(&name, &value)?,
            "--extra-slippage-bps" => args.extra_slippage_bps = parse_value(&name, &value)?,
            "--funding-bps-per-8h" => args.funding_bps_per_8h = parse_value(&name, &value)?,
            "--oos-start" => {
                args.oos_start_ts = Some(
                    DateTime::parse_from_rfc3339(&value)
                        .with_context(|| format!("{name} must be RFC3339"))?
                        .timestamp_millis(),
                )
            }
            "--walk-forward-train-months" => {
                args.walk_forward_train_months = Some(parse_value(&name, &value)?)
            }
            "--walk-forward-test-months" => {
                args.walk_forward_test_months = Some(parse_value(&name, &value)?)
            }
            "--side" => {
                args.side_filter = Some(match value.as_str() {
                    "long" => TradeSideFilter::Long,
                    "short" => TradeSideFilter::Short,
                    _ => bail!("--side must be long or short"),
                })
            }
            "--max-short-ema-distance-ratio" => {
                args.max_short_ema_distance_ratio = Some(parse_value(&name, &value)?)
            }
            "--max-projected-double-execution-cost-r" => {
                args.max_projected_double_execution_cost_r = Some(parse_value(&name, &value)?)
            }
            "--portfolio-universe" => {
                args.portfolio_universe = match value.as_str() {
                    "backtest" => PortfolioUniverse::Backtest,
                    "current-live-monthly-top100" | "current_live_monthly_top100" => {
                        PortfolioUniverse::CurrentLiveMonthlyTop100
                    }
                    _ => bail!(
                        "--portfolio-universe must be backtest or current-live-monthly-top100"
                    ),
                }
            }
            "--rank-universe" => {
                args.rank_universe = match value.as_str() {
                    "backtest" => RankUniverse::Backtest,
                    "all-4h" => RankUniverse::AllAvailable4h,
                    _ => bail!("--rank-universe must be backtest or all-4h"),
                }
            }
            "--rank-activation-source" => {
                args.rank_activation_source = match value.as_str() {
                    "reconstructed-4h" | "reconstructed_4h" => {
                        RankActivationSource::Reconstructed4h
                    }
                    "market-rank-events" | "market_rank_events" => {
                        RankActivationSource::MarketRankEvents
                    }
                    _ => bail!(
                        "--rank-activation-source must be reconstructed-4h or market-rank-events"
                    ),
                }
            }
            "--rank-activation-min-delta" => {
                args.rank_activation_min_delta = Some(parse_value(&name, &value)?)
            }
            "--rank-activation-max-delta" => {
                args.rank_activation_max_delta = Some(parse_value(&name, &value)?)
            }
            "--rank-activation-lookback-bars" => {
                args.rank_activation_lookback_bars = parse_value(&name, &value)?
            }
            "--rank-activation-min-price-change-pct" => {
                args.rank_activation_min_price_change_pct = Some(parse_value(&name, &value)?)
            }
            "--rank-activation-max-price-change-pct" => {
                args.rank_activation_max_price_change_pct = Some(parse_value(&name, &value)?)
            }
            "--rank-activation-price-direction" => {
                args.rank_activation_price_direction = match value.as_str() {
                    "any" => RankPriceDirection::Any,
                    "up" => RankPriceDirection::Up,
                    "down" => RankPriceDirection::Down,
                    _ => bail!("--rank-activation-price-direction must be any, up, or down"),
                }
            }
            "--rank-activation-valid-for-bars" => {
                args.rank_activation_valid_for_bars = parse_value(&name, &value)?
            }
            "--rank-activation-min-wait-bars" => {
                args.rank_activation_min_wait_bars = parse_value(&name, &value)?
            }
            "--rank-activation-min-rsi" => {
                args.rank_activation_min_rsi = Some(parse_value(&name, &value)?)
            }
            "--rank-activation-max-rsi" => {
                args.rank_activation_max_rsi = Some(parse_value(&name, &value)?)
            }
            "--rank-passthrough-id-min" => {
                args.rank_passthrough_id_min = Some(parse_value(&name, &value)?)
            }
            "--rank-passthrough-id-max" => {
                args.rank_passthrough_id_max = Some(parse_value(&name, &value)?)
            }
            _ => bail!("unsupported argument: {name}"),
        }
    }
    validate_args(args)?;
    Ok(args)
}

fn validate_args(args: Args) -> Result<()> {
    if args.backtest_id_min <= 0 || args.backtest_id_max < args.backtest_id_min {
        bail!("a valid inclusive backtest ID range is required");
    }
    if args.max_concurrent == 0 {
        bail!("--max-concurrent must be greater than zero");
    }
    if !args.initial_equity.is_finite() || args.initial_equity <= 0.0 {
        bail!("--initial-equity must be a positive finite number");
    }
    if !args.risk_scale.is_finite() || args.risk_scale <= 0.0 {
        bail!("--risk-scale must be a positive finite number");
    }
    if !args.extra_slippage_bps.is_finite() || args.extra_slippage_bps < 0.0 {
        bail!("--extra-slippage-bps must be a non-negative finite number");
    }
    if !args.funding_bps_per_8h.is_finite() || args.funding_bps_per_8h < 0.0 {
        bail!("--funding-bps-per-8h must be a non-negative finite number");
    }
    match (
        args.walk_forward_train_months,
        args.walk_forward_test_months,
    ) {
        (None, None) => {}
        (Some(train), Some(test)) if train > 0 && test > 0 => {}
        (Some(_), Some(_)) => bail!("walk-forward month windows must be greater than zero"),
        _ => bail!("walk-forward train and test months must be provided together"),
    }
    if args
        .max_short_ema_distance_ratio
        .is_some_and(|value| !value.is_finite() || value < 0.0)
    {
        bail!("--max-short-ema-distance-ratio must be a non-negative finite number");
    }
    if args
        .max_projected_double_execution_cost_r
        .is_some_and(|value| !value.is_finite() || value <= 0.0)
    {
        bail!("--max-projected-double-execution-cost-r must be a positive finite number");
    }
    if let Some(min_delta) = args.rank_activation_min_delta {
        if min_delta <= 0 {
            bail!("--rank-activation-min-delta must be greater than zero");
        }
        if args.rank_activation_lookback_bars == 0 {
            bail!("--rank-activation-lookback-bars must be greater than zero");
        }
        if args.rank_activation_valid_for_bars < args.rank_activation_min_wait_bars {
            bail!("rank activation valid window must include the minimum wait");
        }
        if args.rank_activation_source == RankActivationSource::MarketRankEvents
            && args.rank_activation_lookback_bars != 6
        {
            bail!(
                "market-rank-events source maps to the 24-hour radar and requires 6 lookback bars"
            );
        }
        if args
            .rank_activation_max_delta
            .is_some_and(|max_delta| max_delta < min_delta)
        {
            bail!("--rank-activation-max-delta must not be below the minimum delta");
        }
        for (name, value) in [
            (
                "--rank-activation-min-price-change-pct",
                args.rank_activation_min_price_change_pct,
            ),
            (
                "--rank-activation-max-price-change-pct",
                args.rank_activation_max_price_change_pct,
            ),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
                bail!("{name} must be a non-negative finite number");
            }
        }
        if matches!(
            (
                args.rank_activation_min_price_change_pct,
                args.rank_activation_max_price_change_pct
            ),
            (Some(min), Some(max)) if max < min
        ) {
            bail!("rank activation price-change bounds must be ordered");
        }
        match (args.rank_passthrough_id_min, args.rank_passthrough_id_max) {
            (Some(min), Some(max)) if min > 0 && max >= min => {}
            (None, None) => {}
            _ => bail!("a valid passthrough backtest ID range requires both boundaries"),
        }
    } else if args.rank_activation_max_delta.is_some()
        || args.rank_activation_min_rsi.is_some()
        || args.rank_activation_max_rsi.is_some()
        || args.rank_activation_min_price_change_pct.is_some()
        || args.rank_activation_max_price_change_pct.is_some()
        || args.rank_activation_price_direction != RankPriceDirection::Any
        || args.rank_passthrough_id_min.is_some()
        || args.rank_passthrough_id_max.is_some()
        || args.rank_universe != RankUniverse::Backtest
        || args.rank_activation_source != RankActivationSource::Reconstructed4h
    {
        bail!("rank activation filters require --rank-activation-min-delta");
    }
    match (args.rank_activation_min_rsi, args.rank_activation_max_rsi) {
        (Some(min), Some(max)) if min.is_finite() && max.is_finite() && min < max => {}
        (None, None) => {}
        (Some(value), None) | (None, Some(value)) if value.is_finite() => {}
        _ => bail!("rank activation RSI bounds must be finite and ordered"),
    }
    Ok(())
}

fn parse_value<T>(name: &str, value: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|error| anyhow!("invalid {name}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_filters_require_an_activation_delta() {
        let error = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--rank-activation-min-rsi".to_string(),
            "25".to_string(),
        ])
        .expect_err("RSI-only rank gate must fail");

        assert!(error
            .to_string()
            .contains("require --rank-activation-min-delta"));
    }

    #[test]
    fn parses_explicit_trade_side_filter() {
        let args = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--side".to_string(),
            "short".to_string(),
        ])
        .expect("valid short-side filter");

        assert_eq!(args.side_filter, Some(TradeSideFilter::Short));
        assert!(TradeSideFilter::Short.matches("short"));
        assert!(!TradeSideFilter::Short.matches("long"));
    }

    #[test]
    fn rejects_unknown_trade_side_filter() {
        let error = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--side".to_string(),
            "both".to_string(),
        ])
        .expect_err("unknown side must fail");

        assert!(error.to_string().contains("must be long or short"));
    }

    #[test]
    fn parses_short_ema_distance_filter() {
        let args = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--max-short-ema-distance-ratio".to_string(),
            "0.10".to_string(),
        ])
        .expect("valid short EMA distance filter");

        assert_eq!(args.max_short_ema_distance_ratio, Some(0.10));
    }

    #[test]
    fn parses_projected_double_execution_cost_filter() {
        let args = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--max-projected-double-execution-cost-r".to_string(),
            "0.20".to_string(),
        ])
        .expect("valid projected cost filter");

        assert_eq!(args.max_projected_double_execution_cost_r, Some(0.20));
    }

    #[test]
    fn rejects_non_positive_projected_double_execution_cost_filter() {
        let error = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--max-projected-double-execution-cost-r".to_string(),
            "0".to_string(),
        ])
        .expect_err("zero projected cost threshold must fail");

        assert!(error.to_string().contains("positive finite"));
    }

    #[test]
    fn parses_positive_global_risk_scale() {
        let args = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--risk-scale".to_string(),
            "0.375".to_string(),
        ])
        .expect("valid global risk scale");

        assert_eq!(args.risk_scale, 0.375);
    }

    #[test]
    fn rejects_zero_global_risk_scale() {
        let error = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--risk-scale".to_string(),
            "0".to_string(),
        ])
        .expect_err("zero risk scale must fail");

        assert!(error.to_string().contains("positive finite"));
    }

    #[test]
    fn parses_all_available_rank_universe() {
        let args = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--rank-activation-min-delta".to_string(),
            "3".to_string(),
            "--rank-universe".to_string(),
            "all-4h".to_string(),
        ])
        .expect("valid all-4h universe");

        assert_eq!(args.rank_universe, RankUniverse::AllAvailable4h);
    }

    #[test]
    fn parses_bullish_rank_price_direction() {
        let args = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--rank-activation-min-delta".to_string(),
            "3".to_string(),
            "--rank-activation-price-direction".to_string(),
            "up".to_string(),
        ])
        .expect("valid bullish price direction");

        assert_eq!(args.rank_activation_price_direction, RankPriceDirection::Up);
    }

    #[test]
    fn parses_current_live_monthly_top100_portfolio_universe() {
        let args = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--portfolio-universe".to_string(),
            "current-live-monthly-top100".to_string(),
        ])
        .expect("valid live-only portfolio universe");

        assert_eq!(
            args.portfolio_universe,
            PortfolioUniverse::CurrentLiveMonthlyTop100
        );
    }

    #[test]
    fn parses_archived_market_rank_event_source() {
        let args = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--rank-activation-min-delta".to_string(),
            "2".to_string(),
            "--rank-activation-source".to_string(),
            "market-rank-events".to_string(),
            "--rank-activation-lookback-bars".to_string(),
            "6".to_string(),
        ])
        .expect("valid archived rank-event source");

        assert_eq!(
            args.rank_activation_source,
            RankActivationSource::MarketRankEvents
        );
    }

    #[test]
    fn archived_market_rank_events_require_24h_lookback_mapping() {
        let error = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--rank-activation-min-delta".to_string(),
            "2".to_string(),
            "--rank-activation-source".to_string(),
            "market-rank-events".to_string(),
        ])
        .expect_err("archived 24h events must not accept the default one-bar mapping");

        assert!(error.to_string().contains("requires 6 lookback bars"));
    }

    #[test]
    fn rejects_reversed_rank_price_change_bounds() {
        let error = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--rank-activation-min-delta".to_string(),
            "3".to_string(),
            "--rank-activation-min-price-change-pct".to_string(),
            "10".to_string(),
            "--rank-activation-max-price-change-pct".to_string(),
            "5".to_string(),
        ])
        .expect_err("reversed price-change bounds must fail");

        assert!(error.to_string().contains("bounds must be ordered"));
    }

    #[test]
    fn passthrough_range_requires_both_boundaries() {
        let error = parse_args([
            "--backtest-id-min".to_string(),
            "1".to_string(),
            "--backtest-id-max".to_string(),
            "2".to_string(),
            "--rank-activation-min-delta".to_string(),
            "3".to_string(),
            "--rank-passthrough-id-min".to_string(),
            "10".to_string(),
        ])
        .expect_err("half passthrough range must fail");

        assert!(error.to_string().contains("requires both boundaries"));
    }
}
