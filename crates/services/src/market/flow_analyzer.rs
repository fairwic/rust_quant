use anyhow::Result;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rust_decimal::Decimal;
use rust_quant_domain::entities::{FundFlow, FundFlowAlert, FundFlowSide};
use rust_quant_domain::traits::fund_monitoring_repository::FundFlowAlertRepository;
use rust_quant_market::streams::deep_stream_manager::DeepStreamManager;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// 告警冷却期 (秒)
const ALERT_COOLDOWN_SECS: i64 = 60;

/// 资金流向分析器
pub struct FlowAnalyzer {
    stream_manager: Arc<DeepStreamManager>,
    flow_rx: mpsc::UnboundedReceiver<FundFlow>,
    history: HashMap<String, VecDeque<FundFlow>>,
    alert_repo: Arc<dyn FundFlowAlertRepository>,
    /// 上次告警时间 (用于冷却期)
    last_alert_times: HashMap<String, DateTime<Utc>>,
}

impl FlowAnalyzer {
    pub fn new(alert_repo: Arc<dyn FundFlowAlertRepository>) -> (Self, Arc<DeepStreamManager>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let manager = Arc::new(DeepStreamManager::new(tx));

        (
            Self {
                stream_manager: manager.clone(),
                flow_rx: rx,
                history: HashMap::new(),
                alert_repo,
                last_alert_times: HashMap::new(),
            },
            manager,
        )
    }

    pub async fn run(mut self) {
        info!("Starting FlowAnalyzer...");

        if let Err(e) = self.stream_manager.start().await {
            warn!("Failed to start stream manager: {:?}", e);
            return;
        }

        while let Some(flow) = self.flow_rx.recv().await {
            self.process_flow(flow).await;
        }
    }

    async fn process_flow(&mut self, flow: FundFlow) {
        let symbol = flow.symbol.clone();
        let now = Utc::now();

        // 检查冷却期
        if let Some(&last_time) = self.last_alert_times.get(&symbol) {
            if now - last_time < ChronoDuration::seconds(ALERT_COOLDOWN_SECS) {
                // 仍在冷却期，仅更新历史，不触发告警
                self.update_history(flow);
                return;
            }
        }

        self.update_history(flow.clone());

        // 计算净流入
        let window = self.history.get(&symbol);
        if window.is_none() {
            return;
        }
        let window = window.unwrap();

        let mut net_inflow = Decimal::ZERO;
        let mut total_vol = Decimal::ZERO;

        for f in window.iter() {
            total_vol += f.value;
            match f.side {
                FundFlowSide::Inflow => net_inflow += f.value,
                FundFlowSide::Outflow => net_inflow -= f.value,
            }
        }

        // 异动阈值: |净流入| > 100,000 USDT
        if net_inflow.abs() > Decimal::from(100_000) {
            let direction = if net_inflow > Decimal::ZERO {
                "INFLOW"
            } else {
                "OUTFLOW"
            };

            info!(
                "🌊 [FLOW ALERT] {}: Net {} = {} USDT (Window: 60s, TotalVol: {})",
                symbol, direction, net_inflow, total_vol
            );

            // 更新冷却时间
            self.last_alert_times.insert(symbol.clone(), now);

            // 持久化
            let alert = FundFlowAlert {
                id: None,
                symbol: symbol.clone(),
                net_inflow,
                total_volume: total_vol,
                side: if net_inflow > Decimal::ZERO {
                    FundFlowSide::Inflow
                } else {
                    FundFlowSide::Outflow
                },
                window_secs: 60,
                alert_at: now,
            };

            if let Err(e) = self.alert_repo.save(&alert).await {
                error!("Failed to save fund flow alert for {}: {:?}", symbol, e);
            }
        }
    }

    fn update_history(&mut self, flow: FundFlow) {
        let symbol = flow.symbol.clone();
        let now = Utc::now();

        let window = self.history.entry(symbol).or_default();
        window.push_back(flow);

        // 清理超过 60 秒的数据
        while let Some(front) = window.front() {
            if now - front.timestamp > ChronoDuration::seconds(60) {
                window.pop_front();
            } else {
                break;
            }
        }
    }
}
