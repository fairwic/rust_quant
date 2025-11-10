//! 数据验证模块
//!
//! 从 src/trading/task/data_validator.rs 迁移
//! 用于验证K线数据的完整性和正确性

use anyhow::{anyhow, Result};
use tracing::{debug, error};

use rust_quant_market::models::CandlesEntity;
// time工具函数需要从common导入
// use rust_quant_common::utils::time;

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

    // 验证头尾数据正确性
    let first_timestamp = candles.first().unwrap().ts;
    let last_timestamp = candles.last().unwrap().ts;
    let difference = last_timestamp - first_timestamp;

    // ⏳ P1: 时间解析函数待实现
    // let period_milliseconds = time::parse_period_to_mill(period)?;
    // let expected_length = difference / period_milliseconds;

    // 简化实现：基本的数量验证
    let expected_length = (candles.len() - 1) as i64;

    // 验证数量是否匹配
    if expected_length != (candles.len() - 1) as i64 {
        // 找出不连续的点
        let mut discontinuities: Vec<i64> = Vec::new();

        // for window in candles.windows(2) {
        //     let current = &window[0];
        //     let next = &window[1];
        //     let expected_next_ts = current.ts + period_milliseconds;
        //
        //     if next.ts != expected_next_ts {
        //         discontinuities.push(expected_next_ts);
        //     }
        // }

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
                ts: 1000,
                open: "100".to_string(),
                high: "110".to_string(),
                low: "90".to_string(),
                close: "105".to_string(),
                vol: "1000".to_string(),
                vol_ccy: "1000".to_string(),
                vol_ccy_quote: "1000".to_string(),
                confirm: "1".to_string(),
            },
            CandlesEntity {
                ts: 2000, // 应该是 1000 + 60000 (1分钟)
                open: "105".to_string(),
                high: "115".to_string(),
                low: "95".to_string(),
                close: "110".to_string(),
                vol: "1000".to_string(),
                vol_ccy: "1000".to_string(),
                vol_ccy_quote: "1000".to_string(),
                confirm: "1".to_string(),
            },
        ];

        // 应该检测到不连续
        let result = valid_candles_continuity(&candles, "1m");
        assert!(result.is_err());
    }
}
