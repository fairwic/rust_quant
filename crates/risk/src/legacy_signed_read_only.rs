use anyhow::{anyhow, Result};
const LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV: &str = "LEGACY_SIGNED_READ_ONLY_CONFIRM";
const LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN: &str =
    "I_UNDERSTAND_LEGACY_SIGNED_READ_ONLY_ACCOUNT_READS";
/// 封装当前函数，减少风控调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
pub(crate) fn ensure_legacy_signed_read_only_allowed() -> Result<()> {
    let confirmation = std::env::var(LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV).ok();
    if confirmation.as_deref().map(str::trim) == Some(LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN) {
        return Ok(());
    }
    Err(anyhow!(
        "{}={} is required before using legacy rust_quant_risk signed read-only account queries; prefer the quant_web execution reconciliation path with exact credential_id and target task scope",
        LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV,
        LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN
    ))
}
