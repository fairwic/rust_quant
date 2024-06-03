extern crate rbatis;
use rbatis::{crud, impl_insert, impl_update, RBatis};
use rbatis::rbdc::DateTime;
use serde_json::json;
use crate::trading::model::Db;

/// table
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BizActivity {
    pub id: Option<String>,
    pub name: Option<String>,
    pub pc_link: Option<String>,
    pub h5_link: Option<String>,
    pub pc_banner_img: Option<String>,
    pub h5_banner_img: Option<String>,
    pub sort: Option<String>,
    pub status: Option<i32>,
    pub remark: Option<String>,
    pub create_time: Option<DateTime>,
    pub version: Option<i64>,
    pub delete_flag: Option<i32>,
}

crud!(BizActivity{});
impl_update!(BizActivity{update_by_name(name:&str) => "`where id = '2'`"});

