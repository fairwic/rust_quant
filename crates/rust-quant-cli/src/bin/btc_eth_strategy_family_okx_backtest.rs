use anyhow::{anyhow, Context, Result};
use crypto_exc_all::{
    CryptoSdk, ExchangeId, FundingRate, FundingRateQuery, Instrument, LongShortRatio,
    MarketStatsQuery, OkxExchangeConfig, SdkConfig, TakerBuySellVolume,
};
use okx::api::account::OkxContracts;
use rust_quant_domain::entities::{BacktestDetail, BacktestLog, ExternalMarketSnapshot};
use rust_quant_domain::traits::{BacktestLogRepository, ExternalMarketSnapshotRepository};
use rust_quant_domain::StrategyType;
use rust_quant_infrastructure::repositories::{
    ShardedExternalMarketSnapshotRepository, SqlxBacktestRepository,
};
use rust_quant_market::models::CandlesEntity;
use rust_quant_services::market::get_confirmed_candles_for_backtest;
use rust_quant_strategies::framework::backtest::types::{
    BackTestResult, BasicRiskStrategyConfig, TradeRecord,
};
use rust_quant_strategies::implementations::{
    BearShortPreset, BearShortStackBacktestMarketContext, BearShortStackBacktestTuning,
    BearShortStackStrategy, BtcEthLiquidityScalperBacktestMarketContext,
    BtcEthLiquidityScalperBacktestTuning, BtcEthLiquidityScalperStrategy,
};
use rust_quant_strategies::CandleItem;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use std::collections::BTreeMap;
use std::time::Duration;

#[path = "btc_eth_strategy_family_okx_backtest/micro_scalper_1m.rs"]
mod micro_scalper_1m;
#[path = "btc_eth_strategy_family_okx_backtest/scalper_analysis.rs"]
mod scalper_analysis;
#[path = "btc_eth_strategy_family_okx_backtest/scan.rs"]
mod scan;
#[path = "btc_eth_strategy_family_okx_backtest/volume_reversal_5m.rs"]
mod volume_reversal_5m;

#[cfg(test)]
use micro_scalper_1m::micro_scalper_scan_tunings;
use micro_scalper_1m::{print_micro_scalper_scan, run_micro_scalper_1m};
use scalper_analysis::{
    format_case_reports, format_scalper_diagnostic_reasons, merge_filtered_reason_counts,
    print_scalper_diagnostics, print_scalper_scan, print_scalper_scan_with_tunings,
    scalper_filter_counts, scalper_setup_diagnostics, short_candidate_reports_meet_constraints,
    short_scan_candidate_meets_constraints, sort_scalper_raw_candidates,
    summarize_breakdown_candidate_reports, summarize_exhaustion_candidate_reports,
    summarize_scalper_candidate_reports,
};
#[cfg(test)]
use scan::{breakdown_scan_tunings, exhaustion_scan_tunings};
use scan::{
    print_breakdown_scan, print_exhaustion_scan, scalper_narrow_scan_tunings, scalper_scan_tunings,
};
use volume_reversal_5m::{
    print_btc_volume_reversal_frequency_scan, print_volume_reversal_diagnostics,
    print_volume_reversal_scan, run_btc_volume_reversal_hybrid_5m, run_eth_volume_reversal_5m,
    run_eth_volume_reversal_dual_5m,
};

const DEFAULT_LIMIT: usize = 30_000;
const OKX_SOURCE: &str = "okx";
const FUNDING_RATE_METRIC: &str = "funding_rate";
const OPEN_INTEREST_VOLUME_METRIC: &str = "open_interest_volume";
const TAKER_VOLUME_METRIC: &str = "taker_volume";
const LONG_SHORT_RATIO_METRIC: &str = "long_short_ratio";
const MARKET_CONTEXT_LOOKBACK_MS: i64 = 8 * 60 * 60 * 1_000;
const MARKET_CONTEXT_QUERY_LIMIT: i64 = 250_000;
const OKX_CONTEXT_BACKFILL_PERIOD: &str = "1D";
const OKX_CONTEXT_BACKFILL_WINDOW_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
const OKX_CONTEXT_BACKFILL_PAUSE_MS: u64 = 450;
const OKX_FUNDING_BACKFILL_MAX_PAGES: usize = 16;
const BACKTEST_SIGNAL_WARMUP_CANDLES: usize = 500;

#[derive(Debug, Clone)]
struct Args {
    limit: usize,
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
    debug_trades: bool,
    scan_micro: bool,
    scan_volume_reversal: bool,
    scan_btc_volume_reversal: bool,
    scan_scalper: bool,
    scan_scalper_narrow: bool,
    diagnose_scalper: bool,
    diagnose_volume_reversal: bool,
    scan_breakdown: bool,
    scan_exhaustion: bool,
    use_market_context: bool,
    backfill_okx_market_context: bool,
    case_label: Option<String>,
}

#[derive(Debug, Clone)]
struct StrategyCase {
    label: &'static str,
    symbol: &'static str,
    period: &'static str,
    family: StrategyFamily,
}

#[derive(Debug, Clone)]
struct LoadedCase {
    case: StrategyCase,
    candles: Vec<CandleItem>,
    context: BacktestMarketContext,
    context_required: bool,
}

#[derive(Debug, Clone, Copy)]
enum StrategyFamily {
    Scalper,
    MicroScalper1m,
    EthVolumeReversal5m,
    EthVolumeReversalDual5m,
    BtcVolumeReversalDual5m,
    BtcVolumeReversalHybrid5m,
    Breakdown,
    Exhaustion,
}

#[derive(Debug, Clone)]
struct CaseReport {
    label: String,
    candles: usize,
    entries: usize,
    closed: usize,
    wins: usize,
    losses: usize,
    win_rate_pct: f64,
    pnl: f64,
    final_funds: f64,
    max_drawdown_pct: f64,
    days: f64,
    trades_per_day: f64,
    trades: Vec<ClosedTradeDebug>,
    filtered_signals: usize,
    filtered_reason_counts: Vec<(String, usize)>,
    filtered_signal_snapshots: Vec<FilteredSignalDebug>,
}

#[derive(Debug)]
/// Holds both printable summary and raw backtest output so persistence can save trade details.
struct CaseBacktestRun {
    report: CaseReport,
    result: BackTestResult,
}

#[derive(Debug, Clone)]
struct ScanCandidateReport {
    tuning: BearShortStackBacktestTuning,
    entries: usize,
    wins: usize,
    losses: usize,
    win_rate_pct: f64,
    pnl: f64,
    max_drawdown_pct: f64,
    trades_per_day: f64,
    early_win_rate_pct: f64,
    early_pnl: f64,
    late_win_rate_pct: f64,
    late_pnl: f64,
    remove_top5_pnl: f64,
}

#[derive(Debug, Clone)]
struct ScalperScanCandidateReport {
    tuning: BtcEthLiquidityScalperBacktestTuning,
    entries: usize,
    wins: usize,
    losses: usize,
    win_rate_pct: f64,
    pnl: f64,
    max_drawdown_pct: f64,
    trades_per_day: f64,
    early_win_rate_pct: f64,
    early_pnl: f64,
    late_win_rate_pct: f64,
    late_pnl: f64,
    remove_top5_pnl: f64,
    filtered_reason_counts: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Default)]
struct BacktestMarketContext {
    scalper: Vec<BtcEthLiquidityScalperBacktestMarketContext>,
    bear: Vec<BearShortStackBacktestMarketContext>,
}

#[derive(Debug, Clone, Copy, Default)]
struct ReportTuningOverrides {
    scalper: Option<BtcEthLiquidityScalperBacktestTuning>,
    breakdown: Option<BearShortStackBacktestTuning>,
    exhaustion: Option<BearShortStackBacktestTuning>,
}

/// Aggregates why the scalper candle-structure setup did or did not form.
#[derive(Debug, Clone, Default)]
struct ScalperSetupDiagnostics {
    samples: usize,
    confirmed: usize,
    reasons: BTreeMap<&'static str, usize>,
}

impl ScalperSetupDiagnostics {
    /// Returns the number of windows assigned to one explicit setup outcome.
    fn classified_windows(&self) -> usize {
        self.confirmed + self.reasons.values().sum::<usize>()
    }

    /// Returns the count for a single rejection reason.
    fn reason_count(&self, reason: &str) -> usize {
        self.reasons.get(reason).copied().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default)]
struct MarketContextSnapshotSeries {
    funding: Vec<ExternalMarketSnapshot>,
    open_interest: Vec<ExternalMarketSnapshot>,
    taker: Vec<ExternalMarketSnapshot>,
    long_short: Vec<ExternalMarketSnapshot>,
}

#[derive(Debug, Clone)]
struct ClosedTradeDebug {
    open_time: String,
    close_time: Option<String>,
    open_price: f64,
    close_price: Option<f64>,
    pnl: f64,
    close_type: String,
    entry_snapshot: Option<EntrySnapshotDebug>,
    entry_reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct EntrySnapshotDebug {
    stop_distance_pct: f64,
    atr_pct: f64,
    oi_growth_pct: f64,
    funding_rate: f64,
    long_short_ratio: f64,
    taker_sell_buy_ratio: f64,
    target_r: f64,
    ema_distance_pct: f64,
    volume_multiple: f64,
    downside_excursion_pct: f64,
    rebound_close_pos: f64,
    candle_range_pct: f64,
    body_pct: f64,
    lower_wick_pct: f64,
    upper_wick_pct: f64,
}

#[derive(Debug, Clone)]
struct FilteredSignalDebug {
    ts: i64,
    reasons: Vec<String>,
    snapshot: EntrySnapshotDebug,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let args = parse_args(std::env::args().skip(1))?;
    let load_case_label = args.case_label.as_deref().or_else(|| {
        if args.scan_btc_volume_reversal {
            Some("btc_volume_reversal_dual_5m")
        } else {
            (args.scan_volume_reversal || args.diagnose_volume_reversal)
                .then_some("eth_volume_reversal_5m")
        }
    });
    let mut loaded = load_cases(
        args.limit,
        false,
        load_case_label,
        args.scan_micro || args.scan_volume_reversal || args.scan_btc_volume_reversal,
    )
    .await?;
    if args.backfill_okx_market_context {
        backfill_okx_market_context(&loaded).await?;
    }
    if args.use_market_context {
        attach_sharded_market_context(&mut loaded).await?;
    }
    if args.diagnose_volume_reversal {
        print_volume_reversal_diagnostics(&loaded, args.risk_percent, args.trade_fee_rate);
    } else if args.scan_btc_volume_reversal {
        print_btc_volume_reversal_frequency_scan(&loaded, args.risk_percent, args.trade_fee_rate);
    } else if args.scan_volume_reversal {
        print_volume_reversal_scan(&loaded, args.risk_percent, args.trade_fee_rate);
    } else if args.scan_micro {
        print_micro_scalper_scan(&loaded, args.risk_percent, args.trade_fee_rate);
    } else if args.scan_scalper_narrow {
        print_scalper_scan_with_tunings(
            &loaded,
            args.risk_percent,
            args.trade_fee_rate,
            scalper_narrow_scan_tunings(),
            "no_scalper_narrow_candidates",
        );
    } else if args.scan_scalper {
        print_scalper_scan(&loaded, args.risk_percent, args.trade_fee_rate);
    } else if args.diagnose_scalper {
        print_scalper_diagnostics(&loaded);
    } else if args.scan_breakdown {
        print_breakdown_scan(&loaded, args.risk_percent, args.trade_fee_rate);
    } else if args.scan_exhaustion {
        print_exhaustion_scan(&loaded, args.risk_percent, args.trade_fee_rate);
    } else {
        let risk_config = strategy_family_risk_config(args.risk_percent, args.trade_fee_rate);
        let runs = run_report_backtests(
            &loaded,
            args.risk_percent,
            args.trade_fee_rate,
            ReportTuningOverrides::default(),
        );
        let reports = persist_case_backtest_runs(&loaded, runs, risk_config).await?;
        print_reports(&reports, args.debug_trades);
    }
    Ok(())
}

async fn attach_sharded_market_context(loaded_cases: &mut [LoadedCase]) -> Result<()> {
    let repo = connect_sharded_market_context_repository()?;
    for loaded in loaded_cases {
        loaded.context = load_sharded_market_context(&repo, loaded.case.symbol, &loaded.candles)
            .await
            .with_context(|| {
                format!(
                    "attach sharded market context failed: label={} symbol={}",
                    loaded.case.label, loaded.case.symbol
                )
            })?;
        loaded.context_required = true;
    }
    Ok(())
}

fn parse_args<I>(args: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut limit = DEFAULT_LIMIT;
    let mut risk_percent = 2.0;
    let mut trade_fee_rate = None;
    let mut debug_trades = false;
    let mut scan_micro = false;
    let mut scan_volume_reversal = false;
    let mut scan_btc_volume_reversal = false;
    let mut scan_scalper = false;
    let mut scan_scalper_narrow = false;
    let mut diagnose_scalper = false;
    let mut diagnose_volume_reversal = false;
    let mut scan_breakdown = false;
    let mut scan_exhaustion = false;
    let mut use_market_context = false;
    let mut backfill_okx_market_context = false;
    let mut case_label = None;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--limit" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --limit"))?;
                limit = value
                    .parse::<usize>()
                    .map_err(|e| anyhow!("invalid --limit '{}': {}", value, e))?;
            }
            "--risk-percent" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --risk-percent"))?;
                risk_percent = value
                    .parse::<f64>()
                    .map_err(|e| anyhow!("invalid --risk-percent '{}': {}", value, e))?;
            }
            "--trade-fee-rate" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --trade-fee-rate"))?;
                trade_fee_rate = Some(
                    value
                        .parse::<f64>()
                        .map_err(|e| anyhow!("invalid --trade-fee-rate '{}': {}", value, e))?,
                );
            }
            "--debug-trades" => debug_trades = true,
            "--scan-micro" => scan_micro = true,
            "--scan-volume-reversal" => scan_volume_reversal = true,
            "--scan-btc-volume-reversal" => scan_btc_volume_reversal = true,
            "--scan-scalper" => scan_scalper = true,
            "--scan-scalper-narrow" => scan_scalper_narrow = true,
            "--diagnose-scalper" => diagnose_scalper = true,
            "--diagnose-volume-reversal" => diagnose_volume_reversal = true,
            "--scan-breakdown" => scan_breakdown = true,
            "--scan-exhaustion" => scan_exhaustion = true,
            "--use-market-context" => use_market_context = true,
            "--backfill-okx-market-context" => backfill_okx_market_context = true,
            "--case-label" => {
                case_label = Some(
                    iter.next()
                        .ok_or_else(|| anyhow!("missing value for --case-label"))?,
                );
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    if limit == 0 {
        return Err(anyhow!("--limit must be greater than 0"));
    }
    if risk_percent <= 0.0 {
        return Err(anyhow!("--risk-percent must be greater than 0"));
    }
    if trade_fee_rate.is_some_and(|value| value < 0.0) {
        return Err(anyhow!(
            "--trade-fee-rate must be greater than or equal to 0"
        ));
    }

    Ok(Args {
        limit,
        risk_percent,
        trade_fee_rate,
        debug_trades,
        scan_micro,
        scan_volume_reversal,
        scan_btc_volume_reversal,
        scan_scalper,
        scan_scalper_narrow,
        diagnose_scalper,
        diagnose_volume_reversal,
        scan_breakdown,
        scan_exhaustion,
        use_market_context,
        backfill_okx_market_context,
        case_label,
    })
}

fn print_usage() {
    println!(
        "btc_eth_strategy_family_okx_backtest [--limit N] [--risk-percent P] [--trade-fee-rate RATE] [--debug-trades] [--scan-micro] [--scan-scalper] [--scan-scalper-narrow] [--diagnose-scalper] [--scan-breakdown] [--scan-exhaustion]\n\
         \n\
         Reads quant_core sharded candle tables such as btc-usdt-swap_candles_5m and\n\
         eth-usdt-swap_candles_15m, then runs the new BTC/ETH strategy family through\n\
         the existing rust_quant strategies backtest pipeline. Add --use-market-context to\n\
         require OKX sharded funding/OI/taker context when evaluating signals. Add\n\
         --backfill-okx-market-context to fetch OKX public 1D context into sharded tables. Use\n\
         market_velocity_candle_backfill to backfill the sharded candle tables before running this report.\n\
         Use --case-label scalper_btc_1m to run or scan one case only. Use --scan-volume-reversal\n\
         to scan the research-only ETH 5m volume reversal preset; use --diagnose-volume-reversal\n\
         to print win/loss candle-shape diagnostics for the strongest research candidates."
    );
}

const STRATEGY_CASE_DEFS: [(&str, &str, &str, StrategyFamily); 19] = [
    (
        "scalper_btc_1m",
        "BTC-USDT-SWAP",
        "1m",
        StrategyFamily::Scalper,
    ),
    (
        "scalper_eth_1m",
        "ETH-USDT-SWAP",
        "1m",
        StrategyFamily::Scalper,
    ),
    (
        "micro_scalper_btc_1m",
        "BTC-USDT-SWAP",
        "1m",
        StrategyFamily::MicroScalper1m,
    ),
    (
        "micro_scalper_eth_1m",
        "ETH-USDT-SWAP",
        "1m",
        StrategyFamily::MicroScalper1m,
    ),
    (
        "scalper_btc_5m",
        "BTC-USDT-SWAP",
        "5m",
        StrategyFamily::Scalper,
    ),
    (
        "scalper_eth_5m",
        "ETH-USDT-SWAP",
        "5m",
        StrategyFamily::Scalper,
    ),
    (
        "eth_volume_reversal_5m",
        "ETH-USDT-SWAP",
        "5m",
        StrategyFamily::EthVolumeReversal5m,
    ),
    (
        "eth_volume_reversal_dual_5m",
        "ETH-USDT-SWAP",
        "5m",
        StrategyFamily::EthVolumeReversalDual5m,
    ),
    (
        "btc_volume_reversal_dual_5m",
        "BTC-USDT-SWAP",
        "5m",
        StrategyFamily::BtcVolumeReversalDual5m,
    ),
    (
        "btc_volume_reversal_hybrid_5m",
        "BTC-USDT-SWAP",
        "5m",
        StrategyFamily::BtcVolumeReversalHybrid5m,
    ),
    (
        "sol_volume_reversal_dual_5m",
        "SOL-USDT-SWAP",
        "5m",
        StrategyFamily::EthVolumeReversalDual5m,
    ),
    (
        "breakdown_btc_5m",
        "BTC-USDT-SWAP",
        "5m",
        StrategyFamily::Breakdown,
    ),
    (
        "breakdown_eth_5m",
        "ETH-USDT-SWAP",
        "5m",
        StrategyFamily::Breakdown,
    ),
    (
        "exhaustion_btc_5m",
        "BTC-USDT-SWAP",
        "5m",
        StrategyFamily::Exhaustion,
    ),
    (
        "exhaustion_eth_5m",
        "ETH-USDT-SWAP",
        "5m",
        StrategyFamily::Exhaustion,
    ),
    (
        "breakdown_btc_15m",
        "BTC-USDT-SWAP",
        "15m",
        StrategyFamily::Breakdown,
    ),
    (
        "breakdown_eth_15m",
        "ETH-USDT-SWAP",
        "15m",
        StrategyFamily::Breakdown,
    ),
    (
        "exhaustion_btc_15m",
        "BTC-USDT-SWAP",
        "15m",
        StrategyFamily::Exhaustion,
    ),
    (
        "exhaustion_eth_15m",
        "ETH-USDT-SWAP",
        "15m",
        StrategyFamily::Exhaustion,
    ),
];

fn strategy_cases() -> [StrategyCase; 19] {
    STRATEGY_CASE_DEFS.map(|(label, symbol, period, family)| StrategyCase {
        label,
        symbol,
        period,
        family,
    })
}

fn strategy_cases_for_filter(
    case_label: Option<&str>,
    include_research_cases: bool,
) -> Result<Vec<StrategyCase>> {
    let cases = strategy_cases()
        .into_iter()
        .filter(|case| {
            if let Some(label) = case_label {
                return case.label == label;
            }
            if is_research_case(case) {
                return include_research_cases;
            }
            !is_research_case(case)
        })
        .collect::<Vec<_>>();
    if cases.is_empty() {
        return Err(anyhow!(
            "no BTC/ETH strategy backtest case matched --case-label {:?}",
            case_label
        ));
    }
    Ok(cases)
}

fn is_research_case(case: &StrategyCase) -> bool {
    matches!(
        case.family,
        StrategyFamily::MicroScalper1m
            | StrategyFamily::EthVolumeReversal5m
            | StrategyFamily::EthVolumeReversalDual5m
            | StrategyFamily::BtcVolumeReversalDual5m
            | StrategyFamily::BtcVolumeReversalHybrid5m
    )
}

async fn load_cases(
    limit: usize,
    use_market_context: bool,
    case_label: Option<&str>,
    include_research_cases: bool,
) -> Result<Vec<LoadedCase>> {
    let cases = strategy_cases_for_filter(case_label, include_research_cases)?;
    let context_repo = if use_market_context {
        Some(connect_sharded_market_context_repository()?)
    } else {
        None
    };
    let mut reports = Vec::with_capacity(cases.len());
    for case in cases {
        let candles = load_sharded_candles(case.symbol, case.period, limit).await?;
        let context = if let Some(repo) = context_repo.as_ref() {
            load_sharded_market_context(repo, case.symbol, &candles).await?
        } else {
            BacktestMarketContext::default()
        };
        reports.push(LoadedCase {
            case,
            candles,
            context,
            context_required: use_market_context,
        });
    }
    Ok(reports)
}

fn connect_sharded_market_context_repository() -> Result<ShardedExternalMarketSnapshotRepository> {
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("missing QUANT_CORE_DATABASE_URL for OKX sharded market context backtest")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_lazy(&database_url)
        .context("create quant_core postgres pool for sharded market context")?;
    Ok(ShardedExternalMarketSnapshotRepository::new(pool))
}

async fn backfill_okx_market_context(loaded_cases: &[LoadedCase]) -> Result<()> {
    let repo = connect_sharded_market_context_repository()?;
    let sdk = okx_public_sdk()?;
    let market = sdk.market(ExchangeId::Okx)?;
    let contracts = OkxContracts::new();

    for (symbol, (start_time, end_time)) in market_context_symbol_ranges(loaded_cases) {
        let base = okx_base_coin(&symbol);
        let instrument = Instrument::perp(base.clone(), "USDT");
        let start_time = start_time.saturating_sub(MARKET_CONTEXT_LOOKBACK_MS);

        let funding_rows = backfill_okx_funding_rate(
            &repo,
            &market,
            &symbol,
            instrument.clone(),
            start_time,
            end_time,
        )
        .await?;
        let long_short_rows = backfill_okx_long_short_ratio(
            &repo,
            &market,
            &symbol,
            instrument.clone(),
            start_time,
            end_time,
        )
        .await?;
        let taker_rows =
            backfill_okx_taker_volume(&repo, &market, &symbol, instrument, start_time, end_time)
                .await?;
        let open_interest_rows = backfill_okx_open_interest_volume(
            &repo, &contracts, &symbol, &base, start_time, end_time,
        )
        .await?;

        println!(
            "backfill_okx_market_context symbol={} period={} funding={} open_interest={} taker={} long_short={}",
            symbol,
            OKX_CONTEXT_BACKFILL_PERIOD,
            funding_rows,
            open_interest_rows,
            taker_rows,
            long_short_rows
        );
    }
    Ok(())
}

fn okx_public_sdk() -> Result<CryptoSdk> {
    CryptoSdk::from_config(SdkConfig {
        okx: Some(OkxExchangeConfig {
            api_key: "public".to_string(),
            api_secret: "public".to_string(),
            passphrase: "public".to_string(),
            simulated: false,
            api_url: None,
            request_expiration_ms: Some(10_000),
        }),
        ..SdkConfig::default()
    })
    .map_err(|error| anyhow!("create OKX public sdk failed: {}", error))
}

fn market_context_symbol_ranges(loaded_cases: &[LoadedCase]) -> BTreeMap<String, (i64, i64)> {
    let mut ranges = BTreeMap::new();
    for loaded in loaded_cases {
        let Some((start, end)) = candle_time_range(&loaded.candles) else {
            continue;
        };
        ranges
            .entry(loaded.case.symbol.to_string())
            .and_modify(|range: &mut (i64, i64)| {
                range.0 = range.0.min(start);
                range.1 = range.1.max(end);
            })
            .or_insert((start, end));
    }
    ranges
}

async fn backfill_okx_funding_rate(
    repo: &ShardedExternalMarketSnapshotRepository,
    market: &crypto_exc_all::MarketFacade<'_>,
    symbol: &str,
    instrument: Instrument,
    start_time: i64,
    end_time: i64,
) -> Result<usize> {
    let mut cursor = Some(end_time);
    let mut saved = 0;
    for _ in 0..OKX_FUNDING_BACKFILL_MAX_PAGES {
        let mut query = FundingRateQuery::new(instrument.clone()).with_limit(100);
        if let Some(cursor_time) = cursor {
            query = query.with_after(cursor_time.to_string());
        }
        let items = market
            .funding_rate_history(query)
            .await
            .map_err(|error| anyhow!("fetch OKX funding history failed: {}", error))?;
        if items.is_empty() {
            break;
        }
        let min_ts = items
            .iter()
            .filter_map(|item| item.funding_time.map(|ts| ts as i64))
            .min();
        let snapshots = items
            .iter()
            .filter_map(|item| okx_funding_snapshot(symbol, item, start_time, end_time))
            .collect::<Vec<_>>();
        saved += save_snapshots(repo, snapshots).await?;
        let Some(next_cursor) = min_ts else {
            break;
        };
        if next_cursor <= start_time || cursor.is_some_and(|current| next_cursor >= current) {
            break;
        }
        cursor = Some(next_cursor.saturating_sub(1));
        pause_okx_context_backfill().await;
    }
    Ok(saved)
}

async fn backfill_okx_long_short_ratio(
    repo: &ShardedExternalMarketSnapshotRepository,
    market: &crypto_exc_all::MarketFacade<'_>,
    symbol: &str,
    instrument: Instrument,
    start_time: i64,
    end_time: i64,
) -> Result<usize> {
    let mut saved = 0;
    for (begin, end) in
        market_context_backfill_windows(start_time, end_time, OKX_CONTEXT_BACKFILL_WINDOW_MS)
    {
        let query = MarketStatsQuery::new(instrument.clone(), OKX_CONTEXT_BACKFILL_PERIOD)
            .with_start_time(begin as u64)
            .with_end_time(end as u64)
            .with_limit(100);
        let items = market
            .long_short_ratio(query)
            .await
            .map_err(|error| anyhow!("fetch OKX long-short ratio failed: {}", error))?;
        let snapshots = items
            .iter()
            .filter_map(|item| okx_long_short_snapshot(symbol, item, begin, end))
            .collect::<Vec<_>>();
        saved += save_snapshots(repo, snapshots).await?;
        pause_okx_context_backfill().await;
    }
    Ok(saved)
}

async fn backfill_okx_taker_volume(
    repo: &ShardedExternalMarketSnapshotRepository,
    market: &crypto_exc_all::MarketFacade<'_>,
    symbol: &str,
    instrument: Instrument,
    start_time: i64,
    end_time: i64,
) -> Result<usize> {
    let mut saved = 0;
    for (begin, end) in
        market_context_backfill_windows(start_time, end_time, OKX_CONTEXT_BACKFILL_WINDOW_MS)
    {
        let query = MarketStatsQuery::new(instrument.clone(), OKX_CONTEXT_BACKFILL_PERIOD)
            .with_start_time(begin as u64)
            .with_end_time(end as u64)
            .with_limit(100);
        let items = market
            .taker_buy_sell_volume(query)
            .await
            .map_err(|error| anyhow!("fetch OKX taker volume failed: {}", error))?;
        let snapshots = items
            .iter()
            .filter_map(|item| okx_taker_snapshot(symbol, item, begin, end))
            .collect::<Vec<_>>();
        saved += save_snapshots(repo, snapshots).await?;
        pause_okx_context_backfill().await;
    }
    Ok(saved)
}

async fn backfill_okx_open_interest_volume(
    repo: &ShardedExternalMarketSnapshotRepository,
    contracts: &OkxContracts,
    symbol: &str,
    base: &str,
    start_time: i64,
    end_time: i64,
) -> Result<usize> {
    let mut saved = 0;
    for (begin, end) in
        market_context_backfill_windows(start_time, end_time, OKX_CONTEXT_BACKFILL_WINDOW_MS)
    {
        let items = contracts
            .get_open_interest_volume(
                Some(base),
                Some(begin),
                Some(end),
                Some(OKX_CONTEXT_BACKFILL_PERIOD),
            )
            .await
            .with_context(|| format!("fetch OKX open-interest-volume failed: base={base}"))?;
        let snapshots = items
            .iter()
            .filter_map(|item| {
                let ts = item.ts.parse::<i64>().ok()?;
                if ts < begin || ts > end {
                    return None;
                }
                let mut snapshot = ExternalMarketSnapshot::new(
                    OKX_SOURCE.to_string(),
                    symbol.to_string(),
                    OPEN_INTEREST_VOLUME_METRIC.to_string(),
                    ts,
                );
                snapshot.open_interest = parse_metric_f64(&item.oi);
                snapshot.raw_payload = Some(serde_json::json!({
                    "open_interest": item.oi,
                    "volume": item.vol,
                }));
                Some(snapshot)
            })
            .collect::<Vec<_>>();
        saved += save_snapshots(repo, snapshots).await?;
        pause_okx_context_backfill().await;
    }
    Ok(saved)
}

async fn save_snapshots(
    repo: &ShardedExternalMarketSnapshotRepository,
    snapshots: Vec<ExternalMarketSnapshot>,
) -> Result<usize> {
    let count = snapshots.len();
    if count > 0 {
        repo.save_batch(snapshots).await?;
    }
    Ok(count)
}

async fn pause_okx_context_backfill() {
    tokio::time::sleep(Duration::from_millis(OKX_CONTEXT_BACKFILL_PAUSE_MS)).await;
}

fn okx_funding_snapshot(
    symbol: &str,
    item: &FundingRate,
    start_time: i64,
    end_time: i64,
) -> Option<ExternalMarketSnapshot> {
    let ts = item.funding_time? as i64;
    if ts < start_time || ts > end_time {
        return None;
    }
    let mut snapshot = ExternalMarketSnapshot::new(
        OKX_SOURCE.to_string(),
        symbol.to_string(),
        FUNDING_RATE_METRIC.to_string(),
        ts,
    );
    snapshot.funding_rate = parse_metric_f64(&item.funding_rate);
    snapshot.mark_price = item.mark_price.as_deref().and_then(parse_metric_f64);
    snapshot.raw_payload = Some(item.raw.clone());
    Some(snapshot)
}

fn okx_long_short_snapshot(
    symbol: &str,
    item: &LongShortRatio,
    start_time: i64,
    end_time: i64,
) -> Option<ExternalMarketSnapshot> {
    let ts = item.timestamp? as i64;
    if ts < start_time || ts > end_time {
        return None;
    }
    let mut snapshot = ExternalMarketSnapshot::new(
        OKX_SOURCE.to_string(),
        symbol.to_string(),
        LONG_SHORT_RATIO_METRIC.to_string(),
        ts,
    );
    snapshot.long_short_ratio = parse_metric_f64(&item.ratio);
    snapshot.raw_payload = Some(serde_json::json!({
        "ratio": item.ratio,
        "raw": item.raw,
    }));
    Some(snapshot)
}

fn okx_taker_snapshot(
    symbol: &str,
    item: &TakerBuySellVolume,
    start_time: i64,
    end_time: i64,
) -> Option<ExternalMarketSnapshot> {
    let ts = item.timestamp? as i64;
    if ts < start_time || ts > end_time {
        return None;
    }
    let mut snapshot = ExternalMarketSnapshot::new(
        OKX_SOURCE.to_string(),
        symbol.to_string(),
        TAKER_VOLUME_METRIC.to_string(),
        ts,
    );
    snapshot.raw_payload = Some(serde_json::json!({
        "buy_volume": item.buy_volume,
        "sell_volume": item.sell_volume,
        "raw": item.raw,
    }));
    Some(snapshot)
}

fn parse_metric_f64(value: &str) -> Option<f64> {
    value.parse::<f64>().ok()
}

fn market_context_backfill_windows(
    start_time: i64,
    end_time: i64,
    window_ms: i64,
) -> Vec<(i64, i64)> {
    if window_ms <= 0 || start_time > end_time {
        return Vec::new();
    }
    let mut windows = Vec::new();
    let mut current = start_time;
    while current <= end_time {
        let window_end = current.saturating_add(window_ms - 1).min(end_time);
        windows.push((current, window_end));
        current = window_end.saturating_add(1);
    }
    windows
}

fn okx_base_coin(symbol: &str) -> String {
    symbol
        .split('-')
        .next()
        .unwrap_or(symbol)
        .to_ascii_uppercase()
}

/// Runs normal report cases without discarding trade records required by back_test_detail.
fn run_report_backtests(
    loaded_cases: &[LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
    tunings: ReportTuningOverrides,
) -> Vec<CaseBacktestRun> {
    let risk = strategy_family_risk_config(risk_percent, trade_fee_rate);

    loaded_cases
        .iter()
        .map(|loaded| {
            let scalper_tuning = if matches!(loaded.case.family, StrategyFamily::Scalper) {
                tunings.scalper
            } else {
                None
            };
            let bear_tuning = bear_tuning_for_report_family(loaded.case.family, tunings);
            let result = run_loaded_case(loaded, risk, scalper_tuning, bear_tuning);
            let report = build_report(loaded.case.label, &loaded.candles, &result);
            CaseBacktestRun { report, result }
        })
        .collect()
}

fn strategy_family_risk_config(
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
) -> BasicRiskStrategyConfig {
    BasicRiskStrategyConfig {
        max_loss_percent: risk_percent,
        trade_fee_rate,
        ..BasicRiskStrategyConfig::default()
    }
}

/// Returns the exact risk contract that should be serialized with a persisted backtest.
fn risk_config_for_persistence(
    family: StrategyFamily,
    risk: BasicRiskStrategyConfig,
) -> BasicRiskStrategyConfig {
    match family {
        StrategyFamily::EthVolumeReversal5m
        | StrategyFamily::EthVolumeReversalDual5m
        | StrategyFamily::BtcVolumeReversalDual5m
        | StrategyFamily::BtcVolumeReversalHybrid5m => {
            volume_reversal_5m::volume_reversal_risk_config(risk)
        }
        StrategyFamily::Scalper
        | StrategyFamily::MicroScalper1m
        | StrategyFamily::Breakdown
        | StrategyFamily::Exhaustion => risk,
    }
}

/// Saves normal report runs to back_test_log and back_test_detail using the shared repository.
async fn persist_case_backtest_runs(
    loaded_cases: &[LoadedCase],
    runs: Vec<CaseBacktestRun>,
    base_risk_config: BasicRiskStrategyConfig,
) -> Result<Vec<CaseReport>> {
    if loaded_cases.len() != runs.len() {
        return Err(anyhow!(
            "backtest persistence case/result length mismatch: cases={} runs={}",
            loaded_cases.len(),
            runs.len()
        ));
    }

    let repository = connect_backtest_repository()?;
    let mut reports = Vec::with_capacity(runs.len());
    for (loaded, run) in loaded_cases.iter().zip(runs.into_iter()) {
        let Some(strategy_type) = strategy_type_for_persistence(&loaded.case) else {
            println!(
                "backtest_persistence_skipped label={} reason=unsupported_strategy_type",
                loaded.case.label
            );
            reports.push(run.report);
            continue;
        };
        let strategy_detail = strategy_detail_for_persistence(&loaded.case, &run.report);
        let risk_config = risk_config_for_persistence(loaded.case.family, base_risk_config);
        let result = run.result;
        let log = backtest_log_for_persistence(
            &loaded.case,
            &loaded.candles,
            strategy_type,
            strategy_detail,
            risk_config,
            &result,
        );
        let back_test_id = repository.insert_log(&log).await.with_context(|| {
            format!(
                "persist backtest log failed: label={} strategy_type={}",
                loaded.case.label,
                strategy_type.as_str()
            )
        })?;
        let details = backtest_details_for_persistence(
            back_test_id,
            strategy_type,
            &loaded.case,
            result.trade_records,
        );
        let details_inserted = repository.insert_details(&details).await.with_context(|| {
            format!(
                "persist backtest details failed: label={} strategy_type={} back_test_id={}",
                loaded.case.label,
                strategy_type.as_str(),
                back_test_id
            )
        })?;
        println!(
            "backtest_persisted label={} strategy_type={} back_test_log_id={} details_inserted={}",
            loaded.case.label,
            strategy_type.as_str(),
            back_test_id,
            details_inserted
        );
        reports.push(run.report);
    }
    Ok(reports)
}

/// Builds the quant_core repository used by default backtest persistence.
fn connect_backtest_repository() -> Result<SqlxBacktestRepository> {
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("missing QUANT_CORE_DATABASE_URL for backtest persistence")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_lazy(&database_url)
        .context("create quant_core postgres pool for backtest persistence")?;
    Ok(SqlxBacktestRepository::new(pool))
}

/// Builds the back_test_log row for the normal report persistence path.
fn backtest_log_for_persistence(
    case: &StrategyCase,
    candles: &[CandleItem],
    strategy_type: StrategyType,
    strategy_detail: Value,
    risk_config: BasicRiskStrategyConfig,
    result: &BackTestResult,
) -> BacktestLog {
    BacktestLog::new(
        strategy_type.as_str().to_string(),
        case.symbol.to_string(),
        case.period.to_string(),
        result.win_rate.to_string(),
        result.funds.to_string(),
        result.open_trades as i32,
        Some(strategy_detail.to_string()),
        json!(risk_config).to_string(),
        (result.funds - 100.0).to_string(),
        candles.first().map(|candle| candle.ts).unwrap_or_default(),
        candles.last().map(|candle| candle.ts).unwrap_or_default(),
        candles.len() as i32,
    )
}

/// Builds back_test_detail rows from framework trade records without writing auxiliary tables.
fn backtest_details_for_persistence(
    back_test_id: i64,
    strategy_type: StrategyType,
    case: &StrategyCase,
    trade_records: Vec<TradeRecord>,
) -> Vec<BacktestDetail> {
    trade_records
        .into_iter()
        .map(|trade_record| {
            BacktestDetail::new(
                back_test_id,
                trade_record.option_type,
                strategy_type.as_str().to_string(),
                case.symbol.to_string(),
                case.period.to_string(),
                trade_record.open_position_time,
                trade_record.signal_open_position_time,
                trade_record.signal_status,
                trade_record.close_position_time.unwrap_or_default(),
                trade_record.open_price.to_string(),
                trade_record.close_price.map(|price| price.to_string()),
                trade_record.profit_loss.to_string(),
                trade_record.quantity.to_string(),
                trade_record.full_close.to_string(),
                trade_record.close_type,
                trade_record.win_num,
                trade_record.loss_num,
                trade_record.signal_value.unwrap_or_default(),
                trade_record.signal_result.unwrap_or_default(),
                trade_record.stop_loss_source,
                trade_record.stop_loss_update_history,
            )
        })
        .collect()
}

/// Captures the report metadata that explains which CLI case produced a persisted row.
fn strategy_detail_for_persistence(case: &StrategyCase, report: &CaseReport) -> Value {
    json!({
        "source": "btc_eth_strategy_family_okx_backtest",
        "case_label": case.label,
        "symbol": case.symbol,
        "period": case.period,
        "strategy_family": strategy_family_key(case.family),
        "candles": report.candles,
        "entries": report.entries,
        "closed": report.closed,
        "wins": report.wins,
        "losses": report.losses,
        "win_rate_pct": report.win_rate_pct,
        "pnl": report.pnl,
        "max_drawdown_pct": report.max_drawdown_pct,
        "trades_per_day": report.trades_per_day,
    })
}

/// Provides stable family labels for strategy_detail JSON without changing StrategyType values.
fn strategy_family_key(family: StrategyFamily) -> &'static str {
    match family {
        StrategyFamily::Scalper => "btc_eth_liquidity_scalper",
        StrategyFamily::MicroScalper1m => "micro_scalper_1m",
        StrategyFamily::EthVolumeReversal5m => "eth_volume_reversal_5m",
        StrategyFamily::EthVolumeReversalDual5m => "eth_volume_reversal_dual_5m",
        StrategyFamily::BtcVolumeReversalDual5m => "btc_volume_reversal_dual_5m",
        StrategyFamily::BtcVolumeReversalHybrid5m => "btc_volume_reversal_hybrid_5m",
        StrategyFamily::Breakdown => "bear_breakdown_short",
        StrategyFamily::Exhaustion => "exhaustion_fade_short",
    }
}

async fn load_sharded_candles(symbol: &str, period: &str, limit: usize) -> Result<Vec<CandleItem>> {
    let entities = get_confirmed_candles_for_backtest(symbol, period, limit, None)
        .await
        .with_context(|| {
            format!(
                "load quant_core sharded candles failed: symbol={symbol} period={period} limit={limit}"
            )
        })?;
    let mut candles = entities
        .iter()
        .map(|entity| candle_entity_to_item(entity, symbol, period))
        .collect::<Result<Vec<_>>>()?;
    candles.sort_unstable_by_key(|candle| candle.ts);
    candles.dedup_by_key(|candle| candle.ts);
    Ok(candles)
}

async fn load_sharded_market_context(
    repo: &ShardedExternalMarketSnapshotRepository,
    symbol: &str,
    candles: &[CandleItem],
) -> Result<BacktestMarketContext> {
    let Some((start_time, end_time)) = candle_time_range(candles) else {
        return Ok(BacktestMarketContext::default());
    };
    let start_time = start_time.saturating_sub(MARKET_CONTEXT_LOOKBACK_MS);
    let series = MarketContextSnapshotSeries {
        funding: load_sharded_metric(repo, symbol, FUNDING_RATE_METRIC, start_time, end_time)
            .await?,
        open_interest: load_sharded_metric(
            repo,
            symbol,
            OPEN_INTEREST_VOLUME_METRIC,
            start_time,
            end_time,
        )
        .await?,
        taker: load_sharded_metric(repo, symbol, TAKER_VOLUME_METRIC, start_time, end_time).await?,
        long_short: load_sharded_metric(
            repo,
            symbol,
            LONG_SHORT_RATIO_METRIC,
            start_time,
            end_time,
        )
        .await?,
    };
    Ok(build_backtest_market_context(candles, &series))
}

async fn load_sharded_metric(
    repo: &ShardedExternalMarketSnapshotRepository,
    symbol: &str,
    metric_type: &str,
    start_time: i64,
    end_time: i64,
) -> Result<Vec<ExternalMarketSnapshot>> {
    let mut rows = repo
        .find_range(
            OKX_SOURCE,
            symbol,
            metric_type,
            start_time,
            end_time,
            Some(MARKET_CONTEXT_QUERY_LIMIT),
        )
        .await
        .with_context(|| {
            format!(
                "load OKX sharded market context failed: symbol={symbol} metric_type={metric_type}"
            )
        })?;
    rows.sort_unstable_by_key(|row| row.metric_time);
    Ok(rows)
}

fn candle_time_range(candles: &[CandleItem]) -> Option<(i64, i64)> {
    Some((candles.first()?.ts, candles.last()?.ts))
}

fn candle_entity_to_item(entity: &CandlesEntity, symbol: &str, period: &str) -> Result<CandleItem> {
    Ok(CandleItem {
        ts: entity.ts,
        o: parse_candle_number(&entity.o, "open", entity.ts, symbol, period)?,
        h: parse_candle_number(&entity.h, "high", entity.ts, symbol, period)?,
        l: parse_candle_number(&entity.l, "low", entity.ts, symbol, period)?,
        c: parse_candle_number(&entity.c, "close", entity.ts, symbol, period)?,
        v: parse_candle_number(&entity.vol_ccy, "volume", entity.ts, symbol, period)?,
        confirm: entity.confirm.parse::<i32>().unwrap_or(1),
    })
}

fn parse_candle_number(
    value: &str,
    field: &str,
    ts: i64,
    symbol: &str,
    period: &str,
) -> Result<f64> {
    value.parse::<f64>().with_context(|| {
        format!("invalid candle {field}: symbol={symbol} period={period} ts={ts} value={value}")
    })
}

fn build_backtest_market_context(
    candles: &[CandleItem],
    series: &MarketContextSnapshotSeries,
) -> BacktestMarketContext {
    let mut context = BacktestMarketContext::default();
    for candle in candles {
        let Some(funding) = latest_snapshot_at(&series.funding, candle.ts)
            .and_then(|snapshot| snapshot.funding_rate)
        else {
            continue;
        };
        let Some((oi_growth_pct, _latest_oi)) = oi_growth_at(&series.open_interest, candle.ts)
        else {
            continue;
        };
        let Some((taker_buy, taker_sell)) =
            latest_snapshot_at(&series.taker, candle.ts).and_then(taker_volumes)
        else {
            continue;
        };
        let Some(long_short_ratio) = latest_snapshot_at(&series.long_short, candle.ts)
            .and_then(|snapshot| snapshot.long_short_ratio)
        else {
            continue;
        };
        context
            .scalper
            .push(BtcEthLiquidityScalperBacktestMarketContext {
                ts: candle.ts,
                funding_rate: funding,
                oi_expansion_pct: oi_growth_pct,
                taker_buy_volume: taker_buy,
                taker_sell_volume: taker_sell,
                orderbook_imbalance: 0.0,
                spread_bps: 1.0,
                depth_usd: 25_000_000.0,
            });
        context.bear.push(BearShortStackBacktestMarketContext {
            ts: candle.ts,
            funding_rate: funding,
            oi_growth_pct,
            long_short_ratio,
            taker_buy_volume: taker_buy,
            taker_sell_volume: taker_sell,
        });
    }
    context
}

fn latest_snapshot_at(rows: &[ExternalMarketSnapshot], ts: i64) -> Option<&ExternalMarketSnapshot> {
    rows.iter().take_while(|row| row.metric_time <= ts).last()
}

fn oi_growth_at(rows: &[ExternalMarketSnapshot], ts: i64) -> Option<(f64, f64)> {
    let latest_index = rows
        .iter()
        .enumerate()
        .take_while(|(_, row)| row.metric_time <= ts)
        .filter(|(_, row)| row.open_interest.is_some())
        .map(|(index, _)| index)
        .last()?;
    let previous_index = rows[..latest_index]
        .iter()
        .enumerate()
        .filter(|(_, row)| row.open_interest.is_some())
        .map(|(index, _)| index)
        .last()?;
    let latest = rows[latest_index].open_interest?;
    let previous = rows[previous_index].open_interest?;
    if previous.abs() <= f64::EPSILON {
        return None;
    }
    Some(((latest - previous) / previous.abs() * 100.0, latest))
}

fn taker_volumes(snapshot: &ExternalMarketSnapshot) -> Option<(f64, f64)> {
    let payload = snapshot.raw_payload.as_ref()?;
    let buy = payload_number(payload, &["buy_volume", "buyVolume", "buyVol"])?;
    let sell = payload_number(payload, &["sell_volume", "sellVolume", "sellVol"])?;
    Some((buy, sell))
}

fn payload_number(payload: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| match payload.get(*key)? {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    })
}

fn run_loaded_case(
    loaded: &LoadedCase,
    risk: BasicRiskStrategyConfig,
    scalper_tuning: Option<BtcEthLiquidityScalperBacktestTuning>,
    bear_tuning: Option<BearShortStackBacktestTuning>,
) -> BackTestResult {
    let case = &loaded.case;
    let candles = loaded.candles.as_slice();
    match case.family {
        StrategyFamily::MicroScalper1m => run_micro_scalper_1m(case.symbol, candles, risk),
        StrategyFamily::EthVolumeReversal5m => {
            run_eth_volume_reversal_5m(case.symbol, candles, risk)
        }
        StrategyFamily::EthVolumeReversalDual5m => {
            run_eth_volume_reversal_dual_5m(case.symbol, candles, risk)
        }
        StrategyFamily::BtcVolumeReversalDual5m => {
            volume_reversal_5m::run_btc_volume_reversal_dual_5m(case.symbol, candles, risk)
        }
        StrategyFamily::BtcVolumeReversalHybrid5m => {
            run_btc_volume_reversal_hybrid_5m(case.symbol, candles, risk)
        }
        StrategyFamily::Scalper => {
            if loaded.context_required {
                BtcEthLiquidityScalperStrategy.run_test_with_tuning_and_context(
                    case.symbol,
                    candles,
                    risk,
                    scalper_tuning.unwrap_or_default(),
                    loaded.context.scalper.clone(),
                )
            } else if let Some(tuning) = scalper_tuning {
                BtcEthLiquidityScalperStrategy.run_test_with_tuning(
                    case.symbol,
                    candles,
                    risk,
                    tuning,
                )
            } else {
                BtcEthLiquidityScalperStrategy.run_test(case.symbol, candles, risk)
            }
        }
        StrategyFamily::Breakdown => {
            if loaded.context_required {
                let tuning = bear_tuning_for_context_run(case.family, bear_tuning);
                BearShortStackStrategy::for_preset_with_tuning_and_context(
                    BearShortPreset::BearBreakdown,
                    tuning,
                    loaded.context.bear.clone(),
                )
                .run_test(case.symbol, candles, risk)
            } else if let Some(tuning) = bear_tuning {
                BearShortStackStrategy::for_preset_with_tuning(
                    BearShortPreset::BearBreakdown,
                    tuning,
                )
                .run_test(case.symbol, candles, risk)
            } else {
                BearShortStackStrategy::for_preset(BearShortPreset::BearBreakdown).run_test(
                    case.symbol,
                    candles,
                    risk,
                )
            }
        }
        StrategyFamily::Exhaustion => {
            if loaded.context_required {
                let tuning = bear_tuning_for_context_run(case.family, bear_tuning);
                BearShortStackStrategy::for_preset_with_tuning_and_context(
                    BearShortPreset::ExhaustionFade,
                    tuning,
                    loaded.context.bear.clone(),
                )
                .run_test(case.symbol, candles, risk)
            } else if let Some(tuning) = bear_tuning {
                BearShortStackStrategy::for_preset_with_tuning(
                    BearShortPreset::ExhaustionFade,
                    tuning,
                )
                .run_test(case.symbol, candles, risk)
            } else {
                BearShortStackStrategy::for_preset(BearShortPreset::ExhaustionFade).run_test(
                    case.symbol,
                    candles,
                    risk,
                )
            }
        }
    }
}

fn bear_tuning_for_context_run(
    family: StrategyFamily,
    provided: Option<BearShortStackBacktestTuning>,
) -> BearShortStackBacktestTuning {
    if let Some(tuning) = provided {
        return tuning;
    }
    if matches!(family, StrategyFamily::Breakdown) {
        return context_breakdown_tuning();
    }
    if matches!(family, StrategyFamily::Exhaustion) {
        return context_exhaustion_tuning();
    }
    BearShortStackBacktestTuning::default()
}

fn bear_tuning_for_report_family(
    family: StrategyFamily,
    tunings: ReportTuningOverrides,
) -> Option<BearShortStackBacktestTuning> {
    match family {
        StrategyFamily::Breakdown => tunings.breakdown,
        StrategyFamily::Exhaustion => tunings.exhaustion,
        StrategyFamily::Scalper
        | StrategyFamily::MicroScalper1m
        | StrategyFamily::EthVolumeReversal5m
        | StrategyFamily::EthVolumeReversalDual5m
        | StrategyFamily::BtcVolumeReversalDual5m
        | StrategyFamily::BtcVolumeReversalHybrid5m => None,
    }
}

/// Maps a report case to the versioned strategy type stored in backtest tables.
fn strategy_type_for_persistence(case: &StrategyCase) -> Option<StrategyType> {
    match case.family {
        StrategyFamily::Scalper => Some(StrategyType::BtcEthLiquidityScalper),
        StrategyFamily::EthVolumeReversal5m => Some(StrategyType::EthVolumeReversal5mV1Research),
        StrategyFamily::EthVolumeReversalDual5m => {
            Some(StrategyType::EthVolumeReversalDual5mV1Research)
        }
        StrategyFamily::BtcVolumeReversalDual5m => {
            Some(StrategyType::BtcVolumeReversalDual5mV1Research)
        }
        StrategyFamily::BtcVolumeReversalHybrid5m => {
            Some(StrategyType::BtcVolumeReversalHybrid5mV1Research)
        }
        StrategyFamily::Breakdown | StrategyFamily::Exhaustion => {
            Some(StrategyType::BearShortStack)
        }
        StrategyFamily::MicroScalper1m => None,
    }
}

fn context_breakdown_tuning() -> BearShortStackBacktestTuning {
    BearShortStackBacktestTuning::real_context_default(BearShortPreset::BearBreakdown)
}

fn context_exhaustion_tuning() -> BearShortStackBacktestTuning {
    BearShortStackBacktestTuning::real_context_default(BearShortPreset::ExhaustionFade)
}

fn build_report(label: &str, candles: &[CandleItem], result: &BackTestResult) -> CaseReport {
    let closed = closed_records(result).collect::<Vec<_>>();
    let entry_records = result
        .trade_records
        .iter()
        .filter(|record| !is_exit_record(record))
        .map(|record| (record.open_position_time.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    let trade_outcomes = closed_trade_outcomes(&closed);
    let wins = trade_outcomes.iter().filter(|pnl| **pnl > 0.0).count();
    let losses = trade_outcomes.iter().filter(|pnl| **pnl < 0.0).count();
    let pnl = closed.iter().map(|record| record.profit_loss).sum::<f64>();
    let win_rate_pct = ratio_pct(wins, wins + losses);
    let days = candle_span_days(candles);
    CaseReport {
        label: label.to_string(),
        candles: candles.len(),
        entries: result.open_trades,
        closed: closed.len(),
        wins,
        losses,
        win_rate_pct,
        pnl,
        final_funds: result.funds,
        max_drawdown_pct: max_drawdown_pct(result),
        days,
        trades_per_day: if days > 0.0 {
            result.open_trades as f64 / days
        } else {
            0.0
        },
        trades: closed
            .iter()
            .map(|record| {
                closed_trade_debug(
                    record,
                    entry_records
                        .get(record.open_position_time.as_str())
                        .copied(),
                )
            })
            .collect(),
        filtered_signals: result
            .audit_trail
            .signal_snapshots
            .iter()
            .filter(|snapshot| snapshot.filtered)
            .count(),
        filtered_reason_counts: filtered_reason_counts(result),
        filtered_signal_snapshots: filtered_signal_snapshots(result),
    }
}

fn filtered_reason_counts(result: &BackTestResult) -> Vec<(String, usize)> {
    let mut counts = BTreeMap::<String, usize>::new();
    for snapshot in result
        .audit_trail
        .signal_snapshots
        .iter()
        .filter(|snapshot| snapshot.filtered)
    {
        for reason in &snapshot.filter_reasons {
            *counts.entry(reason.clone()).or_default() += 1;
        }
    }
    let mut counts = counts.into_iter().collect::<Vec<_>>();
    counts.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    counts
}

fn closed_trade_debug(
    record: &TradeRecord,
    entry_record: Option<&TradeRecord>,
) -> ClosedTradeDebug {
    ClosedTradeDebug {
        open_time: record.open_position_time.clone(),
        close_time: record.close_position_time.clone(),
        open_price: record.open_price,
        close_price: record.close_price,
        pnl: record.profit_loss,
        close_type: record.close_type.clone(),
        entry_snapshot: entry_record
            .and_then(|entry| entry.signal_value.as_deref())
            .and_then(parse_entry_snapshot_debug),
        entry_reasons: entry_record
            .and_then(|entry| entry.signal_result.as_deref())
            .map(parse_entry_reasons)
            .unwrap_or_default(),
    }
}

fn filtered_signal_snapshots(result: &BackTestResult) -> Vec<FilteredSignalDebug> {
    result
        .audit_trail
        .signal_snapshots
        .iter()
        .filter(|signal| signal.filtered)
        .filter_map(|signal| {
            let snapshot = parse_filtered_snapshot_debug(&signal.payload)?;
            Some(FilteredSignalDebug {
                ts: signal.ts,
                reasons: signal.filter_reasons.clone(),
                snapshot,
            })
        })
        .collect()
}

fn parse_filtered_snapshot_debug(payload: &str) -> Option<EntrySnapshotDebug> {
    let value = serde_json::from_str::<Value>(payload).ok()?;
    let single_value = value.get("single_value")?.as_str()?;
    parse_entry_snapshot_debug(single_value)
}

fn parse_entry_snapshot_debug(payload: &str) -> Option<EntrySnapshotDebug> {
    let value = serde_json::from_str::<Value>(payload).ok()?;
    let price = json_number(&value, "price")?;
    if price <= 0.0 {
        return None;
    }
    let failed_reclaim_high = json_number(&value, "failed_reclaim_high").unwrap_or(price);
    let stop_distance_pct = json_number(&value, "stop_price")
        .map(|stop_price| (price - stop_price).abs() / price * 100.0)
        .unwrap_or_else(|| (failed_reclaim_high - price).max(0.0) / price * 100.0);
    let atr_15m = json_number(&value, "atr_15m").unwrap_or(0.0);
    let taker_buy_volume = json_number(&value, "taker_buy_volume").unwrap_or(0.0);
    let taker_sell_volume = json_number(&value, "taker_sell_volume").unwrap_or(0.0);
    let ema_distance_pct = json_number(&value, "ema696")
        .map(|ema696| (ema696 - price) / price * 100.0)
        .unwrap_or(0.0);
    Some(EntrySnapshotDebug {
        stop_distance_pct,
        atr_pct: atr_15m / price * 100.0,
        oi_growth_pct: json_number(&value, "oi_growth_pct").unwrap_or(0.0),
        funding_rate: json_number(&value, "funding_rate").unwrap_or(0.0),
        long_short_ratio: json_number(&value, "long_short_ratio").unwrap_or(0.0),
        taker_sell_buy_ratio: if taker_buy_volume > 0.0 {
            taker_sell_volume / taker_buy_volume
        } else {
            0.0
        },
        target_r: json_number(&value, "target_r").unwrap_or(0.0),
        ema_distance_pct,
        volume_multiple: json_number(&value, "volume_multiple").unwrap_or(0.0),
        downside_excursion_pct: json_number(&value, "downside_excursion_pct").unwrap_or(0.0),
        rebound_close_pos: json_number(&value, "rebound_close_pos").unwrap_or(0.0),
        candle_range_pct: json_number(&value, "candle_range_pct").unwrap_or(0.0),
        body_pct: json_number(&value, "body_pct").unwrap_or(0.0),
        lower_wick_pct: json_number(&value, "lower_wick_pct").unwrap_or(0.0),
        upper_wick_pct: json_number(&value, "upper_wick_pct").unwrap_or(0.0),
    })
}

fn json_number(value: &Value, field: &str) -> Option<f64> {
    value.get(field)?.as_f64()
}

fn parse_entry_reasons(payload: &str) -> Vec<String> {
    serde_json::from_str::<Value>(payload)
        .ok()
        .and_then(|value| {
            value.get("reasons")?.as_array().map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect()
            })
        })
        .unwrap_or_default()
}

fn closed_records(result: &BackTestResult) -> impl Iterator<Item = &TradeRecord> {
    result
        .trade_records
        .iter()
        .filter(|record| is_exit_record(record))
}

fn closed_trade_outcomes(closed: &[&TradeRecord]) -> Vec<f64> {
    let mut outcomes = BTreeMap::<&str, f64>::new();
    for record in closed {
        *outcomes
            .entry(record.open_position_time.as_str())
            .or_default() += record.profit_loss;
    }
    outcomes.into_values().collect()
}

fn is_exit_record(record: &TradeRecord) -> bool {
    record.option_type == "close"
}

fn ratio_pct(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}

fn candle_span_days(candles: &[CandleItem]) -> f64 {
    match (candles.first(), candles.last()) {
        (Some(first), Some(last)) if last.ts > first.ts => {
            (last.ts - first.ts) as f64 / 86_400_000.0
        }
        _ => 0.0,
    }
}

fn max_drawdown_pct(result: &BackTestResult) -> f64 {
    let mut equity = 100.0;
    let mut peak = equity;
    let mut max_drawdown = 0.0;

    for record in closed_records(result) {
        equity += record.profit_loss;
        if equity > peak {
            peak = equity;
        }
        if peak > 0.0 {
            let drawdown = (peak - equity) / peak * 100.0;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }
        }
    }

    max_drawdown
}

fn print_reports(reports: &[CaseReport], debug_trades: bool) {
    let total_wins = reports.iter().map(|report| report.wins).sum::<usize>();
    let total_losses = reports.iter().map(|report| report.losses).sum::<usize>();
    let total_pnl = reports.iter().map(|report| report.pnl).sum::<f64>();
    let total_entries = reports.iter().map(|report| report.entries).sum::<usize>();
    let max_drawdown = reports
        .iter()
        .map(|report| report.max_drawdown_pct)
        .fold(0.0, f64::max);
    let combo_days = reports.iter().map(|report| report.days).fold(0.0, f64::max);
    let trades_per_day = if combo_days > 0.0 {
        total_entries as f64 / combo_days
    } else {
        0.0
    };

    for report in reports {
        println!(
            "{} source=quant_core_sharded candles={} entries={} closed={} wins={} losses={} win_rate={:.2}% pnl={:.4} final_funds={:.4} max_dd={:.2}% days={:.2} trades_per_day={:.2}",
            report.label,
            report.candles,
            report.entries,
            report.closed,
            report.wins,
            report.losses,
            report.win_rate_pct,
            report.pnl,
            report.final_funds,
            report.max_drawdown_pct,
            report.days,
            report.trades_per_day
        );
        if debug_trades {
            if report.filtered_signals > 0 {
                println!(
                    "  filtered_signals={} top_reasons={}",
                    report.filtered_signals,
                    format_reason_counts(&report.filtered_reason_counts)
                );
                for filtered in report.filtered_signal_snapshots.iter().take(6) {
                    println!(
                        "    filtered_signal ts={} reasons={} stop_dist={:.4}% atr={:.4}% oi_growth={:.4}% funding={:.6} long_short={:.4} taker_sell_buy={:.4}",
                        filtered.ts,
                        filtered.reasons.join(","),
                        filtered.snapshot.stop_distance_pct,
                        filtered.snapshot.atr_pct,
                        filtered.snapshot.oi_growth_pct,
                        filtered.snapshot.funding_rate,
                        filtered.snapshot.long_short_ratio,
                        filtered.snapshot.taker_sell_buy_ratio
                    );
                }
            }
            for trade in &report.trades {
                println!(
                    "  trade open={} close={:?} open_price={:.4} close_price={:?} pnl={:.4} close_type={}",
                    trade.open_time,
                    trade.close_time,
                    trade.open_price,
                    trade.close_price,
                    trade.pnl,
                    trade.close_type
                );
                if let Some(snapshot) = trade.entry_snapshot {
                    println!(
                        "    entry_snapshot stop_dist={:.4}% atr={:.4}% oi_growth={:.4}% funding={:.6} long_short={:.4} taker_sell_buy={:.4}",
                        snapshot.stop_distance_pct,
                        snapshot.atr_pct,
                        snapshot.oi_growth_pct,
                        snapshot.funding_rate,
                        snapshot.long_short_ratio,
                        snapshot.taker_sell_buy_ratio
                    );
                }
                if !trade.entry_reasons.is_empty() {
                    println!("    entry_reasons={}", trade.entry_reasons.join(","));
                }
            }
        }
    }

    println!(
        "combined source=quant_core_sharded entries={total_entries} wins={total_wins} losses={total_losses} win_rate={:.2}% pnl={total_pnl:.4} max_dd={max_drawdown:.2}% days={combo_days:.2} trades_per_day={trades_per_day:.2}",
        ratio_pct(total_wins, total_wins + total_losses)
    );
}

fn format_reason_counts(counts: &[(String, usize)]) -> String {
    counts
        .iter()
        .take(6)
        .map(|(reason, count)| format!("{reason}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
#[path = "btc_eth_strategy_family_okx_backtest/tests.rs"]
mod btc_eth_strategy_family_okx_backtest_tests;
