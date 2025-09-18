use std::sync::Arc;

use crate::trading::model::market::candles::CandlesModel;
use crate::trading::cache::latest_candle_cache::{LatestCandleCacheProvider, default_provider};
use okx::dto::market_dto::CandleOkxRespDto;
use tracing::{info, warn};
use crate::trading::model::entity::candles::entity::CandlesEntity;

pub struct CandleService {
    cache: Arc<dyn LatestCandleCacheProvider>,
}
impl CandleService {
    pub fn new() -> Self {
        Self { cache: default_provider() }
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
        let new_period_ts = first.ts.parse::<i64>().unwrap_or(0);
        let incoming_confirm = first.confirm.parse::<i32>().unwrap_or(0);

        // 读取当前缓存快照（先内存，后 Redis）
        let mut last_ts: i64 = 0;
        let mut cached_confirm: i32 = 0;
        if let Some(c) = self.cache.get_or_fetch(inst_id, time_interval).await {
            last_ts = c.ts;
            cached_confirm = c.confirm.parse::<i32>().unwrap_or(0);
        }

        // 决策：
        // - new_ts > last_ts: 更新
        // - new_ts == last_ts: 若 incoming_confirm == 1 或 cached_confirm == 0，则更新（同一根K线的滚动/确认）
        // - 其他情况：跳过
        let should_update_cache = if new_period_ts > last_ts {
            true
        } else if new_period_ts == last_ts {
            incoming_confirm == 1 || cached_confirm == 0
        } else {
            false
        };
        if should_update_cache {
            let snap = CandlesEntity { // 从 WS 构造最小快照
                ts: new_period_ts,
                o: first.o.clone(),
                h: first.h.clone(),
                l: first.l.clone(),
                c: first.c.clone(),
                vol: first.v.clone(),
                vol_ccy: first.vol_ccy.clone(),
                confirm: first.confirm.clone(),
                updated_at: Some(rbatis::rbdc::DateTime::now()),
            };
            self.cache.set_both(inst_id, time_interval, &snap).await;
        } else {
            warn!("skip cache update: new_ts={}, last_ts={}, incoming_confirm={}, cached_confirm={}", new_period_ts, last_ts, incoming_confirm, cached_confirm);
            return Ok(());
        }

        // 2) 异步落库（幂等）并回刷缓存
        let inst = inst_id.to_string();
        let per = time_interval.to_string();
        let first_clone = first.clone();
        let cache = Arc::clone(&self.cache);
        let new_ts_captured = new_period_ts;
        tokio::spawn(async move {
            let model = CandlesModel::new().await;
            let _ = model.update_or_create(&first_clone, &inst, &per).await;
            if let Ok(opt) = model.get_one_by_ts(&inst, &per, new_ts_captured).await {
                if let Some(mut c) = opt {
                    if c.updated_at.is_none() { c.updated_at = Some(rbatis::rbdc::DateTime::now()); }
                    cache.set_both(&inst, &per, &c).await;
                }
            }
        });
        Ok(())
    }
}
