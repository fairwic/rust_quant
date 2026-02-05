use anyhow::anyhow;
use okx::api::api_trait::OkxApiTrait;
use okx::config::Credentials;
use okx::OkxClient;
use okx::OkxTrade;
use reqwest::Method;
use rust_quant_domain::entities::ExchangeApiConfig;
use serde_json::json;
use tracing::{info, warn};

/// OKX：把止损移动到开仓价（保本）需要的最小能力抽象
#[async_trait::async_trait]
pub trait StopLossAmender: Send + Sync {
    async fn move_stop_loss_to_price(
        &self,
        inst_id: &str,
        ord_id: &str,
        new_sl_trigger_px: f64,
    ) -> anyhow::Result<()>;
}

/// 使用 okx crate 的 `OkxClient::send_request` 直接调用 OKX `/api/v5/trade/amend-order`
///
/// 备注：
/// - okx crate 当前提供的 `OkxTrade::amend_order` 不包含 `attachAlgoOrds`，无法修改附带止盈止损。
/// - 这里通过查询订单详情获取 `attachAlgoId`，再用自定义请求修改 stop loss 触发价。
pub struct OkxStopLossAmender {
    trade: OkxTrade,
}

impl OkxStopLossAmender {
    pub fn from_env() -> anyhow::Result<Self> {
        let is_prod = std::env::var("APP_ENV")
            .unwrap_or_else(|_| "local".to_string())
            .eq_ignore_ascii_case("prod");
        let mut client = if is_prod {
            OkxClient::from_env()
        } else {
            OkxClient::from_env_with_simulated_trading()
        }
        .map_err(|e| anyhow!("创建OKX客户端失败: {}", e))?;

        Self::apply_request_expiration_override(&mut client);
        let trade = OkxTrade::new(client);
        Ok(Self { trade })
    }

    pub fn from_exchange_api_config(config: &ExchangeApiConfig) -> anyhow::Result<Self> {
        if config.exchange_name.to_lowercase() != "okx" {
            return Err(anyhow!("不支持的交易所: {}", config.exchange_name));
        }

        let passphrase = config
            .passphrase
            .as_ref()
            .ok_or_else(|| anyhow!("OKX需要Passphrase"))?;

        let credentials = Credentials::new(
            &config.api_key,
            &config.api_secret,
            passphrase,
            if config.is_sandbox { "1" } else { "0" },
        );

        let mut client =
            OkxClient::new(credentials).map_err(|e| anyhow!("创建OKX客户端失败: {}", e))?;
        Self::apply_request_expiration_override(&mut client);

        Ok(Self {
            trade: OkxTrade::new(client),
        })
    }

    fn apply_request_expiration_override(client: &mut OkxClient) {
        if let Ok(expiration_ms) = std::env::var("OKX_REQUEST_EXPIRATION_MS") {
            if let Ok(expiration_ms) = expiration_ms.parse::<i64>() {
                if expiration_ms > 0 {
                    client.set_request_expiration(expiration_ms);
                }
            }
        }
    }

    async fn fetch_first_attach_algo_id(
        &self,
        inst_id: &str,
        ord_id: &str,
    ) -> anyhow::Result<String> {
        let details = self
            .trade
            .get_order_details(inst_id, Some(ord_id), None)
            .await
            .map_err(|e| {
                anyhow!(
                    "获取订单详情失败: inst_id={}, ord_id={}, err={}",
                    inst_id,
                    ord_id,
                    e
                )
            })?;
        println!("details: {:#?}", details);
        let first = details
            .first()
            .ok_or_else(|| anyhow!("订单详情为空: inst_id={}, ord_id={}", inst_id, ord_id))?;

        // 优先取包含止损信息的那条 attachAlgo
        for a in first.attach_algo_ords.iter() {
            let has_sl = a
                .sl_trigger_px
                .as_ref()
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            if has_sl {
                return Ok(a.attach_algo_id.clone());
            }
        }

        // 兜底：取第一条
        first
            .attach_algo_ords
            .first()
            .map(|a| a.attach_algo_id.clone())
            .ok_or_else(|| {
                anyhow!(
                    "订单未包含 attach_algo_ords: inst_id={}, ord_id={}",
                    inst_id,
                    ord_id
                )
            })
    }
}

#[async_trait::async_trait]
impl StopLossAmender for OkxStopLossAmender {
    async fn move_stop_loss_to_price(
        &self,
        inst_id: &str,
        ord_id: &str,
        new_sl_trigger_px: f64,
    ) -> anyhow::Result<()> {
        let attach_algo_id = self.fetch_first_attach_algo_id(inst_id, ord_id).await?;
        println!("attach_algo_id: {}", attach_algo_id);
        // OKX: /api/v5/trade/amend-order
        // 当原始订单下单时带了 attachAlgoOrds，则改单也需要带 attachAlgoOrds（否则会报 51538）
        //
        // 这里仅修改 SL：把止损触发价移动到开仓价（保本）
        // - newSlTriggerPx / newSlOrdPx：参考 OKX v5 amend-order 参数命名
        // - newSlOrdPx = -1：市价止损
        let body = json!({
            "instId": inst_id,
            "ordId": ord_id,
            "attachAlgoOrds": [{
                "attachAlgoId": attach_algo_id,
                "newSlTriggerPx": format!("{:.8}", new_sl_trigger_px),
                "newSlOrdPx": "-1",
                "newSlTriggerPxType": "last"
            }]
        });

        let body_str =
            serde_json::to_string(&body).map_err(|e| anyhow!("序列化请求失败: {}", e))?;
        let path = "/api/v5/trade/amend-order";

        info!(
            "触发保本移动止损: inst_id={}, ord_id={}, attach_algo_id={}, new_sl={}",
            inst_id, ord_id, attach_algo_id, new_sl_trigger_px
        );

        let resp: serde_json::Value = self
            .trade
            .client()
            .send_request(Method::POST, path, &body_str)
            .await
            .map_err(|e| {
                anyhow!(
                    "OKX改单失败: inst_id={}, ord_id={}, err={}",
                    inst_id,
                    ord_id,
                    e
                )
            })?;
        println!("resp: {:#?}", resp);

        // resp 形态依赖 OKX 返回，这里只做日志记录
        if let Some(code) = resp
            .get(0)
            .and_then(|v| v.get("sCode"))
            .and_then(|v| v.as_str())
        {
            if code != "0" {
                warn!(
                    "OKX改单返回非0: inst_id={}, ord_id={}, resp={}",
                    inst_id, ord_id, resp
                );
            }
        }

        info!("OKX改单请求已提交: inst_id={}, ord_id={}", inst_id, ord_id);
        Ok(())
    }
}
