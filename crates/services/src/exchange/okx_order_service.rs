//! OKX交易所订单执行服务

use anyhow::{anyhow, Result};
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::{Position, TradingSwapNumResponseData};
use okx::dto::common::EnumToStrTrait;
use okx::dto::common::Side;
use okx::dto::trade::trade_dto::{AttachAlgoOrdReqDto, OrderReqDto, OrderResDto, TdModeEnum};
use okx::dto::trade_dto::CloseOrderReqDto;
use okx::dto::trade_dto::OrdTypeEnum;
use okx::dto::PositionSide;
use okx::{OkxAccount, OkxClient, OkxTrade};
use rust_quant_domain::entities::ExchangeApiConfig;
use rust_quant_strategies::strategy_common::SignalResult;
use tracing::{error, info};

/// OKX订单执行服务
pub struct OkxOrderService;

impl OkxOrderService {
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
