use crate::trading::services::big_data::big_data_service::BigDataContractService;
use crate::trading::services::big_data::big_data_top_contract_service::BigDataTopContractService;
use crate::trading::services::big_data::big_data_top_position_service::BigDataTopPositionService;
use crate::trading::services::big_data::top_contract_service_trait::TopContractServiceTrait;
use crate::trading::services::big_data::{big_data_service, big_data_top_contract_service};
use crate::trading::task::big_data_job;
use tracing::{span, Level};

/** 同步数据 任务**/
pub async fn init_top_contract(
    inst_ids: Option<Vec<&str>>,
    periods: Option<Vec<&str>>,
) -> anyhow::Result<(), anyhow::Error> {
    println!("run init_data_job start");
    let span = span!(Level::DEBUG, "init_top_contract");
    let _enter = span.enter();
    if inst_ids.is_some() && periods.is_some() {
        let inst_ids = inst_ids.unwrap();
        let periods = periods.unwrap();

        // 初始化 精英交易员合约多空持仓人数比
        BigDataTopContractService::init(inst_ids.clone(), periods.clone()).await?;
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        // 初始化 精英交易员合约多空持仓 仓位比
        BigDataTopPositionService::init(inst_ids.clone(), periods.clone()).await?;
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    }
    Ok(())
}

/** 同步数据 任务**/
pub async fn sync_top_contract(
    inst_ids: Option<Vec<&str>>,
    periods: Option<Vec<&str>>,
) -> anyhow::Result<(), anyhow::Error> {
    println!("run sync_data_job start");
    let span = span!(Level::DEBUG, "sync_top_contract");
    let _enter = span.enter();
    if inst_ids.is_some() && periods.is_some() {
        let inst_ids = inst_ids.unwrap();
        let periods = periods.unwrap();
        // 同步  精英交易员合约多空持仓人数比
        BigDataTopContractService::sync(inst_ids.clone(), periods.clone()).await?;
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        // 同步  精英交易员合约多空持仓 仓位比
        BigDataTopPositionService::sync(inst_ids.clone(), periods.clone()).await?;
    }
    Ok(())
}

pub async fn init_big_data_job(inst_ids: Vec<&str>, periods: Vec<&str>) -> anyhow::Result<()> {
    println!("init_big_data_job start");

    // 创建服务实例
    let contract_service = BigDataTopContractService {};
    let position_service = BigDataTopPositionService {};

    // 调用实例方法
    contract_service
        .init(inst_ids.clone(), periods.clone())
        .await?;
    position_service
        .init(inst_ids.clone(), periods.clone())
        .await?;

    Ok(())
}

pub async fn sync_big_data_job(inst_ids: Vec<&str>, periods: Vec<&str>) -> anyhow::Result<()> {
    println!("sync_big_data_job start");

    // 创建服务实例
    let contract_service = BigDataTopContractService {};
    let position_service = BigDataTopPositionService {};

    // 调用实例方法
    contract_service
        .sync(inst_ids.clone(), periods.clone())
        .await?;
    position_service
        .sync(inst_ids.clone(), periods.clone())
        .await?;

    Ok(())
}
