use anyhow::Result;
use tracing::{info, Level};
use tracing::span;

use crate::trading::task::candles_job;

/// 同步数据任务
pub async fn run_sync_data_job(
    inst_ids: &Vec<String>,
    tims: &Vec<String>,
) -> Result<()> {
    info!("run_sync_data_job start");
    let span = span!(Level::DEBUG, "run_sync_data_job");
    let _enter = span.enter();

    candles_job::init_create_table(inst_ids, tims)
        .await
        .expect("init create_table error");

    //初始化获取历史的k线路
    candles_job::init_all_candles(inst_ids, tims).await?;

    //获取最新的k线路
    candles_job::init_before_candles(inst_ids, tims).await?;

    Ok(())
}
