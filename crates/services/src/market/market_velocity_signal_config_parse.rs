use super::market_velocity_signal::{
    MarketVelocityFvgEntryMode, MarketVelocitySignalTradeDirection, MarketVelocityStopLossMode,
};
use anyhow::{anyhow, Result};
use serde_json::Value;

/// 解析入口触发器列表，支持 CSV 与 all/none 语义，供动量信号配置复用。
pub(super) fn parse_env_entry_trigger_list(key: &str, default: &[&str]) -> Result<Vec<String>> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default.iter().map(|value| (*value).to_string()).collect());
    };
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(default.iter().map(|value| (*value).to_string()).collect());
    }
    if matches!(normalized.as_str(), "all" | "*" | "none") {
        return Ok(Vec::new());
    }
    let mut triggers = Vec::new();
    for trigger in value.split(',').map(normalize_entry_trigger) {
        if trigger.is_empty() || triggers.contains(&trigger) {
            continue;
        }
        triggers.push(trigger);
    }
    if triggers.is_empty() {
        return Err(anyhow!("{key} must contain at least one entry trigger"));
    }
    Ok(triggers)
}

/// 解析交易对 blocklist，统一大小写和空值语义。
pub(super) fn parse_env_symbol_list(key: &str, default: &[&str]) -> Result<Vec<String>> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default.iter().map(|value| (*value).to_string()).collect());
    };
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(default.iter().map(|value| (*value).to_string()).collect());
    }
    if matches!(normalized.as_str(), "all" | "*" | "none") {
        return Ok(Vec::new());
    }
    let mut symbols = Vec::new();
    for symbol in value.split(',').map(normalize_symbol) {
        if symbol.is_empty() || symbols.contains(&symbol) {
            continue;
        }
        symbols.push(symbol);
    }
    if symbols.is_empty() {
        return Err(anyhow!("{key} must contain at least one symbol"));
    }
    Ok(symbols)
}

pub(super) fn json_value_is_null(value: &Value, key: &str) -> bool {
    value.get(key).is_some_and(Value::is_null)
}

/// 从 JSON 对象读取非空字符串字段。
pub(super) fn json_string(value: &Value, key: &str) -> Result<Option<String>> {
    let Some(field) = json_field(value, key) else {
        return Ok(None);
    };
    let text = match field {
        Value::String(value) => value.trim().to_string(),
        _ => return Err(anyhow!("{key} must be a string")),
    };
    Ok((!text.is_empty()).then_some(text))
}

pub(super) fn json_fvg_entry_mode(
    value: &Value,
    key: &str,
) -> Result<Option<MarketVelocityFvgEntryMode>> {
    let Some(value) = json_string(value, key)? else {
        return Ok(None);
    };
    MarketVelocityFvgEntryMode::from_str(&value).map(Some)
}

pub(super) fn json_stop_loss_mode(
    value: &Value,
    key: &str,
) -> Result<Option<MarketVelocityStopLossMode>> {
    let Some(value) = json_string(value, key)? else {
        return Ok(None);
    };
    MarketVelocityStopLossMode::from_str(&value).map(Some)
}

pub(super) fn json_signal_trade_direction(
    value: &Value,
    key: &str,
) -> Result<Option<MarketVelocitySignalTradeDirection>> {
    let Some(value) = json_string(value, key)? else {
        return Ok(None);
    };
    MarketVelocitySignalTradeDirection::from_str(&value).map(Some)
}

/// 从 JSON 对象读取整数字段，兼容数字和字符串。
pub(super) fn json_i64(value: &Value, key: &str) -> Result<Option<i64>> {
    let Some(field) = json_field(value, key) else {
        return Ok(None);
    };
    match field {
        Value::Number(number) => number
            .as_i64()
            .ok_or_else(|| anyhow!("{key} must be an integer"))
            .map(Some),
        Value::String(value) => value
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|error| anyhow!("{key} must be an integer: {error}")),
        _ => Err(anyhow!("{key} must be an integer")),
    }
}

pub(super) fn json_i32(value: &Value, key: &str) -> Result<Option<i32>> {
    json_i64(value, key)?
        .map(|value| {
            i32::try_from(value).map_err(|error| anyhow!("{key} is out of i32 range: {error}"))
        })
        .transpose()
}

pub(super) fn json_u32(value: &Value, key: &str) -> Result<Option<u32>> {
    json_i64(value, key)?
        .map(|value| {
            u32::try_from(value).map_err(|error| anyhow!("{key} is out of u32 range: {error}"))
        })
        .transpose()
}

pub(super) fn json_usize(value: &Value, key: &str) -> Result<Option<usize>> {
    json_i64(value, key)?
        .map(|value| {
            usize::try_from(value).map_err(|error| anyhow!("{key} is out of usize range: {error}"))
        })
        .transpose()
}

/// 从 JSON 对象读取小数字段，兼容数字和字符串。
pub(super) fn json_f64(value: &Value, key: &str) -> Result<Option<f64>> {
    let Some(field) = json_field(value, key) else {
        return Ok(None);
    };
    match field {
        Value::Number(number) => number
            .as_f64()
            .ok_or_else(|| anyhow!("{key} must be a number"))
            .map(Some),
        Value::String(value) => value
            .trim()
            .parse::<f64>()
            .map(Some)
            .map_err(|error| anyhow!("{key} must be a number: {error}")),
        _ => Err(anyhow!("{key} must be a number")),
    }
}

pub(super) fn json_f64_any(value: &Value, keys: &[&str]) -> Result<Option<f64>> {
    for key in keys {
        if let Some(value) = json_f64(value, key)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub(super) fn json_bool(value: &Value, key: &str) -> Result<Option<bool>> {
    let Some(field) = json_field(value, key) else {
        return Ok(None);
    };
    match field {
        Value::Bool(value) => Ok(Some(*value)),
        Value::String(value) => parse_bool_text(value, key).map(Some),
        _ => Err(anyhow!("{key} must be a boolean")),
    }
}

pub(super) fn json_entry_trigger_list(value: &Value, key: &str) -> Result<Option<Vec<String>>> {
    json_string_list(value, key, normalize_entry_trigger)
}

pub(super) fn json_symbol_list(value: &Value, key: &str) -> Result<Option<Vec<String>>> {
    json_string_list(value, key, normalize_symbol)
}

/// 将 max_hold_time 秒数上取整为小时。
pub(super) fn max_holding_hours_from_seconds(seconds: i64) -> Result<u32> {
    if seconds <= 0 {
        return Err(anyhow!("max_hold_time must be positive"));
    }
    let hours = (seconds + 3_599) / 3_600;
    u32::try_from(hours).map_err(|error| anyhow!("max_hold_time is out of u32 range: {error}"))
}

pub(super) fn parse_env_i32(key: &str, default: i32) -> Result<i32> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<i32>()
                .map_err(|error| anyhow!("{key} must be an integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

/// 解析带默认值的可选整数；显式 none/null/off 表示无上限，缺省才使用默认值。
pub(super) fn parse_env_optional_i32_with_default(key: &str, default: i32) -> Result<Option<i32>> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(Some(default));
    };
    let value = value.trim();
    if env_optional_value_is_none(value) {
        Ok(None)
    } else {
        value
            .parse::<i32>()
            .map(Some)
            .map_err(|error| anyhow!("{key} must be an integer or none: {error}"))
    }
}

pub(super) fn parse_env_u64(key: &str, default: u64) -> Result<u64> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<u64>()
                .map_err(|error| anyhow!("{key} must be an integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

pub(super) fn parse_env_u32(key: &str, default: u32) -> Result<u32> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<u32>()
                .map_err(|error| anyhow!("{key} must be an integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

pub(super) fn parse_env_usize(key: &str, default: usize) -> Result<usize> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<usize>()
                .map_err(|error| anyhow!("{key} must be an integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

pub(super) fn parse_env_f64(key: &str, default: f64) -> Result<f64> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<f64>()
                .map_err(|error| anyhow!("{key} must be a number: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

pub(super) fn parse_env_optional_f64(key: &str) -> Result<Option<f64>> {
    std::env::var(key)
        .ok()
        .map(|value| {
            let value = value.trim();
            if env_optional_value_is_none(value) {
                Ok(None)
            } else {
                value
                    .parse::<f64>()
                    .map(Some)
                    .map_err(|error| anyhow!("{key} must be a number: {error}"))
            }
        })
        .transpose()
        .map(Option::flatten)
}

/// 解析带默认值的可选小数；显式 none/null/off 表示无上限，缺省才使用默认值。
pub(super) fn parse_env_optional_f64_with_default(key: &str, default: f64) -> Result<Option<f64>> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(Some(default));
    };
    let value = value.trim();
    if env_optional_value_is_none(value) {
        Ok(None)
    } else {
        value
            .parse::<f64>()
            .map(Some)
            .map_err(|error| anyhow!("{key} must be a number or none: {error}"))
    }
}

pub(super) fn parse_env_bool(key: &str, default: bool) -> Result<bool> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default);
    };
    if value.trim().is_empty() {
        return Ok(default);
    }
    parse_bool_text(&value, key)
}

pub(super) fn parse_env_fvg_entry_mode(
    key: &str,
    default: MarketVelocityFvgEntryMode,
) -> Result<MarketVelocityFvgEntryMode> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default);
    };
    if value.trim().is_empty() {
        return Ok(default);
    }
    MarketVelocityFvgEntryMode::from_str(&value)
}

pub(super) fn parse_env_stop_loss_mode(
    key: &str,
    default: MarketVelocityStopLossMode,
) -> Result<MarketVelocityStopLossMode> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default);
    };
    if value.trim().is_empty() {
        return Ok(default);
    }
    MarketVelocityStopLossMode::from_str(&value)
}

pub(super) fn parse_env_signal_trade_direction(
    key: &str,
    default: MarketVelocitySignalTradeDirection,
) -> Result<MarketVelocitySignalTradeDirection> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default);
    };
    if value.trim().is_empty() {
        return Ok(default);
    }
    MarketVelocitySignalTradeDirection::from_str(&value)
}

pub(super) fn parse_env_string(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn json_field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.get(key).filter(|field| !field.is_null())
}

fn json_string_list(
    value: &Value,
    key: &str,
    normalize: fn(&str) -> String,
) -> Result<Option<Vec<String>>> {
    let Some(field) = json_field(value, key) else {
        return Ok(None);
    };
    let raw_items = match field {
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::String(value) => Ok(value.as_str()),
                _ => Err(anyhow!("{key} must be an array of strings")),
            })
            .collect::<Result<Vec<_>>>()?,
        Value::String(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            if matches!(normalized.as_str(), "" | "all" | "*" | "none") {
                return Ok(Some(Vec::new()));
            }
            value.split(',').collect()
        }
        _ => return Err(anyhow!("{key} must be an array of strings or csv string")),
    };
    let mut values = Vec::new();
    for item in raw_items.into_iter().map(normalize) {
        if item.is_empty() || values.contains(&item) {
            continue;
        }
        values.push(item);
    }
    Ok(Some(values))
}

fn env_optional_value_is_none(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "none" | "null" | "off"
    )
}

fn parse_bool_text(value: &str, key: &str) -> Result<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "true" | "yes" | "y" | "on" | "enabled" => Ok(true),
        "0" | "false" | "no" | "n" | "off" | "disabled" => Ok(false),
        _ => Err(anyhow!("{key} must be a boolean")),
    }
}

fn normalize_entry_trigger(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_symbol(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}
