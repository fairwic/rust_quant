use std::env;

/// 读取布尔型环境变量：支持 true/false/1/0（大小写不敏感）
pub fn env_is_true(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(v) => {
            let v = v.trim();
            v.eq_ignore_ascii_case("true") || v == "1"
        }
        Err(_) => default,
    }
}

/// 读取字符串环境变量，若不存在则返回默认值
pub fn env_or_default(key: &str, default: &str) -> String {
    match env::var(key) {
        Ok(v) => v,
        Err(_) => default.to_string(),
    }
}

