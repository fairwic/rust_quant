//! 策略信号日志记录器
//!
//! 用于异步保存策略执行的信号结果

use anyhow::Result;
use serde_json;
use tracing::{error, info};

use rust_quant_strategies::strategy_common::SignalResult;
use rust_quant_strategies::StrategyType;

/// 信号日志数据结构
///
/// 这是一个简化的内存结构，用于日志记录
/// 完整的数据库持久化需要配合infrastructure层的Repository
#[derive(Debug, Clone)]
pub struct SignalLogEntry {
    pub inst_id: String,
    pub period: String,
    pub strategy_type: String,
    pub signal_result: String,
    pub timestamp: i64,
}

impl SignalLogEntry {
    pub fn new(
        inst_id: &str,
        period: &str,
        strategy_type: StrategyType,
        signal_result: &SignalResult,
    ) -> Self {
        let strategy_result_str = match serde_json::to_string(&signal_result) {
            Ok(s) => s,
            Err(e) => {
                error!("序列化 signal_result 失败: {}", e);
                format!("{:?}", signal_result)
            }
        };

        Self {
            inst_id: inst_id.to_string(),
            period: period.to_string(),
            strategy_type: strategy_type.as_str().to_owned(),
            signal_result: strategy_result_str,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

/// 异步保存信号日志
///
/// # 当前实现
/// - ✅ 异步执行，不阻塞主流程
/// - ✅ 错误处理，不影响交易
/// - ⏳ 日志持久化（待实现数据库保存）
///
/// # 集成方式（待实现）
/// ```rust,ignore
/// use rust_quant_infrastructure::repositories::SignalLogRepository;
///
/// let repo = SignalLogRepository::new(db_pool);
/// repo.save(&log_entry).await?;
/// ```
pub fn save_signal_log_async(
    inst_id: String,
    period: String,
    strategy_type: StrategyType,
    signal_result: SignalResult,
) {
    // 创建日志条目
    let log_entry = SignalLogEntry::new(&inst_id, &period, strategy_type, &signal_result);

    let _inst_id_clone = inst_id.clone();

    // 异步保存（不阻塞主流程）
    tokio::spawn(async move {
        // ⏳ P1: 数据库持久化待实现
        // 当前只记录到日志系统
        info!(
            "📝 策略信号记录: inst_id={}, period={}, strategy={}, buy={}, sell={}",
            log_entry.inst_id,
            log_entry.period,
            log_entry.strategy_type,
            signal_result.should_buy,
            signal_result.should_sell
        );

        // 完整实现参考：
        // use rust_quant_infrastructure::repositories::SignalLogRepository;
        // let db_pool = get_db_pool();
        // let repo = SignalLogRepository::new(db_pool);
        //
        // if let Err(e) = repo.save(&log_entry).await {
        //     error!("保存策略信号日志失败: inst_id={}, error={}", inst_id_clone, e);
        // } else {
        //     info!("✅ 策略信号日志已保存: {}", inst_id_clone);
        // }
    });
}

/// 同步保存信号日志（阻塞版本）
///
/// 仅用于测试或关键场景，生产环境建议使用异步版本
pub async fn save_signal_log(
    inst_id: &str,
    period: &str,
    strategy_type: StrategyType,
    signal_result: &SignalResult,
) -> Result<()> {
    let log_entry = SignalLogEntry::new(inst_id, period, strategy_type, signal_result);

    info!(
        "📝 保存策略信号: inst_id={}, period={}, strategy={}",
        log_entry.inst_id, log_entry.period, log_entry.strategy_type
    );

    // ⏳ P1: 数据库持久化待实现
    // 当前只记录到日志系统

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_log_entry_creation() {
        // 使用strategies包的SignalResult
        let signal = SignalResult {
            should_buy: false,
            should_sell: false,
            open_price: 0.0,
            signal_kline_stop_loss_price: None,
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            ts: 0,
            single_value: None,
            single_result: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            stop_loss_source: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::None,
        };
        let entry = SignalLogEntry::new("BTC-USDT", "1H", StrategyType::Vegas, &signal);

        assert_eq!(entry.inst_id, "BTC-USDT");
        assert_eq!(entry.period, "1H");
        assert_eq!(entry.strategy_type, "vegas");
    }

    #[tokio::test]
    #[ignore] // 需要完整环境才能运行
    async fn test_save_signal_log() {
        // 使用strategies包的SignalResult
        let signal = SignalResult {
            should_buy: false,
            should_sell: false,
            open_price: 0.0,
            signal_kline_stop_loss_price: None,
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            ts: 0,
            single_value: None,
            single_result: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            stop_loss_source: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::None,
        };
        let result = save_signal_log("BTC-USDT", "1H", StrategyType::Vegas, &signal).await;

        assert!(result.is_ok());
    }
}
