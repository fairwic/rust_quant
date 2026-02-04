//! 策略数据服务
//!
//! 负责策略数据的初始化（预热）：
//! - 加载历史K线数据
//! - 初始化策略指标缓存

use anyhow::{anyhow, Result};
use tracing::{debug, error, info, warn};

use rust_quant_common::CandleItem;
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
    fn read_env_usize(key: &str) -> Option<usize> {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
    }

    fn determine_warmup_limit(parameters: &serde_json::Value) -> usize {
        const DEFAULT_WARMUP_LIMIT: usize = 500;
        const DEFAULT_WARMUP_LIMIT_MAX: usize = 10_000;

        let base_limit =
            Self::read_env_usize("STRATEGY_WARMUP_LIMIT").unwrap_or(DEFAULT_WARMUP_LIMIT);
        let max_limit =
            Self::read_env_usize("STRATEGY_WARMUP_LIMIT_MAX").unwrap_or(DEFAULT_WARMUP_LIMIT_MAX);

        let min_k_line_num = parameters
            .get("min_k_line_num")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(0);

        base_limit.max(min_k_line_num).min(max_limit)
    }

    fn candle_entity_to_item(c: &rust_quant_market::models::CandlesEntity) -> Result<CandleItem> {
        let o =
            c.o.parse::<f64>()
                .map_err(|e| anyhow!("解析开盘价失败: {}", e))?;
        let h =
            c.h.parse::<f64>()
                .map_err(|e| anyhow!("解析最高价失败: {}", e))?;
        let l =
            c.l.parse::<f64>()
                .map_err(|e| anyhow!("解析最低价失败: {}", e))?;
        let close =
            c.c.parse::<f64>()
                .map_err(|e| anyhow!("解析收盘价失败: {}", e))?;
        let v = c
            .vol_ccy
            .parse::<f64>()
            .map_err(|e| anyhow!("解析成交量失败: {}", e))?;
        let confirm = c
            .confirm
            .parse::<i32>()
            .map_err(|e| anyhow!("解析 confirm 失败: {}", e))?;

        Ok(CandleItem {
            o,
            h,
            l,
            c: close,
            v,
            ts: c.ts,
            confirm,
        })
    }

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
        let warmup_limit = Self::determine_warmup_limit(&config.parameters);
        let dto = SelectCandleReqDto {
            inst_id: inst_id.clone(),
            time_interval: period.to_string(),
            limit: warmup_limit,
            select_time: None,
            confirm: Some(1), // 只获取已确认的K线
        };
        info!(
            "预热K线数量: inst_id={}, period={}, limit={}",
            inst_id, period, warmup_limit
        );

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

        let candle_items = candles
            .iter()
            .map(Self::candle_entity_to_item)
            .collect::<Result<Vec<_>>>()?;

        info!(
            "✅ 加载 {} 根历史K线: inst_id={}, period={}",
            candles.len(),
            inst_id,
            period
        );

        // 3. 调用策略执行器初始化数据
        // strategies::StrategyConfig 就是 domain::StrategyConfig 的重导出
        let strategy_config =
            rust_quant_strategies::framework::config::strategy_config::StrategyConfig::new(
                config.id,
                config.strategy_type,
                config.symbol.clone(),
                config.timeframe,
                config.parameters.clone(),
                config.risk_config.clone(),
            );

        let result = executor
            .initialize_data(&strategy_config, inst_id, period, candle_items)
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

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static ENV_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn set_env(key: &str, value: Option<&str>) {
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn warmup_limit_defaults_to_max_of_base_and_min_k_line_num() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let old_base = std::env::var("STRATEGY_WARMUP_LIMIT").ok();
        let old_max = std::env::var("STRATEGY_WARMUP_LIMIT_MAX").ok();
        set_env("STRATEGY_WARMUP_LIMIT", None);
        set_env("STRATEGY_WARMUP_LIMIT_MAX", None);

        let params = serde_json::json!({"min_k_line_num": 3600});
        let limit = StrategyDataService::determine_warmup_limit(&params);
        assert_eq!(limit, 3600);

        set_env("STRATEGY_WARMUP_LIMIT", old_base.as_deref());
        set_env("STRATEGY_WARMUP_LIMIT_MAX", old_max.as_deref());
    }

    #[test]
    fn warmup_limit_is_capped_by_max() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let old_base = std::env::var("STRATEGY_WARMUP_LIMIT").ok();
        let old_max = std::env::var("STRATEGY_WARMUP_LIMIT_MAX").ok();
        set_env("STRATEGY_WARMUP_LIMIT", Some("500"));
        set_env("STRATEGY_WARMUP_LIMIT_MAX", Some("2000"));

        let params = serde_json::json!({"min_k_line_num": 3600});
        let limit = StrategyDataService::determine_warmup_limit(&params);
        assert_eq!(limit, 2000);

        set_env("STRATEGY_WARMUP_LIMIT", old_base.as_deref());
        set_env("STRATEGY_WARMUP_LIMIT_MAX", old_max.as_deref());
    }
}
