use crate::app_config;
use anyhow::{anyhow, Result};
use rbatis::rbdc::Error;
use rbatis::RBatis;
use rbatis::{crud, impl_select, impl_select_page};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::debug;

// 假设您在app_config里有db::get_db_client()方法
// 也可以根据自己项目的结构，修改相应的导入路径

/// 与 `taker_volume` 表对应的实体结构
///
/// CREATE TABLE `taker_volume` (
///   `id` int NOT NULL,
///   `ts` bigint NOT NULL COMMENT '时间',
///   `inst_id` varchar(20) NOT NULL COMMENT '产品id',
///   `period` varchar(5) NOT NULL COMMENT '周期',
///   `sell_vol` varchar(255) NOT NULL COMMENT '卖出量',
///   `buy_vol` varchar(255) NOT NULL COMMENT '买入量',
///   `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
///   PRIMARY KEY (`id`)
/// ) ...
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct TakerVolumeContractEntity {
    // 主键ID
    pub id: i32,
    // 时间（Unix时间戳，毫秒或秒可根据需要）
    pub ts: i64,
    // 产品id
    pub inst_id: String,
    // 周期
    pub period: String,
    // 卖出量
    pub sell_vol: String,
    // 买入量
    pub buy_vol: String,
    // 创建时间
    pub created_at: Option<String>,
}
pub type ModelEntity = TakerVolumeContractEntity;
struct CountResult {
    count: i64,
}
// 使用 rbatis 提供的 CRUD 宏，为 TakerVolumeEntity 实现基础 CRUD 操作
crud!(ModelEntity {}, "taker_volume_contract");

// 如果需要自定义的查询/更新语句，也可用 impl_select/impl_update 宏
// 这里演示一个fetch_list()，可获取表内所有行
impl_select!(ModelEntity{fetch_list() => ""},"taker_volume_contract");

// 可自定义带条件的查询，示例:
impl_select!(ModelEntity{select_by_inst_id(inst_id: &str,period:&str ) => "`where inst_id = #{inst_id} and period= #{period}  limit 1 `"},"taker_volume_contract");

// 若需要自定义更新，也可这样写:
// impl_update!(TakerVolumeEntity{update_by_id(id:i32) => "`where id = #{id}`"},"taker_volume");

// 可自定义带条件的查询，示例:
impl_select!(ModelEntity{select_new_one_data(inst_id: &str,period:&str ) ->Option=> " `where inst_id = #{inst_id} and period= #{period}  order by ts desc  limit 1 "},"taker_volume_contract");

impl_select!(ModelEntity{select_older_one_data(inst_id: &str,period:&str ) ->Option=> " `where inst_id = #{inst_id} and period= #{period}  order by ts asc  limit 1 "},"taker_volume_contract");

// impl_select!(TakerVolumeEntity{
//     // 函数名: select_count_by_ccy
//     // 参数: ccy:&str
//     // SQL: SELECT count(1) FROM asset WHERE ccy = #{ccy}
//     count_new(inst_id: &str,period:&str )->i64 => " select count(1) from take_volume `where inst_id = #{inst_id} and period= #{period}"},"taker_volume");

pub struct TakerVolumeContractModel {
    db: &'static RBatis,
}

impl TakerVolumeContractModel {}

impl TakerVolumeContractModel {
    /// 初始化model
    pub async fn new() -> Self {
        Self {
            db: app_config::db::get_db_client(),
        }
    }

    pub async fn get_oldest_one_data(
        &self,
        inst_id: &str,
        time_interval: &str,
    ) -> Result<Option<ModelEntity>> {
        let res = ModelEntity::select_older_one_data(self.db, inst_id, time_interval).await;
        match res {
            Ok(list) => Ok(list),
            Err(_) => Err(anyhow!("获取数据库数据异常")),
        }
    }

    /// 批量插入
    pub async fn add_list(&self, list: &Vec<ModelEntity>) -> Result<()> {
        // insert_batch是CRUD宏生成的方法之一
        let data = ModelEntity::insert_batch(self.db, list, list.len() as u64).await?;
        println!("insert_batch = {}", json!(data));
        Ok(())
    }

    // 更多自定义操作 ...
    // 示例: 根据 inst_id 查询
    // pub async fn select_by_inst_id(&self, inst_id: &str) -> anyhow::Result<Vec<TakerVolumeEntity>> {
    //     let res = TakerVolumeEntity::select_by_inst_id(self.db, inst_id).await;
    //     match res {
    //         Ok(list) => Ok(list),
    //         Err(_) => Err(anyhow!("hhahh")),
    //     }
    // }
    // 更多自定义操作 ...
    // 示例: 根据 inst_id 查询
    pub async fn get_new_one_data(
        &self,
        inst_id: &str,
        period: &str,
    ) -> anyhow::Result<Option<ModelEntity>> {
        let res = ModelEntity::select_new_one_data(self.db, inst_id, period).await;
        match res {
            Ok(list) => Ok(list),
            Err(_) => Err(anyhow!("db error ")),
        }
    }
    pub async fn get_new_count(&self, inst_id: &str, period: &str) -> Result<u64> {
        let mut query = format!(
            "select count(1) from {} where inst_id = '{}' AND period = '{}' ;",
            "taker_volume_contract", inst_id, period
        );
        debug!("query: {}", query);
        let res: Result<u64, Error> = self.db.query_decode(&query, vec![]).await;
        match res {
            Ok(list) => Ok(list),
            Err(_) => Err(anyhow!("db error ")),
        }
    }
}
