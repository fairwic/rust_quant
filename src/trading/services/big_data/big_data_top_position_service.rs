use crate::trading::model::big_data::top_contract_position_ratio::{
    ModelEntity, TopContractPositionRatioModel,
};
use crate::trading::model::market::candles::SelectTime;
use chrono::Utc;
use log::info;
use redis::Commands;
use std::error::Error;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, warn};
use crate::trading::services::big_data::top_contract_service_trait::TopContractServiceTrait;
use async_trait::async_trait;
use okx::OkxBigData;
use okx::api::api_trait::OkxApiTrait;

pub struct BigDataTopPositionService {}
impl  BigDataTopPositionService {
    // 静态方法包装
    pub async fn init(inst_ids: Vec<&str>, periods: Vec<&str>) -> anyhow::Result<()> {
        let service = Self {};
        service.init(inst_ids, periods).await
    }
    
    pub async fn sync(inst_ids: Vec<&str>, periods: Vec<&str>) -> anyhow::Result<()> {
        let service = Self {};
        service.sync(inst_ids, periods).await
    }
    
}

#[async_trait]
impl TopContractServiceTrait for BigDataTopPositionService {
    
    //同步精英交易员合约多空持仓人数比
    async fn sync(&self,
        inst_ids: Vec<&str>,
        periods: Vec<&str>,
    ) -> anyhow::Result<()> {
        println!("sync long-short-account-ratio-contract-top-trader...");
        // 遍历所有交易对和周期
        for inst_id in inst_ids {
            for period in periods.iter() {
                // 获取一条最新的数据并处理+1period
                let res = Self::get_new_one_data(inst_id, period).await?;
                let right = crate::time_util::get_period_start_timestamp(period);
                if res.is_none() {
                    error!("请先初始化数据topContract {} {}",inst_id,period);
                    continue; // 数据更新完毕，跳出循环
                }
                let start = res.unwrap().ts;
                if start >= right {
                    info!("新数据已经达到最新时间周期，跳过");
                    continue; // 数据更新完毕，跳出循环
                }
                let mut begin_end = Self::ts_add_n_period(start, period)?;
                loop {
                    //获取当前最新时间的周期
                    let right = crate::time_util::get_period_start_timestamp(period);
                    //延迟100ms
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    // 获取Okx数据并插入
                    let res = Self::fetch_okx_data(inst_id, period, &Some(begin_end.unwrap().0.to_string()), &Some(begin_end.unwrap().1.to_string())).await?;
                    if res.is_empty() {
                        debug!("No old candles need patch: {},{}", inst_id, period);
                        break;
                    }
                    //处理数据
                    let taker_volumes = Self::process_okx_data(res, inst_id, period)?;
                    //插入数据库
                    Self::insert_taker_volumes(&taker_volumes).await?;
                    //更新开始时间结束时间
                    begin_end = Self::get_sync_begin_with_end(inst_id, period).await?;

                    if begin_end.unwrap().0 <= right {
                        info!("新数据已经达到最新时间，跳过");
                        break; // 数据更新完毕，跳出循环
                    }
                }
            }
        }
        Ok(())
    }


    //初始精英交易员合约多空持仓仓位比
     async fn init(&self,
        inst_ids: Vec<&str>,
        periods: Vec<&str>,
    ) -> anyhow::Result<()> {
        println!("init_long-short-position-ratio-contract-top-trader...");
        let limit = 1440; // 设置限制
        for inst_id in inst_ids {
            for period in periods.iter() {
                let (mut begin, mut end) =
                    Self::get_initial_begin_with_end(inst_id, period).await?;

                while let Some(t) =
                    Self::fetch_and_process_okx_data(inst_id, period, &begin, &end).await?
                {
                    // 判断数据量是否达到限制
                    if Self::is_limit_reached(inst_id, period, limit).await? {
                        info!("数据已达到限制，跳过该周期。");
                        break;
                    }
                    // 更新起始时间
                    let new_times = Self::get_initial_begin_with_end(inst_id, period).await?;
                    begin = new_times.0;
                    end = new_times.1;

                    //延迟100ms
                    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                }
            }
        }
        Ok(())
    }
}
impl  BigDataTopPositionService {


    // 获取Okx数据
    async fn fetch_okx_data(
        inst_id: &str,
        period: &str,
        begin: &Option<String>,
        end: &Option<String>,
    ) -> anyhow::Result<Vec<Vec<String>>> {
        let okx = OkxBigData::from_env()?;
        let res = okx.get_long_short_account_ratio_contract_top_trader(
            inst_id,
            Some(period),
            begin.as_deref(),
            end.as_deref(),
            Some("100"),
        )
        .await?;
        Ok(res)
    }

    // 同步 Okx 数据并插入
    async fn fetch_and_process_okx_data(
        inst_id: &str,
        period: &str,
        begin: &Option<String>,
        end: &Option<String>,
    ) -> anyhow::Result<Option<Vec<ModelEntity>>> {
        let res = Self::fetch_okx_data(inst_id, period, begin, end).await?;
        if res.is_empty() {
            return Ok(None);
        }
        let taker_volumes = Self::process_okx_data(res, inst_id, period)?;
        if taker_volumes.is_empty() {
            info!("获取okx数据为空")
        }
        Self::insert_taker_volumes(&taker_volumes).await?;
        Ok(Some(taker_volumes))
    }

    // 获取同步数据的时间范围
    async fn get_sync_begin_with_end(
        inst_id: &str,
        period: &str,
    ) -> anyhow::Result<Option<(i64, i64)>> {
        let res = Self::get_new_one_data(inst_id, period).await?;
        match res {
            Some(t) => {
                let begin = crate::time_util::ts_add_n_period(t.ts, period, 1)?;
                let end = crate::time_util::ts_add_n_period(t.ts, period, 101)?;
                Ok(Some((begin, end)))
            }
            None => Ok(None),
        }
    }


    // 获取初始数据的时间范围
    async fn get_initial_begin_with_end(
        inst_id: &str,
        period: &str,
    ) -> anyhow::Result<(Option<String>, Option<String>)> {
        let res = TopContractPositionRatioModel::new()
            .await
            .get_oldest_one_data(inst_id, period)
            .await?;
        match res {
            None => Ok((None, None)),
            Some(t) => {
                let begin = crate::time_util::ts_reduce_n_period(t.ts, period, 101)?;
                let end = crate::time_util::ts_reduce_n_period(t.ts, period, 1)?;
                Ok((Some(begin.to_string()), Some(end.to_string())))
            }
        }
    }

    // 处理Okx返回的数据
    fn process_okx_data(
        res: Vec<Vec<String>>,
        inst_id: &str,
        period: &str,
    ) -> anyhow::Result<Vec<ModelEntity>> {
        Ok(res
            .into_iter()
            .filter_map(|row| {
                if row.len() != 2 {
                    return None;
                }
                let ts = row.get(0)?.parse::<i64>().unwrap_or(0);
                Some(ModelEntity {
                    id: 0,
                    ts,
                    inst_id: inst_id.to_string(),
                    period: period.to_string(),
                    long_short_pos_ratio: row.get(1)?.to_string(),
                    created_at: None,
                })
            })
            .collect())
    }

    // 插入数据
    async fn insert_taker_volumes(taker_volumes: &Vec<ModelEntity>) -> anyhow::Result<()> {
        TopContractPositionRatioModel::new()
            .await
            .add_list(taker_volumes)
            .await?;
        Ok(())
    }

    // 判断数据是否已达到限制
    async fn is_limit_reached(inst_id: &str, period: &str, limit: i64) -> anyhow::Result<bool> {
        let count = TopContractPositionRatioModel::new()
            .await
            .get_new_count(inst_id, period)
            .await?;
        Ok(count as i64 > limit)
    }

    // 获取最新数据
    async fn get_new_one_data(inst_id: &str, period: &str) -> anyhow::Result<Option<ModelEntity>> {
        TopContractPositionRatioModel::new()
            .await
            .get_new_one_data(inst_id, period)
            .await
    }

    // 获取最新数据
    pub async fn get_list_by_time(
        inst_id: &str,
        period: &str,
        offset:Option<usize>,
        limit: usize,
        select_time: Option<SelectTime>,
    ) -> anyhow::Result<Vec<ModelEntity>> {
        let mut data = TopContractPositionRatioModel::new().await.get_all(inst_id, period,offset, limit, select_time).await?;
        if !data.is_empty() {
            //数据进行反向排序
            data.sort_unstable_by(|a, b| a.ts.cmp(&b.ts));
        }
        Ok(data)
    }
}
