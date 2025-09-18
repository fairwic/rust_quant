use std::sync::Arc;

use crate::trading::cache::latest_candle_cache::{default_provider, LatestCandleCacheProvider};
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::trading::model::market::candles::CandlesModel;
use okx::dto::market_dto::CandleOkxRespDto;
use tracing::info;

pub struct CandleService {
    cache: Arc<dyn LatestCandleCacheProvider>,
}
impl CandleService {
    pub fn new() -> Self {
        Self {
            cache: default_provider(),
        }
    }
    pub fn new_with_cache(cache: Arc<dyn LatestCandleCacheProvider>) -> Self {
        Self { cache }
    }
    pub async fn update_candle(
        &self,
        candle: Vec<CandleOkxRespDto>,
        inst_id: &str,
        time_interval: &str,
    ) -> anyhow::Result<()> {
        // 优先使用最后一条（通常是最新）
        let first = candle.last().unwrap();
        let new_ts = first.ts.parse::<i64>().unwrap_or(0);
        let mut if_update_db = false;
        let mut if_update_cache = false;
        // 读取当前缓存快照（先内存，后 Redis）
        let snap = CandlesEntity {
            // 从 WS 构造最小快照
            ts: new_ts,
            o: first.o.clone(),
            h: first.h.clone(),
            l: first.l.clone(),
            c: first.c.clone(),
            vol: first.v.clone(),
            vol_ccy: first.vol_ccy.clone(),
            confirm: first.confirm.clone(),
            updated_at: Some(rbatis::rbdc::DateTime::now()),
        };
        match self.cache.get_or_fetch(inst_id, time_interval).await {
            Some(cache_candle) => {
                //只有当新数据的时间戳大于等于缓存中的时间戳，并且新数据的成交量大于等于缓存中的成交量时，才更新缓存
                if new_ts >= cache_candle.ts
                    && first.vol_ccy.parse::<f64>().unwrap_or(0.0)
                        >= cache_candle.vol_ccy.parse::<f64>().unwrap_or(0.0)
                {
                    if_update_db = true;
                    if_update_cache = true;
                }
            }
            None => {
                if_update_db = true;
                if_update_cache = true;
            }
        }
        if if_update_cache {
            self.cache.set_both(inst_id, time_interval, &snap).await;
        }
        if if_update_db {
            // 2) 异步落库（幂等）不回刷缓存
            let inst = inst_id.to_string();
            let per = time_interval.to_string();
            let first_clone = first.clone();
            let cache = Arc::clone(&self.cache);
            let new_ts_captured = new_ts;
            tokio::spawn(async move {
                let model = CandlesModel::new().await;
                let _ = model.update_or_create(&first_clone, &inst, &per).await;
                // if let Ok(opt) = model.get_one_by_ts(&inst, &per, new_ts_captured).await {
                // if let Some(mut c) = opt {
                // if c.updated_at.is_none() { c.updated_at = Some(rbatis::rbdc::DateTime::now()); }
                // cache.set_both(&inst, &per, &c).await;
                // }
                // }
            });
        }
        Ok(())
    }
}
