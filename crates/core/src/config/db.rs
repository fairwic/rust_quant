use crate::database::{close_db_pool, get_db_pool, init_db_pool};
use sqlx::PgPool;
use tracing::{debug, error, info};

pub async fn init_db() -> &'static PgPool {
    debug!("Initializing database connection pool...");

    if get_db_pool_if_initialized().is_none() {
        if let Err(e) = init_db_pool().await {
            error!("Failed to connect to Postgres database: {}", e);
            panic!("Database connection failed");
        }
        info!("DB_CLIENT initialized successfully !");
    }

    get_db_pool()
}

pub fn get_db_client() -> &'static PgPool {
    get_db_pool()
}

pub async fn monitor_connection_pool() -> String {
    let pool = get_db_client();
    format!(
        "连接池状态：size={}, idle={}",
        pool.size(),
        pool.num_idle()
    )
}

pub async fn cleanup_connection_pool() -> anyhow::Result<()> {
    info!("开始清理数据库连接池...");
    let pool = get_db_client();
    info!("清理前状态：size={}, idle={}", pool.size(), pool.num_idle());
    close_db_pool().await?;
    info!("数据库连接池已关闭");
    Ok(())
}

fn get_db_pool_if_initialized() -> Option<&'static PgPool> {
    std::panic::catch_unwind(get_db_pool).ok()
}
