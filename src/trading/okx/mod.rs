use serde::{Deserialize, Serialize};

pub mod okx_client;
pub mod public_data;
pub mod account;
pub mod trade;
pub mod market;


// 通用的响应结构体
#[derive(Serialize, Deserialize, Debug)]
pub struct OkxApiResponse<T> {
    code: String,
    msg: String,
    data: T,
}
