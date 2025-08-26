use crate::app_config::db;
use crate::trading::strategy::StrategyType;
use rbatis::impl_select;
use rbatis::rbdc::db::ExecResult;
use rbatis::rbdc::{Date, DateTime};
use rbatis::{crud, impl_insert, impl_update, RBatis};
use serde_json::json;
use std::sync::Arc;
use tracing::debug;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StrategyConfigEntity {
    pub id: i64,
    pub strategy_type: String,
    pub inst_id: String,
    pub time: String,
    pub value: String,
    pub risk_config: String,
    pub kline_start_time: i64,
    pub kline_end_time: i64,
    pub final_fund: f64,
    pub is_deleted: i32,
}

crud!(StrategyConfigEntity {}, "strategy_config");
impl_select!(StrategyConfigEntity{get_all() => "`where is_deleted=0`"},"strategy_config");
impl_update!(StrategyConfigEntity{update_by_name(name:&str) => "`where id = '2'`"},"strategy_config");
impl_select!(StrategyConfigEntity{select_by_strate_type(strategy_type:&str,inst_id:&str,time:&str) =>
    "`where strategy_type=#{strategy_type} and  inst_id = #{inst_id} and time = #{time}`"},"strategy_config");

impl_select!(StrategyConfigEntity{select_by_inst_id(inst_id:&str,time:&str) =>
    "`where inst_id = #{inst_id} and time = #{time}`"},"strategy_config");

pub struct StrategyConfigEntityModel {
    db: &'static RBatis,
}

impl StrategyConfigEntityModel {
    pub async fn new() -> StrategyConfigEntityModel {
        Self {
            db: db::get_db_client(),
        }
    }
    pub async fn add(&self, list: StrategyConfigEntity) -> anyhow::Result<ExecResult> {
        let data = StrategyConfigEntity::insert(self.db, &list).await?;
        debug!("insert_back_test_log_result = {}", json!(data));
        Ok(data)
    }

    pub async fn get_config(
        &self,
        strategy_type: Option<&str>,
        inst_id: &str,
        time: &str,
    ) -> anyhow::Result<Vec<StrategyConfigEntity>> {
        match strategy_type {
            Some(strategy_type) => {
                let data = StrategyConfigEntity::select_by_strate_type(
                    self.db,
                    strategy_type,
                    inst_id,
                    time,
                )
                .await?;
                debug!("query strategy_config result:{}", json!(data));
                Ok(data)
            }
            None => {
                let data = StrategyConfigEntity::select_by_inst_id(self.db, inst_id, time).await?;
                debug!("query strategy_config result:{}", json!(data));
                Ok(data)
            }
        }
    }

    pub async fn get_list(&self) -> anyhow::Result<Vec<StrategyConfigEntity>> {
        let data = StrategyConfigEntity::get_all(self.db).await?;
        debug!("query strategy_config result:{}", json!(data));
        Ok(data)
    }
}
