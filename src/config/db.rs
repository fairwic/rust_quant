use std::env;

use once_cell::sync::OnceCell;
use rbatis::RBatis;
use rbdc_mysql::MysqlDriver;

static DB_CLIENT: OnceCell<RBatis> = OnceCell::new();

pub async fn init_db() -> &'static RBatis {
    let rb = RBatis::new();
    rb.link(MysqlDriver {}, &*env::var("DB_HOST").unwrap()).await.unwrap();
    rb.get_pool().unwrap().set_max_open_conns(1000).await;

    DB_CLIENT.set(rb).expect("Failed to set DB_CLIENT");
    DB_CLIENT.get().expect("DB_CLIENT is not initialized")
}

pub fn get_db_client() -> &'static RBatis {
    DB_CLIENT.get().expect("DB_CLIENT is not initialized")
}
