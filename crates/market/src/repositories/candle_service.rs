use std::sync::Arc;
use tokio::sync::mpsc;

use rust_quant_infrastructure::cache::{default_provider, LatestCandleCacheProvider};
use rust_quant_market::models::CandlesEntity;
use rust_quant_market::models::CandlesModel;
use rust_quant_strategies::strategy_manager::get_strategy_manager;
use rust_quant_market::repositories::persist_worker::PersistTask;
use okx::dto::market_dto::CandleOkxRespDto;
use tracing::{debug, error, info};

pub struct CandleService {
    cache: Arc<dyn LatestCandleCacheProvider>,
    persist_sender: Option<mpsc::UnboundedSender<PersistTask>>,
}

impl CandleService {
    pub fn new() -> Self {
        Self {
            cache: default_provider(),
            persist_sender: None,
        }
    }
    
    pub fn new_with_cache(cache: Arc<dyn LatestCandleCacheProvider>) -> Self {
        Self { 
            cache,
            persist_sender: None,
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
        println!("candles: {:?}", candles);
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
            let snap = CandlesEntity {
                ts: new_ts,
                o: latest.o.clone(),
                h: latest.h.clone(),
                l: latest.l.clone(),
                c: latest.c.clone(),
                vol: latest.v.clone(),
                vol_ccy: latest.vol_ccy.clone(),
                confirm: latest.confirm.clone(),
                updated_at: Some(rbatis::rbdc::DateTime::now()),
            };
            
            self.cache.set_both(inst_id, time_interval, &snap).await;
            
            // 🚀 K线确认时触发策略（不阻塞）
            if snap.confirm == "1" {
                info!("📈 K线已确认，触发策略: inst_id={}, time_interval={}, ts={}", 
                    inst_id, time_interval, new_ts);
                
                let inst_id_owned = inst_id.to_string();
                let time_interval_owned = time_interval.to_string();
                
                tokio::spawn(async move {
                    let strategy_manager = get_strategy_manager();
                    if let Err(e) = strategy_manager
                        .run_ready_to_order_with_manager(&inst_id_owned, &time_interval_owned, Some(snap))
                        .await
                    {
                        error!("❌ 策略执行失败: inst_id={}, time_interval={}, error={}", 
                            inst_id_owned, time_interval_owned, e);
                    } else {
                        info!("✅ 策略执行完成: inst_id={}, time_interval={}", 
                            inst_id_owned, time_interval_owned);
                    }
                });
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
                    let model = CandlesModel::new().await;
                    match model.upsert_batch(candles, &inst, &per).await {
                        Ok(rows) => {
                            debug!("✅ 批量写入成功: inst_id={}, time_interval={}, rows={}", 
                                inst, per, rows);
                        }
                        Err(e) => {
                            error!("❌ 批量写入失败: inst_id={}, time_interval={}, error={:?}", 
                                inst, per, e);
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
        self.update_candles_batch(candle, inst_id, time_interval).await
    }
}
