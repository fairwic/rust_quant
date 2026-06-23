/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
async fn fetch_latest_backtest_response(
    pool: &PgPool,
    query: &LatestBacktestQuery,
) -> Result<LatestBacktestResponse> {
    let Some(log) = fetch_latest_backtest_log(pool, query).await? else {
        return Ok(LatestBacktestResponse {
            summary: default_latest_backtest_summary(None),
            strategy_detail: Value::Null,
            risk_config_detail: Value::Null,
            signals: Vec::new(),
            signal_total: 0,
        });
    };
    let (signals, signal_total) = fetch_latest_backtest_signals(pool, log.id, query).await?;
    Ok(LatestBacktestResponse {
        summary: latest_backtest_summary_from_log(&log),
        strategy_detail: parse_json_value_or_string(&log.strategy_detail),
        risk_config_detail: parse_json_value_or_string(&log.risk_config_detail),
        signals,
        signal_total,
    })
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
async fn fetch_latest_backtest_log(
    pool: &PgPool,
    query: &LatestBacktestQuery,
) -> Result<Option<LatestBacktestLogRow>> {
    let row = sqlx::query_as::<_, LatestBacktestLogRow>(
        r#"
        SELECT
            log.id::INT4 AS id,
            log.strategy_type,
            log.inst_type,
            log.time,
            log.final_fund,
            log.profit,
            log.win_rate,
            log.open_positions_num,
            log.one_bar_after_win_rate,
            log.two_bar_after_win_rate,
            log.three_bar_after_win_rate,
            log.four_bar_after_win_rate,
            log.five_bar_after_win_rate,
            log.ten_bar_after_win_rate,
            log.kline_start_time,
            log.kline_end_time,
            log.kline_nums,
            log.sharpe_ratio,
            log.annual_return,
            log.total_return,
            log.max_drawdown,
            log.volatility,
            log.strategy_detail,
            log.risk_config_detail,
            log.created_at
        FROM back_test_log log
        WHERE LOWER(log.strategy_type) = $1
          AND UPPER(log.inst_type) = $2
          AND UPPER(log.time) = $3
          AND EXISTS (
              SELECT 1
              FROM back_test_detail detail
              WHERE detail.back_test_id = log.id
              LIMIT 1
          )
        ORDER BY log.created_at DESC, log.id DESC
        LIMIT 1
        "#,
    )
    .bind(&query.strategy_key)
    .bind(&query.symbol)
    .bind(&query.timeframe)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
async fn fetch_latest_backtest_signals(
    pool: &PgPool,
    back_test_id: i32,
    query: &LatestBacktestQuery,
) -> Result<(Vec<LatestBacktestSignalItem>, i64)> {
    let rows = sqlx::query_as::<_, LatestBacktestSignalRow>(
        r#"
        SELECT
            id::INT4 AS id,
            back_test_id::INT4 AS back_test_id,
            time,
            option_type,
            close_type,
            open_position_time,
            close_position_time,
            open_price,
            close_price,
            profit_loss,
            quantity,
            COALESCE(signal_value, '') AS signal_value,
            signal_result
        FROM back_test_detail
        WHERE back_test_id = $1
          AND LOWER(option_type) <> 'close'
        ORDER BY open_position_time DESC
        LIMIT $2
        "#,
    )
    .bind(back_test_id)
    .bind(query.limit)
    .fetch_all(pool)
    .await?;
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM back_test_detail WHERE back_test_id = $1 AND LOWER(option_type) <> 'close'",
    )
    .bind(back_test_id)
    .fetch_one(pool)
    .await?;
    let signals = rows
        .into_iter()
        .map(|row| LatestBacktestSignalItem {
            id: row.id,
            back_test_id: row.back_test_id,
            time: row.time,
            option_type: row.option_type,
            close_type: row.close_type,
            open_position_time: row.open_position_time,
            close_position_time: row.close_position_time,
            open_price: row.open_price,
            close_price: row.close_price,
            profit_loss: row.profit_loss,
            quantity: row.quantity,
            signal_value: if query.include_signal_payload {
                parse_json_value_or_string(&row.signal_value)
            } else {
                Value::Null
            },
            signal_result: if query.include_signal_payload {
                row.signal_result
            } else {
                None
            },
        })
        .collect::<Vec<_>>();
    Ok((signals, total.0))
}
/// 校验输入和运行前置条件，提前暴露 回测与策略研究 的不可执行原因。
fn validate_backtest_request(request: &BacktestRunRequest) -> Result<(), &'static str> {
    if request.strategy_key.trim().is_empty() {
        return Err("strategyKey is required");
    }
    if request.symbol.trim().is_empty() {
        return Err("symbol is required");
    }
    if request.timeframe.trim().is_empty() {
        return Err("timeframe is required");
    }
    Ok(())
}
/// 校验输入和运行前置条件，提前暴露 回测与策略研究 的不可执行原因。
fn validate_backtest_runtime_contract(request: &BacktestRunRequest) -> Result<(), String> {
    if request.dry_run {
        return Ok(());
    }
    let source = std::env::var("STRATEGY_CONFIG_SOURCE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !source.is_empty()
        && !matches!(
            source.as_str(),
            "quant_core" | "postgres" | "strategy_config" | "legacy_pg"
        )
    {
        return Err(format!(
            "STRATEGY_CONFIG_SOURCE={} is not supported for non-dry-run backtests",
            source
        ));
    }
    let quant_core_database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .unwrap_or_default()
        .trim()
        .to_string();
    if quant_core_database_url.is_empty() {
        return Err("QUANT_CORE_DATABASE_URL is required for non-dry-run backtests".to_string());
    }
    Ok(())
}
/// 提供回测配置fromrequest的集中实现，避免回测策略调用方重复处理相同细节。
fn backtest_config_from_request(request: &BacktestRunRequest) -> BackTestConfig {
    let mut config = BackTestConfig::default();
    config.strategy_config_id = request
        .strategy_config_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(candle_limit) = read_usize_override(
        &request.config_overrides,
        &["kline_nums", "klineNums", "candle_limit", "candleLimit"],
    ) {
        config.candle_limit = candle_limit;
    }
    if let Some(max_concurrent) = read_usize_override(
        &request.config_overrides,
        &["max_concurrent", "maxConcurrent"],
    ) {
        config.max_concurrent = max_concurrent;
    }
    config.enable_random_test = false;
    config.enable_random_test_vegas = false;
    config.enable_specified_test_vegas = false;
    config.enable_random_test_nwe = false;
    config.enable_specified_test_nwe = false;
    if request.strategy_key.trim().eq_ignore_ascii_case("nwe") {
        config.enable_specified_test_nwe = true;
    } else {
        config.enable_specified_test_vegas = true;
    }
    config
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
fn read_usize_override(overrides: &Value, keys: &[&str]) -> Option<usize> {
    keys.iter()
        .filter_map(|key| overrides.get(*key))
        .find_map(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
}
/// 提供回测response请求体的集中实现，避免回测策略调用方重复处理相同细节。
fn backtest_response_body(run_id: &str, status: &str, request: &BacktestRunRequest) -> Value {
    json!({
        "runId": run_id,
        "status": status,
        "strategyConfigId": request.strategy_config_id,
        "strategyKey": request.strategy_key,
        "symbol": request.symbol,
        "timeframe": request.timeframe,
        "configOverrides": request.config_overrides,
        "dryRun": request.dry_run
    })
}
/// 提供最新回测summaryfromlog的集中实现，避免回测策略调用方重复处理相同细节。
fn latest_backtest_summary_from_log(log: &LatestBacktestLogRow) -> LatestBacktestSummary {
    LatestBacktestSummary {
        has_backtest: true,
        back_test_log_id: Some(log.id),
        strategy_type: Some(log.strategy_type.clone()),
        inst_type: Some(log.inst_type.clone()),
        time: Some(log.time.clone()),
        final_fund: Some(log.final_fund),
        profit: log.profit,
        win_rate: Some(log.win_rate.clone()),
        open_positions_num: Some(log.open_positions_num),
        one_bar_after_win_rate: log.one_bar_after_win_rate,
        two_bar_after_win_rate: log.two_bar_after_win_rate,
        three_bar_after_win_rate: log.three_bar_after_win_rate,
        four_bar_after_win_rate: log.four_bar_after_win_rate,
        five_bar_after_win_rate: log.five_bar_after_win_rate,
        ten_bar_after_win_rate: log.ten_bar_after_win_rate,
        kline_start_time: Some(log.kline_start_time),
        kline_end_time: Some(log.kline_end_time),
        kline_nums: Some(log.kline_nums),
        sharpe_ratio: log.sharpe_ratio,
        annual_return: log.annual_return,
        total_return: log.total_return,
        max_drawdown: log.max_drawdown,
        volatility: log.volatility,
        created_at: Some(log.created_at),
    }
}
/// 提供默认最新回测summary的集中实现，避免回测策略调用方重复处理相同细节。
fn default_latest_backtest_summary(back_test_log_id: Option<i32>) -> LatestBacktestSummary {
    LatestBacktestSummary {
        has_backtest: false,
        back_test_log_id,
        strategy_type: None,
        inst_type: None,
        time: None,
        final_fund: None,
        profit: None,
        win_rate: None,
        open_positions_num: None,
        one_bar_after_win_rate: None,
        two_bar_after_win_rate: None,
        three_bar_after_win_rate: None,
        four_bar_after_win_rate: None,
        five_bar_after_win_rate: None,
        ten_bar_after_win_rate: None,
        kline_start_time: None,
        kline_end_time: None,
        kline_nums: None,
        sharpe_ratio: None,
        annual_return: None,
        total_return: None,
        max_drawdown: None,
        volatility: None,
        created_at: None,
    }
}
