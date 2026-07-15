use anyhow::{anyhow, Context, Result};
use crypto_exc_all::{
    BinanceExchangeConfig, CryptoSdk, ExchangeId, FundingRate, FundingRateQuery,
    HyperliquidExchangeConfig, Instrument, SdkConfig,
};
use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_domain::traits::ExternalMarketSnapshotRepository;
use rust_quant_infrastructure::repositories::ShardedExternalMarketSnapshotRepository;
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

const METRIC_TYPE: &str = "funding_rate";
const PAGE_LIMIT: u32 = 1_000;
const SYMBOLS: [(&str, &str); 4] = [
    ("BTC-USDT-SWAP", "BTC"),
    ("ETH-USDT-SWAP", "ETH"),
    ("SOL-USDT-SWAP", "SOL"),
    ("BCH-USDT-SWAP", "BCH"),
];

/// 允许的公共资金费率来源；每个来源写入独立分表，禁止互相覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FundingSource {
    /// Binance USDⓈ-M 永续资金费率。
    Binance,
    /// Hyperliquid 永续资金费率。
    Hyperliquid,
}

impl FundingSource {
    /// 解析显式来源，拒绝无法审计的默认交易所。
    fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "binance" => Ok(Self::Binance),
            "hyperliquid" => Ok(Self::Hyperliquid),
            _ => anyhow::bail!("funding source must be binance or hyperliquid"),
        }
    }

    /// 返回数据库分表使用的稳定来源标识。
    fn as_str(self) -> &'static str {
        match self {
            Self::Binance => "binance",
            Self::Hyperliquid => "hyperliquid",
        }
    }

    /// 返回统一 SDK 使用的交易所枚举。
    fn exchange_id(self) -> ExchangeId {
        match self {
            Self::Binance => ExchangeId::Binance,
            Self::Hyperliquid => ExchangeId::Hyperliquid,
        }
    }
}

/// 全年资金费率回填窗口，使用 Unix 毫秒时间戳避免本地时区歧义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Args {
    /// 公共资金费率来源，必须显式指定。
    source: FundingSource,
    /// 回填窗口起点，包含该时间。
    start_ts: i64,
    /// 回填窗口终点，包含该时间。
    end_ts: i64,
}

/// 单市场资金费率覆盖摘要，用于回填后的审计核对。
#[derive(Debug, Serialize)]
struct SymbolBackfillSummary {
    /// Core 统一交易对标识。
    symbol: String,
    /// 本次网络响应中落入窗口的记录数；重复记录仍由数据库幂等更新。
    fetched_rows: usize,
    /// 回填后分表在窗口内的总记录数。
    stored_rows: usize,
    /// 窗口内最早资金费率结算时间，Unix 毫秒时间戳。
    first_ts: Option<i64>,
    /// 窗口内最晚资金费率结算时间，Unix 毫秒时间戳。
    last_ts: Option<i64>,
}

/// 公共 funding history 回填报告；非 OKX 来源只能作为代理压力证据。
#[derive(Debug, Serialize)]
struct BackfillReport {
    /// 数据来源，写入独立分表，禁止与 OKX 事实混写。
    source: String,
    /// 指标类型，固定为 funding_rate。
    metric_type: String,
    /// 请求窗口起点，Unix 毫秒时间戳。
    start_ts: i64,
    /// 请求窗口终点，Unix 毫秒时间戳。
    end_ts: i64,
    /// false 表示该数据不能替代 OKX 实际资金费率用于 Promote。
    okx_actual_cost_eligible: bool,
    /// 各市场覆盖摘要。
    symbols: Vec<SymbolBackfillSummary>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_args(std::env::args().skip(1))?;
    let database_url = quant_core_database_url()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core for funding backfill")?;
    let repository = ShardedExternalMarketSnapshotRepository::new(pool);
    let sdk = public_sdk(args.source)?;
    let market = sdk.market(args.source.exchange_id())?;
    let mut summaries = Vec::with_capacity(SYMBOLS.len());

    for (symbol, base) in SYMBOLS {
        summaries.push(
            backfill_symbol(
                args.source,
                &repository,
                &market,
                symbol,
                base,
                args.start_ts,
                args.end_ts,
            )
            .await?,
        );
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&BackfillReport {
            source: args.source.as_str().to_owned(),
            metric_type: METRIC_TYPE.to_owned(),
            start_ts: args.start_ts,
            end_ts: args.end_ts,
            okx_actual_cost_eligible: false,
            symbols: summaries,
        })?
    );
    Ok(())
}

/// 解析显式来源和时间窗口；拒绝默认值，避免不同执行日悄然生成不同数据。
fn parse_args<I, S>(args: I) -> Result<Args>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut source = None;
    let mut start_ts = None;
    let mut end_ts = None;
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        let (key, inline_value) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(key, value)| (key, Some(value)));
        match key {
            "--source" => {
                source = Some(FundingSource::parse(
                    &inline_value
                        .map(str::to_owned)
                        .or_else(|| args.next())
                        .context("--source requires binance or hyperliquid")?,
                )?);
            }
            "--start-ts" => {
                start_ts = Some(parse_timestamp_arg(
                    inline_value
                        .map(str::to_owned)
                        .or_else(|| args.next())
                        .context("--start-ts requires a Unix millisecond value")?,
                    "--start-ts",
                )?);
            }
            "--end-ts" => {
                end_ts = Some(parse_timestamp_arg(
                    inline_value
                        .map(str::to_owned)
                        .or_else(|| args.next())
                        .context("--end-ts requires a Unix millisecond value")?,
                    "--end-ts",
                )?);
            }
            "--help" | "-h" => {
                println!(
                    "Usage: pa_quant_tree_funding_backfill --source <binance|hyperliquid> --start-ts <unix-ms> --end-ts <unix-ms>"
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }
    let args = Args {
        source: source.context("missing --source")?,
        start_ts: start_ts.context("missing --start-ts")?,
        end_ts: end_ts.context("missing --end-ts")?,
    };
    anyhow::ensure!(
        args.start_ts > 0 && args.start_ts < args.end_ts,
        "funding backfill requires 0 < start_ts < end_ts"
    );
    Ok(args)
}

/// 解析 Unix 毫秒参数并拒绝负值或非整数输入。
fn parse_timestamp_arg(value: String, name: &str) -> Result<i64> {
    value
        .parse::<i64>()
        .with_context(|| format!("{name} must be a Unix millisecond integer"))
}

/// 只接受 Core 专用数据库变量，避免把交易事实写入 quant_web。
fn quant_core_database_url() -> Result<String> {
    std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .context("funding backfill requires QUANT_CORE_DATABASE_URL")
}

/// 构造指定交易所的公共市场客户端；本命令不调用签名账户或交易接口。
fn public_sdk(source: FundingSource) -> Result<CryptoSdk> {
    let config = match source {
        FundingSource::Binance => SdkConfig {
            binance: Some(BinanceExchangeConfig {
                api_key: "public".to_owned(),
                api_secret: "public".to_owned(),
                api_url: None,
                sapi_api_url: None,
                web_api_url: None,
                ws_stream_url: None,
                api_timeout_ms: Some(10_000),
                recv_window_ms: Some(5_000),
                proxy_url: std::env::var("BINANCE_PROXY_URL").ok(),
            }),
            ..SdkConfig::default()
        },
        FundingSource::Hyperliquid => SdkConfig {
            hyperliquid: Some(HyperliquidExchangeConfig {
                api_url: None,
                api_timeout_ms: Some(10_000),
                proxy_url: std::env::var("HYPERLIQUID_PROXY_URL").ok(),
                user_address: None,
            }),
            ..SdkConfig::default()
        },
    };
    CryptoSdk::from_config(config)
        .map_err(|error| anyhow!("create {} public sdk failed: {error}", source.as_str()))
}

/// 按时间正序分页回填单市场资金费率，并从分表重新读取覆盖证据。
async fn backfill_symbol(
    source: FundingSource,
    repository: &ShardedExternalMarketSnapshotRepository,
    market: &crypto_exc_all::MarketFacade<'_>,
    symbol: &str,
    base: &str,
    start_ts: i64,
    end_ts: i64,
) -> Result<SymbolBackfillSummary> {
    let instrument = Instrument::perp(base, "USDT");
    let mut cursor = start_ts;
    let mut fetched_rows = 0;
    while cursor <= end_ts {
        let items = market
            .funding_rate_history(
                FundingRateQuery::new(instrument.clone())
                    .with_start_time(cursor as u64)
                    // 交易所结算时间可能比整点晚几十毫秒，同小时桶仍属于研究终点。
                    .with_end_time(end_ts.saturating_add(999) as u64)
                    .with_limit(PAGE_LIMIT),
            )
            .await
            .map_err(|error| {
                anyhow!(
                    "fetch {} funding history failed: {symbol}: {error}",
                    source.as_str()
                )
            })?;
        if items.is_empty() {
            break;
        }
        let snapshots = items
            .iter()
            .filter_map(|item| funding_snapshot(source, symbol, item, start_ts, end_ts).transpose())
            .collect::<Result<Vec<_>>>()?;
        fetched_rows += snapshots.len();
        repository.save_batch(snapshots).await?;
        let max_ts = items.iter().filter_map(|item| item.funding_time).max();
        let Some(max_ts) = max_ts else {
            anyhow::bail!(
                "{} funding response has no funding_time: {symbol}",
                source.as_str()
            );
        };
        let next_cursor = i64::try_from(max_ts)
            .with_context(|| format!("{} funding_time exceeds i64", source.as_str()))?
            .saturating_add(1);
        anyhow::ensure!(
            next_cursor > cursor,
            "{} funding cursor did not advance",
            source.as_str()
        );
        cursor = next_cursor;
        // 不按请求 limit 判断末页：不同交易所会使用更小的服务端固定页大小。
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let stored = repository
        .find_range(
            source.as_str(),
            symbol,
            METRIC_TYPE,
            start_ts,
            end_ts.saturating_add(999),
            Some(10_000),
        )
        .await?;
    Ok(SymbolBackfillSummary {
        symbol: symbol.to_owned(),
        fetched_rows,
        stored_rows: stored.len(),
        first_ts: stored.first().map(|item| item.metric_time),
        last_ts: stored.last().map(|item| item.metric_time),
    })
}

/// 将统一 SDK 响应映射为分源 Core 快照；缺失时间或非法费率会中止回填。
fn funding_snapshot(
    source: FundingSource,
    symbol: &str,
    item: &FundingRate,
    start_ts: i64,
    end_ts: i64,
) -> Result<Option<ExternalMarketSnapshot>> {
    let metric_time = item
        .funding_time
        .with_context(|| format!("{} funding item is missing funding_time", source.as_str()))?;
    let metric_time = i64::try_from(metric_time)
        .with_context(|| format!("{} funding_time exceeds i64", source.as_str()))?;
    const HOUR_MS: i64 = 3_600_000;
    if metric_time.div_euclid(HOUR_MS) < start_ts.div_euclid(HOUR_MS)
        || metric_time.div_euclid(HOUR_MS) > end_ts.div_euclid(HOUR_MS)
    {
        return Ok(None);
    }
    let mut snapshot = ExternalMarketSnapshot::new(
        source.as_str().to_owned(),
        symbol.to_owned(),
        METRIC_TYPE.to_owned(),
        metric_time,
    );
    snapshot.funding_rate = Some(item.funding_rate.parse::<f64>().with_context(|| {
        format!(
            "invalid {} funding rate: {symbol} {metric_time}",
            source.as_str()
        )
    })?);
    snapshot.mark_price = item
        .mark_price
        .as_deref()
        .map(str::parse::<f64>)
        .transpose()
        .with_context(|| {
            format!(
                "invalid {} mark price: {symbol} {metric_time}",
                source.as_str()
            )
        })?;
    snapshot.raw_payload = Some(item.raw.clone());
    Ok(Some(snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn arguments_require_an_explicit_source_and_ordered_window() {
        assert_eq!(
            parse_args([
                "--source",
                "hyperliquid",
                "--start-ts",
                "1000",
                "--end-ts=2000"
            ])
            .unwrap(),
            Args {
                source: FundingSource::Hyperliquid,
                start_ts: 1_000,
                end_ts: 2_000,
            }
        );
        assert!(parse_args([
            "--source",
            "binance",
            "--start-ts",
            "2000",
            "--end-ts",
            "1000"
        ])
        .is_err());
        assert!(parse_args(["--source", "hyperliquid", "--start-ts", "1000"]).is_err());
    }

    #[test]
    fn snapshot_preserves_source_time_rate_and_raw_payload() {
        let item = FundingRate {
            exchange: ExchangeId::Binance,
            instrument: Instrument::perp("BTC", "USDT"),
            exchange_symbol: "BTCUSDT".to_owned(),
            funding_rate: "0.0001".to_owned(),
            funding_time: Some(1_500),
            next_funding_rate: None,
            next_funding_time: None,
            mark_price: Some("100000".to_owned()),
            raw: json!({"symbol": "BTCUSDT", "fundingTime": 1500}),
        };

        let snapshot =
            funding_snapshot(FundingSource::Binance, "BTC-USDT-SWAP", &item, 1_000, 2_000)
                .unwrap()
                .unwrap();
        assert_eq!(snapshot.source, "binance");
        assert_eq!(snapshot.symbol, "BTC-USDT-SWAP");
        assert_eq!(snapshot.metric_time, 1_500);
        assert_eq!(snapshot.funding_rate, Some(0.0001));
        assert_eq!(snapshot.mark_price, Some(100_000.0));
        assert_eq!(snapshot.raw_payload, Some(item.raw));
    }
}
