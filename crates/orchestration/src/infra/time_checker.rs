//! 时间检查器 - K线时间戳验证
//!
//! 用于验证K线时间戳是否需要触发策略执行

use anyhow::{anyhow, Result};
use tracing::info;

/// 检查新时间是否需要执行策略
///
/// # Arguments
/// * `old_time` - 上一次执行的时间戳（毫秒）
/// * `new_time` - 当前K线的时间戳（毫秒）
/// * `period` - 时间周期（如 "1H", "15m"）
/// * `is_close_confirm` - 是否已收盘确认
/// * `just_check_confirm` - 是否仅在收盘确认时执行
///
/// # Returns
/// - `Ok(true)` - 应该执行策略
/// - `Ok(false)` - 应该跳过执行
/// - `Err` - 时间戳异常
///
/// # Logic
/// 1. 时间倒退检查 - 新时间不能小于旧时间
/// 2. 收盘确认模式 - 如果已确认，直接返回true
/// 3. 时间戳更新检查 - 新旧时间相同则跳过
/// 4. 收盘确认要求 - 如果需要确认但未确认，则跳过
/// 5. 默认执行 - 其他情况允许执行
pub fn check_new_time(
    old_time: i64,
    new_time: i64,
    period: &str,
    is_close_confirm: bool,
    just_check_confirm: bool,
) -> Result<bool> {
    // 1. 检查时间倒退异常
    if new_time < old_time {
        return Err(anyhow!(
            "K线时间戳异常: 上一时间戳={}, 当前时间戳={}, 周期={}",
            old_time,
            new_time,
            period
        ));
    }

    // 2. 如果已经收盘确认，直接执行
    if is_close_confirm {
        return Ok(true);
    }

    // 3. 检查时间戳是否更新
    if old_time == new_time {
        info!(
            "K线时间戳未更新，跳过策略执行: period={}, timestamp={}",
            period, new_time
        );
        return Ok(false);
    }

    // 4. 如果要求收盘确认，但当前未确认，跳过执行
    if just_check_confirm && !is_close_confirm {
        info!(
            "K线未收盘确认，跳过策略执行: period={}, timestamp={}",
            period, new_time
        );
        return Ok(false);
    }

    // 5. 其他情况，允许执行
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_new_time_normal() {
        // 正常情况：新时间 > 旧时间
        let result = check_new_time(1000, 2000, "1H", false, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_check_new_time_same_timestamp() {
        // 时间戳相同，应该跳过
        let result = check_new_time(1000, 1000, "1H", false, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), false);
    }

    #[test]
    fn test_check_new_time_backward() {
        // 时间倒退，应该报错
        let result = check_new_time(2000, 1000, "1H", false, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_new_time_close_confirm() {
        // 已收盘确认，应该执行
        let result = check_new_time(1000, 1000, "1H", true, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_check_new_time_require_confirm() {
        // 要求收盘确认但未确认，应该跳过
        let result = check_new_time(1000, 2000, "1H", false, true);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), false);
    }

    #[test]
    fn test_check_new_time_require_and_confirmed() {
        // 要求收盘确认且已确认，应该执行
        let result = check_new_time(1000, 2000, "1H", true, true);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);
    }
}
