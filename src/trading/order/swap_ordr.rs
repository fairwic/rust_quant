use crate::trading::model::order::swap_order::{SwapOrderEntity, SwapOrderEntityModel};
use crate::trading::constants;
use crate::trading::strategy::StrategyType;
use log::{error, warn};

use std::cmp::PartialEq;

use crate::time_util::{self, now_timestamp_mills};

pub struct OrderSignal {
    pub inst_id: String,
    pub should_sell: bool,
    pub price: f64,
}
use crate::trading::strategy::strategy_common::SignalResult;
use chrono::Local;
use core::time;
use okx::dto::account_dto::{Position, TradingSwapNumResponseData};
use okx::dto::common::Side;
use okx::dto::trade::trade_dto::{
    AttachAlgoOrdReqDto, OrderReqDto, OrderResData, TdModeEnum, TpOrdKindEnum,
};
use okx::dto::trade_dto::{CloseOrderReqDto, OrdTypeEnum};
use okx::dto::PositionSide;
use okx::{Error, OkxAccount, OkxClient, OkxTrade};
use serde_json::json;
use tracing::{debug, info};
use okx::api::api_trait::OkxApiTrait;
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
    ) -> Result<Vec<OrderResData>, Error> {
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
        position_list: &Vec<Position>,
        inst_id: &str,
        pos_side: PositionSide,
    ) -> Result<bool, Error> {
        let already_have_position = position_list.len() > 0;
        //是否已经有反向仓位
        let mut have_anthor_position = false;
        if already_have_position {
            // let position = position_list.get(0).unwrap();
            for position in position_list {
                if position.inst_id == inst_id {
                    if position.pos_side == pos_side.to_string() {
                        let params = CloseOrderReqDto {
                            inst_id: inst_id.to_string(),
                            pos_side: Option::from(pos_side.to_string()),
                            mgn_mode: TdModeEnum::ISOLATED.to_string(),
                            ccy: None,
                            auto_cxl: Some(true),
                            cl_ord_id: None,
                            tag: None,
                        };
                        let res = OkxTrade::from_env()?
                            .close_position(
                                inst_id,
                                Some(&pos_side.to_string()),
                                &TdModeEnum::ISOLATED.to_string(),
                            )
                            .await?;
                        info!("close  order position result {:?}", res);
                    } else {
                        have_anthor_position = true
                    }
                } else {
                    error!("close order position not match ");
                }
            }
        }
        Ok(have_anthor_position)
    }

    /// 获取下单数量
    pub fn get_place_order_num(
        &self,
        avalid_num: &TradingSwapNumResponseData,
        price: f64,
        pos_side: PositionSide,
    ) -> String {
        let size = match pos_side {
            PositionSide::Long => {
                format!(
                    "{}",
                    (avalid_num.max_buy.parse::<f64>().unwrap() / 3.00).floor()
                )
            }
            PositionSide::Short => {
                format!(
                    "{}",
                    (avalid_num.max_sell.parse::<f64>().unwrap() / 3.00).floor()
                )
            }
            PositionSide::Net => {
                format!(
                    "{}",
                    (avalid_num.max_buy.parse::<f64>().unwrap() / 3.00).floor()
                )
            }
        };
        size.to_string()
    }
    /// 准备下单
    pub async fn ready_to_order(
        &self,
        strategy_type: StrategyType,
        inst_id: &str,
        period: &str,
        signal: SignalResult,
    ) -> Result<(), Error> {
        if signal.should_buy || signal.should_sell {
            // 获取当前仓位状态
            let account = OkxAccount::from_env()?;
            let position_list =
                account.get_account_positions(Some("SWAP"), Some(inst_id), None)
                .await?;

            // 获取可用账户可用数量
            let max_avail_size = account
                .get_max_size(inst_id, &TdModeEnum::ISOLATED.to_string(), None, None, None)
                .await?;

            let (order_result, side, pos_side) = if signal.should_buy {
                (
                    SwapOrder::new()
                        .start_to_order(
                            inst_id,
                            period,
                            &position_list,
                            PositionSide::Short,
                            Side::Buy,
                            PositionSide::Long,
                            &max_avail_size,
                            signal,
                            strategy_type,
                        )
                        .await,
                    Side::Buy,
                    PositionSide::Long,
                )
            } else if signal.should_sell {
                (
                    SwapOrder::new()
                        .start_to_order(
                            inst_id,
                            period,
                            &position_list,
                            PositionSide::Long,
                            Side::Sell,
                            PositionSide::Short,
                            &max_avail_size,
                            signal,
                            strategy_type,
                        )
                        .await,
                    Side::Sell,
                    PositionSide::Short,
                )
            } else {
                (None, Side::Buy, PositionSide::Long) // 默认值，不会被使用
            };

            if let Some(order_result) = order_result {
                // 记录到订单中
                SwapOrder::new()
                    .record_order(strategy_type, inst_id, period, order_result, side, pos_side)
                    .await?;
            }
        } else {
            debug!("signal result does not require order deal");
        }
        Ok(())
    }

    /// 开始下单
    pub async fn start_to_order(
        &self,
        inst_id: &str,
        time: &str,
        position_list: &Vec<Position>,
        close_pos_side: PositionSide,
        side: Side,
        pos_side: PositionSide,
        max_avail_size: &TradingSwapNumResponseData,
        signal: SignalResult,
        strategy_type: StrategyType,
    ) -> Option<Vec<OrderResData>> {
        //判断相同周期下是否已经有了订单
        let swap_order_list = SwapOrderEntityModel::new()
            .await
            .getOne(inst_id, time, side.to_string(), pos_side.to_string())
            .await
            .ok()?;
        if swap_order_list.len() > 0 {
            info!("same period same inst_id same side, already have order");
            return None;
        }
        let price = signal.open_price;
        let ts = signal.ts;

        // 平掉现有的已经存在的反向仓位
        self.close_position(position_list, inst_id, close_pos_side)
            .await
            .ok()?;

        // 判断当下需要下单的时间，是不是不在交易设定的时间范围内
        let can_order = time_util::is_within_business_hours(ts);
        if !can_order {
            warn!("time is not within business hours or in saturday");
            return None;
        }

        //判断是否当前信号类型，并判断是否要下单当前信号方向的单
        //todo 从数据读取当前配置是否更适合开多，还是开空
        match strategy_type {
            StrategyType::UtBoot => {
                // if PosSide::SHORT == pos_side {
                //     warn!("ut boot strategy is short strategy, no need to place order");
                //     return None;
                // }
            }
            _ => {}
        }

        // 获取下单数量
        let size = SwapOrder::new().get_place_order_num(max_avail_size, price, pos_side);
        info!("allot place order size: {:?}", size);

        // 下单
        let order_result = self
            .order_swap(inst_id, side, pos_side, price, size)
            .await
            .ok()?;
        Some(order_result)
    }

    pub async fn record_order(
        &self,
        strategy_type: StrategyType,
        inst_id: &str,
        time: &str,
        order_results: Vec<OrderResData>,
        side: Side,
        pos_side: PositionSide,
    ) -> Result<(),Error> {
        for order in order_results {
            // 下单成功
            let swap_order_entity = SwapOrderEntity {
                uuid: SwapOrderEntity::gen_uuid(
                    inst_id,
                    time,
                    side.to_string(),
                    pos_side.to_string(),
                ),
                strategy_type: strategy_type.to_string(),
                period: time.to_string(),
                inst_id: inst_id.to_string(),
                side: side.to_string(),
                pos_side: pos_side.to_string(),
                cl_ord_id: order.ord_id.to_string(),
                tag: "".to_string(),
                detail: json!(order).to_string(),
            };
            let res=SwapOrderEntityModel::new()
                .await
                .add(swap_order_entity)
                .await;
            if res.is_err() {
                error!("record order error: {:?}", res);
            }
        }
        Ok(())
    }

    pub fn generate_fibonacci_take_profit_orders(
        &self,
        entry_price: f64,
        fib_levels: &[f64],
        stop_loss_price: f64,
        size: &str,
        side: &Side,
    ) -> Vec<AttachAlgoOrdReqDto> {
        let mut orders = Vec::new();

        //止盈
        for level in fib_levels {
            let tp_trigger_px: f64 = match side {
                Side::Sell => entry_price * (1.0 - level),
                Side::Buy => entry_price * (1.0 + level),
            };

            // fn set_to_multiple(value: &mut i32, multiple_of: i32) {
            // *value = (*value / multiple_of) * multiple_of;
            // }

            // let order_size = (size.parse::<f64>().unwrap() * (level / fib_levels.iter().sum::<f64>())).ceil();
            // let order_size_str = format!("{:.2}", order_size);

            // let tp_ord_px = tp_trigger_px - 100.0; // 根据你的需求调整价格
            let order = AttachAlgoOrdReqDto::new(
                format!("{:.2}", tp_trigger_px),
                format!("{:.2}", -1), // 根据你的需求调整价格
                format!("{:.2}", stop_loss_price),
                format!("{:.2}", -1), // 根据你的需求调整价格
                size.to_string(),
            );
            orders.push(order);
        }
        orders
    }

    //判断开仓产品是否时btc
    pub fn is_btc_swap(&self, inst_id: &str) -> bool {
        inst_id.contains(constants::common_enums::BTC_SWAP_INST_ID)
    }
    //生成cl_ord_id
    pub fn get_cl_ord_id(&self) -> String {
        //不超过32位 "2460254286459469824okx"
        format!("{}okx", Local::now().timestamp_micros())
    }

    //下单合约
    pub async fn order_swap(
        &self,
        inst_id: &str,
        side: Side,
        pos_side: PositionSide,
        entry_price: f64,
        size: String,
    ) -> Result<Vec<OrderResData>, Error> {
        //止盈
        let stop_loss_price: f64 = match side {
            Side::Sell => entry_price * (1.0 + 0.02),
            Side::Buy => entry_price * (1.0 - 0.02),
        };

        // let stop_loss_price = entry_price * (1.0 - 0.1);

        //btc永续合约最大止盈10个点，其他永续合约最大止盈23.6个点
        let fib_levels = if self.is_btc_swap(inst_id) {
            [0.120]
        } else {
            [0.236]
        };
        // let size = "1".to_string();
        let attach_algo_ords = SwapOrder::new().generate_fibonacci_take_profit_orders(
            entry_price,
            &fib_levels,
            stop_loss_price,
            &size,
            &side,
        );
        println!("place order attach_algo_ords{:?}", attach_algo_ords);

        let order_params = OrderReqDto {
            inst_id: inst_id.to_string(),
            td_mode: TdModeEnum::ISOLATED.to_string(),
            ccy: None,
            cl_ord_id: Some(SwapOrder::new().get_cl_ord_id()),
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
        let result = OkxTrade::new(OkxClient::from_env()?).place_order(order_params).await;
        // {"code":"0","data":[{"clOrdId":"","ordId":"1570389280202194944","sCode":"0","sMsg":"Order placed","tag":"","ts":"1719303647602"}],"inTime":"1719303647601726","msg":"","outTime":"1719303647603880"}
        info!("send order request okx result: {:?}", result);
        result
    }
}
