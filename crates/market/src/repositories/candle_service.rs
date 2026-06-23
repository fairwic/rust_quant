use crate::cache::{default_provider, LatestCandleCacheProvider};
use crate::models::{CandlesEntity, CandlesModel};
use crate::repositories::persist_worker::PersistTask;
use chrono::Utc;
use dashmap::DashMap;
use okx::dto::market_dto::CandleOkxRespDto;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
/// 策略触发回调类型
pub type StrategyTrigger = Arc<dyn Fn(String, String, CandlesEntity) + Send + Sync>;
pub struct CandleService {
    /// 缓存。
    cache: Arc<dyn LatestCandleCacheProvider>,
    /// 持久化写入通道；为空时表示不启动异步持久化。
    persist_sender: Option<mpsc::UnboundedSender<PersistTask>>,
    /// 策略触发回调函数
    ///
    /// # 架构说明
    /// - market层不应直接依赖strategies层
    /// - 通过回调函数实现解耦
    /// - 由上层（orchestration/services）注入策略触发逻辑
    strategy_trigger: Option<StrategyTrigger>,
}
/// 确认K线触发去重：确保同一 (inst_id, time_interval) 的同一根确认K线只触发一次
/// key = "{inst_id}:{time_interval}" -> last_triggered_confirmed_ts(ms)
static LAST_TRIGGERED_CONFIRMED_TS: Lazy<DashMap<String, i64>> = Lazy::new(DashMap::new);
impl CandleService {
    /// 构建 行情与市场数据 所需实例，并集中初始化依赖和默认状态。
    pub fn new() -> Self {
        Self {
            cache: default_provider(),
            persist_sender: None,
            strategy_trigger: None,
        }
    }
    /// 提供newwithcache的集中实现，避免行情数据调用方重复处理相同细节。
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
    /// # 参数
    /// * `cache` - K线缓存
    /// * `persist_sender` - 持久化任务发送器
    /// * `strategy_trigger` - 策略触发回调函数
    /// # 架构说明
    /// - 通过依赖注入方式传入策略触发逻辑
    /// - 避免market层直接依赖strategies层
    pub fn new_with_strategy_trigger(
        cache: Arc<dyn LatestCandleCacheProvider>,
        persist_sender: Option<mpsc::UnboundedSender<PersistTask>>,
        strategy_trigger: StrategyTrigger,
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
        let latest = match candles.last() {
            Some(v) => v,
            None => return Ok(()),
        };
        let new_ts = match latest.ts.parse::<i64>() {
            Ok(v) => v,
            Err(e) => {
                error!(
                    "❌ 解析K线 ts 失败: inst_id={}, time_interval={}, ts={}, error={}",
                    inst_id, time_interval, latest.ts, e
                );
                return Ok(());
            }
        };
        // 检查是否需要更新
        let should_update = match self.cache.get_or_fetch(inst_id, time_interval).await {
            Some(cache_candle) => {
                new_ts > cache_candle.ts
                    || (new_ts == cache_candle.ts && {
                        let new_vol = latest.vol_ccy.parse::<f64>().unwrap_or(0.0);
                        let old_vol = cache_candle.vol_ccy.parse::<f64>().unwrap_or(0.0);
                        new_vol >= old_vol
                    })
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
                // 只触发一次：同 ts 的确认K线重复推送（重连/补发）会被抑制
                let trigger_key = format!("{}:{}", inst_id, time_interval);
                let last_ts = LAST_TRIGGERED_CONFIRMED_TS
                    .get(&trigger_key)
                    .map(|v| *v.value());
                let should_trigger = match last_ts {
                    Some(old) => new_ts > old,
                    None => true,
                };
                if !should_trigger {
                    debug!(
                        "跳过重复确认K线触发: inst_id={}, time_interval={}, ts={}, last_ts={:?}",
                        inst_id, time_interval, new_ts, last_ts
                    );
                } else {
                    LAST_TRIGGERED_CONFIRMED_TS.insert(trigger_key, new_ts);
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
            }
            // 🚀 发送到批处理队列（如果启用）或直接写库
            if let Some(sender) = &self.persist_sender {
                let task = PersistTask {
                    candles,
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
impl Default for CandleService {
    fn default() -> Self {
        Self::new()
    }
}
