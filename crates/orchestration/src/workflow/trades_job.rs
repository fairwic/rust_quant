// 交易量任务
use crate::trading::model::asset::AssetModel;
use rust_quant_market::models::CandlesEntity;
use rust_quant_market::models::CandlesModel;
use crate::trading::model::market::tickers::{TickersDataEntity, TicketsModel};
use rust_quant_strategies::redis_operations::{RedisCandle, RedisOperations};
use chrono::Utc;
use hmac::digest::typenum::op;
use okx::api::market::OkxMarket;
use rbatis::rbatis_codegen::ops::AsProxy;
use rbatis::rbdc::datetime;

use tracing::debug;
use tracing::field::debug;

// pub async fn update_trades_to_redis(inst_id: &str) -> anyhow::Result<()> {
//     let res = CandlesModel::new().await.get_new_data(inst_id).await?;
//     println!("res: {:?}", res);
//     let mut before: i64 = 0;
//     if res.is_none() {
//         before = Utc::now().naive_utc().timestamp_millis();
//     } else {
//         before = res.unwrap().ts;
//     }
//     loop {
//         let res = Market::new().get_candles(&inst_id, , None, Some(&before.to_string()), Some("300")).await?;
//         if res.is_empty() {
//             debug!("No new candles patch{},{}",inst_id, );
//             break;
//             //插入数据
//         }
//         let res = CandlesModel::new().await.add(res, inst_id).await?;
//         let res = CandlesModel::new().await.get_new_data(inst_id).await?;
//         before = res.unwrap().ts;
//     }
//
//     Ok(())
// }
