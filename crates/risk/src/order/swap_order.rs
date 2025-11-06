extern crate rbatis;

use std::convert::TryInto;
use std::sync::Arc;

use crate::app_config::db;
use crate::time_util;
use chrono::{DateTime, Utc};
use rbatis::impl_select;
use rbatis::rbdc::db::ExecResult;
use rbatis::{crud, impl_update, RBatis};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

/// table
#[derive(Serialize, Deserialize, Debug, Clone)]
// #[serde(rename_all = "camelCase")]
#[serde(rename_all = "snake_case")]
pub struct SwapOrderEntity {
    // 内部订单id
    pub in_order_id: String,
    // 外部订单id
    pub out_order_id: String,
    // 策略id
    pub strategy_id: i64,
    // 策略类型
    pub strategy_type: String,
    // 周期
    pub period: String,
    // 交易对
    pub inst_id: String,
    // 方向
    pub side: String,
    // 持仓数量
    pub pos_size: String,
    // 持仓方向
    pub pos_side: String,
    // 订单标签
    pub tag: String,
    // 订单详情
    pub detail: String,
    // 平台类型
    pub platform_type: String, //okx,binance,huobi,bitget,
}

impl SwapOrderEntity {
    // 生成订单id
    pub fn gen_order_id(inst_id: &str, period: &str, side: &str, pos_side: &str) -> String {
        let time = time_util::format_to_period_str(period);
        //btc-1d-buy-l-20250710000000
        //eth-1h-sell-s-20250710150000
        //nxpc-1s-buy-s-20250710150101
        //doge-1m-sell-l-20250710150100
        //btc-usdt-swap 只保留 btc
        let inst_id = inst_id.to_lowercase();
        let inst_id = inst_id.split("-").nth(0).unwrap();
        let side = if side == "buy" { "b" } else { "s" };
        let pos_side = if pos_side == "long" { "l" } else { "s" };
        format!("{}{}{}{}{}", inst_id, period, side, pos_side, time)
    }
}

crud!(SwapOrderEntity {}, "swap_orders"); //crud = insert+select_by_column+update_by_column+delete_by_column

impl_select!(SwapOrderEntity{select_by_in_order_id(in_order_id:String) => "`where in_order_id = #{in_order_id}`"},"swap_orders");
impl_select!(SwapOrderEntity{fetch_list() => ""},"swap_orders");

#[derive(Debug)]
enum TimeInterval {
    OneDay,
    OneHour,
    // 其他时间类型可以在这里添加
}

impl TimeInterval {
    fn table_name(&self) -> &'static str {
        match self {
            TimeInterval::OneDay => "btc_candles_1d",
            TimeInterval::OneHour => "btc_candles_1h",
            // 其他时间类型映射到对应的表名
        }
    }
}

pub struct SwapOrderEntityModel {
    db: &'static RBatis,
}

impl SwapOrderEntityModel {
    pub async fn new() -> Self {
        Self {
            db: db::get_db_client(),
        }
    }

    pub async fn add(&self, swap_order_entity: &SwapOrderEntity) -> anyhow::Result<ExecResult> {
        let data = SwapOrderEntity::insert(self.db, &swap_order_entity).await?;
        info!("insert_batch = {}", json!(data));
        Ok(data)
    }
    pub async fn query_one(
        &self,
        inst_id: &str,
        time: &str,
        side: &str,
        pos_side: &str,
    ) -> anyhow::Result<Vec<SwapOrderEntity>> {
        let in_ord_id = SwapOrderEntity::gen_order_id(inst_id, time, side, pos_side);
        let data = SwapOrderEntity::select_by_in_order_id(self.db, in_ord_id).await?;
        Ok(data)
    }
    //
    // pub async fn update(&self, ticker: &TickersData) -> anyhow::Result<()> {
    //     let tickets_data = SwapOrderEntity {
    //         inst_type: ticker.inst_type.clone(),
    //         inst_id: ticker.inst_id.clone(),
    //         last: ticker.last.clone(),
    //         last_sz: ticker.last_sz.clone(),
    //         ask_px: ticker.ask_px.clone(),
    //         ask_sz: ticker.ask_sz.clone(),
    //         bid_px: ticker.bid_px.clone(),
    //         bid_sz: ticker.bid_sz.clone(),
    //         open24h: ticker.open24h.clone(),
    //         high24h: ticker.high24h.clone(),
    //         low24h: ticker.low24h.clone(),
    //         vol_ccy24h: ticker.vol_ccy24h.clone(),
    //         vol24h: ticker.vol24h.clone(),
    //         sod_utc0: ticker.sod_utc0.clone(),
    //         sod_utc8: ticker.sod_utc8.clone(),
    //         ts: ticker.ts.parse().unwrap(),
    //     };
    //     let data = SwapOrderEntity::update_by_column(&self.db, &tickets_data, "inst_id").await;
    //     println!("update_by_column = {}", json!(data));
    //     // let data = TickersDataDb::update_by_name(&self.db, &tickets_data, ticker.inst_id.clone()).await;
    //     // println!("update_by_name = {}", json!(data));
    //     Ok(())
    // }
    // /*获取全部*/
    // pub async fn get_all(&self, inst_ids: Option<&Vec<&str>>) -> Result<Vec<SwapOrderEntity>> {
    //     let sql = if let Some(inst_ids) = inst_ids {
    //         format!(
    //             "SELECT * FROM tickers_data WHERE inst_id IN ({}) and inst_type='SWAP' ORDER BY id DESC",
    //             inst_ids.iter().map(|id| format!("'{}'", id)).collect::<Vec<String>>().join(", ")
    //         )
    //     } else {
    //         "SELECT * FROM tickers_data ORDER BY id DESC".to_string()
    //     };
    //
    //     let results: Vec<SwapOrderEntity> = self.db.query_decode(sql.as_str(), vec![]).await?;
    //     Ok(results)
    // }
}
