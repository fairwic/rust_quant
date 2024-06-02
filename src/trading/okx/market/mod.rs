use anyhow::Result;
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
    inst_type	String	是	产品类型
    SPOT：币币
    SWAP：永续合约
    FUTURES：交割合约
    OPTION：期权
    uly	String	否	标的指数
    适用于交割/永续/期权，如 BTC-USD
    instFamily	String	否	交易品种
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
    inst_type	String	是	产品类型
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


    // 获取交易产品最近的K线数据
    // 获取K线数据。K线数据按请求的粒度分组返回，K线数据每个粒度最多可获取最近1,440条。
    //限速：40次/2s,限速规则ip
    // instId	String	是	产品ID，如 BTC-USDT
    // bar	String	否	时间粒度，默认值1m
    // 如 [1m/3m/5m/15m/30m/1H/2H/4H]
    // 香港时间开盘价k线：[6H/12H/1D/2D/3D/1W/1M/3M]
    // UTC时间开盘价k线：[/6Hutc/12Hutc/1Dutc/2Dutc/3Dutc/1Wutc/1Mutc/3Mutc]
    // after	String	否	请求此时间戳之前（更旧的数据）的分页内容，传的值为对应接口的ts
    // before	String	否	请求此时间戳之后（更新的数据）的分页内容，传的值为对应接口的ts, 单独使用时，会返回最新的数据。
    // limit	String	否	分页返回的结果集数量，最大为300，不填默认返回100条

    pub async fn get_candles(
        &self,
        inst_id: &str,
        bar: &str,
        after: Option<&str>,
        before: Option<&str>,
        limit: Option<&str>,
    ) -> Result<CandleResponse> {
        let mut path = format!("/api/v5/market/candles?instId={}", inst_id);

        if !bar.is_empty() {
            path.push_str(&format!("&bar={}", bar));
        }

        if let Some(after_ts) = after {
            path.push_str(&format!("&after={}", after_ts));
        }

        if let Some(before_ts) = before {
            path.push_str(&format!("&before={}", before_ts));
        }

        if let Some(limit_val) = limit {
            path.push_str(&format!("&limit={}", limit_val));
        }

        okx_client::get_okx_client().send_request(Method::GET, &path, "").await
    }
    pub async fn get_history_candles(
        &self,
        inst_id: &str,
        bar: &str,
        after: Option<&str>,
        before: Option<&str>,
        limit: Option<&str>,
    ) -> Result<CandleResponse> {
        let mut path = format!("/api/v5/market/candles?instId={}", inst_id);

        if !bar.is_empty() {
            path.push_str(&format!("&bar={}", bar));
        }

        if let Some(after_ts) = after {
            path.push_str(&format!("&after={}", after_ts));
        }

        if let Some(before_ts) = before {
            path.push_str(&format!("&before={}", before_ts));
        }

        if let Some(limit_val) = limit {
            path.push_str(&format!("&limit={}", limit_val));
        }

        okx_client::get_okx_client().send_request(Method::GET, &path, "").await
    }
}

