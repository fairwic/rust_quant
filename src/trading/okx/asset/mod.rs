/*获取交易账户余额*/
use reqwest::Method;
use serde::{Deserialize, Serialize};
use tracing::debug;
use crate::trading::okx::{okx_client, OkxApiResponse};

#[derive(Serialize, Deserialize, Debug)]
pub struct AssetData {
    // 币种，如 BTC
    pub(crate) ccy: String,
    // 余额
    pub(crate) bal: String,
    // 冻结余额
    pub(crate) frozen_bal: String,
    // 可用余额
    pub(crate) avail_bal: String,
}

pub(crate) struct Asset {}

impl Asset {
    pub fn new() -> Self {
        Asset {}
    }
    pub async fn get_balances(ccy: &Vec<String>) -> anyhow::Result<OkxApiResponse<AssetData>> {
        // 币种，如 BTC
        // 支持多币种查询（不超过20个），币种之间半角逗号分隔
        let mut path = "/api/v5/asset/balances".to_string();
        let ccy_param = ccy.join(",");
        debug!("ccy_param:{:#?}", ccy_param);
        if !ccy_param.is_empty() {
            path.push_str(&format!("&ccy={}", ccy_param));
        }
        debug!("path:{:#?}", path);
        okx_client::get_okx_client().send_request(Method::GET, &path, "").await
    }
}

