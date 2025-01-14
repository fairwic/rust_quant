use crate::app_config;
use crate::trading::model::market::candles::{SelectTime, TimeDirect};
use anyhow::{anyhow, Result};
use rbatis::rbdc::Error;
use rbatis::RBatis;
use rbatis::{crud, impl_select, impl_select_page};
use rbs::Value;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info};

/// 与 `top_contract_position_ratio` 表对应的实体结构
///
// CREATE TABLE `top_contract_position_ratio` (
// `id` int NOT NULL AUTO_INCREMENT,
// `inst_id` varchar(255) NOT NULL COMMENT '产品id',
// `period` varchar(255) NOT NULL COMMENT '周期',
// `ts` bigint NOT NULL COMMENT '毫秒级时间戳',
// `long_short_pos_aatio` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '多空仓位占总持仓比值',
// `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
// PRIMARY KEY (`id`),
// UNIQUE KEY `inst_id` (`inst_id`,`period`,`ts`)
// ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;
/// ) ...
#[derive(Serialize, Deserialize, Debug,Clone)]
#[serde(rename_all = "snake_case")]
pub struct TopContractPositionRatioEntity {
    // 主键ID
    pub id: i32,
    // 时间（Unix时间戳，毫秒或秒可根据需要）
    pub ts: i64,
    // 产品id
    pub inst_id: String,
    // 周期
    pub period: String,
    // 买入量
    pub long_short_pos_ratio: String,
    // 创建时间
    pub created_at: Option<String>,
}
pub type ModelEntity = TopContractPositionRatioEntity;
struct CountResult {
    count: i64,
}
// 使用 rbatis 提供的 CRUD 宏，为 TakerVolumeEntity 实现基础 CRUD 操作
crud!(ModelEntity {}, "top_contract_position_ratio");

// 如果需要自定义的查询/更新语句，也可用 impl_select/impl_update 宏
// 这里演示一个fetch_list()，可获取表内所有行
impl_select!(ModelEntity{fetch_list() => ""},"top_contract_position_ratio");

// 可自定义带条件的查询，示例:
impl_select!(ModelEntity{select_by_inst_id(inst_id: &str,period:&str ) => "`where inst_id = #{inst_id} and period= #{period}  limit 1 `"},"top_contract_position_ratio");

// 若需要自定义更新，也可这样写:
// impl_update!(TakerVolumeEntity{update_by_id(id:i32) => "`where id = #{id}`"},"taker_volume");

// 可自定义带条件的查询，示例:
impl_select!(ModelEntity{select_new_one_data(inst_id: &str,period:&str ) ->Option=> " `where inst_id = #{inst_id} and period= #{period}  order by ts desc  limit 1 "},"top_contract_position_ratio");

impl_select!(ModelEntity{select_older_one_data(inst_id: &str,period:&str ) ->Option=> " `where inst_id = #{inst_id} and period= #{period}  order by ts asc  limit 1 "},"top_contract_position_ratio");

pub struct TopContractPositionRatioModel {
    db: &'static RBatis,
}

impl TopContractPositionRatioModel {
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
            "top_contract_position_ratio", inst_id, period
        );
        debug!("query: {}", query);
        let res: Result<u64, Error> = self.db.query_decode(&query, vec![]).await;
        match res {
            Ok(list) => Ok(list),
            Err(_) => Err(anyhow!("db error ")),
        }
    }

    pub async fn get_all(
        &self,
        inst_id: &str,
        time_interval: &str,
        limit: usize,
        select_time: Option<SelectTime>,
    ) -> Result<Vec<ModelEntity>> {
        let mut query = format!("select * from `{}` ", "top_contract_position_ratio");
        //如果指定了时间
        if let Some(SelectTime { direct, point_time }) = select_time {
            match direct {
                TimeDirect::BEFORE => {
                    query = format!("{} where ts<= {} ", query, point_time);
                }
                TimeDirect::AFTER => {
                    query = format!("{} where ts>= {} ", query, point_time);
                }
            }
        }
        //默认取最后的条数
        query = format!("{} order by ts DESC limit {}", query, limit);
        info!("query  SQL: {}", query);
        let res: Value = self.db.query(&query, vec![]).await?;
        if res.is_array() && res.as_array().unwrap().is_empty() {
            info!("No data found in MySQL");
            return Ok(vec![]);
        }
        // 将 rbatis::core::value::Value 转换为 serde_json::Value
        let json_value: serde_json::Value = serde_json::from_str(&res.to_string())?;
        // 将 serde_json::Value 转换为 Vec<CandlesEntity>
        let candles: Vec<ModelEntity> = serde_json::from_value(json_value)?;
        Ok(candles)
    }
}
