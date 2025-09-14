use once_cell::sync::OnceCell;
use rbatis::RBatis;
use rbdc_mysql::MysqlDriver;
use std::env;
use std::time::Duration;
use tracing::{error, info,debug};

static DB_CLIENT: OnceCell<RBatis> = OnceCell::new();

// lazy_static! {
//     pub static ref DB_CLIENT: Mutex<Vec<u8>> = Mutex::new(Vec::new());
// }
pub async fn init_db() -> &'static RBatis {
    debug!("Initializing database connection pool...");
    let rb = RBatis::new();
    // 从环境变量获取数据库配置
    let db_host = env::var("DB_HOST").expect("DB_HOST must be set");
    let max_connections = env::var("DB_MAX_CONNECTIONS")
        .unwrap_or_else(|_| "300".to_string())
        .parse::<u32>()
        .expect("DB_MAX_CONNECTIONS must be a number");
    // 连接数据库
    match rb.link(MysqlDriver {}, &db_host).await {
        Ok(_) => debug!("Successfully connected to database"),
        Err(e) => {
            error!("Failed to connect to database: {}", e);
            panic!("Database connection failed");
        }
    }

    // 配置连接池 - 优化连接池设置避免频繁创建销毁
    let pool = rb.get_pool().expect("Failed to get connection pool");
    pool.set_max_open_conns(max_connections as u64).await; // 最大连接数
    pool.set_max_idle_conns(max_connections as u64 / 3).await; // 减少空闲连接数，避免占用过多
    // 连接最大生命周期：1小时，减少频繁创建/销毁
    pool.set_conn_max_lifetime(Some(Duration::from_secs(3600))).await;
    debug!(
        "Connection pool configured with {} max connections",
        max_connections
    );

    match DB_CLIENT.set(rb) {
        Ok(_) => info!("DB_CLIENT initialized successfully !"),
        Err(_) => {
            error!("Failed to set DB_CLIENT");
            panic!("Failed to initialize DB_CLIENT");
        }
    }

    DB_CLIENT.get().expect("DB_CLIENT is not initialized")
}

pub fn get_db_client() -> &'static RBatis {
    DB_CLIENT.get().expect("DB_CLIENT is not initialized")
}

// 添加一个连接池监控函数
pub async fn monitor_connection_pool() -> String {
    let pool = get_db_client().get_pool().expect("Failed to get pool");
    let state = pool.state().await;

    format!("连接池状态：{:?}", state)
}

// 增强的连接池清理函数（避免依赖内部状态结构）
pub async fn cleanup_connection_pool() -> anyhow::Result<()> {
    let pool = get_db_client().get_pool().expect("Failed to get pool");

    info!("开始清理数据库连接池...");
    let before_state = pool.state().await;
    info!("清理前状态：{:?}", before_state);

    // 1) 禁止保留空闲连接，并缩短连接寿命，促使池子快速收缩
    pool.set_max_idle_conns(0).await;
    pool.set_conn_max_lifetime(Some(Duration::from_secs(1))).await;

    // 2) 轮询等待收缩完成（或直到超时）
    const MAX_RETRIES: u32 = 15;           // 最多等待 ~3s
    const RETRY_INTERVAL_MS: u64 = 200;    // 每次检查间隔 200ms

    for i in 1..=MAX_RETRIES {
        tokio::time::sleep(Duration::from_millis(RETRY_INTERVAL_MS)).await;
        let state = pool.state().await;
        info!("清理进度 {}/{}: {:?}", i, MAX_RETRIES, state);
    }

    let after_state = pool.state().await;
    info!("清理后状态：{:?}", after_state);

    Ok(())
}

