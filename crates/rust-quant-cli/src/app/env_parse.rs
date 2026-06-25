use anyhow::{anyhow, bail, Context, Result};

pub(crate) fn first_non_empty_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

pub(crate) fn parse_i64_env(key: &str, default: i64) -> Result<i64> {
    std::env::var(key)
        .ok()
        .map(|value| parse_i64_required_when_present(key, &value, "integer"))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

pub(crate) fn parse_u64_env(key: &str, default: u64) -> Result<u64> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<u64>()
                .map_err(|error| anyhow!("{key} must be an unsigned integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

pub(crate) fn parse_bool_env(key: &str, default: bool) -> Result<bool> {
    let Some(value) = std::env::var(key).ok() else {
        return Ok(default);
    };
    parse_bool_empty_as_false(key, &value)
}

pub(crate) fn parse_i64_env_default_on_empty(key: &str, default: i64) -> Result<i64> {
    match std::env::var(key) {
        Ok(raw) if !raw.trim().is_empty() => raw
            .trim()
            .parse::<i64>()
            .with_context(|| format!("{key} must be i64")),
        _ => Ok(default),
    }
}

pub(crate) fn parse_bool_env_reject_empty(key: &str, default: bool) -> Result<bool> {
    match std::env::var(key) {
        Ok(raw) => parse_bool_required_when_present(key, &raw),
        Err(_) => Ok(default),
    }
}

fn parse_i64_required_when_present(key: &str, value: &str, label: &str) -> Result<i64> {
    value
        .trim()
        .parse::<i64>()
        .map_err(|error| anyhow!("{key} must be an {label}: {error}"))
}

fn parse_bool_empty_as_false(key: &str, value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "off" | "" => Ok(false),
        _ => bail!("{key} must be a boolean"),
    }
}

fn parse_bool_required_when_present(key: &str, value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => bail!("{key} must be boolean"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_parser_can_preserve_empty_as_false_policy() {
        assert!(parse_bool_empty_as_false("FLAG", "yes").unwrap());
        assert!(!parse_bool_empty_as_false("FLAG", "n").unwrap());
        assert!(!parse_bool_empty_as_false("FLAG", "").unwrap());
        assert!(parse_bool_empty_as_false("FLAG", "maybe").is_err());
    }

    #[test]
    fn bool_parser_can_preserve_reject_empty_policy() {
        assert!(parse_bool_required_when_present("FLAG", "on").unwrap());
        assert!(!parse_bool_required_when_present("FLAG", "off").unwrap());
        assert!(parse_bool_required_when_present("FLAG", "").is_err());
        assert!(parse_bool_required_when_present("FLAG", "n").is_err());
    }

    #[test]
    fn first_non_empty_env_uses_priority_order_and_trims_values() {
        let keys = ["RUST_QUANT_TEST_EMPTY_ENV", "RUST_QUANT_TEST_VALUE_ENV"];
        unsafe {
            std::env::set_var(keys[0], "  ");
            std::env::set_var(keys[1], " value ");
        }

        assert_eq!(first_non_empty_env(&keys), Some("value".to_string()));

        unsafe {
            std::env::remove_var(keys[0]);
            std::env::remove_var(keys[1]);
        }
    }
}
