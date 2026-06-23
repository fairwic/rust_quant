use anyhow::Result;
use chrono::Utc;
use okx::api::api_trait::OkxApiTrait;
use okx::api::market::OkxMarket;
use okx::config::Credentials;
use okx::dto::market_dto::TickerOkxResDto;
use okx::{Error as OkxError, OkxClient};
use rust_decimal::Decimal;
use rust_quant_domain::entities::TickerSnapshot;
use std::str::FromStr;
use tracing::{debug, error};
/// OKX市场扫描器
/// 负责轮询OKX的所有Ticker，用于发现异动
pub struct OkxScanner {
    /// 外部服务客户端。
    client: OkxMarket,
}
impl OkxScanner {
    /// 创建新的扫描器实例
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: okx_market_from_env_or_public()?,
        })
    }
    /// 获取全市场所有Ticker (包含SWAP和SPOT)
    pub async fn fetch_all_tickers(&self) -> Result<Vec<TickerSnapshot>> {
        let mut all_tickers = Vec::new();
        // 1. 获取永续合约 (SWAP)
        match self.client.get_tickers("SWAP").await {
            Ok(tickers) => {
                debug!("Fetched {} SWAP tickers", tickers.len());
                for t in tickers {
                    if let Ok(snapshot) = self.map_to_snapshot(t) {
                        all_tickers.push(snapshot);
                    }
                }
            }
            Err(e) => error!("Failed to fetch SWAP tickers: {:?}", e),
        }
        // 2. 获取现货 (SPOT)
        // match self.sdk.market(ExchangeId::Okx)?.tickers("SPOT").await {
        //     Ok(tickers) => {
        //         debug!("Fetched {} SPOT tickers", tickers.len());
        //         for t in tickers {
        //             if let Ok(snapshot) = self.map_to_snapshot(t) {
        //                 all_tickers.push(snapshot);
        //             }
        //         }
        //     }
        //     Err(e) => error!("Failed to fetch SPOT tickers: {:?}", e),
        // }
        Ok(all_tickers)
    }
    /// 将OKX Ticker转换为领域实体
    fn map_to_snapshot(&self, t: TickerOkxResDto) -> Result<TickerSnapshot> {
        let symbol = t.inst_id;
        let price = Decimal::from_str(&t.last).unwrap_or(Decimal::ZERO);
        // OKX API defines:
        // SPOT: vol24h = Base Vol, volCcy24h = Quote Vol
        // SWAP/FUTURES: vol24h = Contract Vol, volCcy24h = Base Vol (Underlying)
        let vol_ccy = Decimal::from_str(&t.vol_ccy24h).unwrap_or(Decimal::ZERO);
        let vol_raw = Decimal::from_str(&t.vol24h).unwrap_or(Decimal::ZERO);
        let (volume_24h_base, volume_24h_quote) = match t.inst_type.as_str() {
            "SPOT" => (vol_raw, vol_ccy),
            "SWAP" | "FUTURES" => {
                let base = vol_ccy;
                (base, base * price)
            }
            _ => {
                // Default fallback
                (vol_raw, vol_ccy)
            }
        };
        let ts_millis = t.ts.parse::<i64>().unwrap_or(0);
        let timestamp = DateTime::from_timestamp_millis(ts_millis).unwrap_or(Utc::now());
        Ok(TickerSnapshot {
            symbol,
            price,
            volume_24h_base,
            volume_24h_quote,
            timestamp,
        })
    }
}
/// 封装当前函数，减少行情数据调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
fn okx_market_from_env_or_public() -> Result<OkxMarket> {
    match OkxMarket::from_env() {
        Ok(client) => Ok(client),
        Err(error) if is_missing_okx_market_credentials(&error) => {
            debug!("OKX private credentials are not configured; using public market client");
            let client = OkxClient::new(Credentials::new(
                "public-market-api-key-not-used",
                "public-market-api-secret-not-used",
                "public-market-passphrase-not-used",
                "0",
            ))?;
            Ok(OkxMarket::new(client))
        }
        Err(error) => Err(error.into()),
    }
}
/// 判断 行情与市场数据 条件是否满足，给上层流程提供布尔决策。
fn is_missing_okx_market_credentials(error: &OkxError) -> bool {
    match error {
        OkxError::ConfigError(message) => [
            "OKX_API_KEY",
            "OKX_API_SECRET",
            "OKX_PASSPHRASE",
            "OKX_SIMULATED_TRADING",
        ]
        .iter()
        .any(|key| message.contains(key)),
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    const OKX_ENV_KEYS: &[&str] = &[
        "OKX_API_KEY",
        "OKX_API_SECRET",
        "OKX_PASSPHRASE",
        "OKX_SIMULATED_TRADING",
    ];
    /// 封装环境变量lock，减少行情数据调用方重复实现相同细节。
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }
    /// 提供快照环境变量的集中实现，避免行情数据调用方重复处理相同细节。
    fn snapshot_env() -> Vec<(&'static str, Option<String>)> {
        OKX_ENV_KEYS
            .iter()
            .map(|key| (*key, std::env::var(key).ok()))
            .collect()
    }
    /// 提供restore环境变量的集中实现，避免行情数据调用方重复处理相同细节。
    fn restore_env(snapshot: Vec<(&'static str, Option<String>)>) {
        for (key, value) in snapshot {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
    #[test]
    fn scanner_uses_public_market_client_without_private_okx_credentials() {
        let _guard = env_lock();
        let snapshot = snapshot_env();
        for key in OKX_ENV_KEYS {
            std::env::remove_var(key);
        }
        let scanner = OkxScanner::new();
        restore_env(snapshot);
        assert!(scanner.is_ok());
    }
}
use chrono::DateTime;
