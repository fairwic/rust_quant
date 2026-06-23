use anyhow::anyhow;
use okx::api::api_trait::OkxApiTrait;
use okx::config::Credentials;
use okx::OkxClient;
use okx::OkxTrade;
use reqwest::Method;
use rust_quant_domain::entities::ExchangeApiConfig;
use serde_json::json;
use tracing::{info, warn};
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
    /// trade。
    trade: OkxTrade,
}
const LEGACY_DIRECT_LIVE_ORDER_CONFIRM_ENV: &str = "LEGACY_DIRECT_LIVE_ORDER_CONFIRM";
const LEGACY_DIRECT_LIVE_ORDER_CONFIRM_TOKEN: &str = "I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS";
impl OkxStopLossAmender {
    /// 提供ensurelegacydirectlive交易所订单allowed的集中实现，避免风控调用方重复处理相同细节。
    fn ensure_legacy_direct_live_exchange_order_allowed() -> anyhow::Result<()> {
        let confirmation = std::env::var(LEGACY_DIRECT_LIVE_ORDER_CONFIRM_ENV).ok();
        Self::ensure_legacy_direct_live_exchange_order_allowed_from_env(confirmation.as_deref())
    }
    /// 校验输入和运行前置条件，提前暴露 交易执行与风控 的不可执行原因。
    fn ensure_legacy_direct_live_exchange_order_allowed_from_env(
        confirmation: Option<&str>,
    ) -> anyhow::Result<()> {
        if confirmation.map(str::trim) == Some(LEGACY_DIRECT_LIVE_ORDER_CONFIRM_TOKEN) {
            return Ok(());
        }
        Err(anyhow!(
            "legacy direct live exchange mutation is blocked; route stop-loss amendments through the audited execution worker path or set {}={} to acknowledge unaudited legacy direct order/cancel/transfer risk",
            LEGACY_DIRECT_LIVE_ORDER_CONFIRM_ENV,
            LEGACY_DIRECT_LIVE_ORDER_CONFIRM_TOKEN
        ))
    }
    /// 从外部输入转换为内部模型，隔离 交易执行与风控 的字段适配细节。
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
    /// 从外部输入转换为内部模型，隔离 交易执行与风控 的字段适配细节。
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
    /// 执行 交易执行与风控 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    fn apply_request_expiration_override(client: &mut OkxClient) {
        if let Ok(expiration_ms) = std::env::var("OKX_REQUEST_EXPIRATION_MS") {
            if let Ok(expiration_ms) = expiration_ms.parse::<i64>() {
                if expiration_ms > 0 {
                    client.set_request_expiration(expiration_ms);
                }
            }
        }
    }
    /// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 封装当前函数，减少风控调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 采用 async 以支持数据库/网络 I/O 的并发调度，避免阻塞。
    async fn move_stop_loss_to_price(
        &self,
        inst_id: &str,
        ord_id: &str,
        new_sl_trigger_px: f64,
    ) -> anyhow::Result<()> {
        Self::ensure_legacy_direct_live_exchange_order_allowed()?;
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// 封装当前函数，减少风控调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    fn legacy_direct_stop_loss_amend_requires_explicit_confirmation() {
        let err =
            OkxStopLossAmender::ensure_legacy_direct_live_exchange_order_allowed_from_env(None)
                .expect_err("legacy direct OKX stop-loss amend should be blocked by default");
        let message = err.to_string();
        assert!(message.contains("LEGACY_DIRECT_LIVE_ORDER_CONFIRM"));
        assert!(message.contains("I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS"));
    }
    #[test]
    fn legacy_direct_stop_loss_amend_accepts_exact_confirmation() {
        OkxStopLossAmender::ensure_legacy_direct_live_exchange_order_allowed_from_env(Some(
            "I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS",
        ))
        .expect("exact legacy confirmation token should allow direct OKX stop-loss amend");
    }
}
