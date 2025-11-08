//! 策略运行器 V2 - 简化版
//! 
//! 通过 services 层调用业务逻辑，orchestration 只做调度和协调

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info};

use rust_quant_domain::{Timeframe, StrategyType};
use rust_quant_services::strategy::StrategyExecutionService;

/// 策略执行状态跟踪 - 用于时间戳去重
#[derive(Debug, Clone)]
struct StrategyExecutionState {
    timestamp: i64,
    start_time: SystemTime,
}

/// 全局策略执行状态管理器 - 防止重复处理相同时间戳的K线
static STRATEGY_EXECUTION_STATES: Lazy<DashMap<String, StrategyExecutionState>> =
    Lazy::new(|| DashMap::new());

/// 策略执行状态管理器
pub struct StrategyExecutionStateManager;

impl StrategyExecutionStateManager {
    /// 检查并标记策略执行状态
    /// 返回 true 表示可以执行，false 表示应该跳过（正在处理或已处理）
    pub fn try_mark_processing(key: &str, timestamp: i64) -> bool {
        let state_key = format!("{}_{}", key, timestamp);

        // 检查是否已经在处理
        if STRATEGY_EXECUTION_STATES.contains_key(&state_key) {
            debug!("跳过重复处理: key={}, timestamp={}", key, timestamp);
            return false;
        }

        // 标记为正在处理
        let state = StrategyExecutionState {
            timestamp,
            start_time: SystemTime::now(),
        };

        STRATEGY_EXECUTION_STATES.insert(state_key.clone(), state);
        info!("标记策略执行状态: key={}, timestamp={}", key, timestamp);
        true
    }

    /// 完成策略执行，清理状态
    pub fn mark_completed(key: &str, timestamp: i64) {
        let state_key = format!("{}_{}", key, timestamp);
        if let Some((_, state)) = STRATEGY_EXECUTION_STATES.remove(&state_key) {
            let duration = SystemTime::now()
                .duration_since(state.start_time)
                .unwrap_or(Duration::from_millis(0));
            info!(
                "策略执行完成: key={}, timestamp={}, 耗时={:?}",
                key, timestamp, duration
            );
        }
    }

    /// 清理过期的执行状态（超过5分钟的记录）
    pub fn cleanup_expired_states() {
        let now = SystemTime::now();
        let mut expired_keys = Vec::new();

        for entry in STRATEGY_EXECUTION_STATES.iter() {
            if let Ok(duration) = now.duration_since(entry.value().start_time) {
                if duration > Duration::from_secs(300) {
                    expired_keys.push(entry.key().clone());
                }
            }
        }

        for key in expired_keys {
            STRATEGY_EXECUTION_STATES.remove(&key);
            debug!("清理过期状态: {}", key);
        }
    }

    /// 获取统计信息
    pub fn get_stats() -> (usize, Vec<String>) {
        let count = STRATEGY_EXECUTION_STATES.len();
        let keys: Vec<String> = STRATEGY_EXECUTION_STATES
            .iter()
            .map(|e| e.key().clone())
            .collect();
        (count, keys)
    }
}

/// 执行策略 - 简化版接口
/// 
/// # Arguments
/// * `inst_id` - 交易对（如 "BTC-USDT"）
/// * `timeframe` - 时间周期
/// * `strategy_type` - 策略类型
/// * `config_id` - 策略配置ID（可选）
/// 
/// # Returns
/// 返回策略信号结果
pub async fn execute_strategy(
    inst_id: &str,
    timeframe: Timeframe,
    strategy_type: StrategyType,
    config_id: Option<i64>,
) -> Result<()> {
    let key = format!("{}_{:?}_{:?}", inst_id, timeframe, strategy_type);
    
    info!(
        "🚀 开始执行策略: inst_id={}, timeframe={:?}, strategy={:?}",
        inst_id, timeframe, strategy_type
    );

    // 检查是否应该跳过（去重）
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs() as i64;
    
    if !StrategyExecutionStateManager::try_mark_processing(&key, timestamp) {
        debug!("策略正在执行中，跳过: {}", key);
        return Ok(());
    }

    // 通过 StrategyExecutionService 执行策略
    let service = StrategyExecutionService::new();
    
    let result = service
        .execute_strategy(inst_id, timeframe, strategy_type, config_id)
        .await;

    // 标记完成
    StrategyExecutionStateManager::mark_completed(&key, timestamp);

    match result {
        Ok(signal_result) => {
            info!(
                "✅ 策略执行成功: {} - {:?}",
                key, signal_result.signal
            );
            Ok(())
        }
        Err(e) => {
            error!("❌ 策略执行失败: {} - {:?}", key, e);
            Err(e)
        }
    }
}

/// 批量执行多个策略
pub async fn execute_multiple_strategies(
    strategies: Vec<(String, Timeframe, StrategyType, Option<i64>)>,
) -> Result<Vec<Result<()>>> {
    info!("🚀 批量执行 {} 个策略", strategies.len());
    
    let mut results = Vec::new();
    
    for (inst_id, timeframe, strategy_type, config_id) in strategies {
        let result = execute_strategy(&inst_id, timeframe, strategy_type, config_id).await;
        results.push(result);
    }
    
    Ok(results)
}

/// 测试随机策略 - 保持向后兼容
/// 
/// 这是一个兼容接口，实际通过 services 层调用
pub async fn test_random_strategy(
    inst_id: String,
    period: String,
) -> Result<()> {
    info!("🎲 测试随机策略: inst_id={}, period={}", inst_id, period);
    
    // 解析时间周期
    let timeframe = parse_period_to_timeframe(&period)?;
    
    // 默认使用 Vegas 策略
    execute_strategy(&inst_id, timeframe, StrategyType::Vegas, None).await
}

/// 测试指定策略 - 保持向后兼容
pub async fn test_specified_strategy(
    inst_id: String,
    period: String,
    strategy_type: StrategyType,
    config_id: Option<i64>,
) -> Result<()> {
    info!(
        "🎯 测试指定策略: inst_id={}, period={}, strategy={:?}",
        inst_id, period, strategy_type
    );
    
    // 解析时间周期
    let timeframe = parse_period_to_timeframe(&period)?;
    
    execute_strategy(&inst_id, timeframe, strategy_type, config_id).await
}

/// 辅助函数：解析 period 字符串到 Timeframe
fn parse_period_to_timeframe(period: &str) -> Result<Timeframe> {
    match period {
        "1m" => Ok(Timeframe::M1),
        "5m" => Ok(Timeframe::M5),
        "15m" => Ok(Timeframe::M15),
        "30m" => Ok(Timeframe::M30),
        "1H" | "1h" => Ok(Timeframe::H1),
        "2H" | "2h" => Ok(Timeframe::H2),
        "4H" | "4h" => Ok(Timeframe::H4),
        "6H" | "6h" => Ok(Timeframe::H6),
        "12H" | "12h" => Ok(Timeframe::H12),
        "1D" | "1d" => Ok(Timeframe::D1),
        "1W" | "1w" => Ok(Timeframe::W1),
        _ => Err(anyhow!("不支持的时间周期: {}", period)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_period() {
        assert!(matches!(
            parse_period_to_timeframe("1H").unwrap(),
            Timeframe::H1
        ));
        assert!(matches!(
            parse_period_to_timeframe("1D").unwrap(),
            Timeframe::D1
        ));
    }

    #[test]
    fn test_state_manager() {
        let key = "test_key";
        let ts = 12345;
        
        // 第一次应该成功
        assert!(StrategyExecutionStateManager::try_mark_processing(key, ts));
        
        // 第二次应该失败（去重）
        assert!(!StrategyExecutionStateManager::try_mark_processing(key, ts));
        
        // 清理
        StrategyExecutionStateManager::mark_completed(key, ts);
        
        // 清理后应该又可以执行
        assert!(StrategyExecutionStateManager::try_mark_processing(key, ts));
    }
}

