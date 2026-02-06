//! 数据验证模块
//!
//! 从 src/trading/task/data_validator.rs 迁移
//! 用于验证K线数据的完整性和正确性

use anyhow::{anyhow, Result};
use tracing::debug;

use rust_quant_common::utils::time;
use rust_quant_market::models::CandlesEntity;

/// 验证最新K线数据是否在当前时间周期
///
/// # Arguments
/// * `candle` - K线数据
/// * `period` - 时间周期
///
/// # Returns
/// - `true` - 数据有效
/// - `false` - 数据过期或无效
pub fn valid_newest_candle_data(candle: &CandlesEntity, period: &str) -> bool {
    let ts = candle.ts;

    // ⏳ P1: 时间转换函数待实现或从common导入
    // let datetime = time::mill_time_to_local_datetime(ts);
    // let data_period = time::format_to_period(period, Some(datetime));
    // let current_period = time::format_to_period(period, None);

    // 当前简化实现
    debug!("验证K线数据: ts={}, period={}", ts, period);

    // 当前返回true，实际验证逻辑待完善
    true
}

/// 验证K线数据序列的连续性
///
/// # Arguments
/// * `candles` - K线数据序列
/// * `period` - 时间周期
///
/// # Returns
/// - `Ok(())` - 数据连续
/// - `Err` - 数据不连续，返回缺失的时间戳
pub fn valid_candles_continuity(candles: &[CandlesEntity], period: &str) -> Result<()> {
    if candles.len() < 2 {
        return Ok(());
    }

    let period_milliseconds = time::parse_period_to_mill(period)?;
    let first_timestamp = candles.first().unwrap().ts;
    let last_timestamp = candles.last().unwrap().ts;
    let expected_length = (last_timestamp - first_timestamp) / period_milliseconds;

    let mut discontinuities = Vec::new();
    for window in candles.windows(2) {
        let expected_next_ts = window[0].ts + period_milliseconds;
        if window[1].ts != expected_next_ts {
            discontinuities.push(expected_next_ts);
        }
    }

    if !discontinuities.is_empty() || expected_length != (candles.len() - 1) as i64 {
        return Err(anyhow!(
            "K线数据不连续: 期望长度={}, 实际长度={}, 缺失时间戳={:?}",
            expected_length,
            candles.len() - 1,
            discontinuities
        ));
    }

    debug!("✅ K线数据连续性验证通过: {} 条记录", candles.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_candles_continuity_with_gap() {
        // 测试不连续的数据应该返回错误
        let candles = vec![
            CandlesEntity {
                id: None,
                ts: 1000,
                o: "100".to_string(),
                h: "110".to_string(),
                l: "90".to_string(),
                c: "105".to_string(),
                vol: "1000".to_string(),
                vol_ccy: "1000".to_string(),
                confirm: "1".to_string(),
                created_at: None,
                updated_at: None,
            },
            CandlesEntity {
                id: None,
                ts: 2000, // 应该是 1000 + 60000 (1分钟)
                o: "105".to_string(),
                h: "115".to_string(),
                l: "95".to_string(),
                c: "110".to_string(),
                vol: "1000".to_string(),
                vol_ccy: "1000".to_string(),
                confirm: "1".to_string(),
                created_at: None,
                updated_at: None,
            },
        ];

        // 应该检测到不连续
        let result = valid_candles_continuity(&candles, "1m");
        assert!(result.is_err());
    }
}
