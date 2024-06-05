use anyhow::Result;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use crate::trading::model::market::candles::CandlesEntity;
use crate::trading::okx::{okx_client, OkxApiResponse};
use crate::trading::okx::market::Market;
use crate::trading::okx::public_data::CandleData;


#[derive(Debug, Serialize, Deserialize)]
pub struct RequestParams {
    pub inst_id: String, // 产品ID，如 BTC-USDT
    pub bar: Option<String>, // 时间粒度，默认值1m
    pub after: Option<String>, // 请求此时间戳之前（更旧的数据）的分页内容
    pub before: Option<String>, // 请求此时间戳之后（更新的数据）的分页内容
    pub limit: Option<String>, // 分页返回的结果集数量，最大为300，不填默认返回100条
}

// 使用类型别名来定义特定的响应类型
pub type CandleResponse = OkxApiResponse<Vec<CandlesEntity>>;

impl Market {
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
        inst_id: &str,
        bar: &str,
        after: Option<&str>,
        before: Option<&str>,
        limit: Option<&str>,
    ) -> Result<Vec<CandlesEntity>> {
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
        println!("path:{}", path);

        let res: CandleResponse = okx_client::get_okx_client().send_request(Method::GET, &path, "").await?;

        Ok(res.data)
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

