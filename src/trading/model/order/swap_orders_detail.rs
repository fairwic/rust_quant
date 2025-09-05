extern crate rbatis;

use std::convert::TryInto;
use std::sync::Arc;

use crate::app_config::db;
use crate::time_util;
use chrono::{DateTime, Utc};
use okx::dto::trade_dto::OrderDetailRespDto;
use rbatis::impl_select;
use rbatis::rbdc::db::ExecResult;
use rbatis::{crud, impl_update, RBatis};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// 合约订单详情表
#[derive(Serialize, Deserialize, Debug, Clone)]
// #[serde(rename_all = "camelCase")]
#[serde(rename_all = "snake_case")]
pub struct SwapOrderDetailEntity {
    // 交易对
    pub inst_id: String,
    // 产品类型
    pub inst_type: String,
    // 订单id
    pub ord_id: String,
    // 内部订单id 唯一
    pub cl_ord_id: String,
    // 委托价格
    pub px: String,
    // 订单标签(时间-策略类型-产品id-周期-side-postside)
    pub tag: String,
    // 委托数量
    pub pos_size: String,
    // 收益
    pub pnl: String,
    // 订单类型
    pub ord_type: String,
    // 方向
    pub side: String,
    // 多、空
    pub pos_side: String,
    // 最新成交数量
    pub fill_sz: String,
    // 最新成交时间
    pub fill_time: String,
    // 成交均价
    pub avg_px: String,
    // 订单状态
    pub state: String,
    // 杠杆倍数
    pub lever: String,
    // 下单附带止盈止损时，客户自定义的策略订单ID
    pub attach_algo_cl_ord_id: String,
    // 止盈触发价
    pub tp_trigger_px: String,
    // 止盈触发价类型
    pub tp_trigger_px_type: String,
    // 止盈委托价格
    pub tp_ord_px: String,
    // 止损触发类型
    pub sl_trigger_px_type: String,
    // 止损触发价格
    pub sl_trigger_px: String,
    // 止损委托价格
    pub sl_ord_px: String,
    // 止盈止损订单id
    pub attach_algo_ords: String,
    // 手续费
    pub fee: String,
    // 订单来源
    pub source: String,
    // 订单种类
    pub category: String,
    // 是否为限价止盈
    pub is_tp_limit: String,
    // 币币市价单委托数量sz的单位
    pub tgt_ccy: String,
    // 订单状态更新时间
    pub u_time: String,
    // 订单创建时间
    pub c_time: String,
    // 创建时间
    pub created_at: Option<String>,
    // 更新时间
    pub update_at: Option<String>,
}

impl SwapOrderDetailEntity {
    pub fn from(order_detail: OrderDetailRespDto) -> Self {
        Self {
            inst_id: order_detail.inst_id,
            side: order_detail.side,
            inst_type: order_detail.inst_type,
            ord_id: order_detail.ord_id,
            cl_ord_id: order_detail.cl_ord_id,
            px: order_detail.px,
            tag: order_detail.tag,
            pos_size: order_detail.sz,
            pnl: order_detail.pnl,
            ord_type: order_detail.ord_type,
            pos_side: order_detail.pos_side,
            fill_sz: order_detail.fill_sz,
            fill_time: order_detail.fill_time,
            avg_px: order_detail.avg_px,
            state: order_detail.state,
            lever: order_detail.lever,
            attach_algo_cl_ord_id: order_detail.attach_algo_cl_ord_id,
            tp_trigger_px: order_detail.tp_trigger_px,
            tp_trigger_px_type: order_detail.tp_trigger_px_type,
            tp_ord_px: order_detail.tp_ord_px,
            sl_trigger_px_type: order_detail.sl_trigger_px_type,
            sl_trigger_px: order_detail.sl_trigger_px,
            sl_ord_px: order_detail.sl_ord_px,
            fee: order_detail.fee,
            attach_algo_ords: json!(order_detail.attach_algo_ords).to_string(),
            source: order_detail.source,
            category: order_detail.category,
            is_tp_limit: order_detail.is_tp_limit,
            tgt_ccy: order_detail.tgt_ccy,
            u_time: order_detail.u_time,
            c_time: order_detail.c_time,
            created_at: None,
            update_at: None,
        }
    }
}

const TABLE_NAME: &str = "swap_orders_detail";
crud!(SwapOrderDetailEntity {}, TABLE_NAME);
impl_select!(SwapOrderDetailEntity{select_by_in_order_id(in_order_id:String) => "`where in_order_id = #{in_order_id}`"},TABLE_NAME);
impl_select!(SwapOrderDetailEntity{fetch_list() => ""},TABLE_NAME);

///模型结构体
pub struct SwapOrderDetailEntityModel {
    db: &'static RBatis,
}

impl SwapOrderDetailEntityModel {
    pub async fn new() -> Self {
        Self {
            db: db::get_db_client(),
        }
    }

    pub async fn add(
        &self,
        swap_order_entity: &SwapOrderDetailEntity,
    ) -> anyhow::Result<ExecResult> {
        let data = SwapOrderDetailEntity::insert(self.db, &swap_order_entity).await?;
        println!("insert_batch = {}", json!(data));
        Ok(data)
    }

    // pub async fn batch_update(
    //     &self,
    //     swap_order_entity: &SwapOrderDetailEntity,
    // ) -> anyhow::Result<ExecResult> {
    //     let data = SwapOrderDetailEntity::update_by_column_batch(
    //         self.db,
    //         &[swap_order_entity],
    //         "in_order_id",
    //         1,
    //     )
    //     .await?;
    //     println!("update_batch = {}", json!(data));
    //     Ok(data)
    // }
    pub async fn update(
        &self,
        swap_order_entity: &SwapOrderDetailEntity,
    ) -> anyhow::Result<ExecResult> {
        let data =
            SwapOrderDetailEntity::update_by_column(self.db, &swap_order_entity, "in_order_id")
                .await?;
        println!("update_batch = {}", json!(data));
        Ok(data)
    }
}
