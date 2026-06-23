use anyhow::Result;
use rust_quant_infrastructure::external_data::DuneQueryPerformance;
use rust_quant_services::market::{DuneMarketSyncService, ExternalMarketSyncService};
use std::collections::HashMap;
use tracing::{error, info};
pub struct ExternalMarketSyncJob;
impl ExternalMarketSyncJob {
    pub fn new() -> Self {
        Self
    }
}
impl Default for ExternalMarketSyncJob {
    fn default() -> Self {
        Self::new()
    }
}
impl ExternalMarketSyncJob {
    /// 封装当前函数，减少行情数据调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    pub async fn sync_hyperliquid_coin(
        coin: &str,
        start_time: i64,
        end_time: i64,
        snapshot_time: i64,
    ) -> Result<()> {
        let service = ExternalMarketSyncService::new()?;
        log_sync_result(
            format!("coin={}", coin),
            service
                .sync_hyperliquid_coin(coin, start_time, end_time, snapshot_time)
                .await,
            "外部市场快照同步",
        )
    }
    /// 同步 行情与市场数据 数据，保证本地状态与外部事实源保持一致。
    pub async fn sync_dune_template(
        metric_type: &str,
        symbol: &str,
        template_path: &str,
        params: HashMap<String, String>,
        performance: DuneQueryPerformance,
    ) -> Result<()> {
        let service = DuneMarketSyncService::new()?;
        log_sync_result(
            format!("metric_type={}, symbol={}", metric_type, symbol),
            service
                .sync_template_file(
                    metric_type.to_string(),
                    symbol.to_string(),
                    template_path,
                    params,
                    performance,
                )
                .await,
            "Dune 外部市场快照同步",
        )
    }
}
/// 封装当前函数，减少行情数据调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
fn log_sync_result(context: String, result: Result<usize>, label: &str) -> Result<()> {
    match result {
        Ok(saved) => {
            info!("✅ {}完成: {}, saved={}", label, context, saved);
            Ok(())
        }
        Err(err) => {
            error!("❌ {}失败: {}, err={}", label, context, err);
            Err(err)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::log_sync_result;
    use anyhow::anyhow;
    #[test]
    fn log_sync_result_propagates_error() {
        let result = log_sync_result(
            "metric_type=test, symbol=ETH".to_string(),
            Err(anyhow!("boom")),
            "Dune",
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "boom");
    }
    #[test]
    fn log_sync_result_keeps_success() {
        let result = log_sync_result("metric_type=test, symbol=ETH".to_string(), Ok(3), "Dune");
        assert!(result.is_ok());
    }
}
