use anyhow::Result;
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::trading::okx::{okx_client, OkxApiResponse};

mod error;
pub mod contracts;

#[derive(Serialize, Deserialize, Debug)]
pub struct Balance {
    ccy: String,
    bal: String,
    // 其他字段...
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CandleData {
    pub(crate) ts: String,
    pub(crate) o: String,
    pub(crate) h: String,
    pub(crate) l: String,
    pub(crate) c: String,
    pub(crate) vol: String,
    pub(crate) vol_ccy: String,
    pub(crate) vol_ccy_quote: String,
    pub(crate) confirm: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Ts {
    ts: String,
}

// 使用类型别名来定义特定的响应类型
pub type CandleResponse = OkxApiResponse<Vec<CandleData>>;
pub type TimeResponse = OkxApiResponse<Vec<Ts>>;

pub struct OkxPublicData {}

impl OkxPublicData {
    pub fn new(&self) -> &OkxPublicData {
        self
    }
    pub async fn get_time() -> Result<String, anyhow::Error> {
        let path = "/api/v5/public/time";
        let res: Result<TimeResponse> = okx_client::get_okx_client().send_request(Method::GET, &path, "").await;

        match res {
            Ok(res) => {
                let res = res.data;
                let res = res.get(0);
                if res.is_none() {
                    return Ok("".to_string());
                } else {
                    return Ok(res.unwrap().ts.to_string());
                }
            }
            Err(err) => {
                return Err(err);
            }
        }
    }

    /**
    获取交易产品基础信息
    获取所有可交易产品的信息列表。
    inst_type String 是 产品类型
    SPOT：币币
    MARGIN：币币杠杆
    SWAP：永续合约
    FUTURES：交割合约
    OPTION：期权
    uly String 可选 标的指数，仅适用于交割/永续/期权，期权必填
    inst_family	Stringl 交易品种，仅适用于交割/永续/期权
    inst_id	String 否 产品ID
     **/
    pub async fn get_instruments(
        inst_type: &str,
        uly: Option<&str>,
        inst_family: Option<&str>,
        inst_id: Option<&str>,
    ) -> Result<CandleResponse> {
        let mut path = format!("/api/v5/account/instruments?instType={}", inst_type);

        if let Some(uly) = uly {
            path.push_str(&format!("&uly={}", uly));
        }

        if let Some(instFamily) = inst_family {
            path.push_str(&format!("&intFamily={}", instFamily));
        }

        if let Some(instId) = inst_id {
            path.push_str(&format!("&instId={}", instId));
        }
        okx_client::get_okx_client().send_request(Method::GET, &path, "").await
    }
}

