use std::sync::Arc;
use tokio::sync::mpsc;

use chrono::Utc;
use okx::dto::market_dto::CandleOkxRespDto;
use tracing::{debug, error, info, warn};

use crate::cache::{default_provider, LatestCandleCacheProvider};
use crate::models::{CandlesEntity, CandlesModel};
use crate::repositories::persist_worker::PersistTask;

pub struct CandleService {
    cache: Arc<dyn LatestCandleCacheProvider>,
    persist_sender: Option<mpsc::UnboundedSender<PersistTask>>,
    /// 策略触发回调函数
    /// 
    /// # 架构说明
    /// - market层不应直接依赖strategies层
    /// - 通过回调函数实现解耦
    /// - 由上层（orchestration/services）注入策略触发逻辑
    strategy_trigger: Option<Arc<dyn Fn(String, String, CandlesEntity) + Send + Sync>>,
}

impl CandleService {
    pub fn new() -> Self {
        Self {
            cache: default_provider(),
            persist_sender: None,
            strategy_trigger: None,
        }
    }

    pub fn new_with_cache(cache: Arc<dyn LatestCandleCacheProvider>) -> Self {
        Self {
            cache,
            persist_sender: None,
            strategy_trigger: None,
        }
    }

    /// [已优化] 创建带批处理Worker的服务实例
    pub fn new_with_persist_worker(
        cache: Arc<dyn LatestCandleCacheProvider>,
        persist_sender: mpsc::UnboundedSender<PersistTask>,
    ) -> Self {
        Self {
            cache,
            persist_sender: Some(persist_sender),
            strategy_trigger: None,
        }
    }

    /// 创建带策略触发回调的服务实例
    /// 
    /// # 参数
    /// * `cache` - K线缓存
    /// * `persist_sender` - 持久化任务发送器
    /// * `strategy_trigger` - 策略触发回调函数
    /// 
    /// # 架构说明
    /// - 通过依赖注入方式传入策略触发逻辑
    /// - 避免market层直接依赖strategies层
    pub fn new_with_strategy_trigger(
        cache: Arc<dyn LatestCandleCacheProvider>,
        persist_sender: Option<mpsc::UnboundedSender<PersistTask>>,
        strategy_trigger: Arc<dyn Fn(String, String, CandlesEntity) + Send + Sync>,
    ) -> Self {
        Self {
            cache,
            persist_sender,
            strategy_trigger: Some(strategy_trigger),
        }
    }
    /// [已优化] 批量处理K线数据（处理完整数据集）
    /// 性能提升：处理所有历史数据，确保数据完整性
    pub async fn update_candles_batch(
        &self,
        candles: Vec<CandleOkxRespDto>,
        inst_id: &str,
        time_interval: &str,
    ) -> anyhow::Result<()> {
        if candles.is_empty() {
            return Ok(());
        }
        // 取最后一条作为缓存（最新数据）
        let latest = candles.last().unwrap();
        let new_ts = latest.ts.parse::<i64>().unwrap_or(0);

        // 检查是否需要更新
        let should_update = match self.cache.get_or_fetch(inst_id, time_interval).await {
            Some(cache_candle) => {
                new_ts > cache_candle.ts
                    || (new_ts == cache_candle.ts
                        && latest.vol_ccy.parse::<f64>().unwrap_or(0.0)
                            >= cache_candle.vol_ccy.parse::<f64>().unwrap_or(0.0))
            }
            None => true,
        };

        if should_update {
            // 更新缓存（只缓存最新数据）
            let now = Utc::now().naive_utc();
            let snap = CandlesEntity {
                id: None,
                ts: new_ts,
                o: latest.o.clone(),
                h: latest.h.clone(),
                l: latest.l.clone(),
                c: latest.c.clone(),
                vol: latest.v.clone(),
                vol_ccy: latest.vol_ccy.clone(),
                confirm: latest.confirm.clone(),
                created_at: None,
                updated_at: Some(now),
            };

            self.cache.set_both(inst_id, time_interval, &snap).await;

            // 🚀 K线确认时触发策略执行
            if snap.confirm == "1" {
                info!(
                    "📈 K线确认，触发策略执行: inst_id={}, time_interval={}, ts={}",
                    inst_id, time_interval, new_ts
                );

                // 如果注入了策略触发回调，则异步触发
                if let Some(trigger) = &self.strategy_trigger {
                    let inst_id_owned = inst_id.to_string();
                    let time_interval_owned = time_interval.to_string();
                    let snap_clone = snap.clone();
                    let trigger_clone = Arc::clone(trigger);

                    tokio::spawn(async move {
                        trigger_clone(inst_id_owned, time_interval_owned, snap_clone);
                    });
                } else {
                    warn!(
                        "⚠️  未注入策略触发回调，跳过策略执行: inst_id={}, time_interval={}",
                        inst_id, time_interval
                    );
                }
            }

            // 🚀 发送到批处理队列（如果启用）或直接写库
            if let Some(sender) = &self.persist_sender {
                let task = PersistTask {
                    candles: candles.clone(),
                    inst_id: inst_id.to_string(),
                    time_interval: time_interval.to_string(),
                };

                if let Err(e) = sender.send(task) {
                    error!("❌ 发送持久化任务失败: {:?}", e);
                }
            } else {
                // 没有Worker时，直接批量写库
                let inst = inst_id.to_string();
                let per = time_interval.to_string();
                tokio::spawn(async move {
                    let model = CandlesModel::new();
                    match model.upsert_batch(candles, &inst, &per).await {
                        Ok(rows) => {
                            debug!(
                                "✅ 批量写入成功: inst_id={}, time_interval={}, rows={}",
                                inst, per, rows
                            );
                        }
                        Err(e) => {
                            error!(
                                "❌ 批量写入失败: inst_id={}, time_interval={}, error={:?}",
                                inst, per, e
                            );
                        }
                    }
                });
            }
        }

        Ok(())
    }

    /// [保留兼容] 旧版本方法，内部调用批处理方法
    pub async fn update_candle(
        &self,
        candle: Vec<CandleOkxRespDto>,
        inst_id: &str,
        time_interval: &str,
    ) -> anyhow::Result<()> {
        self.update_candles_batch(candle, inst_id, time_interval)
            .await
    }
}
