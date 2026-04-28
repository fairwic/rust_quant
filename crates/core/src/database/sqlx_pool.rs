//! SQLx 数据库连接池管理
//!
//! 使用 sqlx 替代 rbatis，提供类型安全的数据库访问

use once_cell::sync::OnceCell;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tracing::info;

static DB_POOL: OnceCell<PgPool> = OnceCell::new();

fn database_url_from_env() -> anyhow::Result<String> {
    std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .map_err(|_| anyhow::anyhow!("QUANT_CORE_DATABASE_URL or DATABASE_URL must be set"))
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
