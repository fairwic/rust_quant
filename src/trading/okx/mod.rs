use serde::{Deserialize, Serialize};

pub mod okx_client;
pub mod public_data;
pub mod account;
pub mod trade;
pub mod market;
pub mod okx_websocket;
pub mod okx_websocket_client;
pub mod asset;


// 通用的响应结构体
#[derive(Serialize, Deserialize, Debug)]
pub struct OkxApiResponse<T> {
    code: String,
    msg: String,
    data: T,
}
