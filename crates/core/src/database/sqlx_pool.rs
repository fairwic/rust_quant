//! SQLx 数据库连接池管理
//! 
//! 使用 sqlx 替代 rbatis，提供类型安全的数据库访问

use once_cell::sync::OnceCell;
use sqlx::{MySql, MySqlPool, Pool};
use std::time::Duration;
use tracing::{error, info};

static DB_POOL: OnceCell<Pool<MySql>> = OnceCell::new();

/// 初始化数据库连接池
pub async fn init_db_pool() -> anyhow::Result<()> {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    
    info!("正在初始化数据库连接池...");
    
    let pool = MySqlPool::connect_with(
        database_url.parse()
            .map_err(|e| anyhow::anyhow!("数据库URL解析失败: {}", e))?
    )
    .await
    .map_err(|e| anyhow::anyhow!("数据库连接失败: {}", e))?;
    
    DB_POOL.set(pool).map_err(|_| anyhow::anyhow!("数据库连接池已初始化"))?;
    
    info!("✓ 数据库连接池初始化成功");
    Ok(())
}

/// 获取数据库连接池
pub fn get_db_pool() -> &'static Pool<MySql> {
    DB_POOL.get().expect("数据库连接池未初始化，请先调用 init_db_pool()")
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

