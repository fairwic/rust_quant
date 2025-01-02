use crate::trading::model::big_data::take_volume::{TakerVolumeEntity, TakerVolumeModel};
use crate::trading::model::big_data::take_volume_contract::{
    ModelEntity, TakerVolumeContractModel,
};
use crate::trading::okx::big_data::{BigDataOkxApi, TakerVolume};
use crate::trading::okx::market::Market;
use chrono::Utc;
use log::info;
use redis::Commands;
use std::error::Error;
use std::time::Duration;
use tracing::debug;

pub struct BigDataContractService {}

impl BigDataContractService {
    //同步合约主动买入/卖出情况
    pub async fn sync_taker_volume_contract(
        inst_ids: Vec<&str>,
        periods: Vec<&str>,
    ) -> anyhow::Result<()> {
        println!("sync_taker_volume_contract...");
        // 遍历所有交易对和周期
        for inst_id in inst_ids {
            for period in periods.iter() {
                // 获取一条最新的数据并处理
                let mut begin_end = Self::get_sync_begin_with_end(inst_id, period).await?;

                while let Some(t) = Self::get_new_one_data(inst_id, period).await? {
                    let right = crate::time_util::get_period_start_timestamp(period);
                    if t.ts < right {
                        // 获取Okx数据并插入
                        let res = Self::fetch_okx_data(inst_id, period, &begin_end.0, &begin_end.1)
                            .await?;
                        if res.is_empty() {
                            debug!("No old candles need patch: {},{}", inst_id, period);
                            break;
                        }
                        let taker_volumes = Self::process_okx_data(res, inst_id, period)?;
                        Self::insert_taker_volumes(&taker_volumes).await?;
                    } else {
                        info!("新数据，已经是最新时间，跳过");
                        break; // 数据更新完毕，跳出循环
                    }
                }
            }
        }
        Ok(())
    }

    //初始化
    pub async fn init_taker_volume_contract(
        inst_ids: Vec<&str>,
        periods: Vec<&str>,
    ) -> anyhow::Result<()> {
        println!("init_taker_volume_contract...");
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
                        info!("旧数据已达到限制，跳过该周期。");
                        break;
                    }
                    // 更新起始时间
                    let new_times = Self::get_initial_begin_with_end(inst_id, period).await?;
                    begin = new_times.0;
                    end = new_times.1;
                }
            }
        }
        Ok(())
    }

    // 获取Okx数据
    async fn fetch_okx_data(
        inst_id: &str,
        period: &str,
        begin: &Option<String>,
        end: &Option<String>,
    ) -> anyhow::Result<Vec<Vec<String>>> {
        BigDataOkxApi::get_taker_volume_contract(
            inst_id,
            Some(period),
            Some("2"),
            begin.as_deref(),
            end.as_deref(),
            Some("100"),
        )
        .await
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
        Self::insert_taker_volumes(&taker_volumes).await?;
        Ok(Some(taker_volumes))
    }

    // 获取同步数据的时间范围
    async fn get_sync_begin_with_end(
        inst_id: &str,
        period: &str,
    ) -> anyhow::Result<(Option<String>, Option<String>)> {
        let res = Self::get_new_one_data(inst_id, period).await?;
        match res {
            Some(t) => {
                let begin = crate::time_util::ts_add_n_period(t.ts, period, 1)?;
                let end = crate::time_util::ts_add_n_period(t.ts, period, 101)?;
                Ok((Some(begin.to_string()), Some(end.to_string())))
            }
            None => Ok((None, None)),
        }
    }

    // 获取初始数据的时间范围
    async fn get_initial_begin_with_end(
        inst_id: &str,
        period: &str,
    ) -> anyhow::Result<(Option<String>, Option<String>)> {
        let res = TakerVolumeContractModel::new()
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
                if row.len() != 3 {
                    return None;
                }
                let ts = row.get(0)?.parse::<i64>().unwrap_or(0);
                let sell_vol = row.get(1)?.to_string();
                let buy_vol = row.get(2)?.to_string();
                Some(ModelEntity {
                    id: 0,
                    ts,
                    inst_id: inst_id.to_string(),
                    period: period.to_string(),
                    sell_vol,
                    buy_vol,
                    created_at: None,
                })
            })
            .collect())
    }

    // 插入数据
    async fn insert_taker_volumes(taker_volumes: &Vec<ModelEntity>) -> anyhow::Result<()> {
        TakerVolumeContractModel::new()
            .await
            .add_list(taker_volumes)
            .await?;
        Ok(())
    }

    // 判断数据是否已达到限制
    async fn is_limit_reached(inst_id: &str, period: &str, limit: i64) -> anyhow::Result<bool> {
        let count = TakerVolumeContractModel::new()
            .await
            .get_new_count(inst_id, period)
            .await?;
        Ok(count as i64 > limit)
    }

    // 获取最新数据
    async fn get_new_one_data(inst_id: &str, period: &str) -> anyhow::Result<Option<ModelEntity>> {
        TakerVolumeContractModel::new()
            .await
            .get_new_one_data(inst_id, period)
            .await
    }
}
