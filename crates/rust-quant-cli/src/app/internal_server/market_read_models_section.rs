async fn fetch_market_klines_response(
    pool: &PgPool,
    query: &MarketKlineQuery,
) -> Result<Vec<MarketKlineItem>> {
    let mut rows = fetch_unified_market_klines(pool, query).await?;
    if rows.is_empty() {
        rows = fetch_legacy_market_klines(pool, query).await?;
    }
    rows.sort_by_key(|item| item.time);
    Ok(rows)
}

async fn fetch_market_rank_events_response(
    pool: &PgPool,
    query: &MarketRankEventsQuery,
) -> Result<Vec<MarketRankEventItem>> {
    if market_rank_sort_requires_legacy_volume_before_limit(query.sort.as_deref()) {
        return fetch_volume_15m_market_rank_events_response(pool, query).await;
    }

    if market_rank_sort_can_use_recent_query(query.sort.as_deref()) {
        return fetch_recent_market_rank_events_response(pool, query).await;
    }

    let sql = r#"
        SELECT
            latest.id,
            latest.exchange,
            latest.symbol,
            latest.event_type,
            latest.timeframe,
            latest.old_rank,
            latest.new_rank,
            latest.delta_rank,
            CASE
                WHEN latest.old_rank IS NOT NULL
                     AND latest.old_rank > 0
                     AND latest.delta_rank IS NOT NULL
                THEN ABS(latest.delta_rank)::FLOAT8 / latest.old_rank::FLOAT8 * 100.0
                ELSE NULL
            END AS rank_change_pct,
            latest.volume_24h_quote::FLOAT8 AS volume_24h_quote,
            previous.previous_volume_24h_quote,
            CASE
                WHEN previous.previous_volume_24h_quote IS NOT NULL
                     AND previous.previous_volume_24h_quote > 0
                     AND latest.volume_24h_quote IS NOT NULL
                THEN (latest.volume_24h_quote::FLOAT8 - previous.previous_volume_24h_quote)
                     / previous.previous_volume_24h_quote * 100.0
                ELSE NULL
            END AS volume_24h_change_pct,
            NULL::FLOAT8 AS volume_15m_quote,
            NULL::FLOAT8 AS volume_15m_change_pct,
            latest.current_price::FLOAT8 AS current_price,
            latest.previous_price::FLOAT8 AS previous_price,
            latest.price_change_pct::FLOAT8 AS price_change_pct,
            latest.price_direction,
            latest.price_change_pct::FLOAT8 AS price_change_24h_pct,
            latest.technical_timeframe,
            latest.technical_period,
            latest.technical_close_price::FLOAT8 AS technical_close_price,
            latest.technical_ma_value::FLOAT8 AS technical_ma_value,
            latest.technical_ema_value::FLOAT8 AS technical_ema_value,
            latest.technical_ma_distance_pct::FLOAT8 AS technical_ma_distance_pct,
            latest.technical_ema_distance_pct::FLOAT8 AS technical_ema_distance_pct,
            latest.technical_ma_state,
            latest.technical_ema_state,
            latest.technical_candle_count,
            latest.technical_snapshot_at,
            latest.technical_snapshot_status,
            latest.detected_at,
            latest.source,
            latest.notification_state
        FROM (
            SELECT DISTINCT ON (UPPER(symbol))
                id,
                exchange,
                symbol,
                event_type,
                timeframe,
                old_rank,
                new_rank,
                delta_rank,
                volume_24h_quote,
                current_price,
                previous_price,
                price_change_pct,
                price_direction,
                technical_timeframe,
                technical_period,
                technical_close_price,
                technical_ma_value,
                technical_ema_value,
                technical_ma_distance_pct,
                technical_ema_distance_pct,
                technical_ma_state,
                technical_ema_state,
                technical_candle_count,
                technical_snapshot_at,
                technical_snapshot_status,
                detected_at,
                source,
                notification_state
            FROM market_rank_events
            WHERE LOWER(exchange) = LOWER($1)
              AND ($2::TEXT IS NULL OR UPPER(symbol) = UPPER($2))
              AND ($3::TEXT IS NULL OR event_type = $3)
              AND ($4::TEXT IS NULL OR LOWER(COALESCE(timeframe, '')) = LOWER($4))
              AND detected_at >= NOW() - ($5::INTEGER * INTERVAL '1 minute')
            ORDER BY UPPER(symbol), detected_at DESC, id DESC
        ) latest
        LEFT JOIN LATERAL (
            SELECT previous.volume_24h_quote::FLOAT8 AS previous_volume_24h_quote
            FROM market_rank_events previous
            WHERE previous.exchange = latest.exchange
              AND previous.symbol = latest.symbol
              AND previous.volume_24h_quote IS NOT NULL
              AND previous.detected_at <= latest.detected_at - INTERVAL '15 minutes'
              AND previous.detected_at >= latest.detected_at - INTERVAL '24 hours'
            ORDER BY previous.detected_at ASC, previous.id ASC
            LIMIT 1
        ) previous ON TRUE
        WHERE latest.new_rank <= 50 OR latest.old_rank <= 50
        "#;
    let result = sqlx::query_as::<_, MarketRankEventItem>(sql)
        .bind(&query.exchange)
        .bind(query.symbol.as_deref())
        .bind(query.event_type.as_deref())
        .bind(query.timeframe.as_deref())
        .bind(query.lookback_minutes as i32)
        .fetch_all(pool)
        .await;

    match result {
        Ok(mut rows) => {
            if market_rank_sort_requires_legacy_volume_before_limit(query.sort.as_deref()) {
                attach_legacy_volume_15m(pool, &mut rows).await?;
                return Ok(finalize_market_rank_rows(
                    rows,
                    query.sort.as_deref(),
                    query.limit,
                ));
            }

            let mut rows = finalize_market_rank_rows(rows, query.sort.as_deref(), query.limit);
            attach_legacy_volume_15m(pool, &mut rows).await?;
            Ok(rows)
        }
        Err(err) if is_undefined_table_error(&err) => Ok(Vec::new()),
        Err(err) => Err(err.into()),
    }
}

async fn fetch_volume_15m_market_rank_events_response(
    pool: &PgPool,
    query: &MarketRankEventsQuery,
) -> Result<Vec<MarketRankEventItem>> {
    let mut recent_query = query.clone();
    recent_query.sort = None;
    recent_query.limit = MAX_MARKET_RANK_EVENT_LIMIT;

    let mut rows = fetch_recent_market_rank_events_response(pool, &recent_query).await?;
    attach_legacy_volume_15m(pool, &mut rows).await?;
    Ok(finalize_market_rank_rows(
        rows,
        query.sort.as_deref(),
        query.limit,
    ))
}

async fn fetch_recent_market_rank_events_response(
    pool: &PgPool,
    query: &MarketRankEventsQuery,
) -> Result<Vec<MarketRankEventItem>> {
    let sql = recent_market_rank_events_sql(query.sort.as_deref());

    let result = sqlx::query_as::<_, MarketRankEventItem>(&sql)
        .bind(&query.exchange)
        .bind(query.symbol.as_deref())
        .bind(query.event_type.as_deref())
        .bind(query.timeframe.as_deref())
        .bind(query.lookback_minutes as i32)
        .bind(query.limit)
        .fetch_all(pool)
        .await;

    match result {
        Ok(rows) => Ok(finalize_market_rank_rows(
            rows,
            query.sort.as_deref(),
            query.limit,
        )),
        Err(err) if is_undefined_table_error(&err) => Ok(Vec::new()),
        Err(err) => Err(err.into()),
    }
}

fn recent_market_rank_events_sql(sort: Option<&str>) -> String {
    format!(
        r#"
        WITH latest AS (
            SELECT DISTINCT ON (UPPER(symbol))
                id,
                exchange,
                symbol,
                event_type,
                timeframe,
                old_rank,
                new_rank,
                delta_rank,
                volume_24h_quote,
                current_price,
                previous_price,
                price_change_pct,
                price_direction,
                technical_timeframe,
                technical_period,
                technical_close_price,
                technical_ma_value,
                technical_ema_value,
                technical_ma_distance_pct,
                technical_ema_distance_pct,
                technical_ma_state,
                technical_ema_state,
                technical_candle_count,
                technical_snapshot_at,
                technical_snapshot_status,
                detected_at,
                source,
                notification_state
            FROM market_rank_events
            WHERE LOWER(exchange) = LOWER($1)
              AND ($2::TEXT IS NULL OR UPPER(symbol) = UPPER($2))
              AND ($3::TEXT IS NULL OR event_type = $3)
              AND ($4::TEXT IS NULL OR LOWER(COALESCE(timeframe, '')) = LOWER($4))
              AND detected_at >= NOW() - ($5::INTEGER * INTERVAL '1 minute')
              AND (new_rank <= 50 OR old_rank <= 50)
            ORDER BY UPPER(symbol), detected_at DESC, id DESC
        )
        SELECT
            id,
            exchange,
            symbol,
            event_type,
            timeframe,
            old_rank,
            new_rank,
            delta_rank,
            CASE
                WHEN old_rank IS NOT NULL
                     AND old_rank > 0
                     AND delta_rank IS NOT NULL
                THEN ABS(delta_rank)::FLOAT8 / old_rank::FLOAT8 * 100.0
                ELSE NULL
            END AS rank_change_pct,
            volume_24h_quote::FLOAT8 AS volume_24h_quote,
            NULL::FLOAT8 AS previous_volume_24h_quote,
            NULL::FLOAT8 AS volume_24h_change_pct,
            NULL::FLOAT8 AS volume_15m_quote,
            NULL::FLOAT8 AS volume_15m_change_pct,
            current_price::FLOAT8 AS current_price,
            previous_price::FLOAT8 AS previous_price,
            price_change_pct::FLOAT8 AS price_change_pct,
            price_direction,
            price_change_pct::FLOAT8 AS price_change_24h_pct,
            technical_timeframe,
            technical_period,
            technical_close_price::FLOAT8 AS technical_close_price,
            technical_ma_value::FLOAT8 AS technical_ma_value,
            technical_ema_value::FLOAT8 AS technical_ema_value,
            technical_ma_distance_pct::FLOAT8 AS technical_ma_distance_pct,
            technical_ema_distance_pct::FLOAT8 AS technical_ema_distance_pct,
            technical_ma_state,
            technical_ema_state,
            technical_candle_count,
            technical_snapshot_at,
            technical_snapshot_status,
            detected_at,
            source,
            notification_state
        FROM latest
        ORDER BY {}
        LIMIT $6
        "#,
        market_rank_recent_order_clause(sort)
    )
}

fn market_rank_sort_can_use_recent_query(sort: Option<&str>) -> bool {
    matches!(sort, None | Some("detected_at") | Some("delta_rank"))
}

fn market_rank_recent_order_clause(sort: Option<&str>) -> &'static str {
    match sort {
        None | Some("delta_rank") => {
            "rank_change_pct DESC NULLS LAST, ABS(COALESCE(delta_rank, 0)) DESC, detected_at DESC, id DESC"
        }
        _ => "detected_at DESC, id DESC",
    }
}

fn market_rank_sort_requires_legacy_volume_before_limit(sort: Option<&str>) -> bool {
    matches!(sort, Some("volume_15m"))
}

async fn attach_legacy_volume_15m(pool: &PgPool, rows: &mut [MarketRankEventItem]) -> Result<()> {
    for row in rows {
        let stats = fetch_legacy_volume_15m_stats(pool, &row.symbol, row.detected_at).await?;
        row.volume_15m_quote = stats.as_ref().and_then(|item| item.volume_15m_quote);
        row.volume_15m_change_pct = stats.as_ref().and_then(|item| item.volume_15m_change_pct);
    }
    Ok(())
}

async fn fetch_legacy_volume_15m_stats(
    pool: &PgPool,
    symbol: &str,
    detected_at: DateTime<Utc>,
) -> Result<Option<CandleVolume15mStats>> {
    let table_name = PostgresCandleRepository::quoted_table_name(symbol, Timeframe::M15)?;
    let detected_at_millis = detected_at.timestamp_millis();
    let detected_at_secs = detected_at.timestamp();
    let sql = format!(
        r#"
        WITH latest AS (
            SELECT
                ts,
                NULLIF(vol_ccy, '')::FLOAT8 AS volume_15m_quote
            FROM {table_name}
            WHERE (
                ts > 10000000000
                AND ts > $1::BIGINT - 1800000
                AND ts <= $1::BIGINT
            ) OR (
                ts <= 10000000000
                AND ts > $2::BIGINT - 1800
                AND ts <= $2::BIGINT
            )
            ORDER BY ts DESC
            LIMIT 1
        ),
        baseline AS (
            SELECT AVG(NULLIF(c.vol_ccy, '')::FLOAT8) AS avg_volume_15m_quote
            FROM {table_name} c
            JOIN latest ON TRUE
            WHERE c.ts < latest.ts
              AND c.ts >= latest.ts - CASE WHEN latest.ts > 10000000000 THEN 7200000 ELSE 7200 END
        )
        SELECT
            latest.volume_15m_quote,
            CASE
                WHEN baseline.avg_volume_15m_quote IS NOT NULL
                     AND baseline.avg_volume_15m_quote > 0
                THEN (latest.volume_15m_quote - baseline.avg_volume_15m_quote)
                     / baseline.avg_volume_15m_quote * 100.0
                ELSE NULL
            END AS volume_15m_change_pct
        FROM latest
        LEFT JOIN baseline ON TRUE
        "#
    );

    let result = sqlx::query_as::<_, CandleVolume15mStats>(&sql)
        .bind(detected_at_millis)
        .bind(detected_at_secs)
        .fetch_optional(pool)
        .await;

    match result {
        Ok(stats) => Ok(stats),
        Err(err) if is_undefined_table_error(&err) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn finalize_market_rank_rows(
    mut rows: Vec<MarketRankEventItem>,
    sort: Option<&str>,
    limit: i64,
) -> Vec<MarketRankEventItem> {
    for row in &mut rows {
        row.technical_context = build_market_rank_technical_context(MarketRankTechnicalSource {
            timeframe: row.technical_timeframe.as_deref(),
            period: row.technical_period,
            close_price: row.technical_close_price,
            ma_value: row.technical_ma_value,
            ema_value: row.technical_ema_value,
            ma_distance_pct: row.technical_ma_distance_pct,
            ema_distance_pct: row.technical_ema_distance_pct,
            ma_state: row.technical_ma_state.as_deref(),
            ema_state: row.technical_ema_state.as_deref(),
            candle_count: row.technical_candle_count,
            snapshot_at: row.technical_snapshot_at,
            snapshot_status: row.technical_snapshot_status.as_deref(),
        });
        if row.rank_change_pct.is_none() {
            row.rank_change_pct = compute_rank_change_pct(row.old_rank, row.delta_rank);
        }
        if row.volume_24h_change_pct.is_none() {
            row.volume_24h_change_pct =
                compute_change_pct(row.volume_24h_quote, row.previous_volume_24h_quote);
        }
    }
    rows.retain(is_market_rank_top_boundary_row);

    rows.sort_by(compare_market_rank_latest);

    let mut seen = HashSet::new();
    rows.retain(|row| seen.insert(row.symbol.trim().to_ascii_uppercase()));

    match sort {
        None | Some("delta_rank") => rows.sort_by(compare_market_rank_by_rank_change_pct),
        Some("volume_24h") => rows.sort_by(compare_market_rank_by_volume_24h_change_pct),
        Some("volume_15m") => rows.sort_by(compare_market_rank_by_volume_15m_change_pct),
        _ => rows.sort_by(compare_market_rank_latest),
    }

    rows.truncate(limit.max(0) as usize);
    rows
}

fn is_market_rank_top_boundary_row(row: &MarketRankEventItem) -> bool {
    rank_is_within_top_boundary(row.new_rank) || rank_is_within_top_boundary(row.old_rank)
}

fn rank_is_within_top_boundary(rank: Option<i32>) -> bool {
    rank.is_some_and(|value| value > 0 && value <= MARKET_RANK_TOP_BOUNDARY)
}

fn compute_rank_change_pct(old_rank: Option<i32>, delta_rank: Option<i32>) -> Option<f64> {
    let old_rank = old_rank?;
    let delta_rank = delta_rank?;
    if old_rank <= 0 {
        return None;
    }
    Some(delta_rank.abs() as f64 / old_rank as f64 * 100.0)
}

fn compute_change_pct(current: Option<f64>, previous: Option<f64>) -> Option<f64> {
    let current = current?;
    let previous = previous?;
    if !current.is_finite() || !previous.is_finite() || previous <= 0.0 {
        return None;
    }
    Some((current - previous) / previous * 100.0)
}

fn compare_market_rank_latest(left: &MarketRankEventItem, right: &MarketRankEventItem) -> Ordering {
    right
        .detected_at
        .cmp(&left.detected_at)
        .then_with(|| right.id.cmp(&left.id))
}

fn compare_market_rank_by_rank_change_pct(
    left: &MarketRankEventItem,
    right: &MarketRankEventItem,
) -> Ordering {
    compare_optional_f64_desc(left.rank_change_pct, right.rank_change_pct)
        .then_with(|| {
            i32::abs(right.delta_rank.unwrap_or(0)).cmp(&i32::abs(left.delta_rank.unwrap_or(0)))
        })
        .then_with(|| compare_market_rank_latest(left, right))
}

fn compare_market_rank_by_volume_24h_change_pct(
    left: &MarketRankEventItem,
    right: &MarketRankEventItem,
) -> Ordering {
    compare_optional_f64_abs_desc(left.volume_24h_change_pct, right.volume_24h_change_pct)
        .then_with(|| compare_market_rank_latest(left, right))
}

fn compare_market_rank_by_volume_15m_change_pct(
    left: &MarketRankEventItem,
    right: &MarketRankEventItem,
) -> Ordering {
    compare_optional_f64_abs_desc(left.volume_15m_change_pct, right.volume_15m_change_pct)
        .then_with(|| compare_market_rank_latest(left, right))
}

fn compare_optional_f64_desc(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (finite_f64(left), finite_f64(right)) {
        (Some(left), Some(right)) => right.partial_cmp(&left).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn finite_f64(value: Option<f64>) -> Option<f64> {
    value.filter(|item| item.is_finite())
}

fn compare_optional_f64_abs_desc(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (finite_f64(left), finite_f64(right)) {
        (Some(left), Some(right)) => right
            .abs()
            .partial_cmp(&left.abs())
            .unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

async fn fetch_unified_market_klines(
    pool: &PgPool,
    query: &MarketKlineQuery,
) -> Result<Vec<MarketKlineItem>> {
    let result = sqlx::query_as::<_, MarketKlineItem>(
        r#"
        SELECT
            EXTRACT(EPOCH FROM open_time)::BIGINT AS time,
            open_price::FLOAT8 AS open,
            high_price::FLOAT8 AS high,
            low_price::FLOAT8 AS low,
            close_price::FLOAT8 AS close,
            COALESCE(volume, quote_volume, 0)::FLOAT8 AS volume,
            'UTC+8'::TEXT AS timezone
        FROM market_candles
        WHERE LOWER(exchange) = LOWER($1)
          AND UPPER(symbol) = UPPER($2)
          AND UPPER(timeframe) = UPPER($3)
          AND ($4::BIGINT IS NULL OR EXTRACT(EPOCH FROM open_time)::BIGINT < $4)
          AND ($5::BIGINT IS NULL OR EXTRACT(EPOCH FROM open_time)::BIGINT > $5)
        ORDER BY open_time DESC
        LIMIT $6
        "#,
    )
    .bind(&query.exchange)
    .bind(&query.symbol)
    .bind(&query.timeframe)
    .bind(query.before)
    .bind(query.after)
    .bind(query.limit)
    .fetch_all(pool)
    .await;

    match result {
        Ok(rows) => Ok(rows),
        Err(err) if is_undefined_table_error(&err) => Ok(Vec::new()),
        Err(err) => Err(err.into()),
    }
}

async fn fetch_legacy_market_klines(
    pool: &PgPool,
    query: &MarketKlineQuery,
) -> Result<Vec<MarketKlineItem>> {
    let table_name = legacy_kline_table_name(&query.symbol, &query.timeframe)?;
    let mut query_builder = QueryBuilder::<Postgres>::new(format!(
        r#"
        SELECT
            CASE WHEN ts > 10000000000 THEN ts / 1000 ELSE ts END AS time,
            NULLIF(o, '')::FLOAT8 AS open,
            NULLIF(h, '')::FLOAT8 AS high,
            NULLIF(l, '')::FLOAT8 AS low,
            NULLIF(c, '')::FLOAT8 AS close,
            COALESCE(NULLIF(vol, ''), NULLIF(vol_ccy, ''), '0')::FLOAT8 AS volume,
            'UTC+8'::TEXT AS timezone
        FROM {table_name}
        WHERE 1=1
        "#
    ));

    if let Some(before) = query.before {
        query_builder
            .push(" AND ts < ")
            .push_bind(seconds_to_legacy_millis(before));
    }
    if let Some(after) = query.after {
        query_builder
            .push(" AND ts > ")
            .push_bind(seconds_to_legacy_millis(after));
    }
    query_builder
        .push(" ORDER BY ts DESC LIMIT ")
        .push_bind(query.limit);

    let result = query_builder
        .build_query_as::<MarketKlineItem>()
        .fetch_all(pool)
        .await;

    match result {
        Ok(rows) => Ok(rows),
        Err(err) if is_undefined_table_error(&err) => Ok(Vec::new()),
        Err(err) => Err(err.into()),
    }
}

fn legacy_kline_table_name(symbol: &str, timeframe: &str) -> Result<String> {
    let symbol = normalize_legacy_symbol(symbol)?;
    let timeframe = normalize_legacy_timeframe(timeframe)?;
    Ok(format!("\"{}_candles_{}\"", symbol, timeframe))
}

fn normalize_legacy_symbol(raw: &str) -> Result<String> {
    let upper = raw.trim().to_ascii_uppercase();
    let normalized = if upper.contains('-') {
        upper
    } else if upper.len() > 4 && upper.ends_with("USDT") {
        format!("{}-USDT-SWAP", &upper[..upper.len() - 4])
    } else {
        anyhow::bail!("unsupported symbol for legacy kline table: {raw}");
    };

    let lower = normalized.to_ascii_lowercase();
    if lower
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_'))
    {
        Ok(lower)
    } else {
        anyhow::bail!("illegal legacy kline symbol: {raw}");
    }
}

fn normalize_legacy_timeframe(raw: &str) -> Result<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1m" => Ok("1m"),
        "3m" => Ok("3m"),
        "5m" => Ok("5m"),
        "15m" => Ok("15m"),
        "30m" => Ok("30m"),
        "1h" => Ok("1h"),
        "2h" => Ok("2h"),
        "4h" => Ok("4h"),
        "6h" => Ok("6h"),
        "12h" => Ok("12h"),
        "1d" | "1dutc" => Ok("1dutc"),
        "1w" => Ok("1w"),
        "1mn" | "1mnutc" => Ok("1m"),
        _ => anyhow::bail!("unsupported timeframe for legacy kline table: {raw}"),
    }
}

fn normalize_kline_sync_timeframe(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1m" => Ok("1M".to_string()),
        "3m" => Ok("3M".to_string()),
        "5m" => Ok("5M".to_string()),
        "15m" => Ok("15M".to_string()),
        "30m" => Ok("30M".to_string()),
        "1h" => Ok("1H".to_string()),
        "2h" => Ok("2H".to_string()),
        "4h" => Ok("4H".to_string()),
        "6h" => Ok("6H".to_string()),
        "12h" => Ok("12H".to_string()),
        "1d" | "1dutc" => Ok("1DUTC".to_string()),
        "1w" => Ok("1W".to_string()),
        other if other.is_empty() => Err("timeframe is required".to_string()),
        other => Err(format!("unsupported timeframe: {other}")),
    }
}

fn kline_sync_period_for_job(timeframe: &str) -> Result<String> {
    let period = match timeframe {
        "1M" => "1m",
        "3M" => "3m",
        "5M" => "5m",
        "15M" => "15m",
        "30M" => "30m",
        "1DUTC" => "1Dutc",
        value => value,
    };
    Ok(period.to_string())
}

fn seconds_to_legacy_millis(timestamp: i64) -> i64 {
    if timestamp > 10_000_000_000 {
        timestamp
    } else {
        timestamp.saturating_mul(1_000)
    }
}

fn normalize_market_rank_event_type(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase().replace('-', "_");
    match normalized.as_str() {
        "rank_velocity" | "rankvelocity" => Ok("rank_velocity".to_string()),
        "top_entry" | "topentry" => Ok("top_entry".to_string()),
        "top_exit" | "topexit" => Ok("top_exit".to_string()),
        other => Err(format!("unsupported eventType: {other}")),
    }
}

fn normalize_market_rank_sort(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase().replace(['-', '.'], "_");
    match normalized.as_str() {
        "" | "time" | "latest" | "detected_at" => Ok("detected_at".to_string()),
        "delta" | "delta_rank" | "rank_delta" | "rank_movement" | "volatility" => {
            Ok("delta_rank".to_string())
        }
        "volume_24h" | "volume24h" | "volume_24h_quote" => Ok("volume_24h".to_string()),
        "volume_15m" | "volume15m" | "volume_15m_quote" => Ok("volume_15m".to_string()),
        other => Err(format!("unsupported sort: {other}")),
    }
}

fn is_undefined_table_error(err: &sqlx::Error) -> bool {
    err.as_database_error()
        .and_then(|database_error| database_error.code())
        .is_some_and(|code| code == "42P01")
}
