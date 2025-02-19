use std::env;

use once_cell::sync::OnceCell;
use rbatis::RBatis;
use rbdc_mysql::MysqlDriver;

static DB_CLIENT: OnceCell<RBatis> = OnceCell::new();

// lazy_static! {
//     pub static ref DB_CLIENT: Mutex<Vec<u8>> = Mutex::new(Vec::new());
// }
pub async fn init_db() -> &'static RBatis {
    // println!("init db config ant connecting ");
    let rb = RBatis::new();
    rb.link(MysqlDriver {}, &*env::var("DB_HOST").unwrap()).await.expect("Failed to connect db");
    //这里建议 需要调整数据库的最大连接数
    rb.get_pool().unwrap().set_max_open_conns(300).await;

    DB_CLIENT.set(rb).expect("Failed to set DB_CLIENT");
    DB_CLIENT.get().expect("DB_CLIENT is not initialized")
}

pub fn get_db_client() -> &'static RBatis {
    DB_CLIENT.get().expect("DB_CLIENT is not initialized")
}
