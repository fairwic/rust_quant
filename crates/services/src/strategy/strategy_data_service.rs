//! 策略数据服务
//!
//! 负责策略数据的初始化（预热）：
//! - 加载历史K线数据
//! - 初始化策略指标缓存

use anyhow::{anyhow, Result};
use tracing::{debug, error, info, warn};

use rust_quant_domain::StrategyConfig;
use rust_quant_market::models::{CandlesModel, SelectCandleReqDto};
use rust_quant_strategies::framework::strategy_registry::get_strategy_registry;

/// 策略数据服务
///
/// 职责:
/// - 加载历史K线数据
/// - 初始化策略指标缓存
/// - 批量预热多个策略
pub struct StrategyDataService;

impl StrategyDataService {
    /// 初始化单个策略数据
    ///
    /// # 参数
    /// * `config` - 策略配置
    ///
    /// # 返回
    /// * `Ok(())` - 初始化成功
    /// * `Err` - 初始化失败
    pub async fn initialize_strategy(config: &StrategyConfig) -> Result<()> {
        let inst_id = &config.symbol;
        let period = config.timeframe.as_str();
        let strategy_type = &config.strategy_type;

        info!(
            "🔥 预热策略数据: inst_id={}, period={}, type={:?}",
            inst_id, period, strategy_type
        );

        // 1. 获取策略执行器
        let registry = get_strategy_registry();
        let executor = registry
            .get(strategy_type.as_str())
            .map_err(|e| anyhow!("获取策略执行器失败: {}", e))?;

        // 2. 加载历史K线数据
        let candles_model = CandlesModel::new();
        let dto = SelectCandleReqDto {
            inst_id: inst_id.clone(),
            time_interval: period.to_string(),
            limit: 500, // 加载500根K线用于预热
            select_time: None,
            confirm: Some(1), // 只获取已确认的K线
        };

        let mut candles = candles_model
            .get_all(dto)
            .await
            .map_err(|e| anyhow!("加载历史K线失败: {}", e))?;

        if candles.is_empty() {
            return Err(anyhow!(
                "历史K线数据为空: inst_id={}, period={}",
                inst_id,
                period
            ));
        }

        // 按时间升序排列
        candles.sort_unstable_by(|a, b| a.ts.cmp(&b.ts));

        info!(
            "✅ 加载 {} 根历史K线: inst_id={}, period={}",
            candles.len(),
            inst_id,
            period
        );

        // 3. 调用策略执行器初始化数据
        // strategies::StrategyConfig 就是 domain::StrategyConfig 的重导出
        let strategy_config = rust_quant_strategies::framework::config::strategy_config::StrategyConfig::new(
            config.id,
            config.strategy_type,
            config.symbol.clone(),
            config.timeframe,
            config.parameters.clone(),
            config.risk_config.clone(),
        );

        let result = executor
            .initialize_data(&strategy_config, inst_id, period, candles)
            .await?;

        info!(
            "✅ 策略数据预热完成: hash_key={}, last_ts={}",
            result.hash_key, result.last_timestamp
        );

        Ok(())
    }

    /// 批量初始化多个策略数据
    ///
    /// # 参数
    /// * `configs` - 策略配置列表
    ///
    /// # 返回
    /// * `Vec<Result<()>>` - 每个策略的初始化结果
    pub async fn initialize_multiple_strategies(configs: &[StrategyConfig]) -> Vec<Result<()>> {
        let mut results = Vec::with_capacity(configs.len());

        for config in configs {
            let result = Self::initialize_strategy(config).await;

            if let Err(ref e) = result {
                error!(
                    "❌ 策略预热失败: id={}, symbol={}, error={}",
                    config.id, config.symbol, e
                );
            } else {
                debug!(
                    "✅ 策略预热成功: id={}, symbol={}",
                    config.id, config.symbol
                );
            }

            results.push(result);
        }

        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let fail_count = results.len() - success_count;

        if fail_count > 0 {
            warn!(
                "⚠️  批量预热完成: 成功 {}, 失败 {}",
                success_count, fail_count
            );
        } else {
            info!("✅ 批量预热全部成功: {} 个策略", success_count);
        }

        results
    }
}

