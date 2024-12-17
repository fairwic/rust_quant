mod candles;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use crate::trading::okx::{okx_client, OkxApiResponse};
use crate::trading::okx::public_data::CandleData;


#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TickersData {
    #[serde(rename = "instType")]
    pub inst_type: String,
    #[serde(rename = "instId")]
    pub inst_id: String,
    pub last: String,
    #[serde(rename = "lastSz")]
    pub last_sz: String,
    #[serde(rename = "askPx")]
    pub ask_px: String,
    #[serde(rename = "askSz")]
    pub ask_sz: String,
    #[serde(rename = "bidPx")]
    pub bid_px: String,
    #[serde(rename = "bidSz")]
    pub bid_sz: String,
    pub open24h: String,
    pub high24h: String,
    pub low24h: String,
    #[serde(rename = "volCcy24h")]
    pub vol_ccy24h: String,
    pub vol24h: String,
    #[serde(rename = "sodUtc0")]
    pub sod_utc0: String,
    #[serde(rename = "sodUtc8")]
    pub sod_utc8: String,
    pub ts: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Ts {
    ts: String,
}


// 使用类型别名来定义特定的响应类型
pub type CandleResponse = OkxApiResponse<Vec<CandleData>>;
pub type TickersResponse = OkxApiResponse<Vec<TickersData>>;
pub type TimeResponse = OkxApiResponse<Vec<Ts>>;

pub(crate) struct Market {}

impl Market {
    pub fn new() -> Self {
        Market {}
    }
    /**
    获取所有产品行情信息
    inst_type String 是 产品类型
    SPOT：币币
    SWAP：永续合约
    FUTURES：交割合约
    OPTION：期权
    uly	String 否 标的指数
    适用于交割/永续/期权，如 BTC-USD
    instFamily String 否  易品种
    适用于交割/永续/期权，如 BTC-USD
    **/
    pub async fn get_tickers(inst_type: &str, uly: Option<String>, inst_family: Option<String>) -> anyhow::Result<Vec<TickersData>> {
        let mut path = format!("/api/v5/market/tickers?instType={}", inst_type);
        if let Some(uly) = uly {
            path.push_str(&format!("&inst_id={}", uly));
        }

        if let Some(inst_family) = inst_family {
            path.push_str(&format!("&inst_family={}", inst_family));
        }
        let res: TickersResponse = okx_client::get_okx_client().send_request(Method::GET, &path, "").await?;
        Ok(res.data)
    }

    /**
    获取单个产品行情信息
    inst_type String 是 产品类型
    SPOT：币币
    SWAP：永续合约
    FUTURES：交割合约
    OPTION：期权
    uly	String	否	标的指数
    适用于交割/永续/期权，如 BTC-USD
    instFamily	String	否	交易品种
    适用于交割/永续/期权，如 BTC-USD
    **/
    pub async fn get_ticker(inst_id: &str) -> anyhow::Result<Vec<TickersData>> {
        let path = format!("/api/v5/market/ticker?instId={}", inst_id);
        let res: TickersResponse = okx_client::get_okx_client().send_request(Method::GET, &path, "").await?;
        Ok(res.data)
    }
}

