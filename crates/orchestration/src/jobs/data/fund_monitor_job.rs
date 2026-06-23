use anyhow::Result;
use rust_quant_market::streams::deep_stream_manager::DeepStreamManager;
use rust_quant_services::market::{
    CandleService, FlowAnalyzer, MarketVelocityStrategySignalConfig, ScannerService,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{sleep, Duration, Instant};
use tracing::{error, info, instrument};
/// 资金异动监控任务
///
/// 负责定时运行 全市场扫描(ScannerService)，发现异常后打印日志/报警
/// 并根据异动情况动态调整 WebSocket 订阅 (DeepStreamManager)
pub struct FundMonitorJob {
    /// 市场扫描服务。
    scanner_service: ScannerService,
    /// 行情流管理器。
    stream_manager: Arc<DeepStreamManager>,
    // 记录正在监控的币种及其开始时间: Symbol -> PromotedAt
    active_promotions: HashMap<String, Instant>,
    /// 秒级时长。
    interval_secs: u64,
    /// 秒级时长。
    promotion_duration_secs: u64,
}
use rust_quant_domain::traits::fund_monitoring_repository::{
    FundFlowAlertRepository, MarketAnomalyRepository,
};
impl FundMonitorJob {
    pub fn new(
        interval_secs: u64,
        anomaly_repo: Arc<dyn MarketAnomalyRepository>,
        alert_repo: Arc<dyn FundFlowAlertRepository>,
    ) -> Result<(Self, FlowAnalyzer)> {
        Self::new_with_candle_service(interval_secs, anomaly_repo, alert_repo, None)
    }
    /// 提供newwithK 线service的集中实现，避免量化核心调用方重复处理相同细节。
    pub fn new_with_candle_service(
        interval_secs: u64,
        anomaly_repo: Arc<dyn MarketAnomalyRepository>,
        alert_repo: Arc<dyn FundFlowAlertRepository>,
        candle_service: Option<Arc<CandleService>>,
    ) -> Result<(Self, FlowAnalyzer)> {
        Self::new_with_candle_service_and_market_velocity_signal_config(
            interval_secs,
            anomaly_repo,
            alert_repo,
            candle_service,
            None,
        )
    }
    /// 提供newwithK 线serviceand市场动量信号配置的集中实现，避免量化核心调用方重复处理相同细节。
    pub fn new_with_candle_service_and_market_velocity_signal_config(
        interval_secs: u64,
        anomaly_repo: Arc<dyn MarketAnomalyRepository>,
        alert_repo: Arc<dyn FundFlowAlertRepository>,
        candle_service: Option<Arc<CandleService>>,
        market_velocity_signal_config: Option<MarketVelocityStrategySignalConfig>,
    ) -> Result<(Self, FlowAnalyzer)> {
        // 创建 FlowAnalyzer 同时获取 manager 句柄
        let (analyzer, manager) = FlowAnalyzer::new(alert_repo);
        let job = Self {
            scanner_service:
                ScannerService::new_with_technical_candle_service_and_market_velocity_signal_config(
                    anomaly_repo,
                    candle_service,
                    market_velocity_signal_config,
                )?,
            stream_manager: manager,
            active_promotions: HashMap::new(),
            interval_secs,
            promotion_duration_secs: 600, // 默认关联监控 10 分钟
        };
        Ok((job, analyzer))
    }
    /// 运行任务 (阻塞式循环)
    /// 实际生产中应配合 tokio::spawn 或 JobScheduler 使用
    pub async fn run_loop(&mut self) {
        info!(
            "Starting Fund Movement Monitor Job (Interval: {}s)",
            self.interval_secs
        );
        // 初始化：从数据库恢复状态
        if let Err(e) = self.scanner_service.initialize().await {
            error!("Failed to initialize scanner service: {:?}", e);
        }
        loop {
            // 1. 执行扫描
            match self.scanner_service.scan_and_analyze().await {
                Ok(anomalies) => {
                    // 2. 处理异动 -> 提升关注 (Promote)
                    if !anomalies.is_empty() {
                        info!("Found {} anomalies in this scan:", anomalies.len());
                        for (symbol, vol_delta) in anomalies.iter().take(5) {
                            // 只处理前5个最剧烈的
                            info!(
                                "🚨 [ANOMALY] {}: 24h Vol Changed by +{} USDT",
                                symbol, vol_delta
                            );
                            // 尝试 Promote
                            if !self.active_promotions.contains_key(symbol) {
                                match self.stream_manager.promote(symbol).await {
                                    Ok(_) => {
                                        info!("🔥 Promoted {} to Deep Stream", symbol);
                                        self.active_promotions
                                            .insert(symbol.clone(), Instant::now());
                                    }
                                    Err(e) => error!("Failed to promote {}: {:?}", symbol, e),
                                }
                            } else {
                                // 已经处于Promote状态，刷新时间（续期）
                                self.active_promotions
                                    .insert(symbol.clone(), Instant::now());
                            }
                        }
                    } else {
                        info!("No significant anomalies detected.");
                    }
                }
                Err(e) => {
                    error!("Error during scan: {:?}", e);
                }
            }
            // 3. 清理过期关注 (Demote)
            let now = Instant::now();
            let expired: Vec<String> = self
                .active_promotions
                .iter()
                .filter(|(_, &start_time)| {
                    now.duration_since(start_time).as_secs() > self.promotion_duration_secs
                })
                .map(|(k, _)| k.clone())
                .collect();
            for symbol in expired {
                match self.stream_manager.demote(&symbol).await {
                    Ok(_) => {
                        info!("❄️ Demoted {} from Deep Stream (Expired)", symbol);
                        self.active_promotions.remove(&symbol);
                    }
                    Err(e) => error!("Failed to demote {}: {:?}", symbol, e),
                }
            }
            // 4. 等待下一次
            sleep(Duration::from_secs(self.interval_secs)).await;
        }
    }
}
