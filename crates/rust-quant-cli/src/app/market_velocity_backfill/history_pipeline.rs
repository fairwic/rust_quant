use super::*;

pub(super) async fn load_okx_symbol_list_time_ms(
    pool: &PgPool,
    symbol: &str,
) -> Result<Option<i64>> {
    sqlx::query_scalar(
        r#"
        SELECT NULLIF(raw_payload ->> 'listTime', '')::BIGINT
        FROM exchange_symbols
        WHERE exchange = 'okx'
          AND market_type = 'perpetual'
          AND exchange_symbol = $1
        "#,
    )
    .bind(symbol)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load OKX symbol list time: {symbol}"))
    .map(Option::flatten)
}

pub(super) fn aligned_symbol_start_ms(
    configured_start_ms: i64,
    listed_at_ms: Option<i64>,
    candle_ms: i64,
) -> i64 {
    let available_start_ms = listed_at_ms
        .unwrap_or(configured_start_ms)
        .max(configured_start_ms);
    align_up_to_candle_boundary(available_start_ms, candle_ms)
}

fn align_up_to_candle_boundary(timestamp_ms: i64, candle_ms: i64) -> i64 {
    let remainder = timestamp_ms.rem_euclid(candle_ms);
    if remainder == 0 {
        timestamp_ms
    } else {
        timestamp_ms.saturating_add(candle_ms - remainder)
    }
}

/// 读取本地 K 线窗口的连续性摘要；已结束但仍未确认的 K 线也视为修复点，避免回测静默跳过历史数据。
pub(super) async fn load_candle_continuity_status(
    pool: &PgPool,
    symbol: &str,
    timeframe: &str,
    start_ms: i64,
    end_ms: i64,
    candle_ms: i64,
) -> Result<CandleContinuityStatus> {
    let table_name = CandlesModel::get_table_name(symbol, timeframe);
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = 'public'
              AND table_name = $1
        )",
    )
    .bind(&table_name)
    .fetch_one(pool)
    .await
    .with_context(|| format!("check candle table exists: {table_name}"))?;
    if !table_exists {
        return Ok(CandleContinuityStatus::default());
    }

    let quoted_table_name = quote_legacy_table_name(&table_name)?;
    let query = format!(
        r#"
        WITH windowed AS (
          SELECT ts, confirm
          FROM {quoted_table_name}
          WHERE ts >= $1
            AND ts <= $2
        ),
        bounds AS (
          SELECT
            MIN(ts) AS earliest_ts,
            MAX(ts) AS latest_ts,
            COUNT(*) FILTER (
              WHERE confirm = '1'
                 OR ts > $2 - $3
            )::BIGINT AS actual_count
          FROM windowed
        ),
        ordered AS (
          SELECT
            ts,
            LAG(ts) OVER (ORDER BY ts) AS prev_ts
          FROM windowed
        ),
        repair_points AS (
          SELECT ts
          FROM ordered
          WHERE prev_ts IS NOT NULL
            AND ts - prev_ts > $3
          UNION ALL
          SELECT ts
          FROM windowed
          WHERE confirm <> '1'
            AND ts <= $2 - $3
        ),
        gaps AS (
          SELECT MIN(ts) AS earliest_gap_start_ts
          FROM repair_points
        )
        SELECT
          bounds.earliest_ts,
          bounds.latest_ts,
          bounds.actual_count,
          gaps.earliest_gap_start_ts
        FROM bounds
        CROSS JOIN gaps
        "#
    );
    let row = sqlx::query(&query)
        .bind(start_ms)
        .bind(end_ms)
        .bind(candle_ms)
        .fetch_one(pool)
        .await
        .with_context(|| format!("load candle continuity status: {table_name}"))?;
    let earliest_ts = row.get::<Option<i64>, _>("earliest_ts");
    let latest_ts = row.get::<Option<i64>, _>("latest_ts");
    let actual_count = row.get::<i64, _>("actual_count");
    let expected_count = expected_candle_count(Some(start_ms), latest_ts, candle_ms);
    let earliest_gap_start_ts = row.get::<Option<i64>, _>("earliest_gap_start_ts");
    Ok(CandleContinuityStatus {
        earliest_ts,
        latest_ts,
        actual_count,
        expected_count,
        earliest_gap_start_ts,
    })
}

pub(super) fn resolve_incremental_backfill_window(
    configured_start_ms: i64,
    _end_ms: i64,
    candle_ms: i64,
    continuity: CandleContinuityStatus,
) -> IncrementalBackfillWindow {
    if continuity.latest_ts.is_none() {
        return IncrementalBackfillWindow {
            fetch_start_ms: configured_start_ms,
            reason: BackfillWindowReason::EmptyOrMissingTable,
        };
    }
    if continuity
        .earliest_ts
        .is_some_and(|earliest_ts| earliest_ts > configured_start_ms)
    {
        return IncrementalBackfillWindow {
            fetch_start_ms: configured_start_ms,
            reason: BackfillWindowReason::GapRepair,
        };
    }
    if continuity.has_missing_candles() {
        let repair_anchor = continuity
            .earliest_gap_start_ts
            .or(continuity.earliest_ts)
            .unwrap_or(configured_start_ms);
        return IncrementalBackfillWindow {
            fetch_start_ms: overlap_start_ms(repair_anchor, configured_start_ms, candle_ms),
            reason: BackfillWindowReason::GapRepair,
        };
    }
    IncrementalBackfillWindow {
        fetch_start_ms: overlap_start_ms(
            continuity.latest_ts.unwrap_or(configured_start_ms),
            configured_start_ms,
            candle_ms,
        ),
        reason: BackfillWindowReason::IncrementalTail,
    }
}

fn expected_candle_count(earliest_ts: Option<i64>, latest_ts: Option<i64>, candle_ms: i64) -> i64 {
    match (earliest_ts, latest_ts) {
        (Some(earliest_ts), Some(latest_ts)) if latest_ts >= earliest_ts && candle_ms > 0 => {
            ((latest_ts - earliest_ts) / candle_ms) + 1
        }
        _ => 0,
    }
}

fn overlap_start_ms(anchor_ms: i64, configured_start_ms: i64, candle_ms: i64) -> i64 {
    anchor_ms.saturating_sub(candle_ms).max(configured_start_ms)
}
/// 加载 行情与市场数据 运行所需数据，并把缺失或异常交给调用方处理。
pub async fn fetch_okx_history_candles(
    client: &Client,
    okx_rest_base: &str,
    symbol: &str,
    timeframe: &str,
    start_ms: i64,
    end_ms: i64,
    limit: usize,
    request_sleep_ms: u64,
) -> Result<Vec<CandleOkxRespDto>> {
    let mut candles_by_ts = BTreeMap::new();
    let mut after_ms = None;
    let candle_ms = candle_interval_ms(timeframe)?;
    let okx_bar = okx_bar_for_timeframe(timeframe)?;
    let max_pages = max_history_pages(start_ms, end_ms, candle_ms, limit);
    for page_index in 0..max_pages {
        let url = build_okx_history_candles_url(okx_rest_base, symbol, okx_bar, after_ms, limit)?;
        let payload = request_okx_history_candles_page(client, url, symbol).await?;
        if payload.code != "0" {
            bail!(okx_history_candles_api_error(
                &payload.code,
                &payload.msg,
                symbol
            ));
        }
        if payload.data.is_empty() {
            break;
        }
        let mut page_oldest = i64::MAX;
        for row in payload.data {
            let candle = parse_okx_candle_row(row)?;
            let ts = candle
                .ts
                .parse::<i64>()
                .context("parsed OKX candle timestamp should be numeric")?;
            page_oldest = page_oldest.min(ts);
            if start_ms <= ts && ts <= end_ms {
                candles_by_ts.insert(ts, candle);
            }
        }
        if page_oldest <= start_ms {
            break;
        }
        if after_ms.is_some_and(|previous_after| page_oldest >= previous_after) {
            warn!(
                "OKX history-candles pagination did not move older: symbol={}, previous_after={:?}, page_oldest={}",
                symbol, after_ms, page_oldest
            );
            break;
        }
        after_ms = Some(page_oldest);
        if page_index + 1 < max_pages && request_sleep_ms > 0 {
            sleep(Duration::from_millis(request_sleep_ms)).await;
        }
    }
    Ok(candles_by_ts.into_values().collect())
}

/// 请求单页 OKX 历史 K 线，并对限频、服务端错误和瞬时传输失败做同页退避重试。
async fn request_okx_history_candles_page(
    client: &Client,
    url: Url,
    symbol: &str,
) -> Result<OkxHistoryCandlesResponse> {
    for attempt in 0..=OKX_RATE_LIMIT_MAX_RETRIES {
        let response = match client
            .get(url.clone())
            .header("User-Agent", "rust-quant-market-velocity-backfill/1.0")
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) if attempt < OKX_RATE_LIMIT_MAX_RETRIES => {
                let backoff_ms = OKX_RATE_LIMIT_BACKOFF_MS * (attempt as u64 + 1);
                warn!(
                    "OKX history-candles transport failed; retrying page: symbol={}, attempt={}, backoff_ms={}, error={}",
                    symbol,
                    attempt + 1,
                    backoff_ms,
                    error
                );
                sleep(Duration::from_millis(backoff_ms)).await;
                continue;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("request OKX history-candles failed: symbol={symbol}")
                });
            }
        };

        if (response.status() == StatusCode::TOO_MANY_REQUESTS
            || response.status().is_server_error())
            && attempt < OKX_RATE_LIMIT_MAX_RETRIES
        {
            let backoff_ms = OKX_RATE_LIMIT_BACKOFF_MS * (attempt as u64 + 1);
            warn!(
                "OKX history-candles HTTP retry: symbol={}, status={}, attempt={}, backoff_ms={}",
                symbol,
                response.status(),
                attempt + 1,
                backoff_ms
            );
            sleep(Duration::from_millis(backoff_ms)).await;
            continue;
        }

        let response = response
            .error_for_status()
            .with_context(|| format!("OKX history-candles HTTP status failed: symbol={symbol}"))?;
        match response.json::<OkxHistoryCandlesResponse>().await {
            Ok(payload) => return Ok(payload),
            Err(error) if attempt < OKX_RATE_LIMIT_MAX_RETRIES => {
                let backoff_ms = OKX_RATE_LIMIT_BACKOFF_MS * (attempt as u64 + 1);
                warn!(
                    "OKX history-candles response interrupted; retrying page: symbol={}, attempt={}, backoff_ms={}, error={}",
                    symbol,
                    attempt + 1,
                    backoff_ms,
                    error
                );
                sleep(Duration::from_millis(backoff_ms)).await;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("decode OKX history-candles response failed: symbol={symbol}")
                });
            }
        }
    }

    unreachable!("OKX history-candles retry loop always returns on the final attempt")
}
/// 组装 OKX K 线接口错误文本，并保留 code/msg/symbol 供上层识别可落库的永久不可用交易对。
pub(super) fn okx_history_candles_api_error(code: &str, msg: &str, symbol: &str) -> String {
    format!("OKX history-candles returned code={code} msg={msg} symbol={symbol}")
}

/// 判断 OKX 是否明确返回交易对不存在；这是永久阻塞，应写回 DB 状态避免反复重试。
pub fn is_okx_missing_instrument_error(error: &anyhow::Error) -> bool {
    let error_text = format!("{error:#}");
    error_text.contains(&format!("code={OKX_MISSING_INSTRUMENT_CODE}"))
        && error_text.to_ascii_lowercase().contains("instrument")
}

/// 只有显式写入模式才能把 OKX 不存在的合约同步为已删除，保证 dry-run 不修改元数据。

pub fn build_okx_history_candles_url(
    okx_rest_base: &str,
    symbol: &str,
    timeframe: &str,
    after_ms: Option<i64>,
    limit: usize,
) -> Result<Url> {
    let base = okx_rest_base.trim_end_matches('/');
    let mut url = Url::parse(&format!("{base}/api/v5/market/history-candles"))
        .context("parse OKX REST base URL")?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("instId", symbol);
        pairs.append_pair("bar", timeframe);
        pairs.append_pair("limit", &limit.to_string());
        if let Some(after_ms) = after_ms {
            pairs.append_pair("after", &after_ms.to_string());
        }
    }
    Ok(url)
}
/// 构建 行情与市场数据 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_okx_http_client(proxy_url: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30));
    if let Some(proxy_url) = proxy_url.map(str::trim).filter(|value| !value.is_empty()) {
        builder = builder.proxy(Proxy::all(proxy_url).context("configure OKX REST proxy")?);
    }
    builder.build().context("build OKX REST HTTP client")
}
/// 判断K 线intervalms，给行情数据流程提供布尔结果。
pub fn candle_interval_ms(timeframe: &str) -> Result<i64> {
    match timeframe.trim().to_ascii_lowercase().as_str() {
        "1m" => Ok(CANDLE_1M_MS),
        "5m" => Ok(CANDLE_5M_MS),
        "15m" => Ok(CANDLE_15M_MS),
        "1h" => Ok(CANDLE_1H_MS),
        "4h" => Ok(CANDLE_4H_MS),
        other => bail!(
            "unsupported market velocity candle backfill timeframe: {other}; supported: 1m, 5m, 15m, 1h, 4h"
        ),
    }
}
/// 提供OKXbarfortimeframe的集中实现，避免行情数据调用方重复处理相同细节。
pub fn okx_bar_for_timeframe(timeframe: &str) -> Result<&'static str> {
    match timeframe.trim().to_ascii_lowercase().as_str() {
        "1m" => Ok("1m"),
        "5m" => Ok("5m"),
        "15m" => Ok("15m"),
        "1h" => Ok("1H"),
        "4h" => Ok("4H"),
        other => {
            bail!(
                "unsupported market velocity OKX candle bar: {other}; supported: 1m, 5m, 15m, 1h, 4h"
            )
        }
    }
}
pub fn parse_okx_candle_row(row: Vec<String>) -> Result<CandleOkxRespDto> {
    if row.len() < 9 {
        bail!(
            "OKX candle row has {} columns, expected at least 9",
            row.len()
        );
    }
    row[0]
        .parse::<i64>()
        .with_context(|| format!("invalid OKX candle timestamp: {}", row[0]))?;
    Ok(CandleOkxRespDto {
        ts: row[0].clone(),
        o: row[1].clone(),
        h: row[2].clone(),
        l: row[3].clone(),
        c: row[4].clone(),
        v: row[5].clone(),
        vol_ccy: row[6].clone(),
        vol_ccy_quote: row[7].clone(),
        confirm: row[8].clone(),
    })
}
/// 计算最大historypages，并把公式边界留在行情数据内部。
pub fn max_history_pages(start_ms: i64, end_ms: i64, candle_ms: i64, limit: usize) -> usize {
    if end_ms <= start_ms || candle_ms <= 0 || limit == 0 {
        return 1;
    }
    let expected_candles = ((end_ms - start_ms) as f64 / candle_ms as f64).ceil() as usize;
    (expected_candles / limit).saturating_add(8).max(1)
}
