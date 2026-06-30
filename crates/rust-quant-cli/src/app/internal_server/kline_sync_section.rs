/// 封装当前函数，减少行情数据调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
async fn sync_kline_request(request: &KlineSyncRequest) -> Result<i64> {
    let service = create_kline_sync_candle_service()?;
    let period = kline_sync_period_for_job(&request.timeframe)?;
    let timeframe = Timeframe::from_str(&period)
        .map_err(|error| anyhow::anyhow!("无效的K线周期: {}", error))?;
    let latest_candle = service
        .get_latest_candle(&request.symbol, timeframe)
        .await?;
    let after = latest_candle.and_then(|candle| {
        candle
            .timestamp
            .checked_add(1)
            .and_then(|timestamp| u64::try_from(timestamp).ok())
    });
    let candles = service
        .fetch_candles_from_crypto_exc_all(
            &request.exchange,
            &request.symbol,
            &period,
            after,
            None,
            request.limit as u32,
        )
        .await?;
    if candles.is_empty() {
        return Ok(0);
    }
    Ok(service.save_candles(candles).await? as i64)
}
/// 创建 行情与市场数据 资源，并在入口处完成必要的参数归一。
fn create_kline_sync_candle_service() -> Result<CandleService> {
    should_use_quant_core_candle_source()?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .context("CANDLE_SOURCE=quant_core 时必须设置 QUANT_CORE_DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_lazy(&database_url)?;
    let repository = PostgresCandleRepository::new(pool);
    Ok(CandleService::new(Box::new(repository)))
}
