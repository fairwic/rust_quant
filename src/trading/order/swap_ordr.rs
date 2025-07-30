use crate::trading::constants;
use crate::trading::model::order::swap_order::{SwapOrderEntity, SwapOrderEntityModel};
use crate::trading::strategy::StrategyType;
use log::{error, warn};

use std::cmp::PartialEq;

use crate::time_util::{self, now_timestamp_mills};

pub struct OrderSignal {
    pub inst_id: String,
    pub should_sell: bool,
    pub price: f64,
}
use crate::error::app_error::AppError;
use crate::trading::strategy::strategy_common::{BasicRiskStrategyConfig, SignalResult};
use chrono::Local;
use core::time;
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::{Position, TradingSwapNumResponseData};
use okx::dto::common::Side;
use okx::dto::trade::trade_dto::{
    AttachAlgoOrdReqDto, OrderReqDto, OrderResDto, TdModeEnum, TpOrdKindEnum,
};
use okx::dto::trade_dto::{CloseOrderReqDto, OrdTypeEnum};
use okx::dto::PositionSide;
use okx::{Error, OkxAccount, OkxClient, OkxTrade};
use serde::de;
use serde_json::json;
use tracing::{debug, info};
pub struct SwapOrder {}
impl SwapOrder {
    pub fn new() -> Self {
        Self {}
    }
    //下单现货
    pub async fn place_order_spot(
        &self,
        inst_id: &str,
        side: Side,
        px: f64,
    ) -> Result<Vec<OrderResDto>, Error> {
        //todo 获取当前可以开仓的数量
        let sz = 1;
        //todo 设置止盈止损
        let px = 3000.00;

        let order_params = OrderReqDto {
            inst_id: inst_id.to_string(),
            td_mode: TdModeEnum::CASH.to_string(),
            side: side.to_string(),
            ord_type: OrdTypeEnum::LIMIT.to_string(),
            sz: sz.to_string(),
            px: Option::from(px.to_string()),
            reduce_only: Some(false),
            stp_mode: Some("cancel_maker".to_string()),
            attach_algo_ords: Some(vec![AttachAlgoOrdReqDto {
                attach_algo_cl_ord_id: None,
                tp_trigger_px: Some("3500".to_string()),
                tp_ord_px: Some("-1".to_string()),
                tp_ord_kind: Some(TpOrdKindEnum::CONDITION.to_string()),
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
        position_list: Vec<Position>,
        inst_id: &str,
        pos_side: PositionSide,
    ) -> Result<bool, Error> {
        let already_have_position = position_list.len() > 0;
        //是否已经有反向仓位
        let mut have_another_position = false;
        if already_have_position {
            // let position = position_list.get(0).unwrap();
            for position in position_list.into_iter() {
                //且持仓量不为0
                if position.inst_id == inst_id && position.pos != "0" {
                    //如果当前仓位是反向仓位，则平仓
                    if position.pos_side == pos_side.to_string() {
                        let params = CloseOrderReqDto {
                            inst_id: inst_id.to_string(),
                            pos_side: Option::from(pos_side.to_string()),
                            mgn_mode: position.mgn_mode,
                            ccy: None,
                            auto_cxl: Some(true), //自动撤单
                            cl_ord_id: None,
                            tag: None,
                        };
                        let res = OkxTrade::from_env()?.close_position(params).await?;
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

    /// 获取下单数量
    pub fn get_place_order_num(&self, valid_num: &TradingSwapNumResponseData) -> String {
        format!(
            "{}",
            (valid_num.max_buy.parse::<f64>().unwrap() / 1.1).floor()
        )
    }
    /// 准备下单
    pub async fn ready_to_order(
        &self,
        strategy_type: StrategyType,
        inst_id: &str,
        period: &str,
        signal: SignalResult,
        risk_config: BasicRiskStrategyConfig,
        strategy_config_id: i64,
    ) -> Result<(), AppError> {
        // 获取当前仓位状态
        let account = OkxAccount::from_env().map_err(|e| {
            error!("create okx account client error: {:?}", e);
            AppError::OkxApiError(e.to_string())
        })?;

        //todo 如有反向的仓位，应该开启异步去立即关闭
        let position_list = account
            .get_account_positions(Some("SWAP"), Some(inst_id), None)
            .await
            .map_err(|e| {
                error!("get position error: {:?}", e);
                AppError::OkxApiError(e.to_string())
            })?;
        info!(
            "current okx position_list: {:?}",
            json!(position_list).to_string()
        );
        let max_avail_size = account
            .get_max_size(inst_id, &TdModeEnum::CROSS.to_string(), None, None, None)
            .await
            .map_err(|e| {
                error!("get max size error: {:?}", e);
                AppError::OkxApiError(e.to_string())
            })?;

        if max_avail_size.len() == 0 || max_avail_size[0].inst_id != inst_id.to_string() {
            error!("max_avail_size is empty or inst_id not match");
            return Err(AppError::BizError(
                "max_avail_size is empty or inst_id not match".to_string(),
            ));
        }
        let trad_swap_nums = max_avail_size[0].clone();
        info!("max_avail_size: {:?}", trad_swap_nums);
        // 处理下单数量
        let pos_size = SwapOrder::new().get_place_order_num(&trad_swap_nums);

        // if pos_size.parse::<f64>().unwrap() < 1.0 {
        //     error!("pos_size is  small than 1.0, not enough to place order");
        //     return Err(AppError::BizError(
        //         "pos_size is  small than 1.0, not enough to place order".to_string(),
        //     ));
        // }
        let pos_size = "2.0".to_string();

        info!("ready to place order size: {:?}", pos_size);
        //平掉现有的已经存在的反向仓位
        let pos_side = match signal.should_buy {
            true => PositionSide::Short,
            false => PositionSide::Long,
        };
        self.sync_close_order(inst_id, period, position_list, pos_side)
            .await
            .map_err(|e| {
                error!("close position error: {:?}", e);
                AppError::OkxApiError(e.to_string())
            })?;

        let (order_result, side, pos_side) = if signal.should_buy {
            //生成in_order_id
            let in_order_id = SwapOrderEntity::gen_order_id(
                inst_id,
                period,
                Side::Buy.to_string(),
                PositionSide::Long.to_string(),
            );
            (
                SwapOrder::new()
                    .start_to_order(
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
            //生成in_order_id
            let in_order_id = SwapOrderEntity::gen_order_id(
                inst_id,
                period,
                Side::Buy.to_string(),
                PositionSide::Short.to_string(),
            );
            (
                SwapOrder::new()
                    .start_to_order(
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
            SwapOrder::new()
                .record_order(
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
    pub async fn sync_close_order(
        &self,
        inst_id: &str,
        time: &str,
        position_list: Vec<Position>,
        close_pos_side: PositionSide,
    ) -> Result<(), AppError> {
        // todo 异步平掉现有的已经存在的反向仓位
        self.close_position(position_list, inst_id, close_pos_side)
            .await
            .map_err(|e| {
                error!("close position error: {:?}", e);
                AppError::OkxApiError(e.to_string())
            })?;
        Ok(())
    }
    /// 开始下单
    pub async fn start_to_order(
        &self,
        inst_id: &str,
        time: &str,
        in_order_id: String,
        close_pos_side: PositionSide,
        side: Side,
        pos_side: PositionSide,
        pos_size: String,
        signal: SignalResult,
        strategy_type: StrategyType,
        risk_config: BasicRiskStrategyConfig,
    ) -> Result<Vec<OrderResDto>, AppError> {
        //判断相同周期下是否已经有了订单
        let swap_order_list = SwapOrderEntityModel::new()
            .await
            .query_one(inst_id, time, side.to_string(), pos_side.to_string())
            .await
            .map_err(|e| {
                error!("get swap order list error: {:?}", e);
                AppError::DbError(e.to_string())
            })?;
        if swap_order_list.len() > 0 {
            info!("same period same inst_id same side, already have order");
            return Ok(vec![]);
        }
        // 判断当下需要下单的时间，是不是不在交易设定的时间范围内
        // let can_order = time_util::is_within_business_hours(ts);
        // if !can_order {
        //     warn!("time is not within business hours or in saturday");
        //     return None;
        // }
        let price = signal.open_price;
        let ts = signal.ts;

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

    pub async fn record_order(
        &self,
        strategy_type: StrategyType,
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
            let swap_order_entity = SwapOrderEntity {
                strategy_id,
                in_order_id: order.cl_ord_id.clone().unwrap_or("".to_string()),
                strategy_type: strategy_type.to_string(),
                period: time.to_string(),
                inst_id: inst_id.to_string(),
                side: side.to_string(),
                pos_side: pos_side.to_string(),
                pos_size: pos_size.clone(),
                out_order_id: order.ord_id.to_string(),
                tag: "".to_string(),
                detail: json!(order).to_string(),
                platform_type: "okx".to_string(),
            };
            let res = SwapOrderEntityModel::new()
                .await
                .add(&swap_order_entity)
                .await;
            if res.is_err() {
                error!("record order error: {:?} {:?}", res, swap_order_entity);
            }
        }
        Ok(())
    }

    pub fn generate_fibonacci_take_profit_orders(
        &self,
        entry_price: f64,
        stop_loss_price: f64,
        size: &str,
        side: &Side,
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
        let order = AttachAlgoOrdReqDto::new(
            None,                                    // 止盈触发价
            None,                                    // 止盈委托价 -1 表示市价
            Some(format!("{:.2}", stop_loss_price)), // 止损触发价
            Some("-1".to_string()),                  // 止损委托价 -1 表示市价
            size.to_string(),
        );
        orders.push(order);
        orders
    }

    //判断开仓产品是否时btc
    pub fn is_btc_swap(&self, inst_id: &str) -> bool {
        inst_id.contains(constants::common_enums::BTC_SWAP_INST_ID)
    }

    //下单合约
    pub async fn order_swap(
        &self,
        inst_id: &str,
        in_order_id: String,
        side: Side,
        pos_side: PositionSide,
        entry_price: f64,
        size: String,
        signal: SignalResult,
        risk_config: BasicRiskStrategyConfig,
    ) -> Result<Vec<OrderResDto>, AppError> {
        //最大止盈
        println!("risk_config: {:?}", risk_config);
        println!("signal: {:?}", signal);
        let max_loss_percent = risk_config.max_loss_percent;
        let mut stop_loss_price: f64 = match side {
            Side::Sell => entry_price * (1.0 + max_loss_percent),
            Side::Buy => entry_price * (1.0 - max_loss_percent),
        };
        //如果使用信号k线止盈，则使用信号k线止盈
        if risk_config.is_used_signal_k_line_stop_loss
            && signal.signal_kline_stop_loss_price.is_some()
        {
            stop_loss_price = signal.signal_kline_stop_loss_price.unwrap();
        }
        //valid 如果是做空，开仓价格要<止损价格,否则不进行下单
        //valid 如果是做多，开仓价格要>止损价格,否则不进行下单
        if pos_side == PositionSide::Short && entry_price > stop_loss_price {
            error!("entry_price > stop_loss_price, not place order");
            return Err(AppError::BizError(
                "entry_price > stop_loss_price, not place order".to_string(),
            ));
        }
        if pos_side == PositionSide::Long && entry_price < stop_loss_price {
            error!("entry_price < stop_loss_price, not place order");
            return Err(AppError::BizError(
                "entry_price < stop_loss_price, not place order".to_string(),
            ));
        }
        //todo 确保最大止损不会触发爆仓
        let attach_algo_ords = SwapOrder::new().generate_fibonacci_take_profit_orders(
            entry_price,
            stop_loss_price,
            &size,
            &side,
        );
        println!("place order attach_algo_ords{:?}", attach_algo_ords);

        let order_params = OrderReqDto {
            inst_id: inst_id.to_string(),
            td_mode: TdModeEnum::ISOLATED.to_string(),
            ccy: None,
            cl_ord_id: Some(in_order_id),
            tag: None,
            side: side.to_string(),
            pos_side: Option::from(pos_side.to_string()),
            // pos_side: None,
            ord_type: OrdTypeEnum::MARKET.to_string(),
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
        let result = OkxTrade::from_env()?
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
