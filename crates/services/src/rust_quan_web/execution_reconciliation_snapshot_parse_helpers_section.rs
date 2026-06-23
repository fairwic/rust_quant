/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
fn required_trimmed<F>(lookup: &F, key: &str) -> Result<String>
where
    F: Fn(&str) -> Option<String>,
{
    lookup(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("{key} is required"))
}
/// 封装必需i64，减少Web 商业链路调用方重复实现相同细节。
fn required_i64<F>(lookup: &F, key: &str) -> Result<i64>
where
    F: Fn(&str) -> Option<String>,
{
    let value = required_trimmed(lookup, key)?;
    let parsed = value
        .parse::<i64>()
        .map_err(|_| anyhow!("{key} must be a positive integer"))?;
    if parsed <= 0 {
        bail!("{key} must be a positive integer");
    }
    Ok(parsed)
}
/// 提供optionali64的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn optional_i64<F>(lookup: &F, key: &str) -> Result<Option<i64>>
where
    F: Fn(&str) -> Option<String>,
{
    let Some(value) = lookup(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let parsed = value
        .parse::<i64>()
        .map_err(|_| anyhow!("{key} must be a positive integer"))?;
    if parsed <= 0 {
        bail!("{key} must be a positive integer");
    }
    Ok(Some(parsed))
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
fn parse_bool_default_true(value: &str) -> Result<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "off" => Ok(false),
        _ => bail!("RECONCILIATION_SNAPSHOT_REPORT must be a boolean"),
    }
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
fn parse_bool_default_false(value: &str) -> Result<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "" | "0" | "false" | "no" | "n" | "off" => Ok(false),
        _ => bail!("RECONCILIATION_SNAPSHOT_INCLUDE_FILLS must be a boolean"),
    }
}
fn expected_close_fill_writeback_intent(combo_id: i64, task_id: i64, symbol: &str) -> String {
    format!("web-close-fill:combo={combo_id}:task={task_id}:symbol={symbol}")
}
/// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
fn require_candidate_string(candidate: &Value, key: &str, expected: &str) -> Result<()> {
    let actual = required_candidate_string(candidate, key)?;
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("{key} must be {expected}");
    }
    Ok(())
}
/// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
fn require_candidate_i64(candidate: &Value, key: &str, expected: i64) -> Result<()> {
    let actual = required_candidate_i64(candidate, key)?;
    if actual != expected {
        bail!("{key} must be {expected}");
    }
    Ok(())
}
/// 校验输入和运行前置条件，提前暴露 Web 商业、会员和执行准备度 的不可执行原因。
fn require_candidate_bool(candidate: &Value, key: &str, expected: bool) -> Result<()> {
    let actual = candidate
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("{key} must be a boolean"))?;
    if actual != expected {
        bail!("{key} must be {expected}");
    }
    Ok(())
}
/// 封装必需candidate字符串，减少Web 商业链路调用方重复实现相同细节。
fn required_candidate_string(candidate: &Value, key: &str) -> Result<String> {
    candidate
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("{key} is required"))
}
/// 提供optional候选string的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn optional_candidate_string(candidate: &Value, key: &str) -> Option<String> {
    candidate
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
/// 封装必需candidatei64，减少Web 商业链路调用方重复实现相同细节。
fn required_candidate_i64(candidate: &Value, key: &str) -> Result<i64> {
    let Some(value) = candidate.get(key) else {
        bail!("{key} is required");
    };
    if let Some(parsed) = value.as_i64() {
        return Ok(parsed);
    }
    let Some(raw) = value.as_str() else {
        bail!("{key} must be an integer");
    };
    raw.trim()
        .parse::<i64>()
        .map_err(|_| anyhow!("{key} must be an integer"))
}
/// 提供optional候选i64的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn optional_candidate_i64(candidate: &Value, key: &str) -> Option<i64> {
    candidate.get(key).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_str()?.trim().parse().ok())
    })
}
/// 封装必需candidatef64，减少Web 商业链路调用方重复实现相同细节。
fn required_candidate_f64(candidate: &Value, key: &str) -> Result<f64> {
    let Some(value) = candidate.get(key) else {
        bail!("{key} is required");
    };
    let parsed = if let Some(parsed) = value.as_f64() {
        parsed
    } else if let Some(raw) = value.as_str() {
        raw.trim()
            .parse::<f64>()
            .map_err(|_| anyhow!("{key} must be numeric"))?
    } else {
        bail!("{key} must be numeric");
    };
    if !parsed.is_finite() {
        bail!("{key} must be finite");
    }
    Ok(parsed)
}
/// 提供optional候选f64的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn optional_candidate_f64(candidate: &Value, key: &str) -> Result<Option<f64>> {
    if candidate.get(key).is_none_or(Value::is_null) {
        return Ok(None);
    }
    required_candidate_f64(candidate, key).map(Some)
}
