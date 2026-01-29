use anyhow::Result;
use chrono::Utc;
use okx::api::api_trait::OkxApiTrait;
use okx::api::market::OkxMarket;
use okx::dto::market_dto::TickerOkxResDto;

use rust_decimal::Decimal;
use rust_quant_domain::entities::{FundFlow, FundFlowSide, TickerSnapshot};
use std::str::FromStr;
use tracing::{debug, error};

/// OKX市场扫描器
/// 负责轮询OKX的所有Ticker，用于发现异动
pub struct OkxScanner {
    client: OkxMarket,
}

impl OkxScanner {
    /// 创建新的扫描器实例
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: OkxMarket::from_env()?,
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
        // match self.client.get_tickers("SPOT").await {
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

        let mut volume_24h_base = Decimal::ZERO;
        let mut volume_24h_quote = Decimal::ZERO;

        let vol_ccy = Decimal::from_str(&t.vol_ccy24h).unwrap_or(Decimal::ZERO);
        let vol_raw = Decimal::from_str(&t.vol24h).unwrap_or(Decimal::ZERO);

        match t.inst_type.as_str() {
            "SPOT" => {
                volume_24h_base = vol_raw; // Base Volume (e.g. BTC)
                volume_24h_quote = vol_ccy; // Quote Volume (e.g. USDT)
            }
            "SWAP" | "FUTURES" => {
                volume_24h_base = vol_ccy; // Base Volume (e.g. BTC)
                                           // Need to calculate Quote Volume manually: Base Vol * Price
                volume_24h_quote = volume_24h_base * price;
            }
            _ => {
                // Default fallback
                volume_24h_base = vol_raw;
                volume_24h_quote = vol_ccy;
            }
        }

        let ts_str = t.ts;
        let ts_millis = ts_str.parse::<i64>().unwrap_or(0);
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

use chrono::DateTime;
use chrono::TimeZone;
