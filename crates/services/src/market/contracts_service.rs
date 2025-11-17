//! 合约服务
//!
//! 封装交易所Contracts API调用（持仓量、成交量等）
//!
//! # 架构
//! - 依赖domain::traits::ExchangeContracts接口
//! - 支持多交易所扩展

use anyhow::Result;
use rust_quant_infrastructure::ExchangeFactory;
use tracing::info;

/// 合约服务
///
/// 职责：封装交易所合约API调用（持仓量、成交量等），支持多交易所
pub struct ContractsService;

impl ContractsService {
    pub fn new() -> Self {
        Self
    }

    /// 从交易所获取持仓量和成交量数据
    ///
    /// # Arguments
    /// * `inst_id` - 交易对基础币种（如 "BTC"）
    /// * `begin` - 开始时间
    /// * `end` - 结束时间
    /// * `period` - 时间周期（如 "1D"）
    ///
    /// # Returns
    /// * 持仓量和成交量数据（JSON格式）
    ///
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE）
    pub async fn fetch_open_interest_volume_from_exchange(
        &self,
        inst_id: Option<&str>,
        begin: Option<i64>,
        end: Option<i64>,
        period: Option<&str>,
    ) -> Result<serde_json::Value> {
        let exchange = ExchangeFactory::create_default_contracts()?;
        let items = exchange
            .fetch_open_interest_volume(inst_id, begin, end, period)
            .await?;

        info!("✅ 从交易所 {} 获取了持仓量数据", exchange.name());
        Ok(items)
    }
}

impl Default for ContractsService {
    fn default() -> Self {
        Self::new()
    }
}
