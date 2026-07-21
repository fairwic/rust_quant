use crate::cache::{default_provider, LatestCandleCacheProvider};
use crate::models::{CandlesEntity, CandlesModel};
use crate::repositories::persist_worker::PersistTask;
use crate::streams::{timeframe_duration_ms, CandleRuntimeRegistry, WatchdogDecision};
use chrono::Utc;
use okx::dto::market_dto::CandleOkxRespDto;
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
    /// WebSocket 与 DB watchdog 共用的触发幂等和运行态状态。
    runtime_registry: Arc<CandleRuntimeRegistry>,
}
impl CandleService {
    /// 构建 行情与市场数据 所需实例，并集中初始化依赖和默认状态。
    pub fn new() -> Self {
        Self {
            cache: default_provider(),
            persist_sender: None,
            strategy_trigger: None,
            runtime_registry: Arc::new(CandleRuntimeRegistry::default()),
        }
    }
    /// 提供newwithcache的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn new_with_cache(cache: Arc<dyn LatestCandleCacheProvider>) -> Self {
        Self {
            cache,
            persist_sender: None,
            strategy_trigger: None,
            runtime_registry: Arc::new(CandleRuntimeRegistry::default()),
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
            runtime_registry: Arc::new(CandleRuntimeRegistry::default()),
        }
    }
    /// 创建带批处理 Worker 和共享运行态 registry 的服务实例。
    pub fn new_with_persist_worker_and_runtime(
        cache: Arc<dyn LatestCandleCacheProvider>,
        persist_sender: mpsc::UnboundedSender<PersistTask>,
        runtime_registry: Arc<CandleRuntimeRegistry>,
    ) -> Self {
        Self {
            cache,
            persist_sender: Some(persist_sender),
            strategy_trigger: None,
            runtime_registry,
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
            runtime_registry: Arc::new(CandleRuntimeRegistry::default()),
        }
    }
    /// 创建带策略触发和共享运行态 registry 的服务实例。
    pub fn new_with_strategy_trigger_and_runtime(
        cache: Arc<dyn LatestCandleCacheProvider>,
        persist_sender: Option<mpsc::UnboundedSender<PersistTask>>,
        strategy_trigger: StrategyTrigger,
        runtime_registry: Arc<CandleRuntimeRegistry>,
    ) -> Self {
        Self {
            cache,
            persist_sender,
            strategy_trigger: Some(strategy_trigger),
            runtime_registry,
        }
    }

    /// 通过统一幂等门禁触发确认 K 线，供 WS 与 DB watchdog 共用。
    pub fn trigger_confirmed_candle(
        &self,
        inst_id: &str,
        time_interval: &str,
        snap: CandlesEntity,
        source: &str,
    ) -> bool {
        self.trigger_confirmed_candle_at(
            inst_id,
            time_interval,
            snap,
            source,
            Utc::now().timestamp_millis(),
        )
    }

    /// 使用显式观察时间执行统一触发门禁，便于固定十秒边界测试。
    fn trigger_confirmed_candle_at(
        &self,
        inst_id: &str,
        time_interval: &str,
        snap: CandlesEntity,
        source: &str,
        observed_at_ms: i64,
    ) -> bool {
        self.runtime_registry.record_confirmed_candle(
            inst_id,
            time_interval,
            snap.ts,
            observed_at_ms,
        );
        if self
            .runtime_registry
            .is_at_or_before_startup_baseline(inst_id, time_interval, snap.ts)
        {
            debug!(
                "跳过启动前确认K线: inst_id={}, time_interval={}, ts={}, source={}",
                inst_id, time_interval, snap.ts, source
            );
            return false;
        }
        let timeframe_ms = match timeframe_duration_ms(time_interval) {
            Ok(value) => value,
            Err(error) => {
                error!(
                    event = "confirmed_candle_invalid_timeframe",
                    inst_id,
                    time_interval,
                    candle_ts = snap.ts,
                    source,
                    error = %error,
                    "确认 K 线周期非法，拒绝进入策略回调"
                );
                return false;
            }
        };
        let last_handled = self
            .runtime_registry
            .latest_handled_candle_ts(inst_id, time_interval);
        match WatchdogDecision::for_confirmed_candle(
            snap.ts,
            timeframe_ms,
            observed_at_ms,
            last_handled,
        ) {
            WatchdogDecision::AlreadyHandled => {
                debug!(
                    "跳过重复确认K线触发: inst_id={}, time_interval={}, ts={}, source={}",
                    inst_id, time_interval, snap.ts, source
                );
                return false;
            }
            WatchdogDecision::NotDue => {
                warn!(
                    event = "confirmed_candle_before_close_boundary",
                    inst_id,
                    time_interval,
                    candle_ts = snap.ts,
                    source,
                    "确认 K 线早于本地收盘边界，暂不进入策略回调"
                );
                return false;
            }
            WatchdogDecision::Expired => {
                if self
                    .runtime_registry
                    .record_expired(inst_id, time_interval, snap.ts)
                {
                    error!(
                        event = "expired_missed_trigger",
                        inst_id,
                        time_interval,
                        candle_ts = snap.ts,
                        source,
                        age_after_close_ms =
                            observed_at_ms.saturating_sub(snap.ts.saturating_add(timeframe_ms)),
                        action = "audit_only_no_execution_task",
                        "确认 K 线超过十秒时效窗口，禁止补执行和补下单"
                    );
                }
                return false;
            }
            WatchdogDecision::Trigger => {}
        }
        if !self
            .runtime_registry
            .try_claim_trigger(inst_id, time_interval, snap.ts)
        {
            return false;
        }
        let Some(trigger) = &self.strategy_trigger else {
            warn!(
                "未注入策略触发回调，跳过策略执行: inst_id={}, time_interval={}, ts={}, source={}",
                inst_id, time_interval, snap.ts, source
            );
            return false;
        };
        info!(
            "K线确认，触发策略执行: inst_id={}, time_interval={}, ts={}, source={}",
            inst_id, time_interval, snap.ts, source
        );
        trigger(inst_id.to_string(), time_interval.to_string(), snap.clone());
        self.runtime_registry.record_trigger_success(
            inst_id,
            time_interval,
            snap.ts,
            Utc::now().timestamp_millis(),
        );
        true
    }
    /// 更新最新 K 线，并且只把交易所已确认收盘的数据送入持久化链路。
    ///
    /// 未确认 K 线会被交易所高频重复推送，只更新进程内缓存；确认后才同步 Redis、
    /// 触发策略并写数据库，避免订阅规模放大 Redis 与数据库写入压力。
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
        // 同一时间戳从未确认推进到确认时，即使成交量未变化也必须接收，否则会漏掉收盘事件。
        let should_update = match self.cache.get_or_fetch(inst_id, time_interval).await {
            Some(cache_candle) => {
                new_ts > cache_candle.ts
                    || (new_ts == cache_candle.ts && {
                        let new_vol = latest.vol_ccy.parse::<f64>().unwrap_or(0.0);
                        let old_vol = cache_candle.vol_ccy.parse::<f64>().unwrap_or(0.0);
                        (latest.confirm == "1" && cache_candle.confirm != "1") || new_vol > old_vol
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
            if snap.confirm != "1" {
                self.cache.set(inst_id, time_interval, snap);
                return Ok(());
            }

            self.cache.set_both(inst_id, time_interval, &snap).await;
            self.trigger_confirmed_candle(inst_id, time_interval, snap, "websocket");
            let confirmed_candles = candles
                .into_iter()
                .filter(|candle| candle.confirm == "1")
                .collect::<Vec<_>>();
            if confirmed_candles.is_empty() {
                return Ok(());
            }

            // 持久化队列只承接已收盘事实，盘中更新由进程内缓存吸收。
            if let Some(sender) = &self.persist_sender {
                let task = PersistTask {
                    candles: confirmed_candles,
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
                    match model.upsert_batch(confirmed_candles, &inst, &per).await {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// 记录内存与 Redis 两类写入次数，用于锁定热路径的持久化边界。
    #[derive(Default)]
    struct RecordingCache {
        latest: Mutex<Option<CandlesEntity>>,
        memory_set_count: AtomicUsize,
        redis_set_count: AtomicUsize,
    }

    impl LatestCandleCacheProvider for RecordingCache {
        fn get(&self, _inst_id: &str, _period: &str) -> Option<CandlesEntity> {
            self.latest
                .lock()
                .expect("recording cache poisoned")
                .clone()
        }

        fn set(&self, _inst_id: &str, _period: &str, candle: CandlesEntity) {
            self.memory_set_count.fetch_add(1, Ordering::Relaxed);
            *self.latest.lock().expect("recording cache poisoned") = Some(candle);
        }

        fn remove(&self, _inst_id: &str, _period: &str) {
            *self.latest.lock().expect("recording cache poisoned") = None;
        }

        fn get_or_fetch<'a>(
            &'a self,
            _inst_id: &'a str,
            _period: &'a str,
        ) -> Pin<Box<dyn Future<Output = Option<CandlesEntity>> + Send + 'a>> {
            let latest = self
                .latest
                .lock()
                .expect("recording cache poisoned")
                .clone();
            Box::pin(async move { latest })
        }

        fn set_both<'a>(
            &'a self,
            _inst_id: &'a str,
            _period: &'a str,
            candle: &'a CandlesEntity,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async move {
                self.redis_set_count.fetch_add(1, Ordering::Relaxed);
                *self.latest.lock().expect("recording cache poisoned") = Some(candle.clone());
            })
        }
    }

    /// 构造交易所 WebSocket K 线更新，覆盖未确认到确认的状态推进。
    fn websocket_candle(ts: i64, confirm: &str) -> CandleOkxRespDto {
        CandleOkxRespDto {
            ts: ts.to_string(),
            o: "100".to_string(),
            h: "101".to_string(),
            l: "99".to_string(),
            c: "100".to_string(),
            v: "1".to_string(),
            vol_ccy: "100".to_string(),
            vol_ccy_quote: "100".to_string(),
            confirm: confirm.to_string(),
        }
    }

    /// 构造确认 K 线测试数据。
    fn confirmed_candle(ts: i64) -> CandlesEntity {
        CandlesEntity {
            id: None,
            ts,
            o: "100".to_string(),
            h: "101".to_string(),
            l: "99".to_string(),
            c: "100".to_string(),
            vol: "1".to_string(),
            vol_ccy: "100".to_string(),
            confirm: "1".to_string(),
            created_at: None,
            updated_at: None,
        }
    }

    /// 构造只记录调用次数的策略触发服务。
    fn service_with_trigger_counter(counter: Arc<AtomicUsize>) -> CandleService {
        let trigger: StrategyTrigger = Arc::new(move |_, _, _| {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        CandleService::new_with_strategy_trigger_and_runtime(
            default_provider(),
            None,
            trigger,
            Arc::new(CandleRuntimeRegistry::default()),
        )
    }

    #[test]
    fn expired_confirmation_never_calls_strategy_trigger() {
        let counter = Arc::new(AtomicUsize::new(0));
        let service = service_with_trigger_counter(Arc::clone(&counter));

        assert!(!service.trigger_confirmed_candle_at(
            "ETH-USDT-SWAP",
            "1m",
            confirmed_candle(1_000),
            "test",
            71_001,
        ));
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn exact_ten_second_confirmation_calls_strategy_trigger_once() {
        let counter = Arc::new(AtomicUsize::new(0));
        let service = service_with_trigger_counter(Arc::clone(&counter));

        assert!(service.trigger_confirmed_candle_at(
            "ETH-USDT-SWAP",
            "1m",
            confirmed_candle(1_000),
            "test",
            71_000,
        ));
        assert!(!service.trigger_confirmed_candle_at(
            "ETH-USDT-SWAP",
            "1m",
            confirmed_candle(1_000),
            "test",
            71_000,
        ));
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    /// 未确认更新不得产生外部写入，确认和重复确认只能落盘一次。
    #[tokio::test]
    async fn provisional_updates_stay_in_memory_and_confirmed_candle_is_persisted_once() {
        let cache = Arc::new(RecordingCache::default());
        let (persist_tx, mut persist_rx) = mpsc::unbounded_channel();
        let service = CandleService::new_with_persist_worker(cache.clone(), persist_tx);

        service
            .update_candles_batch(vec![websocket_candle(1_000, "0")], "ETH-USDT-SWAP", "1m")
            .await
            .expect("provisional update should succeed");

        assert_eq!(cache.memory_set_count.load(Ordering::Relaxed), 1);
        assert_eq!(cache.redis_set_count.load(Ordering::Relaxed), 0);
        assert!(persist_rx.try_recv().is_err());

        service
            .update_candles_batch(vec![websocket_candle(1_000, "1")], "ETH-USDT-SWAP", "1m")
            .await
            .expect("confirmed update should succeed");

        assert_eq!(cache.redis_set_count.load(Ordering::Relaxed), 1);
        let task = persist_rx
            .try_recv()
            .expect("confirmed candle should enter persistence queue");
        assert_eq!(task.candles.len(), 1);
        assert_eq!(task.candles[0].confirm, "1");

        service
            .update_candles_batch(vec![websocket_candle(1_000, "1")], "ETH-USDT-SWAP", "1m")
            .await
            .expect("duplicate confirmed update should be ignored");

        assert_eq!(cache.redis_set_count.load(Ordering::Relaxed), 1);
        assert!(persist_rx.try_recv().is_err());
    }
}
