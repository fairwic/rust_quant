//! StrategyConfig 兼容层
//!
//! 提供辅助函数来适配新的 domain::StrategyConfig 结构
//!
//! ## 背景
//!
//! domain::StrategyConfig 使用 JsonValue 存储参数，而旧代码使用 String
//! 这个模块提供便捷的转换函数

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use rust_quant_domain::StrategyConfig;

/// 从 StrategyConfig 提取策略参数
///
/// # 类型参数
/// * `T` - 要提取的参数类型，必须实现 Deserialize
///
/// # 参数
/// * `config` - StrategyConfig 引用
///
/// # 返回
/// * `Ok(T)` - 成功提取的参数
/// * `Err` - 提取失败
///
/// # 示例
/// ```rust,ignore
/// let vegas_config: VegasStrategyConfig = extract_parameters(&strategy_config)?;
/// ```
pub fn extract_parameters<T: for<'de> Deserialize<'de>>(config: &StrategyConfig) -> Result<T> {
    serde_json::from_value(config.parameters.clone())
        .map_err(|e| anyhow!("Failed to extract parameters: {}", e))
}

/// 从 StrategyConfig 提取风险配置
///
/// # 类型参数
/// * `T` - 要提取的风险配置类型，必须实现 Deserialize
///
/// # 参数
/// * `config` - StrategyConfig 引用
///
/// # 返回
/// * `Ok(T)` - 成功提取的风险配置
/// * `Err` - 提取失败
///
/// # 示例
/// ```rust,ignore
/// let risk_config: BasicRiskConfig = extract_risk_config(&strategy_config)?;
/// ```
pub fn extract_risk_config<T: for<'de> Deserialize<'de>>(config: &StrategyConfig) -> Result<T> {
    serde_json::from_value(config.risk_config.clone())
        .map_err(|e| anyhow!("Failed to extract risk_config: {}", e))
}

/// 将策略参数和风险配置打包为 JsonValue
///
/// # 类型参数
/// * `P` - 策略参数类型
/// * `R` - 风险配置类型
///
/// # 参数
/// * `parameters` - 策略参数
/// * `risk_config` - 风险配置
///
/// # 返回
/// * 元组 (parameters_json, risk_config_json)
pub fn pack_config<P: Serialize, R: Serialize>(
    parameters: &P,
    risk_config: &R,
) -> Result<(JsonValue, JsonValue)> {
    let parameters_json = serde_json::to_value(parameters)
        .map_err(|e| anyhow!("Failed to serialize parameters: {}", e))?;

    let risk_config_json = serde_json::to_value(risk_config)
        .map_err(|e| anyhow!("Failed to serialize risk_config: {}", e))?;

    Ok((parameters_json, risk_config_json))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        value: i32,
    }

    #[test]
    fn test_pack_and_extract() {
        let test_config = TestConfig { value: 42 };
        let (params_json, _) = pack_config(&test_config, &test_config).unwrap();

        let extracted: TestConfig = serde_json::from_value(params_json).unwrap();
        assert_eq!(extracted, test_config);
    }
}
