use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::trading::okx::{okx_client};
use crate::trading::okx::okx_client::OkxApiResponse;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VolumeData {
    pub(crate) ts: String,
    pub(crate) oi: String,  //持仓总量（USD
    pub(crate) vol: String, //交易总量（USD）
}

pub type VolumeDataResponse = OkxApiResponse<Vec<VolumeData>>;

pub struct Contracts {}

impl Contracts {
    pub fn new(&self) -> &Contracts {
        self
    }
    //获取未平仓合约的持仓量和交易总量
    pub async fn get_open_interest_volume(
        ccy: Option<&str>,
        begin: Option<i64>,
        end: Option<i64>,
        period: Option<&str>,
    ) -> anyhow::Result<Vec<VolumeData>, anyhow::Error> {
        let mut path = "/api/v5/rubik/stat/contracts/open-interest-volume?".to_string();
        if let Some(ccy) = ccy {
            path.push_str(&format!("&ccy={}", ccy));
        }

        if let Some(begin) = begin {
            path.push_str(&format!("&begin={}", begin));
        }
        if let Some(end) = end {
            path.push_str(&format!("&end={}", end));
        }

        if let Some(period) = period {
            path.push_str(&format!("&period={}", period));
        }

        let res: anyhow::Result<VolumeDataResponse> = okx_client::get_okx_client()
            .send_request(Method::GET, &path, "")
            .await;

        match res {
            Ok(res) => {
                Ok(res.data)
            }
            Err(err) => {
                return Err(err);
            }
        }
    }
}
