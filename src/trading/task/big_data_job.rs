use crate::trading::services::big_data::big_data_service::BigDataContractService;
use crate::trading::services::big_data::big_data_top_contract_service::BigDataTopContractService;
use crate::trading::services::big_data::{big_data_service, big_data_top_contract_service};
use crate::trading::task::big_data_job;
use tracing::{span, Level};
use crate::trading::services::big_data::big_data_top_position_service::BigDataTopPositionService;

/** 同步数据 任务**/
pub async fn run_take_volume_job(inst_ids: Option<Vec<&str>>, periods: Option<Vec<&str>> ) -> anyhow::Result<(), anyhow::Error> {
    println!("run_sync_data_job start");

    let span = span!(Level::DEBUG, "run_sync_data_job");
    let _enter = span.enter();

    if inst_ids.is_some() && periods.is_some(){
        // 初始化获取历史的k线路
        //big_data_service::BigDataService::sync_taker_volume(inst_ids, periods).await?;

        // 初始化获取历史的k线路
        // big_data_service::BigDataService::init_taker_volume_contract(inst_ids.clone(), periods.clone())
        //     .await?;
        // 初始化获取历史的k线路
        // BigDataContractService::sync_taker_volume_contract(inst_ids.clone(), periods.clone()).await?;

        let inst_ids= inst_ids.unwrap();
        let periods= periods.unwrap();

        // 初始化 精英交易员合约多空持仓人数比
        BigDataTopContractService::init_top_contract_account_ratio(inst_ids.clone(), periods.clone()).await?;
        // 同步  精英交易员合约多空持仓人数比
        BigDataTopContractService::sync_top_contract_account_ratio(inst_ids.clone(), periods.clone()).await?;

        // 初始化 精英交易员合约多空持仓 仓位比
        BigDataTopPositionService::init_top_contract_position_ratio(inst_ids.clone(), periods.clone()).await?;
        // 同步  精英交易员合约多空持仓 仓位比
        BigDataTopPositionService::sync_top_contract_position_ratio(inst_ids.clone(), periods.clone()).await?;

    }




    Ok(())
}
