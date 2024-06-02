use reqwest::Method;
use serde::{Deserialize, Serialize};
use crate::trading::okx::{okx_client, OkxApiResponse};

use anyhow::{Result, Error, anyhow};

#[derive(Serialize, Deserialize, Debug)]
pub struct Balance {
    ccy: String,
    bal: String,
    // 其他字段...
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CandleData {
    ts: String,
    o: String,
    h: String,
    l: String,
    c: String,
    vol: String,
    vol_ccy: String,
    vol_ccy_quote: String,
    confirm: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Ts {
    ts: String,
}

// 使用类型别名来定义特定的响应类型
pub type CandleResponse = OkxApiResponse<Vec<CandleData>>;
pub type TimeResponse = OkxApiResponse<Vec<Ts>>;

pub(crate) struct Trade {}

impl Trade {
    pub fn new(&self) -> &Trade {
        self
    }
    pub async fn order() -> Result<String, anyhow::Error> {
        let path = "/api/v5/trade/order";
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

}

