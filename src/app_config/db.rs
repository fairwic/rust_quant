use std::env;
use std::time::Duration;
use once_cell::sync::OnceCell;
use rbatis::RBatis;
use rbdc_mysql::MysqlDriver;
use tracing::{info, error};

static DB_CLIENT: OnceCell<RBatis> = OnceCell::new();

// lazy_static! {
//     pub static ref DB_CLIENT: Mutex<Vec<u8>> = Mutex::new(Vec::new());
// }
pub async fn init_db() -> &'static RBatis {
    info!("Initializing database connection pool...");
    let rb = RBatis::new();
    // 从环境变量获取数据库配置
    let db_host = env::var("DB_HOST").expect("DB_HOST must be set");
    let max_connections = env::var("DB_MAX_CONNECTIONS")
        .unwrap_or_else(|_| "200".to_string())
        .parse::<u32>()
        .expect("DB_MAX_CONNECTIONS must be a number");
    // 连接数据库
    match rb.link(MysqlDriver {}, &db_host).await {
        Ok(_) => info!("Successfully connected to database"),
        Err(e) => {
            error!("Failed to connect to database: {}", e);
            panic!("Database connection failed");
        }
    }

    // 配置连接池
    let pool = rb.get_pool().expect("Failed to get connection pool");
    pool.set_max_open_conns(max_connections as u64).await;  // 最大连接数
    pool.set_max_idle_conns(max_connections as u64 / 2).await;  // 最大空闲连接数
    pool.set_conn_max_lifetime(Some(Duration::from_secs(3600))).await;  // 连接最大生命周期
    info!("Connection pool configured with {} max connections", max_connections);

    match DB_CLIENT.set(rb) {
        Ok(_) => info!("DB_CLIENT initialized successfully"),
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
