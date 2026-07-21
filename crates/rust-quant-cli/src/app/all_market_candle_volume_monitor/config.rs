use anyhow::{bail, Context, Result};
use okx::config::CONFIG;
use rust_decimal::Decimal;
use std::collections::HashMap;

const DEFAULT_WS_SHARD_SIZE: usize = 150;
const DEFAULT_QUEUE_CAPACITY: usize = 4_096;
const DEFAULT_VOLUME_RATIO: &str = "2.0";
const DEFAULT_REST_REQUEST_SLEEP_MS: u64 = 120;

/// 全市场收盘 K 线成交量监听器的运行参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllMarketCandleVolumeMonitorConfig {
    /// quant_core PostgreSQL 连接串。
    pub database_url: String,
    /// 单个 OKX WebSocket 分片承载的交易对数量。
    pub websocket_shard_size: usize,
    /// WebSocket 与单任务聚合器之间的有界队列容量。
    pub confirmed_queue_capacity: usize,
    /// 触发结构化放量观测日志的最小倍数。
    pub minimum_volume_ratio: Decimal,
    /// OKX REST 根地址，只用于启动预热缺失数据和运行中补洞。
    pub okx_rest_base: String,
    /// 可选 HTTP 代理。
    pub proxy_url: Option<String>,
    /// REST 预热请求之间的限速间隔。
    pub rest_request_sleep_ms: u64,
    /// 仅用于受控 smoke 的交易对上限；生产默认不限制。
    pub max_symbols: Option<usize>,
}

impl AllMarketCandleVolumeMonitorConfig {
    /// 从环境变量加载配置，并拒绝使用可能指向 quant_web 的通用数据库变量。
    pub fn from_env() -> Result<Self> {
        let envs = std::env::vars().collect::<HashMap<_, _>>();
        Self::from_map(&envs)
    }

    /// 从显式键值集合解析并校验生产边界，测试复用此入口而不污染进程环境。
    fn from_map(envs: &HashMap<String, String>) -> Result<Self> {
        let database_url = non_empty(envs, "QUANT_CORE_DATABASE_URL")
            .or_else(|| non_empty(envs, "POSTGRES_QUANT_CORE_DATABASE_URL"))
            .context(
                "QUANT_CORE_DATABASE_URL or POSTGRES_QUANT_CORE_DATABASE_URL must be set for the all-market candle monitor",
            )?
            .to_string();
        let websocket_shard_size = parse_or_default(
            envs,
            "ALL_MARKET_CANDLE_WS_SHARD_SIZE",
            DEFAULT_WS_SHARD_SIZE,
        )?;
        if !(1..=200).contains(&websocket_shard_size) {
            bail!("ALL_MARKET_CANDLE_WS_SHARD_SIZE must be between 1 and 200");
        }
        let confirmed_queue_capacity = parse_or_default(
            envs,
            "ALL_MARKET_CANDLE_QUEUE_CAPACITY",
            DEFAULT_QUEUE_CAPACITY,
        )?;
        if confirmed_queue_capacity == 0 {
            bail!("ALL_MARKET_CANDLE_QUEUE_CAPACITY must be positive");
        }
        let minimum_volume_ratio = non_empty(envs, "ALL_MARKET_CANDLE_MIN_VOLUME_RATIO")
            .unwrap_or(DEFAULT_VOLUME_RATIO)
            .parse::<Decimal>()
            .context("ALL_MARKET_CANDLE_MIN_VOLUME_RATIO must be a decimal")?;
        if minimum_volume_ratio <= Decimal::ZERO {
            bail!("ALL_MARKET_CANDLE_MIN_VOLUME_RATIO must be positive");
        }
        let okx_rest_base = non_empty(envs, "OKX_API_URL")
            .unwrap_or(CONFIG.api_url.as_str())
            .trim_end_matches('/')
            .to_string();
        let proxy_url = non_empty(envs, "OKX_PROXY_URL").map(ToOwned::to_owned);
        let rest_request_sleep_ms = parse_or_default(
            envs,
            "ALL_MARKET_CANDLE_REST_REQUEST_SLEEP_MS",
            DEFAULT_REST_REQUEST_SLEEP_MS,
        )?;
        let max_symbols = non_empty(envs, "ALL_MARKET_CANDLE_MAX_SYMBOLS")
            .map(str::parse::<usize>)
            .transpose()
            .context("ALL_MARKET_CANDLE_MAX_SYMBOLS must be a positive integer")?;
        if max_symbols == Some(0) {
            bail!("ALL_MARKET_CANDLE_MAX_SYMBOLS must be positive");
        }

        Ok(Self {
            database_url,
            websocket_shard_size,
            confirmed_queue_capacity,
            minimum_volume_ratio,
            okx_rest_base,
            proxy_url,
            rest_request_sleep_ms,
            max_symbols,
        })
    }
}

/// 把缺失、空字符串和纯空白统一视为未配置。
fn non_empty<'a>(envs: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    envs.get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// 解析可选环境变量；未设置时使用经过代码审计的默认值。
fn parse_or_default<T>(envs: &HashMap<String, String>, key: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    non_empty(envs, key)
        .map(str::parse::<T>)
        .transpose()
        .with_context(|| format!("{key} has an invalid value"))
        .map(|value| value.unwrap_or(default))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_generic_database_url_and_oversized_websocket_shard() {
        let only_generic = HashMap::from([(
            "DATABASE_URL".to_string(),
            "postgres://localhost/quant_web".to_string(),
        )]);
        assert!(AllMarketCandleVolumeMonitorConfig::from_map(&only_generic).is_err());

        let invalid_shard = HashMap::from([
            (
                "QUANT_CORE_DATABASE_URL".to_string(),
                "postgres://localhost/quant_core".to_string(),
            ),
            (
                "ALL_MARKET_CANDLE_WS_SHARD_SIZE".to_string(),
                "201".to_string(),
            ),
        ]);
        assert!(AllMarketCandleVolumeMonitorConfig::from_map(&invalid_shard).is_err());
    }

    #[test]
    fn defaults_to_bounded_three_shard_friendly_configuration() {
        let envs = HashMap::from([(
            "QUANT_CORE_DATABASE_URL".to_string(),
            "postgres://localhost/quant_core".to_string(),
        )]);
        let config = AllMarketCandleVolumeMonitorConfig::from_map(&envs).expect("valid defaults");
        assert_eq!(config.websocket_shard_size, 150);
        assert_eq!(config.confirmed_queue_capacity, 4_096);
        assert_eq!(config.minimum_volume_ratio, Decimal::from(2));
    }
}
