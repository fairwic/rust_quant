//! 策略运行器 V2 - 简化版
//!
//! 通过 services 层调用业务逻辑，orchestration 只做调度和协调

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use redis::AsyncCommands;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info, warn};

use rust_quant_core::cache::get_redis_connection;
use rust_quant_domain::{StrategyType, Timeframe};
use rust_quant_market::models::CandlesEntity;
use rust_quant_services::strategy::{StrategyConfigService, StrategyExecutionService};

/// 策略执行状态跟踪 - 用于时间戳去重
#[derive(Debug, Clone)]
struct StrategyExecutionState {
    #[allow(dead_code)]
    timestamp: i64,
    start_time: SystemTime,
}

/// 全局策略执行状态管理器 - 防止重复处理相同时间戳的K线
static STRATEGY_EXECUTION_STATES: Lazy<DashMap<String, StrategyExecutionState>> =
    Lazy::new(DashMap::new);

/// 策略执行状态管理器
pub struct StrategyExecutionStateManager;

impl StrategyExecutionStateManager {
    const PERSIST_TTL_SECS: u64 = 86400 * 14;

    fn persistent_key(key: &str, timestamp: i64) -> String {
        format!("live_confirm_candle_processed:{}:{}", key, timestamp)
    }

    pub async fn is_persisted_processed(key: &str, timestamp: i64) -> bool {
        let rkey = Self::persistent_key(key, timestamp);
        let mut conn = match get_redis_connection().await {
            Ok(conn) => conn,
            Err(e) => {
                warn!("获取Redis连接失败，跳过持久化去重检查: {}", e);
                return false;
            }
        };

        match conn.get::<_, Option<String>>(&rkey).await {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(e) => {
                warn!("读取持久化去重键失败: key={}, err={}", rkey, e);
                false
            }
        }
    }

    pub async fn mark_persisted_completed(key: &str, timestamp: i64) {
        let rkey = Self::persistent_key(key, timestamp);
        let mut conn = match get_redis_connection().await {
            Ok(conn) => conn,
            Err(e) => {
                warn!("获取Redis连接失败，跳过持久化完成标记: {}", e);
                return;
            }
        };

        if let Err(e) = conn
            .set_ex::<_, _, ()>(&rkey, "1", Self::PERSIST_TTL_SECS)
            .await
        {
            warn!("写入持久化去重键失败: key={}, err={}", rkey, e);
        }
    }

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
            let duration = match SystemTime::now().duration_since(state.start_time) {
                Ok(d) => d,
                Err(_) => Duration::from_millis(0),
            };
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
///
/// # Architecture Note
/// 本函数仅作为占位符和接口定义，实际的策略执行应该：
/// 1. 在应用层（bootstrap）创建已配置的 service 实例
/// 2. 通过参数传入或使用全局单例模式
/// 3. Orchestration 层只做任务调度，不创建 service 实例
///
#[allow(clippy::too_many_arguments)]
pub async fn execute_strategy(
    inst_id: &str,
    timeframe: Timeframe,
    strategy_type: StrategyType,
    config_id: Option<i64>,
    trigger_ts: Option<i64>,
    snap: Option<CandlesEntity>,
    config_service: &StrategyConfigService,
    execution_service: &StrategyExecutionService,
) -> Result<()> {
    // 去重 key 必须包含 config_id：
    // - 同一 symbol+timeframe+strategy_type 下可能存在多条配置（不同参数/风控）
    // - 不包含 config_id 会导致多配置互相“误去重”，只有第一条能执行
    let cfg_part = match config_id {
        Some(id) => id.to_string(),
        None => "none".to_string(),
    };
    let key = format!(
        "{}_{:?}_{:?}_{}",
        inst_id, timeframe, strategy_type, cfg_part
    );

    info!(
        "🚀 开始执行策略: inst_id={}, timeframe={:?}, strategy={:?}",
        inst_id, timeframe, strategy_type
    );

    // 检查是否应该跳过（去重）
    // - WebSocket 触发：用“确认K线的 ts”（毫秒）作为去重维度，避免重复消息/重连导致重复执行
    // - 定时/手动触发：退化为“当前时间秒”作为并发保护（同秒重复触发会被合并）
    let timestamp = match trigger_ts.or_else(|| snap.as_ref().map(|s| s.ts)) {
        Some(ts) => ts,
        None => SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as i64,
    };

    if StrategyExecutionStateManager::is_persisted_processed(&key, timestamp).await {
        info!(
            "⏭️ 跳过已持久化处理的确认K线: key={}, timestamp={}",
            key, timestamp
        );
        return Ok(());
    }

    if !StrategyExecutionStateManager::try_mark_processing(&key, timestamp) {
        debug!("策略正在执行中，跳过: {}", key);
        return Ok(());
    }

    let timeframe_str = timeframe.as_str();
    let strategy_name = strategy_type.as_str();

    // 1. 加载策略配置
    let config = if let Some(id) = config_id {
        config_service.load_config_by_id(id).await?
    } else {
        let mut configs = config_service
            .load_configs(inst_id, timeframe_str, Some(strategy_name))
            .await?;

        if configs.is_empty() {
            warn!(
                "⚠️  未找到策略配置，跳过执行: inst_id={}, timeframe={}, strategy={}",
                inst_id, timeframe_str, strategy_name
            );
            StrategyExecutionStateManager::mark_completed(&key, timestamp);
            return Ok(());
        }

        configs.remove(0)
    };

    // 2. 验证策略配置
    if let Err(e) = config_service.validate_config(&config) {
        error!(
            "❌ 策略配置验证失败: key={}, config_id={}, error={}",
            key, config.id, e
        );
        StrategyExecutionStateManager::mark_completed(&key, timestamp);
        return Err(e);
    } else {
        info!("✅ 策略配置验证成功: key={}, config_id={}", key, config.id);
    }

    // 3. 执行策略
    let exec_result = execution_service
        .execute_strategy(inst_id, timeframe_str, &config, snap)
        .await;

    // 标记完成
    StrategyExecutionStateManager::mark_completed(&key, timestamp);

    match exec_result {
        Ok(signal_result) => {
            StrategyExecutionStateManager::mark_persisted_completed(&key, timestamp).await;
            info!(
                "✅ 策略执行成功: {} - buy={}, sell={}",
                key, signal_result.should_buy, signal_result.should_sell
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
    config_service: &StrategyConfigService,
    execution_service: &StrategyExecutionService,
) -> Result<Vec<Result<()>>> {
    info!("🚀 批量执行 {} 个策略", strategies.len());

    let mut results = Vec::new();

    for (inst_id, timeframe, strategy_type, config_id) in strategies {
        let result = execute_strategy(
            &inst_id,
            timeframe,
            strategy_type,
            config_id,
            None,
            None,
            config_service,
            execution_service,
        )
        .await;
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
    config_service: &StrategyConfigService,
    execution_service: &StrategyExecutionService,
) -> Result<()> {
    info!("🎲 测试随机策略: inst_id={}, period={}", inst_id, period);

    // 解析时间周期
    let timeframe = parse_period_to_timeframe(&period)?;

    // 默认使用 Vegas 策略
    execute_strategy(
        &inst_id,
        timeframe,
        StrategyType::Vegas,
        None,
        None,
        None,
        config_service,
        execution_service,
    )
    .await
}

/// 测试指定策略 - 保持向后兼容
pub async fn test_specified_strategy(
    inst_id: String,
    period: String,
    strategy_type: StrategyType,
    config_id: Option<i64>,
    config_service: &StrategyConfigService,
    execution_service: &StrategyExecutionService,
) -> Result<()> {
    info!(
        "🎯 测试指定策略: inst_id={}, period={}, strategy={:?}",
        inst_id, period, strategy_type
    );

    // 解析时间周期
    let timeframe = parse_period_to_timeframe(&period)?;

    execute_strategy(
        &inst_id,
        timeframe,
        strategy_type,
        config_id,
        None,
        None,
        config_service,
        execution_service,
    )
    .await
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
    use redis::AsyncCommands;
    use rust_quant_core::cache::{get_redis_connection, init_redis_pool};

    #[test]
    fn test_parse_period() {
        match parse_period_to_timeframe("1H") {
            Ok(tf) => assert!(matches!(tf, Timeframe::H1)),
            Err(e) => panic!("解析 1H 失败: {}", e),
        }
        match parse_period_to_timeframe("1D") {
            Ok(tf) => assert!(matches!(tf, Timeframe::D1)),
            Err(e) => panic!("解析 1D 失败: {}", e),
        }
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

    #[tokio::test]
    async fn test_persisted_state_manager_roundtrip() {
        if std::env::var("REDIS_HOST").is_err() {
            std::env::set_var("REDIS_HOST", "redis://127.0.0.1:6379/");
        }
        if init_redis_pool().await.is_err() {
            eprintln!("skip test_persisted_state_manager_roundtrip: redis unavailable");
            return;
        }

        let key = "persisted_test_key";
        let ts = 67890;
        let redis_key = StrategyExecutionStateManager::persistent_key(key, ts);

        let mut conn = match get_redis_connection().await {
            Ok(conn) => conn,
            Err(_) => {
                eprintln!("skip test_persisted_state_manager_roundtrip: redis unavailable");
                return;
            }
        };
        let _: redis::RedisResult<()> = conn.del(&redis_key).await;

        assert!(!StrategyExecutionStateManager::is_persisted_processed(key, ts).await);

        StrategyExecutionStateManager::mark_persisted_completed(key, ts).await;
        assert!(StrategyExecutionStateManager::is_persisted_processed(key, ts).await);

        let _: redis::RedisResult<()> = conn.del(&redis_key).await;
    }
}
