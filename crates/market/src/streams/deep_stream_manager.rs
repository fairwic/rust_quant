use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use okx::dto::market_dto::TradeOkxResDto;
use okx::dto::websocket_dto::OkxWsResDto;
use okx::websocket::auto_reconnect_client::AutoReconnectWebsocketClient;
use okx::websocket::{Args, ChannelType};
use rust_decimal::Decimal;
use rust_quant_domain::entities::{FundFlow, FundFlowSide};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};
use tracing::{error, info, warn};

/// 深度流管理器
/// 负责动态管理 WebSocket 连接，订阅/取消订阅重点币种的成交数据
pub struct DeepStreamManager {
    // client is !Sync, so wrap in Mutex to make DeepStreamManager Sync
    client: Mutex<AutoReconnectWebsocketClient>,
    active_subs: Arc<Mutex<HashSet<String>>>,
    // 限流器状态: (上次请求时间, 剩余令牌数)
    rate_limit_state: Arc<Mutex<(Instant, u32)>>,
    // 资金流数据发送端
    flow_tx: mpsc::UnboundedSender<FundFlow>,
}

impl DeepStreamManager {
    /// 创建新的深度流管理器
    /// # Arguments
    /// * `flow_tx` - 用于发送解析后的资金流数据的通道
    pub fn new(flow_tx: mpsc::UnboundedSender<FundFlow>) -> Self {
        Self {
            client: Mutex::new(AutoReconnectWebsocketClient::new_public()),
            active_subs: Arc::new(Mutex::new(HashSet::new())),
            // OKX Limit: 480 req / hour -> ~8 req / min.
            //  conservative: 1 req / 2s.
            rate_limit_state: Arc::new(Mutex::new((Instant::now(), 10))),
            flow_tx,
        }
    }

    /// 启动管理器
    pub async fn start(&self) -> Result<()> {
        info!("Starting DeepStreamManager...");
        // Client is cheap to clone (Arc internals)
        let client = self.client.lock().await.clone();
        let mut rx = client
            .start()
            .await
            .map_err(|e| anyhow!("Failed to start WS client: {}", e))?;

        let flow_tx = self.flow_tx.clone();

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                // 解析 Trade 数据
                if let Ok(trade_resp) =
                    serde_json::from_value::<OkxWsResDto<TradeOkxResDto>>(msg.clone())
                {
                    for trade in trade_resp.data {
                        if let Ok(flow) = Self::map_to_fund_flow(trade) {
                            if let Err(e) = flow_tx.send(flow) {
                                error!("Failed to send fund flow: {:?}", e);
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// 提升关注: 订阅指定币种的 Trades
    pub async fn promote(&self, symbol: &str) -> Result<()> {
        if self.check_sub_exists(symbol).await {
            return Ok(());
        }

        self.wait_for_rate_limit().await;

        let args = Args::new().with_inst_id(symbol.to_string());

        let client = self.client.lock().await.clone();
        if let Err(e) = client.subscribe(ChannelType::Trades, args).await {
            error!("Failed to subscribe trades for {}: {:?}", symbol, e);
            return Err(anyhow!("Sub failed: {:?}", e));
        }

        let mut subs = self.active_subs.lock().await;
        subs.insert(symbol.to_string());
        info!("Promoted (Subscribed) {}", symbol);
        Ok(())
    }

    /// 降低关注: 取消订阅
    pub async fn demote(&self, symbol: &str) -> Result<()> {
        if !self.check_sub_exists(symbol).await {
            return Ok(());
        }

        self.wait_for_rate_limit().await;

        let args = Args::new().with_inst_id(symbol.to_string());

        let client = self.client.lock().await.clone();
        if let Err(e) = client.unsubscribe(ChannelType::Trades, args).await {
            error!("Failed to unsubscribe trades for {}: {:?}", symbol, e);
            return Err(anyhow!("Unsub failed: {:?}", e));
        }

        let mut subs = self.active_subs.lock().await;
        subs.remove(symbol);
        info!("Demoted (Unsubscribed) {}", symbol);
        Ok(())
    }

    async fn check_sub_exists(&self, symbol: &str) -> bool {
        let subs = self.active_subs.lock().await;
        subs.contains(symbol)
    }

    /// 简单的令牌桶限流 (Token Bucket)
    /// 限制订阅/取消订阅频率，防止触发 480 req/hour
    async fn wait_for_rate_limit(&self) {
        let mut state = self.rate_limit_state.lock().await;
        let now = Instant::now();
        let elapsed = now.duration_since(state.0).as_secs();

        // 每 10 秒恢复 1 个令牌
        let new_tokens = (elapsed / 10) as u32;
        if new_tokens > 0 {
            state.1 = std::cmp::min(state.1 + new_tokens, 20); // Max capacity 20
            state.0 = now;
        }

        if state.1 == 0 {
            warn!("Rate limit hit, waiting...");
            tokio::time::sleep(Duration::from_secs(5)).await;
            state.1 = 1; // Grant one after wait
            state.0 = Instant::now();
        }

        state.1 -= 1;
    }

    fn map_to_fund_flow(t: TradeOkxResDto) -> Result<FundFlow> {
        let price = Decimal::from_str(&t.px)?;
        let amount = Decimal::from_str(&t.sz)?;

        // OKX Trade side: "buy" or "sell"
        let side = match t.side.as_str() {
            "buy" => FundFlowSide::Inflow,
            "sell" => FundFlowSide::Outflow,
            _ => return Err(anyhow!("Unknown side: {}", t.side)),
        };

        // Timestamp
        let ts_millis = t.ts.parse::<i64>().unwrap_or(0);
        let timestamp = DateTime::from_timestamp_millis(ts_millis).unwrap_or(Utc::now());

        // Value estimation (Price * Amount) - NOTE: Contract multiplier might be needed for SWAP
        // For simplicity assuming Spot or 1:1 for now, or raw contract value
        let value = price * amount;

        Ok(FundFlow {
            symbol: t.inst_id,
            value,
            side,
            timestamp,
        })
    }
}
