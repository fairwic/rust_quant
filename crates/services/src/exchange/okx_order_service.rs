//! OKX交易所订单执行服务

use anyhow::{anyhow, Result};
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::{Position, TradingSwapNumResponseData};
use okx::dto::asset::asset_dto::TransferOkxReqDto;
use okx::dto::common::EnumToStrTrait;
use okx::dto::common::Side;
use okx::dto::trade::trade_dto::{AttachAlgoOrdReqDto, OrderReqDto, OrderResDto, TdModeEnum};
use okx::dto::trade_dto::CloseOrderReqDto;
use okx::dto::trade_dto::OrdTypeEnum;
use okx::dto::PositionSide;
use okx::enums::account_enums::AccountType;
use okx::{OkxAccount, OkxAsset, OkxClient, OkxTrade};
use reqwest::Method;
use rust_quant_domain::entities::ExchangeApiConfig;
use serde::{Deserialize, Serialize};
use rust_quant_strategies::strategy_common::SignalResult;
use tracing::{error, info};

/// OKX订单执行服务
pub struct OkxOrderService;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutoCloseInspection {
    pub inst_id: String,
    pub ord_id: Option<String>,
    pub cl_ord_id: Option<String>,
    pub order_found: bool,
    pub order_state: Option<String>,
    pub order_source: Option<String>,
    pub pos_side: Option<String>,
    pub attach_algo_ids: Vec<String>,
    pub attach_algo_cl_ord_id: Option<String>,
    pub pending_algo_ids: Vec<String>,
    pub history_algo_ids: Vec<String>,
    pub has_open_position: bool,
    pub position_closed: bool,
    pub auto_close_likely: bool,
}

impl OkxOrderService {
    fn collect_related_algo_ids(
        order: &okx::dto::trade_dto::OrderDetailRespDto,
    ) -> (Vec<String>, Option<String>) {
        let mut ids = Vec::new();
        for algo in &order.attach_algo_ords {
            if !algo.attach_algo_id.trim().is_empty() {
                ids.push(algo.attach_algo_id.clone());
            }
        }
        ids.sort();
        ids.dedup();
        let attach_algo_cl_ord_id = if order.attach_algo_cl_ord_id.trim().is_empty() {
            None
        } else {
            Some(order.attach_algo_cl_ord_id.clone())
        };
        (ids, attach_algo_cl_ord_id)
    }

    fn extract_matching_algo_ids(
        raw: &serde_json::Value,
        target_algo_ids: &[String],
        target_algo_cl_ord_id: Option<&str>,
    ) -> Vec<String> {
        let mut result = Vec::new();
        let Some(items) = raw.get("data").and_then(|v| v.as_array()) else {
            return result;
        };

        for item in items {
            let algo_id = item
                .get("algoId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let algo_cl_ord_id = item
                .get("algoClOrdId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();

            let matched = (!algo_id.is_empty() && target_algo_ids.iter().any(|id| id == &algo_id))
                || (!algo_cl_ord_id.is_empty()
                    && target_algo_cl_ord_id
                        .map(|target| target == algo_cl_ord_id)
                        .unwrap_or(false));
            if matched && !algo_id.is_empty() {
                result.push(algo_id);
            }
        }

        result.sort();
        result.dedup();
        result
    }

    async fn get_algo_orders_raw(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        history: bool,
        algo_id: &str,
    ) -> Result<serde_json::Value> {
        let client = Self::create_okx_client(api_config)?;
        let trade = OkxTrade::new(client);
        let path = if history {
            format!(
                "/api/v5/trade/orders-algo-history?ordType=conditional&instId={}&algoId={}",
                inst_id, algo_id
            )
        } else {
            format!(
                "/api/v5/trade/orders-algo-pending?ordType=conditional&instId={}&algoId={}",
                inst_id, algo_id
            )
        };

        trade
            .client()
            .send_request::<serde_json::Value>(Method::GET, &path, "")
            .await
            .or_else(|e| {
                let msg = format!("{e}");
                if msg.contains("51603") || msg.contains("Order does not exist") {
                    Ok(serde_json::json!({ "data": [] }))
                } else {
                    Err(anyhow!("获取策略委托订单失败: {}", e))
                }
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

    /// 从API配置创建OKX客户端
    fn create_okx_client(config: &ExchangeApiConfig) -> Result<OkxClient> {
        if config.exchange_name.to_lowercase() != "okx" {
            return Err(anyhow!("不支持的交易所: {}", config.exchange_name));
        }

        let passphrase = config
            .passphrase
            .as_ref()
            .ok_or_else(|| anyhow!("OKX需要Passphrase"))?;

        use okx::config::Credentials;
        let credentials = Credentials::new(
            &config.api_key,
            &config.api_secret,
            passphrase,
            if config.is_sandbox { "1" } else { "0" },
        );

        let mut client =
            OkxClient::new(credentials).map_err(|e| anyhow!("创建OKX客户端失败: {}", e))?;
        Self::apply_request_expiration_override(&mut client);
        Ok(client)
    }

    pub fn build_cancel_close_algo_body(inst_id: &str, algo_ids: &[String]) -> serde_json::Value {
        serde_json::json!({
            "instId": inst_id,
            "algoIds": algo_ids,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_place_close_algo_body(
        inst_id: &str,
        mgn_mode: &str,
        side: &str,
        pos_side: &str,
        take_profit_trigger_px: Option<f64>,
        stop_loss_trigger_px: Option<f64>,
        algo_cl_ord_id: Option<&str>,
        tag: Option<&str>,
    ) -> serde_json::Value {
        let mut body = serde_json::Map::new();
        body.insert("instId".to_string(), serde_json::json!(inst_id));
        body.insert("tdMode".to_string(), serde_json::json!(mgn_mode));
        body.insert("side".to_string(), serde_json::json!(side));
        body.insert("posSide".to_string(), serde_json::json!(pos_side));
        body.insert("algoType".to_string(), serde_json::json!("conditional"));
        body.insert("closeFraction".to_string(), serde_json::json!("1"));
        if let Some(cl_ord_id) = algo_cl_ord_id {
            body.insert("algoClOrdId".to_string(), serde_json::json!(cl_ord_id));
        }
        if let Some(tag) = tag {
            body.insert("tag".to_string(), serde_json::json!(tag));
        }

        if let Some(tp) = take_profit_trigger_px {
            body.insert(
                "tpTriggerPx".to_string(),
                serde_json::json!(format!("{:.8}", tp)),
            );
            body.insert("tpOrdPx".to_string(), serde_json::json!("-1"));
            body.insert("tpTriggerPxType".to_string(), serde_json::json!("last"));
        }

        if let Some(sl) = stop_loss_trigger_px {
            body.insert(
                "slTriggerPx".to_string(),
                serde_json::json!(format!("{:.8}", sl)),
            );
            body.insert("slOrdPx".to_string(), serde_json::json!("-1"));
            body.insert("slTriggerPxType".to_string(), serde_json::json!("last"));
        }

        serde_json::Value::Object(body)
    }

    /// 执行下单操作（市价单）
    pub async fn place_order(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        side: Side,
        pos_side: PositionSide,
        size: String,
        cl_ord_id: Option<String>,
    ) -> Result<Vec<OrderResDto>> {
        info!(
            "执行下单: exchange={}, inst_id={}, side={:?}, pos_side={:?}, size={}, cl_ord_id={:?}",
            api_config.exchange_name, inst_id, side, pos_side, size, cl_ord_id
        );

        // 1. 创建客户端
        let client = Self::create_okx_client(api_config)?;
        let trade = OkxTrade::new(client.clone());

        // 2. 构建订单请求（市价单，与原实现一致）
        let order_req = OrderReqDto {
            inst_id: inst_id.to_string(),
            td_mode: TdModeEnum::ISOLATED.as_str().to_owned(),
            side: side.as_str().to_string(),
            ord_type: OrdTypeEnum::MARKET.as_str().to_owned(), // 市价单，与原实现一致
            sz: size,
            px: None, // 市价单不需要价格
            reduce_only: Some(false),
            pos_side: Some(pos_side.as_str().to_string()),
            stp_mode: Some("cancel_maker".to_string()),
            attach_algo_ords: None,
            ban_amend: Some(false),
            tgt_ccy: None,
            ccy: None,
            cl_ord_id, // 设置订单ID，用于追踪
            tag: None,
            px_usd: None,
            px_vol: None,
            quick_mgn_type: None,
            stp_id: None,
        };

        // 3. 提交订单
        let result = trade.place_order(order_req).await.map_err(|e| {
            error!("下单失败: {}", e);
            anyhow!("下单失败: {}", e)
        })?;

        info!("下单成功: {:?}", result);
        Ok(result)
    }

    /// 下单并附带止盈/止损（attachAlgoOrds）
    /// 下单并附带止盈/止损（attachAlgoOrds）
    #[allow(clippy::too_many_arguments)]
    pub async fn place_order_with_algo_orders(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        side: Side,
        pos_side: PositionSide,
        size: String,
        take_profit_trigger_px: Option<f64>,
        stop_loss_trigger_px: Option<f64>,
        cl_ord_id: Option<String>,
    ) -> Result<Vec<OrderResDto>> {
        info!(
            "执行下单(附带止盈止损): exchange={}, inst_id={}, side={:?}, pos_side={:?}, size={}, tp={:?}, sl={:?}, cl_ord_id={:?}",
            api_config.exchange_name,
            inst_id,
            side,
            pos_side,
            size,
            take_profit_trigger_px,
            stop_loss_trigger_px,
            cl_ord_id
        );

        let client = Self::create_okx_client(api_config)?;
        let trade = OkxTrade::new(client.clone());

        let attach_algo_ords = if take_profit_trigger_px.is_some() || stop_loss_trigger_px.is_some()
        {
            Some(vec![AttachAlgoOrdReqDto::new(
                take_profit_trigger_px.map(|v| format!("{:.8}", v)),
                take_profit_trigger_px.map(|_| "-1".to_string()),
                stop_loss_trigger_px.map(|v| format!("{:.8}", v)),
                stop_loss_trigger_px.map(|_| "-1".to_string()),
                size.clone(),
            )])
        } else {
            None
        };

        let order_req = OrderReqDto {
            inst_id: inst_id.to_string(),
            td_mode: TdModeEnum::ISOLATED.as_str().to_owned(),
            side: side.as_str().to_string(),
            ord_type: OrdTypeEnum::MARKET.as_str().to_owned(),
            sz: size,
            px: None,
            reduce_only: Some(false),
            pos_side: Some(pos_side.as_str().to_string()),
            stp_mode: Some("cancel_maker".to_string()),
            attach_algo_ords,
            ban_amend: Some(false),
            tgt_ccy: None,
            ccy: None,
            cl_ord_id,
            tag: None,
            px_usd: None,
            px_vol: None,
            quick_mgn_type: None,
            stp_id: None,
        };

        let result = trade.place_order(order_req).await.map_err(|e| {
            error!("下单失败(附带止盈止损): {}", e);
            anyhow!("下单失败(附带止盈止损): {}", e)
        })?;

        info!("下单成功(附带止盈止损): {:?}", result);
        Ok(result)
    }

    /// 下单并附带止损（attachAlgoOrds），用于后续“移动止损到开仓价”的改单能力
    #[allow(clippy::too_many_arguments)]
    pub async fn place_order_with_stop_loss(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        side: Side,
        pos_side: PositionSide,
        size: String,
        stop_loss_trigger_px: f64,
        cl_ord_id: Option<String>,
    ) -> Result<Vec<OrderResDto>> {
        self.place_order_with_algo_orders(
            api_config,
            inst_id,
            side,
            pos_side,
            size,
            None,
            Some(stop_loss_trigger_px),
            cl_ord_id,
        )
        .await
    }

    /// 市价平仓
    pub async fn close_position(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        pos_side: PositionSide,
        mgn_mode: &str,
    ) -> Result<()> {
        info!(
            "执行平仓: exchange={}, inst_id={}, pos_side={:?}, mgn_mode={}",
            api_config.exchange_name, inst_id, pos_side, mgn_mode
        );

        let client = Self::create_okx_client(api_config)?;
        let trade = OkxTrade::new(client.clone());

        let req = CloseOrderReqDto {
            inst_id: inst_id.to_string(),
            pos_side: Some(pos_side.as_str().to_string()),
            mgn_mode: mgn_mode.to_string(),
            ccy: None,
            auto_cxl: Some(true),
            cl_ord_id: None,
            tag: None,
        };

        let resp = trade.close_position(&req).await.map_err(|e| {
            error!("平仓失败: {}", e);
            anyhow!("平仓失败: {}", e)
        })?;

        info!("平仓请求已提交: {:?}", resp);
        Ok(())
    }

    /// 撤销平仓策略委托（止盈/止损）
    pub async fn cancel_close_algos(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        algo_ids: &[String],
    ) -> Result<()> {
        if algo_ids.is_empty() {
            return Ok(());
        }

        let client = Self::create_okx_client(api_config)?;
        let trade = OkxTrade::new(client);
        let body = Self::build_cancel_close_algo_body(inst_id, algo_ids);
        let body_str =
            serde_json::to_string(&body).map_err(|e| anyhow!("序列化撤单请求失败: {}", e))?;
        let path = "/api/v5/trade/cancel-algos";

        let resp: serde_json::Value = trade
            .client()
            .send_request(Method::POST, path, &body_str)
            .await
            .map_err(|e| anyhow!("撤销平仓策略委托失败: {}", e))?;

        info!("撤销平仓策略委托返回: {}", resp);
        Ok(())
    }

    /// 下达平仓策略委托（止盈/止损）
    #[allow(clippy::too_many_arguments)]
    pub async fn place_close_algo(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        mgn_mode: &str,
        side: &str,
        pos_side: &str,
        take_profit_trigger_px: Option<f64>,
        stop_loss_trigger_px: Option<f64>,
        algo_cl_ord_id: Option<&str>,
        tag: Option<&str>,
    ) -> Result<Vec<String>> {
        let client = Self::create_okx_client(api_config)?;
        let trade = OkxTrade::new(client);
        let body = Self::build_place_close_algo_body(
            inst_id,
            mgn_mode,
            side,
            pos_side,
            take_profit_trigger_px,
            stop_loss_trigger_px,
            algo_cl_ord_id,
            tag,
        );
        let body_str =
            serde_json::to_string(&body).map_err(|e| anyhow!("序列化下单请求失败: {}", e))?;
        let path = "/api/v5/trade/order-algo";

        let resp: serde_json::Value = trade
            .client()
            .send_request(Method::POST, path, &body_str)
            .await
            .map_err(|e| anyhow!("下达平仓策略委托失败: {}", e))?;

        info!("下达平仓策略委托返回: {}", resp);
        let mut algo_ids = Vec::new();
        if let Some(items) = resp.get("data").and_then(|v| v.as_array()) {
            for item in items {
                if let Some(id) = item
                    .get("algoId")
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.is_empty())
                {
                    algo_ids.push(id.to_string());
                }
            }
        }
        Ok(algo_ids)
    }

    /// 获取账户持仓
    pub async fn get_positions(
        &self,
        api_config: &ExchangeApiConfig,
        inst_type: Option<&str>,
        inst_id: Option<&str>,
    ) -> Result<Vec<Position>> {
        let client = Self::create_okx_client(api_config)?;
        let account = OkxAccount::new(client.clone());

        account
            .get_account_positions(inst_type, inst_id, None)
            .await
            .map_err(|e| {
                error!("获取持仓失败: {}", e);
                anyhow!("获取持仓失败: {}", e)
            })
    }

    /// 获取最大可用数量
    pub async fn get_max_available_size(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
    ) -> Result<TradingSwapNumResponseData> {
        let client = Self::create_okx_client(api_config)?;
        let account = OkxAccount::new(client.clone());

        let result = account
            .get_max_size(inst_id, TdModeEnum::ISOLATED.as_str(), None, None, None)
            .await
            .map_err(|e| {
                error!("获取最大可用数量失败: {}", e);
                anyhow!("获取最大可用数量失败: {}", e)
            })?;

        if result.is_empty() {
            return Err(anyhow!("未找到交易对 {} 的最大可用数量", inst_id));
        }

        Ok(result[0].clone())
    }

    pub async fn get_order_details(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        ord_id: Option<&str>,
        cl_ord_id: Option<&str>,
    ) -> Result<Vec<okx::dto::trade_dto::OrderDetailRespDto>> {
        let client = Self::create_okx_client(api_config)?;
        let trade = OkxTrade::new(client);
        trade
            .get_order_details(inst_id, ord_id, cl_ord_id)
            .await
            .map_err(|e| anyhow!("获取订单详情失败: {}", e))
    }

    pub async fn inspect_auto_close_by_order(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        ord_id: Option<&str>,
        cl_ord_id: Option<&str>,
    ) -> Result<AutoCloseInspection> {
        let order_details = self
            .get_order_details(api_config, inst_id, ord_id, cl_ord_id)
            .await?;
        let order = order_details.first();

        let (attach_algo_ids, attach_algo_cl_ord_id) = match order {
            Some(order) => Self::collect_related_algo_ids(order),
            None => (Vec::new(), None),
        };

        let mut pending_algos = Vec::new();
        let mut history_algos = Vec::new();
        for algo_id in &attach_algo_ids {
            let pending_raw = self
                .get_algo_orders_raw(api_config, inst_id, false, algo_id)
                .await?;
            pending_algos.extend(Self::extract_matching_algo_ids(
                &pending_raw,
                std::slice::from_ref(algo_id),
                attach_algo_cl_ord_id.as_deref(),
            ));

            let history_raw = self
                .get_algo_orders_raw(api_config, inst_id, true, algo_id)
                .await?;
            history_algos.extend(Self::extract_matching_algo_ids(
                &history_raw,
                std::slice::from_ref(algo_id),
                attach_algo_cl_ord_id.as_deref(),
            ));
        }
        pending_algos.sort();
        pending_algos.dedup();
        history_algos.sort();
        history_algos.dedup();

        let positions = self.get_positions(api_config, Some("SWAP"), Some(inst_id)).await?;
        let has_open_position = match order {
            Some(order) => positions.iter().any(|position| {
                let qty = position.pos.parse::<f64>().unwrap_or(0.0).abs();
                qty > 1e-12
                    && (order.pos_side.trim().is_empty()
                        || position.pos_side.eq_ignore_ascii_case(&order.pos_side))
            }),
            None => positions
                .iter()
                .any(|position| position.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12),
        };

        let position_closed = !has_open_position;
        let auto_close_likely = position_closed
            && (!history_algos.is_empty()
                || (order.is_some()
                    && (!attach_algo_ids.is_empty() || attach_algo_cl_ord_id.is_some())));

        Ok(AutoCloseInspection {
            inst_id: inst_id.to_string(),
            ord_id: ord_id.map(|v| v.to_string()),
            cl_ord_id: cl_ord_id.map(|v| v.to_string()),
            order_found: order.is_some(),
            order_state: order.map(|o| o.state.clone()),
            order_source: order.map(|o| o.source.clone()),
            pos_side: order.map(|o| o.pos_side.clone()),
            attach_algo_ids,
            attach_algo_cl_ord_id,
            pending_algo_ids: pending_algos,
            history_algo_ids: history_algos,
            has_open_position,
            position_closed,
            auto_close_likely,
        })
    }

    pub async fn get_trade_available_equity(
        &self,
        api_config: &ExchangeApiConfig,
        currency: &str,
    ) -> Result<f64> {
        let client = Self::create_okx_client(api_config)?;
        let account = OkxAccount::new(client.clone());

        let balances = account
            .get_balance(Some(currency))
            .await
            .map_err(|e| anyhow!("获取交易账户余额失败: {}", e))?;

        let balance = balances
            .first()
            .ok_or_else(|| anyhow!("未找到交易账户中的{}余额", currency))?;

        if let Ok(value) = balance.avail_eq.parse::<f64>() {
            return Ok(value);
        }

        if let Some(detail) = balance
            .details
            .iter()
            .find(|detail| detail.ccy.eq_ignore_ascii_case(currency))
        {
            if let Ok(value) = detail.avail_bal.parse::<f64>() {
                return Ok(value);
            }
            if let Ok(value) = detail.cash_bal.parse::<f64>() {
                return Ok(value);
            }
            if let Ok(value) = detail.eq.parse::<f64>() {
                return Ok(value);
            }
        }

        Err(anyhow!(
            "解析交易账户余额失败: availEq={}, currency={}",
            balance.avail_eq,
            currency
        ))
    }

    pub async fn get_funding_available_balance(
        &self,
        api_config: &ExchangeApiConfig,
        currency: &str,
    ) -> Result<f64> {
        let client = Self::create_okx_client(api_config)?;
        let asset = OkxAsset::new(client);
        let currencies = vec![currency.to_string()];

        let balances = asset
            .get_balances(Some(&currencies))
            .await
            .map_err(|e| anyhow!("获取资金账户余额失败: {}", e))?;

        let balance = balances
            .first()
            .ok_or_else(|| anyhow!("未找到资金账户中的{}余额", currency))?;

        balance
            .avail_bal
            .parse::<f64>()
            .map_err(|e| anyhow!("解析资金账户余额失败: {}", e))
    }

    pub async fn transfer_between_accounts(
        &self,
        api_config: &ExchangeApiConfig,
        currency: &str,
        amount: f64,
        from: AccountType,
        to: AccountType,
    ) -> Result<serde_json::Value> {
        let client = Self::create_okx_client(api_config)?;
        let asset = OkxAsset::new(client);
        let transfer_req = TransferOkxReqDto {
            transfer_type: Some("0".to_string()),
            ccy: currency.to_string(),
            amt: format!("{:.8}", amount),
            from,
            to,
            sub_acct: None,
        };

        asset
            .transfer(&transfer_req)
            .await
            .map_err(|e| anyhow!("执行账户划转失败: {}", e))
    }

    /// 根据信号执行订单
    /// 与原实现 swap_order_service.rs::order_swap 保持一致
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_order_from_signal(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        signal: &SignalResult,
        size: String,
        stop_loss_trigger_px: Option<f64>,
        take_profit_trigger_px: Option<f64>,
        cl_ord_id: Option<String>,
    ) -> Result<Vec<OrderResDto>> {
        let (side, pos_side) = if signal.should_buy {
            (Side::Buy, PositionSide::Long)
        } else if signal.should_sell {
            (Side::Sell, PositionSide::Short)
        } else {
            return Err(anyhow!("信号无效，无交易方向"));
        };

        if stop_loss_trigger_px.is_some() || take_profit_trigger_px.is_some() {
            self.place_order_with_algo_orders(
                api_config,
                inst_id,
                side,
                pos_side,
                size,
                take_profit_trigger_px,
                stop_loss_trigger_px,
                cl_ord_id,
            )
            .await
        } else {
            self.place_order(api_config, inst_id, side, pos_side, size, cl_ord_id)
                .await
        }
    }
}
