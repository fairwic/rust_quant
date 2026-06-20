use serde_json::Value;

pub(super) fn parse_json_value_or_string(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Value::Null;
    }

    serde_json::from_str::<Value>(trimmed).unwrap_or_else(|_| Value::String(trimmed.to_string()))
}
