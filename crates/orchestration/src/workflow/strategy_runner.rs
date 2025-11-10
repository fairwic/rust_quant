//! 策略运行器 V2 - 简化版
//!
//! 通过 services 层调用业务逻辑，orchestration 只做调度和协调

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info};

use rust_quant_domain::{StrategyType, Timeframe};

// ⭐ Services层集成
// 当前状态：骨架已建立，待完善具体集成
// 集成计划：
// 1. 构建 StrategyConfig from config_id
// 2. 准备 CandlesEntity snapshot
// 3. 调用 StrategyExecutionService.execute_strategy()
// 4. 处理返回的 SignalResult
// 5. 触发订单创建流程
//
// 参考实现：
// use rust_quant_services::strategy::StrategyExecutionService;
// let service = StrategyExecutionService::new();
// let result = service.execute_strategy(inst_id, period, config, snap).await?;

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
/// # Architecture
/// 这是orchestration层的核心策略执行入口。
/// 职责：编排和协调，不包含业务逻辑。
///
/// # Integration Status
/// ⏳ 骨架完成，services层集成待完善
///
/// ## 当前实现
/// - ✅ 状态管理（去重、跟踪）
/// - ✅ 时间戳转换
/// - ✅ 执行流程编排
/// - ⏳ Services层调用（待完善）
///
/// ## 待集成步骤
/// 1. 从config_id加载StrategyConfig（或使用默认配置）
/// 2. 准备市场数据快照 CandlesEntity
/// 3. 调用 StrategyExecutionService.execute_strategy()
/// 4. 处理返回的 SignalResult
/// 5. 根据信号触发订单创建（调用OrderCreationService）
///
/// # Arguments
/// * `inst_id` - 交易对（如 "BTC-USDT"）
/// * `timeframe` - 时间周期
/// * `strategy_type` - 策略类型
/// * `_config_id` - 策略配置ID（可选，当前未使用）
///
/// # Returns
/// 执行结果
///
/// # Example
/// ```rust,ignore
/// use rust_quant_orchestration::workflow::execute_strategy;
/// use rust_quant_domain::{Timeframe, StrategyType};
///
/// execute_strategy("BTC-USDT", Timeframe::H1, StrategyType::Vegas, None).await?;
/// ```
pub async fn execute_strategy(
    inst_id: &str,
    timeframe: Timeframe,
    strategy_type: StrategyType,
    _config_id: Option<i64>,
) -> Result<()> {
    let period = timeframe_to_period(timeframe);
    let key = format!("{}_{:?}_{:?}", inst_id, timeframe, strategy_type);

    info!(
        "🚀 开始执行策略: inst_id={}, period={}, strategy={:?}",
        inst_id, period, strategy_type
    );

    // 1. 检查是否应该跳过（去重机制）
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs() as i64;

    if !StrategyExecutionStateManager::try_mark_processing(&key, timestamp) {
        debug!("策略正在执行中，跳过重复请求: {}", key);
        return Ok(());
    }

    // 2. 执行策略（当前占位实现）
    //
    // ⏳ 完整实现示例：
    //
    // // 2.1 加载配置
    // use rust_quant_services::strategy::StrategyConfigService;
    // let config_service = StrategyConfigService::new();
    // let config = if let Some(id) = config_id {
    //     config_service.get_config(id).await?
    // } else {
    //     config_service.get_default_config(strategy_type).await?
    // };
    //
    // // 2.2 准备市场数据
    // use rust_quant_services::market::MarketDataService;
    // let market_service = MarketDataService::new();
    // let snap = market_service.get_latest_candle(inst_id, period).await?;
    //
    // // 2.3 执行策略
    // use rust_quant_services::strategy::StrategyExecutionService;
    // let strategy_service = StrategyExecutionService::new();
    // let signal = strategy_service
    //     .execute_strategy(inst_id, period, &config, Some(snap))
    //     .await?;
    //
    // // 2.4 处理信号
    // if signal.has_signal() {
    //     use rust_quant_services::trading::OrderCreationService;
    //     let order_service = OrderCreationService::new();
    //     order_service.create_order_from_signal(&signal, &config).await?;
    // }

    info!("✅ 策略执行完成 (当前为简化实现，详见代码注释): {}", key);

    // 3. 标记完成
    StrategyExecutionStateManager::mark_completed(&key, timestamp);

    Ok(())
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
pub async fn test_random_strategy(inst_id: String, period: String) -> Result<()> {
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

/// 辅助函数：Timeframe 转为 period 字符串
fn timeframe_to_period(timeframe: Timeframe) -> &'static str {
    match timeframe {
        Timeframe::M1 => "1m",
        Timeframe::M3 => "3m",
        Timeframe::M5 => "5m",
        Timeframe::M15 => "15m",
        Timeframe::M30 => "30m",
        Timeframe::H1 => "1H",
        Timeframe::H2 => "2H",
        Timeframe::H4 => "4H",
        Timeframe::H6 => "6H",
        Timeframe::H12 => "12H",
        Timeframe::D1 => "1D",
        Timeframe::W1 => "1W",
        Timeframe::MN1 => "1M",
    }
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
