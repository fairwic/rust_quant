use rust_quant_common::constants;
use rust_quant_risk::order::SwapOrderEntity;
use rust_quant_strategies::StrategyType;
pub struct OrderSignal {
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 是否应该触发卖出。
    pub should_sell: bool,
    /// 价格。
    pub price: f64,
}
use rust_quant_common::AppError;
use rust_quant_strategies::framework::risk::{StopLossCalculator, StopLossSide};
use rust_quant_strategies::strategy_common::{BasicRiskStrategyConfig, SignalResult};
// use core::time; // ⭐ 注释掉，与time模块冲突
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::{Position, TradingSwapNumResponseData};
use okx::dto::common::EnumToStrTrait;
use okx::dto::common::Side;
use okx::dto::trade::trade_dto::{
    AttachAlgoOrdReqDto, OrderReqDto, OrderResDto, TdModeEnum, TpOrdKindEnum,
};
use okx::dto::trade_dto::{CloseOrderReqDto, OrdTypeEnum};
use okx::dto::PositionSide;
use okx::{Error, OkxAccount, OkxTrade};
use serde_json::json;
use tracing::{debug, error, info, warn};
/// [已优化] 配置化的风控参数
pub struct OrderSizeConfig {
    /// 安全系数：实际使用最大可用量的百分比
    /// 默认 0.9 表示使用 90%
    pub safety_factor: f64,
    /// 最小下单量
    pub min_order_size: f64,
    /// 精度（小数位数）
    pub precision: u32,
}
impl Default for OrderSizeConfig {
    /// 提供默认参数，保证 交易执行与风控 在未显式配置时仍有稳定初始值。
    fn default() -> Self {
        Self {
            safety_factor: 0.9,  // 90% 安全边际
            min_order_size: 1.0, // 最小1张
            precision: 2,        // 保留2位小数
        }
    }
}
pub struct SwapOrderService {}
const LEGACY_DIRECT_LIVE_ORDER_CONFIRM_ENV: &str = "LEGACY_DIRECT_LIVE_ORDER_CONFIRM";
const LEGACY_DIRECT_LIVE_ORDER_CONFIRM_TOKEN: &str = "I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS";
impl SwapOrderService {
    pub fn new() -> Self {
        Self {}
    }
}
impl Default for SwapOrderService {
    fn default() -> Self {
        Self::new()
    }
}
impl SwapOrderService {
    /// 提供legacydirectlive交易所订单allowed的集中实现，避免交易执行调用方重复处理相同细节。
    fn legacy_direct_live_exchange_order_allowed() -> bool {
        let confirmation = std::env::var(LEGACY_DIRECT_LIVE_ORDER_CONFIRM_ENV).ok();
        confirmation.as_deref().map(str::trim) == Some(LEGACY_DIRECT_LIVE_ORDER_CONFIRM_TOKEN)
    }
    /// 提供legacydirectlive交易所订单errormessage的集中实现，避免交易执行调用方重复处理相同细节。
    fn legacy_direct_live_exchange_order_error_message() -> String {
        format!(
            "{}={} is required before using legacy rust_quant_execution SwapOrderService live mutations; use the quant_web execution worker path instead",
            LEGACY_DIRECT_LIVE_ORDER_CONFIRM_ENV,
            LEGACY_DIRECT_LIVE_ORDER_CONFIRM_TOKEN
        )
    }
    /// 校验输入和运行前置条件，提前暴露 交易执行与风控 的不可执行原因。
    fn ensure_legacy_direct_live_exchange_order_allowed() -> Result<(), AppError> {
        if Self::legacy_direct_live_exchange_order_allowed() {
            Ok(())
        } else {
            Err(AppError::Config(
                Self::legacy_direct_live_exchange_order_error_message(),
            ))
        }
    }
    /// 校验输入和运行前置条件，提前暴露 交易执行与风控 的不可执行原因。
    fn ensure_legacy_direct_live_exchange_order_allowed_sdk() -> Result<(), Error> {
        if Self::legacy_direct_live_exchange_order_allowed() {
            Ok(())
        } else {
            Err(Error::ConfigError(
                Self::legacy_direct_live_exchange_order_error_message(),
            ))
        }
    }
    /// 校验输入和运行前置条件，提前暴露 交易执行与风控 的不可执行原因。
    fn ensure_positive_finite_price(label: &str, price: f64) -> Result<(), AppError> {
        if price.is_finite() && price > 0.0 {
            Ok(())
        } else {
            Err(AppError::BizError(format!(
                "{label} must be a positive finite price before legacy live order placement"
            )))
        }
    }
    /// 校验输入和运行前置条件，提前暴露 交易执行与风控 的不可执行原因。
    fn ensure_max_loss_percent_ratio(max_loss_percent: f64) -> Result<(), AppError> {
        if max_loss_percent.is_finite() && max_loss_percent > 0.0 && max_loss_percent < 1.0 {
            Ok(())
        } else {
            Err(AppError::BizError(format!(
                "max_loss_percent must be a finite ratio greater than 0 and less than 1 before legacy live order placement; got {max_loss_percent}"
            )))
        }
    }
    //下单现货
    /// 提供place订单spot的集中实现，避免交易执行调用方重复处理相同细节。
    pub async fn place_order_spot(
        &self,
        inst_id: &str,
        side: Side,
        _px: f64,
    ) -> Result<Vec<OrderResDto>, Error> {
        Self::ensure_legacy_direct_live_exchange_order_allowed_sdk()?;
        //todo 获取当前可以开仓的数量
        let sz = 1;
        //todo 设置止盈止损
        let px = 3000.00;
        let order_params = OrderReqDto {
            inst_id: inst_id.to_string(),
            td_mode: TdModeEnum::CASH.as_str().to_owned(),
            side: side.as_str().to_string(),
            ord_type: OrdTypeEnum::LIMIT.as_str().to_owned(),
            sz: sz.to_string(),
            px: Option::from(px.to_string()),
            reduce_only: Some(false),
            stp_mode: Some("cancel_maker".to_string()),
            attach_algo_ords: Some(vec![AttachAlgoOrdReqDto {
                attach_algo_cl_ord_id: None,
                tp_trigger_px: Some("3500".to_string()),
                tp_ord_px: Some("-1".to_string()),
                tp_ord_kind: Some(TpOrdKindEnum::CONDITION.as_str().to_owned()),
                sl_trigger_px: Some("2200".to_string()),
                sl_ord_px: Some("-1".to_string()),
                tp_trigger_px_type: Some("last".to_string()),
                sl_trigger_px_type: Some("last".to_string()),
                sz: None,
                amend_px_on_trigger_type: Some(0),
            }]),
            ban_amend: None,
            tgt_ccy: None,
            pos_side: None,
            ccy: None,
            cl_ord_id: None,
            tag: None,
            px_usd: None,
            px_vol: None,
            quick_mgn_type: None,
            stp_id: None,
        };
        //下单
        let result = OkxTrade::from_env()?.place_order(order_params).await;
        // okx_response: {"code":"1","data":[{"clOrdId":"","ordId":"","sCode":"51094","sMsg":"You can't place TP limit orders in spot, margin, or options trading.","tag":"","ts":"1718339551210"}],"inTime":"1718339551209444","msg":"All operations failed","outTime":"1718339551210787"}
        // okx_response: {"code":"0","data":[{"clOrdId":"","ordId":"1538100941143183360","sCode":"0","sMsg":"Order placed","tag":"","ts":"1718341380112"}],"inTime":"1718341380111025","msg":"","outTime":"1718341380112306"}
        info!("Order result: {:#?}", result);
        result
    }
    /// 平仓
    pub async fn close_position(
        &self,
        position_list: &[Position],
        inst_id: &str,
        pos_side: &PositionSide,
    ) -> Result<bool, Error> {
        Self::ensure_legacy_direct_live_exchange_order_allowed_sdk()?;
        let already_have_position = !position_list.is_empty();
        //是否已经有反向仓位
        let mut have_another_position = false;
        if already_have_position {
            // let position = position_list.get(0).unwrap();
            for position in position_list.iter() {
                //且持仓量不为0
                if position.inst_id == inst_id && position.pos != "0" {
                    //如果当前仓位是反向仓位，则平仓
                    if position.pos_side == pos_side.as_str() {
                        let params = CloseOrderReqDto {
                            inst_id: inst_id.to_string(),
                            pos_side: Option::from(pos_side.as_str().to_owned()),
                            mgn_mode: position.mgn_mode.clone(),
                            ccy: None,
                            auto_cxl: Some(true), //自动撤单
                            cl_ord_id: None,
                            tag: None,
                        };
                        let res = OkxTrade::from_env()?.close_position(&params).await?;
                        info!("close  order position result {:?}", res);
                    } else {
                        have_another_position = true
                    }
                } else {
                    debug!(
                        "not close order position match inst_id {} or pos is 0 {:?}",
                        inst_id, position.pos_side
                    );
                }
            }
        }
        Ok(have_another_position)
    }
    /// [已优化] 获取下单数量 - 统一风控策略
    /// 风控策略：使用最大可用量的指定百分比（默认90%）
    /// - 安全边际：避免因市场波动导致下单失败
    /// - 精度保留：直接使用f64计算，减少字符串转换
    pub fn get_place_order_num_optimized(
        &self,
        valid_num: &TradingSwapNumResponseData,
        safety_factor: f64, // 安全系数 0.0-1.0，默认0.9
    ) -> Result<String, AppError> {
        // 1. 解析并验证
        let max_buy = valid_num.max_buy.parse::<f64>().map_err(|e| {
            error!("解析max_buy失败: value={}, error={}", valid_num.max_buy, e);
            AppError::BizError(format!("Invalid max_buy: {}", valid_num.max_buy))
        })?;
        // 2. 验证有效性
        if !max_buy.is_finite() {
            warn!("max_buy非有限值: {}", max_buy);
            return Ok("0".to_string());
        }
        if max_buy < 0.0 {
            warn!("max_buy为负数: {}", max_buy);
            return Ok("0".to_string());
        }
        // 3. 应用安全系数（一次性计算）
        let order_size = max_buy * safety_factor;
        // 4. 向下取整到交易所要求的精度（2位小数）
        let order_size_rounded = (order_size * 100.0).floor() / 100.0;
        info!(
            "计算下单量: max_buy={}, safety_factor={}, result={}",
            max_buy, safety_factor, order_size_rounded
        );
        Ok(order_size_rounded.to_string())
    }
    /// 准备下单
    pub async fn ready_to_order(
        &self,
        strategy_type: &StrategyType,
        inst_id: &str,
        period: &str,
        signal: &SignalResult,
        risk_config: &BasicRiskStrategyConfig,
        strategy_config_id: i64,
    ) -> Result<(), AppError> {
        // 无信号早返回，避免后续不必要开销
        if !(signal.should_buy || signal.should_sell) {
            return Ok(());
        }
        Self::ensure_legacy_direct_live_exchange_order_allowed()?;
        Self::ensure_max_loss_percent_ratio(risk_config.max_loss_percent)?;
        // 幂等校验前置：同品种×周期×方向×持仓方向的在途单直接返回
        // TODO: SwapOrderEntity需要实现query_one方法
        /*
        let (pre_side, pre_pos_side) = if signal.should_buy {
            (Side::Buy, PositionSide::Long)
        } else {
            (Side::Sell, PositionSide::Short)
        };
        let exists = SwapOrderEntity::query_one(inst_id, period, pre_side.as_str(), pre_pos_side.as_str())
            .await
            .map_err(|e| {
                error!("get swap order list error: {:?}", e);
                AppError::DbError(e.to_string())
            })?;
        if exists.len() > 0 {
            info!("exists order: {:?}", exists);
            return Ok(vec![]);
        }
        */
        // 临时跳过幂等校验
        // warn!("幂等校验暂时禁用");
        // 获取当前仓位状态与可开仓数量（并发请求，降低总时延）
        let account = OkxAccount::from_env().map_err(|e| {
            error!("create okx account client error: {:?}", e);
            AppError::OkxApiError(e.to_string())
        })?;
        let cross = TdModeEnum::ISOLATED.as_str().to_owned();
        //后续考虑极端情况下，当多个产品都出现信号，此处是否会触发交易所的api请求限制
        let (position_list, max_avail_size) = tokio::try_join!(
            account.get_account_positions(Some("SWAP"), Some(inst_id), None),
            account.get_max_size(inst_id, &cross, None, None, None)
        )
        .map_err(|e| {
            error!("get account data error: {:?}", e);
            AppError::OkxApiError(e.to_string())
        })?;
        info!("current okx position_count: {}", position_list.len());
        if max_avail_size.is_empty() || max_avail_size[0].inst_id != inst_id {
            error!("max_avail_size is empty or inst_id not match");
            return Err(AppError::BizError(
                "max_avail_size is empty or inst_id not match".to_string(),
            ));
        }
        let trad_swap_nums = max_avail_size[0].clone();
        info!(
            "max_avail_size(inst_id={}): max_buy={}",
            inst_id, trad_swap_nums.max_buy
        );
        // 处理下单数量
        let pos_size = self
            .get_place_order_num_optimized(&trad_swap_nums, 0.9)
            .unwrap();
        if pos_size == "0" {
            info!("pos_size is 0, skip placing order");
            return Ok(());
        }
        // if pos_size.parse::<f64>().unwrap() < 1.0 {
        //     error!("pos_size is  small than 1.0, not enough to place order");
        //     return Err(AppError::BizError(
        //         "pos_size is  small than 1.0, not enough to place order".to_string(),
        //     ));
        // }
        //避免极端情况下又其他仓位的情况下，导致下单数量减少，下单数量超过最大可用数量
        info!("ready to place order size: {:?}", pos_size);
        //平掉现有的已经存在的反向仓位
        let pos_side = match signal.should_buy {
            true => PositionSide::Short,
            false => PositionSide::Long,
        };
        self.async_ready_close_order(inst_id, period, &position_list, &pos_side)
            .await?;
        let (order_result, side, pos_side) = if signal.should_buy {
            //买入开多
            let in_order_id = SwapOrderEntity::gen_order_id(
                inst_id,
                period,
                Side::Buy.as_str(),
                PositionSide::Long.as_str(),
            );
            (
                self.start_to_order(
                    inst_id,
                    period,
                    in_order_id,
                    PositionSide::Short,
                    Side::Buy,
                    PositionSide::Long,
                    pos_size.clone(),
                    signal,
                    strategy_type,
                    risk_config,
                )
                .await,
                Side::Buy,
                PositionSide::Long,
            )
        } else if signal.should_sell {
            //买入做空
            let in_order_id = SwapOrderEntity::gen_order_id(
                inst_id,
                period,
                Side::Sell.as_str(),
                PositionSide::Short.as_str(),
            );
            (
                self.start_to_order(
                    inst_id,
                    period,
                    in_order_id,
                    PositionSide::Long,
                    Side::Sell,
                    PositionSide::Short,
                    pos_size.clone(),
                    signal,
                    strategy_type,
                    risk_config,
                )
                .await,
                Side::Sell,
                PositionSide::Short,
            )
        } else {
            (Ok(vec![]), Side::Buy, PositionSide::Long) // 默认值，不会被使用
        };
        if let Ok(order_result) = order_result {
            // 记录到订单中
            self.record_order(
                strategy_type,
                inst_id,
                period,
                order_result,
                side,
                pos_side,
                strategy_config_id,
                pos_size,
            )
            .await?;
        }
        Ok(())
    }
    //同步平掉现有的已经存在的反向仓位
    /// 提供asyncready平仓订单的集中实现，避免交易执行调用方重复处理相同细节。
    pub async fn async_ready_close_order(
        &self,
        inst_id: &str,
        _time: &str,
        position_list: &[Position],
        close_pos_side: &PositionSide,
    ) -> Result<(), AppError> {
        let needs_close = position_list.iter().any(|position| {
            position.inst_id == inst_id
                && position.pos != "0"
                && position.pos_side == close_pos_side.as_str()
        });
        if !needs_close {
            debug!("no reverse position needs closing before opening order");
            return Ok(());
        }
        self.close_position(position_list, inst_id, close_pos_side)
            .await
            .map_err(|error| {
                error!("判断关闭反向仓位失败 position error: {:?}", error);
                AppError::OkxApiError(error.to_string())
            })?;
        debug!("判断关闭反向仓位结束");
        Ok(())
    }
    /// 开始下单
    pub async fn start_to_order(
        &self,
        inst_id: &str,
        _time: &str,
        in_order_id: String,
        _close_pos_side: PositionSide,
        side: Side,
        pos_side: PositionSide,
        pos_size: String,
        signal: &SignalResult,
        _strategy_type: &StrategyType,
        risk_config: &BasicRiskStrategyConfig,
    ) -> Result<Vec<OrderResDto>, AppError> {
        //判断相同周期下是否已经有了订单
        // TODO: SwapOrderEntity需要实现query_one方法
        /*
        let swap_order_list = SwapOrderEntity::query_one(inst_id, time, side.as_str(), pos_side.as_str())
            .await
            .map_err(|e| {
                error!("get swap order list error: {:?}", e);
                AppError::DbError(e.to_string())
            })?;
        if swap_order_list.len() > 0 {
            info!("same period same inst_id same side, already have order");
            return Ok(vec![]);
        }
        */
        // 判断当下需要下单的时间，是不是不在交易设定的时间范围内
        // let can_order = time_util::is_within_business_hours(ts);
        // if !can_order {
        //     warn!("time is not within business hours or in saturday");
        //     return None;
        // }
        let price = signal.open_price;
        let _ts = signal.ts;
        //todo 当前下单数量不足的时候自动划转交易资金
        // 下单
        let order_result = self
            .order_swap(
                inst_id,
                in_order_id,
                side,
                pos_side,
                price,
                pos_size,
                signal,
                risk_config,
            )
            .await?;
        Ok(order_result)
    }
    #[allow(clippy::too_many_arguments)]
    /// 持久化 交易执行与风控 结果，保证写入路径和幂等语义集中处理。
    pub async fn record_order(
        &self,
        strategy_type: &StrategyType,
        inst_id: &str,
        time: &str,
        order_results: Vec<OrderResDto>,
        side: Side,
        pos_side: PositionSide,
        strategy_id: i64,
        pos_size: String,
    ) -> Result<(), AppError> {
        for order in order_results.into_iter() {
            // 下单成功
            let _swap_order_entity = SwapOrderEntity {
                strategy_id,
                in_order_id: order.cl_ord_id.clone().unwrap_or("".to_string()),
                strategy_type: strategy_type.as_str().to_owned(),
                period: time.to_string(),
                inst_id: inst_id.to_string(),
                side: side.as_str().to_owned(),
                pos_side: pos_side.as_str().to_owned(),
                pos_size: pos_size.clone(),
                out_order_id: order.ord_id.to_string(),
                tag: "".to_string(),
                detail: json!(order).to_string(),
                platform_type: "okx".to_string(),
            };
            // TODO: SwapOrderEntity需要实现insert方法
            /*
            let res = swap_order_entity.insert().await;
            if res.is_err() {
                error!("record order error: {:?} {:?}", res, swap_order_entity);
            }
            */
            warn!("订单记录暂未实现");
        }
        Ok(())
    }
    /// 生成 交易执行与风控 需要的派生数据，供后续执行、展示或审计使用。
    pub fn generate_fibonacci_take_profit_orders(
        &self,
        _entry_price: f64,
        stop_loss_price: f64,
        tp_price: Option<f64>,
        size: &str,
        _side: &Side,
    ) -> Vec<AttachAlgoOrdReqDto> {
        let mut orders = Vec::new();
        //止盈
        // for level in fib_levels {
        //     let tp_trigger_px: f64 = match side {
        //         Side::Sell => entry_price * (1.0 - level),
        //         Side::Buy => entry_price * (1.0 + level),
        //     };
        // fn set_to_multiple(value: &mut i32, multiple_of: i32) {
        // *value = (*value / multiple_of) * multiple_of;
        // }
        // let order_size = (size.parse::<f64>().unwrap() * (level / fib_levels.iter().sum::<f64>())).ceil();
        // let order_size_str = format!("{:.2}", order_size);
        // let tp_ord_px = tp_trigger_px - 100.0; // 根据你的需求调整价格
        // }
        //-1 表示市价
        let mut order = AttachAlgoOrdReqDto::new(
            None,                                    // 止盈触发价
            None,                                    // 止盈委托价 -1 表示市价
            Some(format!("{:.2}", stop_loss_price)), // 止损触发价
            Some("-1".to_string()),                  // 止损委托价 -1 表示市价
            size.to_string(),
        );
        if let Some(price) = tp_price {
            order.tp_ord_px = Some("-1".to_string());
            order.tp_trigger_px = Some(price.to_string());
        }
        orders.push(order);
        orders
    }
    //判断开仓产品是否时btc
    pub fn is_btc_swap(&self, inst_id: &str) -> bool {
        inst_id.contains(constants::common_enums::BTC_SWAP_INST_ID)
    }
    //下单合约
    #[allow(clippy::too_many_arguments)]
    /// 提供订单swap的集中实现，避免交易执行调用方重复处理相同细节。
    pub async fn order_swap(
        &self,
        inst_id: &str,
        in_order_id: String,
        side: Side,
        pos_side: PositionSide,
        entry_price: f64,
        size: String,
        signal: &SignalResult,
        risk_config: &BasicRiskStrategyConfig,
    ) -> Result<Vec<OrderResDto>, AppError> {
        Self::ensure_legacy_direct_live_exchange_order_allowed()?;
        Self::ensure_positive_finite_price("entry_price", entry_price)?;
        //最大止损
        let max_loss_percent = risk_config.max_loss_percent;
        Self::ensure_max_loss_percent_ratio(max_loss_percent)?;
        let tp_price = signal.atr_take_profit_level_1;
        let max_loss_stop: f64 = match side {
            Side::Sell => entry_price * (1.0 + max_loss_percent),
            Side::Buy => entry_price * (1.0 - max_loss_percent),
        };
        let mut candidates = vec![max_loss_stop];
        if risk_config.is_used_signal_k_line_stop_loss.unwrap_or(false) {
            if let Some(signal_stop_loss) = signal.signal_kline_stop_loss_price {
                candidates.push(signal_stop_loss);
            }
        }
        let sl_side = match side {
            Side::Sell => StopLossSide::Short,
            Side::Buy => StopLossSide::Long,
        };
        let stop_loss_price = StopLossCalculator::select(sl_side, entry_price, &candidates)
            .ok_or_else(|| AppError::BizError("no valid stop_loss candidate".to_string()))?;
        Self::ensure_positive_finite_price("stop-loss", stop_loss_price)?;
        //valid 如果是做空，开仓价格要<止损价格,否则不进行下单
        //valid 如果是做多，开仓价格要>止损价格,否则不进行下单
        if pos_side == PositionSide::Short && entry_price > stop_loss_price {
            error!("entry_price > stop_loss_price, not place order, entry_price: {}, stop_loss_price: {}", entry_price, stop_loss_price);
            return Err(AppError::BizError(
                "entry_price > stop_loss_price, not place order".to_string(),
            ));
        }
        if pos_side == PositionSide::Long && entry_price < stop_loss_price {
            error!("entry_price < stop_loss_price, not place order, entry_price: {}, stop_loss_price: {}", entry_price, stop_loss_price);
            return Err(AppError::BizError(
                "entry_price < stop_loss_price, not place order".to_string(),
            ));
        }
        //todo 确保最大止损不会触发爆仓
        let attach_algo_ords = SwapOrderService::new().generate_fibonacci_take_profit_orders(
            entry_price,
            stop_loss_price,
            tp_price,
            &size,
            &side,
        );
        debug!("place order attach_algo_ords{:?}", attach_algo_ords);
        let order_params = OrderReqDto {
            inst_id: inst_id.to_string(),
            td_mode: TdModeEnum::ISOLATED.as_str().to_owned(),
            ccy: None,
            cl_ord_id: Some(in_order_id),
            tag: None,
            side: side.as_str().to_string(),
            pos_side: Option::from(pos_side.as_str().to_string()),
            // pos_side: None,
            ord_type: OrdTypeEnum::MARKET.as_str().to_owned(),
            sz: size,
            px: None,
            // px: Some("30000".to_string()),
            px_usd: None,
            px_vol: None,
            reduce_only: Some(false),
            tgt_ccy: None,
            ban_amend: Some(false),
            quick_mgn_type: None,
            stp_id: None,
            stp_mode: Some("cancel_maker".to_string()),
            attach_algo_ords: Some(attach_algo_ords),
        };
        //下单
        let result = OkxTrade::from_env()
            .map_err(|e| AppError::OkxApiError(e.to_string()))?
            .place_order(order_params)
            .await
            .map_err(|e| {
                error!("place order error: {:?}", e);
                AppError::OkxApiError(e.to_string())
            })?;
        // {"code":"0","data":[{"clOrdId":"","ordId":"1570389280202194944","sCode":"0","sMsg":"Order placed","tag":"","ts":"1719303647602"}],"inTime":"1719303647601726","msg":"","outTime":"1719303647603880"}
        info!("send order request okx result: {:?}", result);
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::{
        BasicRiskStrategyConfig, SignalResult, StopLossCalculator, StopLossSide, SwapOrderService,
    };
    use okx::dto::account_dto::Position;
    use okx::dto::common::Side;
    use okx::dto::PositionSide;
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};
    const TEST_DIRECT_CONFIRM_ENV: &str = "LEGACY_DIRECT_LIVE_ORDER_CONFIRM";
    /// 封装当前函数，减少交易执行调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
    struct EnvSnapshot {
        /// direct确认标记；为空时表示该条件不启用。
        direct_confirm: Option<String>,
    }
    impl EnvSnapshot {
        /// 提供capture的集中实现，避免交易执行调用方重复处理相同细节。
        fn capture() -> Self {
            Self {
                direct_confirm: std::env::var(TEST_DIRECT_CONFIRM_ENV).ok(),
            }
        }
    }
    impl Drop for EnvSnapshot {
        /// 封装释放，减少交易执行调用方重复实现相同细节。
        fn drop(&mut self) {
            match &self.direct_confirm {
                Some(value) => std::env::set_var(TEST_DIRECT_CONFIRM_ENV, value),
                None => std::env::remove_var(TEST_DIRECT_CONFIRM_ENV),
            }
        }
    }
    /// 判断 交易执行与风控 条件是否满足，给上层流程提供布尔决策。
    fn allow_legacy_direct_live_order_for_test() -> EnvSnapshot {
        let snapshot = EnvSnapshot::capture();
        std::env::set_var(
            TEST_DIRECT_CONFIRM_ENV,
            "I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS",
        );
        snapshot
    }
    #[test]
    fn select_tightest_short_stop() {
        let entry = 100.0;
        let candidates = vec![105.0, 103.0, 110.0];
        let selected = StopLossCalculator::select(StopLossSide::Short, entry, &candidates);
        assert_eq!(selected, Some(103.0));
    }
    /// 提供OKX仓位的集中实现，避免交易执行调用方重复处理相同细节。
    fn okx_position(inst_id: &str, pos_side: &str, pos: &str) -> Position {
        serde_json::from_value(json!({
            "instType": "SWAP",
            "mgnMode": "isolated",
            "posId": "test-position",
            "posSide": pos_side,
            "pos": pos,
            "instId": inst_id
        }))
        .expect("test position payload should deserialize")
    }
    #[tokio::test]
    async fn ready_close_order_propagates_close_guard_failure_for_reverse_position() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _snapshot = EnvSnapshot::capture();
        std::env::remove_var(TEST_DIRECT_CONFIRM_ENV);
        let position = okx_position("ETH-USDT-SWAP", "short", "1");
        let error = SwapOrderService::new()
            .async_ready_close_order("ETH-USDT-SWAP", "4h", &[position], &PositionSide::Short)
            .await
            .expect_err("reverse position close failure must block opening the next order");
        let message = error.to_string();
        assert!(message.contains("LEGACY_DIRECT_LIVE_ORDER_CONFIRM"));
        assert!(message.contains("I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS"));
    }
    #[tokio::test]
    async fn legacy_swap_order_requires_confirmation_before_okx_env() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _snapshot = EnvSnapshot::capture();
        std::env::remove_var(TEST_DIRECT_CONFIRM_ENV);
        let signal = SignalResult {
            should_buy: true,
            open_price: 100.0,
            atr_take_profit_level_1: Some(104.0),
            ..Default::default()
        };
        let risk_config = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            is_used_signal_k_line_stop_loss: Some(false),
            ..Default::default()
        };
        let error = super::SwapOrderService::new()
            .order_swap(
                "ETH-USDT-SWAP",
                "legacy-test-order".to_string(),
                Side::Buy,
                PositionSide::Long,
                100.0,
                "1".to_string(),
                &signal,
                &risk_config,
            )
            .await
            .expect_err("legacy direct swap order must be blocked without confirmation");
        let message = error.to_string();
        assert!(message.contains("LEGACY_DIRECT_LIVE_ORDER_CONFIRM"));
        assert!(message.contains("I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS"));
    }
    #[tokio::test]
    async fn legacy_swap_order_rejects_long_out_of_range_max_loss_percent_before_okx_env() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _snapshot = allow_legacy_direct_live_order_for_test();
        let signal = SignalResult {
            should_buy: true,
            open_price: 100.0,
            ..Default::default()
        };
        let risk_config = BasicRiskStrategyConfig {
            max_loss_percent: 1.5,
            is_used_signal_k_line_stop_loss: Some(false),
            ..Default::default()
        };
        let error = SwapOrderService::new()
            .order_swap(
                "ETH-USDT-SWAP",
                "legacy-test-order".to_string(),
                Side::Buy,
                PositionSide::Long,
                100.0,
                "1".to_string(),
                &signal,
                &risk_config,
            )
            .await
            .expect_err("legacy direct swap order must reject out-of-range max loss");
        let message = error.to_string();
        assert!(
            message.contains("max_loss_percent"),
            "unexpected error: {message}"
        );
        assert!(
            !message.contains("OKX"),
            "max-loss guard must run before OKX env/client initialization: {message}"
        );
    }
    #[tokio::test]
    async fn legacy_swap_order_rejects_out_of_range_max_loss_percent_before_okx_env() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _snapshot = allow_legacy_direct_live_order_for_test();
        let signal = SignalResult {
            should_sell: true,
            open_price: 100.0,
            ..Default::default()
        };
        let risk_config = BasicRiskStrategyConfig {
            max_loss_percent: 1.5,
            is_used_signal_k_line_stop_loss: Some(false),
            ..Default::default()
        };
        let error = SwapOrderService::new()
            .order_swap(
                "ETH-USDT-SWAP",
                "legacy-test-order".to_string(),
                Side::Sell,
                PositionSide::Short,
                100.0,
                "1".to_string(),
                &signal,
                &risk_config,
            )
            .await
            .expect_err("legacy direct swap order must reject out-of-range max loss");
        let message = error.to_string();
        assert!(
            message.contains("max_loss_percent"),
            "unexpected error: {message}"
        );
        assert!(
            !message.contains("OKX"),
            "max-loss guard must run before OKX env/client initialization: {message}"
        );
    }
    #[tokio::test]
    async fn legacy_swap_order_rejects_non_positive_entry_before_okx_env() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _snapshot = allow_legacy_direct_live_order_for_test();
        let signal = SignalResult {
            should_buy: true,
            open_price: 0.0,
            ..Default::default()
        };
        let risk_config = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            is_used_signal_k_line_stop_loss: Some(false),
            ..Default::default()
        };
        let error = SwapOrderService::new()
            .order_swap(
                "ETH-USDT-SWAP",
                "legacy-test-order".to_string(),
                Side::Buy,
                PositionSide::Long,
                0.0,
                "1".to_string(),
                &signal,
                &risk_config,
            )
            .await
            .expect_err("legacy direct swap order must reject non-positive entry");
        let message = error.to_string();
        assert!(
            message.contains("entry_price"),
            "unexpected error: {message}"
        );
        assert!(
            !message.contains("OKX"),
            "entry guard must run before OKX env/client initialization: {message}"
        );
    }
}
