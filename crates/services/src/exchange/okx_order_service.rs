//! OKX交易所订单执行服务

use anyhow::{anyhow, Result};
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::{Position, TradingSwapNumResponseData};
use okx::dto::common::EnumToStrTrait;
use okx::dto::common::Side;
use okx::dto::trade::trade_dto::{OrderReqDto, OrderResDto, TdModeEnum};
use okx::dto::trade_dto::OrdTypeEnum;
use okx::dto::PositionSide;
use okx::{Error, OkxAccount, OkxClient, OkxTrade};
use rust_quant_domain::entities::ExchangeApiConfig;
use rust_quant_strategies::strategy_common::SignalResult;
use tracing::{error, info, warn};

/// OKX订单执行服务
pub struct OkxOrderService;

impl OkxOrderService {
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

        OkxClient::new(credentials).map_err(|e| anyhow!("创建OKX客户端失败: {}", e))
    }

    /// 执行下单操作
    pub async fn place_order(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        side: Side,
        pos_side: PositionSide,
        size: String,
        price: Option<f64>,
    ) -> Result<Vec<OrderResDto>> {
        info!(
            "执行下单: exchange={}, inst_id={}, side={:?}, pos_side={:?}, size={}",
            api_config.exchange_name, inst_id, side, pos_side, size
        );

        // 1. 创建客户端
        let client = Self::create_okx_client(api_config)?;
        let trade = OkxTrade::new(client.clone());

        // 2. 构建订单请求
        let order_req = OrderReqDto {
            inst_id: inst_id.to_string(),
            td_mode: TdModeEnum::ISOLATED.as_str().to_owned(),
            side: side.as_str().to_string(),
            ord_type: OrdTypeEnum::LIMIT.as_str().to_owned(),
            sz: size,
            px: price.map(|p| p.to_string()),
            reduce_only: Some(false),
            pos_side: Some(pos_side.as_str().to_string()),
            stp_mode: Some("cancel_maker".to_string()),
            attach_algo_ords: None,
            ban_amend: None,
            tgt_ccy: None,
            ccy: None,
            cl_ord_id: None,
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
    pub async fn execute_order_from_signal(
        &self,
        api_config: &ExchangeApiConfig,
        inst_id: &str,
        signal: &SignalResult,
        size: String,
        price: Option<f64>,
    ) -> Result<Vec<OrderResDto>> {
        let (side, pos_side) = if signal.should_buy {
            (Side::Buy, PositionSide::Long)
        } else if signal.should_sell {
            (Side::Sell, PositionSide::Short)
        } else {
            return Err(anyhow!("信号无效，无交易方向"));
        };

        self.place_order(api_config, inst_id, side, pos_side, size, price)
            .await
    }
}

