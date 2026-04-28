use anyhow::{anyhow, Result};
use chrono::{FixedOffset, NaiveDateTime, TimeZone, Utc};
use dotenv::dotenv;
use rust_quant_core::database::{get_db_pool, init_db_pool};
use rust_quant_domain::traits::StrategyConfigRepository;
use rust_quant_infrastructure::repositories::SqlxStrategyConfigRepository;
use rust_quant_market::models::{CandlesModel, SelectCandleReqDto, SelectTime, TimeDirect};
use rust_quant_services::strategy::{
    compare_parity_rows, compare_timing_parity, replay_live_with_warmup, to_parity_trade_rows,
    TimingParityReport,
};
use rust_quant_strategies::framework::strategy_registry::{
    get_strategy_registry, register_strategy_on_demand,
};
use sqlx::FromRow;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
struct ReplayArgs {
    config_id: Option<i64>,
    backtest_id: Option<i64>,
    inst_id: Option<String>,
    period: Option<String>,
    strategy_type: Option<String>,
    total_candles: usize,
    warmup_candles: usize,
    initial_funds: f64,
    price_eps: f64,
    pnl_eps: f64,
    output_dir: String,
    use_backtest_window: bool,
}

#[derive(Debug, Clone, FromRow)]
struct BacktestDetailRow {
    option_type: String,
    open_position_time: NaiveDateTime,
    close_position_time: NaiveDateTime,
    open_price: String,
    close_price: Option<String>,
    profit_loss: String,
    quantity: String,
    close_type: String,
    signal_status: i32,
}

#[derive(Debug, Clone, FromRow)]
struct BacktestLogRow {
    id: i64,
    strategy_type: String,
    inst_type: String,
    time: String,
    kline_start_time: Option<i64>,
    kline_end_time: Option<i64>,
    kline_nums: Option<i32>,
    strategy_detail: Option<String>,
    risk_config_detail: Option<String>,
}

fn parse_args() -> Result<ReplayArgs> {
    let mut kv = HashMap::<String, String>::new();
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0usize;
    while i < raw.len() {
        let item = &raw[i];
        if item.starts_with("--") {
            let key = item.trim_start_matches("--").to_string();
            let value = raw
                .get(i + 1)
                .ok_or_else(|| anyhow!("参数缺少值: {}", item))?
                .to_string();
            kv.insert(key, value);
            i += 2;
            continue;
        }
        i += 1;
    }

    Ok(ReplayArgs {
        config_id: kv.get("config-id").and_then(|v| v.parse::<i64>().ok()),
        backtest_id: kv.get("backtest-id").and_then(|v| v.parse::<i64>().ok()),
        inst_id: kv.get("inst-id").cloned(),
        period: kv.get("period").cloned(),
        strategy_type: kv.get("strategy-type").cloned(),
        total_candles: kv
            .get("total-candles")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(4000),
        warmup_candles: kv
            .get("warmup-candles")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(2000),
        initial_funds: kv
            .get("initial-funds")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(100.0),
        price_eps: kv
            .get("price-eps")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(1e-8),
        pnl_eps: kv
            .get("pnl-eps")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(1e-8),
        output_dir: kv
            .get("output-dir")
            .cloned()
            .unwrap_or_else(|| "dist/live_parity".to_string()),
        use_backtest_window: kv
            .get("use-backtest-window")
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(true),
    })
}

async fn load_config_by_id(config_id: i64) -> Result<rust_quant_domain::StrategyConfig> {
    let repo = SqlxStrategyConfigRepository::new(get_db_pool().clone());
    repo.find_by_id(config_id)
        .await?
        .ok_or_else(|| anyhow!("策略配置不存在: {}", config_id))
}

async fn load_backtest_log(backtest_id: i64) -> Result<BacktestLogRow> {
    let pool = get_db_pool();
    let log = sqlx::query_as::<_, BacktestLogRow>(
        "SELECT id, strategy_type, inst_type, time, kline_start_time, kline_end_time, kline_nums, strategy_detail, risk_config_detail FROM back_test_log WHERE id = $1 LIMIT 1",
    )
    .bind(backtest_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow!("back_test_log 不存在: {}", backtest_id))?;
    Ok(log)
}

async fn resolve_config_from_backtest(
    log: &BacktestLogRow,
) -> Result<rust_quant_domain::StrategyConfig> {
    let pool = get_db_pool();

    let repo = SqlxStrategyConfigRepository::new(pool.clone());
    let configs = repo
        .get_config(
            Some(log.strategy_type.as_str()),
            log.inst_type.as_str(),
            log.time.as_str(),
        )
        .await?;
    let latest = configs
        .into_iter()
        .max_by_key(|c| c.id)
        .ok_or_else(|| anyhow!("未找到对应策略配置: backtest_id={}", log.id))?;

    let mut config = latest.to_domain()?;
    if let Some(strategy_detail) = log.strategy_detail.as_ref() {
        config.parameters = serde_json::from_str(strategy_detail)
            .map_err(|e| anyhow!("解析 back_test_log.strategy_detail 失败: {}", e))?;
    }
    if let Some(risk_detail) = log.risk_config_detail.as_ref() {
        config.risk_config = serde_json::from_str(risk_detail)
            .map_err(|e| anyhow!("解析 back_test_log.risk_config_detail 失败: {}", e))?;
    }

    if let Some(start) = log.kline_start_time.filter(|v| *v > 0) {
        config.backtest_start = Some(start);
    }
    if let Some(end) = log.kline_end_time.filter(|v| *v > 0) {
        config.backtest_end = Some(end);
    }

    Ok(config)
}

async fn load_confirmed_candles(
    inst_id: &str,
    period: &str,
    total_candles: usize,
    select_time: Option<SelectTime>,
) -> Result<Vec<rust_quant_market::models::CandlesEntity>> {
    let model = CandlesModel::new();
    let mut candles = model
        .get_all(SelectCandleReqDto {
            inst_id: inst_id.to_string(),
            time_interval: period.to_string(),
            limit: total_candles,
            select_time,
            confirm: Some(1),
        })
        .await?;
    candles.sort_unstable_by_key(|a| a.ts);
    Ok(candles)
}

async fn load_backtest_expected_rows(
    backtest_id: i64,
) -> Result<Vec<rust_quant_services::strategy::ParityTradeRow>> {
    let rows = sqlx::query_as::<_, BacktestDetailRow>(
        "SELECT option_type, open_position_time, close_position_time, open_price, close_price, profit_loss, quantity, close_type, signal_status FROM back_test_detail WHERE back_test_id = $1 ORDER BY id ASC",
    )
    .bind(backtest_id)
    .fetch_all(get_db_pool())
    .await?;

    let mapped = rows
        .into_iter()
        .map(|r| {
            let open_price = r.open_price.parse::<f64>().unwrap_or(0.0);
            let close_price = r.close_price.and_then(|v| v.parse::<f64>().ok());
            let profit_loss = r.profit_loss.parse::<f64>().unwrap_or(0.0);
            let quantity = r.quantity.parse::<f64>().unwrap_or(0.0);
            rust_quant_services::strategy::ParityTradeRow {
                option_type: r.option_type,
                open_position_time: r.open_position_time.format("%Y-%m-%d %H:%M:%S").to_string(),
                close_position_time: Some(
                    r.close_position_time
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string(),
                ),
                open_price,
                close_price,
                profit_loss,
                quantity,
                close_type: r.close_type,
                signal_status: r.signal_status,
            }
        })
        .collect::<Vec<_>>();
    Ok(mapped)
}

fn build_analysis_text(
    args: &ReplayArgs,
    sim_result: &rust_quant_services::strategy::LiveReplayResult,
    report: &rust_quant_services::strategy::ParityComparisonReport,
    timing_report: &TimingParityReport,
) -> String {
    let mut text = String::new();
    text.push_str("# Live Replay Parity Report\n\n");
    text.push_str(&format!(
        "- total_candles: {}\n- warmup_candles: {}\n- initial_funds: {}\n",
        args.total_candles, args.warmup_candles, args.initial_funds
    ));
    text.push_str(&format!(
        "- simulated_trade_records: {}\n- simulated_paper_orders: {}\n- simulated_final_funds: {:.8}\n",
        sim_result.trade_records.len(),
        sim_result.paper_orders.len(),
        sim_result.final_funds
    ));
    text.push_str(&format!(
        "- expected_records: {}\n- matched_rows: {}\n- only_simulated: {}\n- only_expected: {}\n- differences: {}\n\n",
        report.expected_count,
        report.matched_rows,
        report.only_simulated,
        report.only_expected,
        report.differences.len()
    ));

    if report.expected_count > 0 {
        let match_ratio = if report.expected_count == 0 {
            0.0
        } else {
            report.matched_rows as f64 / report.expected_count as f64
        };
        text.push_str(&format!("- parity_match_ratio: {:.4}\n\n", match_ratio));
    }

    text.push_str("## Timing Parity (Open/Close Time Only)\n\n");
    text.push_str(&format!(
        "- matched_time_pairs: {}\n- pair_precision: {:.4}\n- pair_recall: {:.4}\n- pair_f1: {:.4}\n- matched_open_times: {} (precision={:.4}, recall={:.4})\n- matched_close_times: {} (precision={:.4}, recall={:.4})\n\n",
        timing_report.matched_time_pairs,
        timing_report.pair_precision,
        timing_report.pair_recall,
        timing_report.pair_f1,
        timing_report.matched_open_times,
        timing_report.open_precision,
        timing_report.open_recall,
        timing_report.matched_close_times,
        timing_report.close_precision,
        timing_report.close_recall,
    ));

    text.push_str("### Timing Difference Samples\n\n");
    if timing_report.only_expected_pair_samples.is_empty()
        && timing_report.only_simulated_pair_samples.is_empty()
    {
        text.push_str("- no timing differences\n\n");
    } else {
        for pair in timing_report.only_expected_pair_samples.iter().take(10) {
            text.push_str(&format!(
                "- only_expected open={} close={:?}\n",
                pair.open_position_time, pair.close_position_time
            ));
        }
        for pair in timing_report.only_simulated_pair_samples.iter().take(10) {
            text.push_str(&format!(
                "- only_simulated open={} close={:?}\n",
                pair.open_position_time, pair.close_position_time
            ));
        }
        text.push('\n');
    }

    text.push_str("## Difference Samples\n\n");
    if report.differences.is_empty() {
        text.push_str("- no differences\n");
    } else {
        for diff in report.differences.iter().take(20) {
            text.push_str(&format!(
                "- idx={} field={} simulated={} expected={}\n",
                diff.index, diff.field, diff.simulated, diff.expected
            ));
        }
    }
    text
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt::init();
    init_db_pool().await?;

    let args = parse_args()?;
    if args.warmup_candles >= args.total_candles {
        return Err(anyhow!(
            "warmup-candles 必须小于 total-candles: warmup={}, total={}",
            args.warmup_candles,
            args.total_candles
        ));
    }

    let backtest_log = if let Some(backtest_id) = args.backtest_id {
        Some(load_backtest_log(backtest_id).await?)
    } else {
        None
    };

    let config = if let Some(id) = args.config_id {
        load_config_by_id(id).await?
    } else if let Some(log) = backtest_log.as_ref() {
        resolve_config_from_backtest(log).await?
    } else {
        return Err(anyhow!(
            "缺少策略配置来源。请提供 --config-id 或 --backtest-id"
        ));
    };

    if let Some(inst_id) = args.inst_id.as_ref() {
        if &config.symbol != inst_id {
            return Err(anyhow!(
                "inst_id 与策略配置不一致: cfg={}, arg={}",
                config.symbol,
                inst_id
            ));
        }
    }
    if let Some(period) = args.period.as_ref() {
        if config.timeframe.as_str() != period {
            return Err(anyhow!(
                "period 与策略配置不一致: cfg={}, arg={}",
                config.timeframe.as_str(),
                period
            ));
        }
    }
    if let Some(strategy_type) = args.strategy_type.as_ref() {
        if config.strategy_type.as_str().to_lowercase() != strategy_type.to_lowercase() {
            return Err(anyhow!(
                "strategy_type 与策略配置不一致: cfg={}, arg={}",
                config.strategy_type.as_str(),
                strategy_type
            ));
        }
    }

    let inst_id = config.symbol.clone();
    let period = config.timeframe.as_str().to_string();
    let strategy_type = config.strategy_type;
    println!(
        "Replay config: strategy_config_id={}, strategy_type={}, inst_id={}, period={}",
        config.id,
        strategy_type.as_str(),
        inst_id,
        period
    );

    let mut candle_limit = args.total_candles;
    let mut select_time = None;
    if args.use_backtest_window {
        if let Some(log) = backtest_log.as_ref() {
            if let (Some(start), Some(end)) = (
                log.kline_start_time.filter(|v| *v > 0),
                log.kline_end_time.filter(|v| *v > 0),
            ) {
                select_time = Some(SelectTime {
                    start_time: start,
                    end_time: Some(end),
                    direct: TimeDirect::AFTER,
                });
                if let Some(kline_nums) = log.kline_nums.filter(|v| *v > 0) {
                    candle_limit = candle_limit.max(kline_nums as usize);
                }
                println!(
                    "Using backtest window: start_ts={}, end_ts={}, limit={}",
                    start, end, candle_limit
                );
            }
        }
    }

    let candles = load_confirmed_candles(&inst_id, &period, candle_limit, select_time).await?;
    if candles.len() <= args.warmup_candles {
        return Err(anyhow!(
            "可用K线不足: got={}, warmup={}",
            candles.len(),
            args.warmup_candles
        ));
    }
    println!(
        "Loaded candles: total={}, replay_segment={}, warmup={}",
        candles.len(),
        candles.len() - args.warmup_candles,
        args.warmup_candles
    );
    let replay_start_ts = candles[args.warmup_candles].ts;
    let replay_start_cn = FixedOffset::east_opt(8 * 3600)
        .and_then(|tz| tz.timestamp_millis_opt(replay_start_ts).single())
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "invalid-ts".to_string());
    let replay_start_utc = Utc
        .timestamp_millis_opt(replay_start_ts)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "invalid-ts".to_string());
    println!(
        "Replay start candle: ts={}, cn(+08)={}, utc={}",
        replay_start_ts, replay_start_cn, replay_start_utc
    );

    register_strategy_on_demand(&strategy_type);
    let executor = get_strategy_registry()
        .get(strategy_type.as_str())
        .map_err(|e| anyhow!("获取策略执行器失败: {}", e))?;

    let sim_result = replay_live_with_warmup(
        executor,
        &config,
        &candles,
        args.warmup_candles,
        args.initial_funds,
    )
    .await?;
    let simulated_rows = to_parity_trade_rows(&sim_result.trade_records);

    let expected_rows = if let Some(backtest_id) = args.backtest_id {
        load_backtest_expected_rows(backtest_id).await?
    } else {
        Vec::new()
    };

    let report = compare_parity_rows(
        &simulated_rows,
        &expected_rows,
        args.price_eps,
        args.pnl_eps,
    );
    let timing_report = compare_timing_parity(&simulated_rows, &expected_rows, 100);
    let analysis_md = build_analysis_text(&args, &sim_result, &report, &timing_report);

    let mut out_dir = PathBuf::from(args.output_dir.clone());
    fs::create_dir_all(&out_dir)?;
    let run_key = format!("cfg{}_{}_{}", config.id, inst_id.replace('-', "_"), period);

    out_dir.push(format!("{}_sim_trade_records.json", run_key));
    fs::write(
        &out_dir,
        serde_json::to_string_pretty(&sim_result.trade_records)?,
    )?;
    out_dir.pop();

    out_dir.push(format!("{}_sim_paper_orders.json", run_key));
    fs::write(
        &out_dir,
        serde_json::to_string_pretty(&sim_result.paper_orders)?,
    )?;
    out_dir.pop();

    if !expected_rows.is_empty() {
        out_dir.push(format!("{}_expected_backtest_rows.json", run_key));
        fs::write(&out_dir, serde_json::to_string_pretty(&expected_rows)?)?;
        out_dir.pop();
    }

    out_dir.push(format!("{}_parity_report.json", run_key));
    fs::write(&out_dir, serde_json::to_string_pretty(&report)?)?;
    out_dir.pop();

    out_dir.push(format!("{}_timing_parity_report.json", run_key));
    fs::write(&out_dir, serde_json::to_string_pretty(&timing_report)?)?;
    out_dir.pop();

    out_dir.push(format!("{}_parity_analysis.md", run_key));
    fs::write(&out_dir, analysis_md.as_bytes())?;

    println!("Parity report saved: {}", out_dir.display());
    println!(
        "Summary: matched_rows={}, expected={}, differences={}",
        report.matched_rows,
        report.expected_count,
        report.differences.len()
    );
    Ok(())
}
