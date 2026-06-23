//! SQLx 数据库连接池管理
//!
//! 使用 sqlx 替代 rbatis，提供类型安全的数据库访问
use once_cell::sync::OnceCell;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::info;
static DB_POOL: OnceCell<PgPool> = OnceCell::new();
/// 封装数据库URLfrom环境变量，减少配置运行时调用方重复实现相同细节。
fn database_url_from_env() -> anyhow::Result<String> {
    let envs: HashMap<String, String> = std::env::vars().collect();
    database_url_from_map(&envs)
}
/// 从环境变量映射中读取数据库 URL，并优先保留 Core 专用连接配置。
fn database_url_from_map(envs: &HashMap<String, String>) -> anyhow::Result<String> {
    if let Some(database_url) = non_empty_env(envs, "QUANT_CORE_DATABASE_URL")
        .or_else(|| non_empty_env(envs, "POSTGRES_QUANT_CORE_DATABASE_URL"))
    {
        return Ok(database_url.to_string());
    }
    let database_url = non_empty_env(envs, "DATABASE_URL").ok_or_else(|| {
        anyhow::anyhow!(
            "QUANT_CORE_DATABASE_URL, POSTGRES_QUANT_CORE_DATABASE_URL or DATABASE_URL must be set"
        )
    })?;
    if !database_url_targets_quant_core(database_url) {
        anyhow::bail!(
            "QUANT_CORE_DATABASE_URL must be set for rust_quant Core database access; DATABASE_URL points to a non-core database"
        );
    }
    Ok(database_url.to_string())
}
/// 读取非空环境变量值，避免空字符串覆盖有效默认配置。
fn non_empty_env<'a>(envs: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    envs.get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}
/// 判断数据库 URL 是否指向 quant_core，防止 Core 误连其他业务库。
fn database_url_targets_quant_core(database_url: &str) -> bool {
    database_url
        .split('?')
        .next()
        .unwrap_or(database_url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .map(|database_name| database_name.eq_ignore_ascii_case("quant_core"))
        .unwrap_or(false)
}
/// 初始化数据库连接池
pub async fn init_db_pool() -> anyhow::Result<()> {
    let database_url = database_url_from_env()?;
    info!("正在初始化数据库连接池...");
    let pool = PgPoolOptions::new()
        .connect(&database_url)
        .await
        .map_err(|e| anyhow::anyhow!("数据库连接失败: {}", e))?;
    DB_POOL
        .set(pool)
        .map_err(|_| anyhow::anyhow!("数据库连接池已初始化"))?;
    info!("✓ 数据库连接池初始化成功");
    Ok(())
}
/// 获取数据库连接池
pub fn get_db_pool() -> &'static PgPool {
    DB_POOL
        .get()
        .expect("数据库连接池未初始化，请先调用 init_db_pool()")
}
/// 关闭数据库连接池
pub async fn close_db_pool() -> anyhow::Result<()> {
    if let Some(pool) = DB_POOL.get() {
        info!("正在关闭数据库连接池...");
        pool.close().await;
        info!("✓ 数据库连接池已关闭");
    }
    Ok(())
}
/// 健康检查
pub async fn health_check() -> anyhow::Result<()> {
    let pool = get_db_pool();
    sqlx::query("SELECT 1")
        .fetch_one(pool)
        .await
        .map_err(|e| anyhow::anyhow!("数据库健康检查失败: {}", e))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn internal_server_rejects_web_database_url_without_quant_core_url() {
        let envs = HashMap::from([
            ("IS_RUN_INTERNAL_SERVER".to_string(), "true".to_string()),
            (
                "DATABASE_URL".to_string(),
                "postgres://postgres:secret@localhost:5432/quant_web".to_string(),
            ),
        ]);
        let error = database_url_from_map(&envs).expect_err("quant_web must be rejected");
        assert!(
            error.to_string().contains("QUANT_CORE_DATABASE_URL"),
            "unexpected error: {error}"
        );
    }
    #[test]
    fn database_url_fallback_allows_quant_core_database_url() {
        let envs = HashMap::from([(
            "DATABASE_URL".to_string(),
            "postgres://postgres:secret@localhost:5432/quant_core?sslmode=disable".to_string(),
        )]);
        assert_eq!(
            database_url_from_map(&envs).expect("quant_core fallback should be selected"),
            "postgres://postgres:secret@localhost:5432/quant_core?sslmode=disable"
        );
    }
    #[test]
    fn internal_server_prefers_quant_core_database_url() {
        let envs = HashMap::from([
            ("IS_RUN_INTERNAL_SERVER".to_string(), "true".to_string()),
            (
                "QUANT_CORE_DATABASE_URL".to_string(),
                "postgres://postgres:secret@localhost:5432/quant_core".to_string(),
            ),
            (
                "DATABASE_URL".to_string(),
                "postgres://postgres:secret@localhost:5432/quant_web".to_string(),
            ),
        ]);
        assert_eq!(
            database_url_from_map(&envs).expect("quant_core url should be selected"),
            "postgres://postgres:secret@localhost:5432/quant_core"
        );
    }
}
