use super::InternalHttpJsonResponse;
use serde_json::json;
const INTERNAL_SECRET_HEADER: &str = "x-alpha-execution-secret";
/// 解析 HTTP 头中的内部认证信息。
pub fn parse_headers(header: &str) -> Vec<(String, String)> {
    header
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect()
}
/// 提供authorizeinternalrequest的集中实现，避免量化核心调用方重复处理相同细节。
pub fn authorize_internal_request(
    method: &str,
    route: &str,
    headers: &[(String, String)],
) -> Option<InternalHttpJsonResponse> {
    if method == "GET" && matches!(route, "/internal/health" | "/api/internal/health") {
        return None;
    }
    let Some(expected_secret) = internal_secret_from_env() else {
        return Some(auth_response(
            500,
            "internal service secret is not configured",
        ));
    };
    let provided_secret = header_value(headers, INTERNAL_SECRET_HEADER).unwrap_or_default();
    if !constant_time_eq(provided_secret.as_bytes(), expected_secret.as_bytes()) {
        return Some(auth_response(403, "internal service secret is invalid"));
    }
    None
}
/// 提供internalsecretfrom环境变量的集中实现，避免量化核心调用方重复处理相同细节。
fn internal_secret_from_env() -> Option<String> {
    std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("ALPHA_EXECUTION_INTERNAL_SECRET"))
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
/// 提供header值的集中实现，避免量化核心调用方重复处理相同细节。
fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}
/// 提供constant时间eq的集中实现，避免量化核心调用方重复处理相同细节。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (lhs, rhs) in a.iter().zip(b.iter()) {
        difference |= lhs ^ rhs;
    }
    difference == 0
}
/// 提供authresponse的集中实现，避免量化核心调用方重复处理相同细节。
fn auth_response(status_code: u16, message: &str) -> InternalHttpJsonResponse {
    InternalHttpJsonResponse {
        status_code,
        body: json!({ "error": message }),
    }
}
#[cfg(test)]
mod tests {
    use super::{authorize_internal_request, parse_headers};
    use std::sync::{Mutex, OnceLock};
    /// 封装环境变量lock，减少量化核心调用方重复实现相同细节。
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }
    #[test]
    fn internal_routes_require_matching_secret() {
        let _guard = env_lock();
        std::env::set_var("EXECUTION_EVENT_SECRET", "core-secret");
        std::env::remove_var("ALPHA_EXECUTION_INTERNAL_SECRET");
        std::env::remove_var("RUST_QUAN_WEB_INTERNAL_SECRET");
        let headers = parse_headers(
            "GET /internal/klines HTTP/1.1\r\nx-alpha-execution-secret: core-secret\r\n\r\n",
        );
        assert!(authorize_internal_request("GET", "/internal/klines", &headers).is_none());
        let response = authorize_internal_request("GET", "/internal/klines", &[])
            .expect("missing secret must fail");
        assert_eq!(response.status_code, 403);
        std::env::remove_var("EXECUTION_EVENT_SECRET");
    }
    #[test]
    fn health_route_remains_available_without_secret() {
        let _guard = env_lock();
        std::env::remove_var("EXECUTION_EVENT_SECRET");
        std::env::remove_var("ALPHA_EXECUTION_INTERNAL_SECRET");
        std::env::remove_var("RUST_QUAN_WEB_INTERNAL_SECRET");
        assert!(authorize_internal_request("GET", "/internal/health", &[]).is_none());
    }
}
