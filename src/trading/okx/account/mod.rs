/*获取交易账户余额*/
use reqwest::Method;
use serde::{Deserialize, Serialize};
use crate::trading::okx::okx_client;
use anyhow::{Result, Error, anyhow};

#[derive(Serialize, Deserialize, Debug)]
pub struct Balance {
    ccy: String,
    bal: String,
    // 其他字段...
}

#[derive(Serialize, Deserialize, Debug)]
struct CandleData {
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
pub struct CandleResponse {
    code: String,
    msg: String,
    data: Vec<CandleData>,
}

pub(crate) struct Account {}

impl Account {
    pub fn new() -> Self {
        Account {}
    }
    pub async fn get_balances(ccy: &[String]) -> anyhow::Result<CandleResponse> {
        let ccy_param = ccy.join(",");
        let path = format!("/api/v5/account/balance?ccy={}", ccy_param);
        okx_client::get_okx_client().send_request(Method::GET, &path, "").await
    }

    /**
    获取该账户下拥有实际持仓的信息。账户为买卖模式会显示净持仓（net），账户为开平仓模式下会分别返回开多（long）或开空（short）的仓位。按照仓位创建时间倒序排列。
    instType	String	否	产品类型
    MARGIN：币币杠杆
    SWAP：永续合约
    FUTURES：交割合约
    OPTION：期权
    instType和instId同时传入的时候会校验instId与instType是否一致。
    instId	String	否	交易产品ID，如：BTC-USDT-SWAP
    支持多个instId查询（不超过10个），半角逗号分隔
    posId	String	否	持仓ID
    支持多个posId查询（不超过20个）。
    存在有效期的属性，自最近一次完全平仓算起，满30天 posId 以及整个仓位会被清除。**/
    pub async fn get_account_positions(
        &self,
        inst_type: Option<&str>,
        inst_id: Option<&str>,
        post_id: Option<&str>,
    ) -> Result<CandleResponse> {
        let mut path = format!("/api/v5/account/positions?test=1");
        if let Some(instType) = inst_type {
            path.push_str(&format!("&instId={}", instType));
        }
        if let Some(instId) = inst_id {
            path.push_str(&format!("&instId={}", instId));
        }

        if let Some(postId) = post_id {
            path.push_str(&format!("&postId={}", postId));
        }
        okx_client::get_okx_client().send_request(Method::GET, &path, "").await
    }
}


