pub mod biz_activity;
pub mod biz_activity_model;
pub mod market;

use rbatis::RBatis;
use rbdc_mysql::MysqlDriver;

// pub struct Model {
//     db: Db,
// }
//
// impl Model {
//     fn new() -> &mut Model {
//         Self.db = Db::get_db_client();
//         Self
//     }
// }

pub struct Db {}

impl Db {
    pub async fn get_db_client() -> RBatis {
        /// enable log crate to show sql logs
        fast_log::init(fast_log::Config::new().console()).expect("rbatis init fail");
        /// initialize rbatis. also you can call rb.clone(). this is  an Arc point
        let rb = RBatis::new();
        /// connect to database

        //init() just set driver
        //rb.init(rbdc_sqlite::driver::SqliteDriver {}, "sqlite://target/sqlite.db" ).unwrap();

        // link() will set driver and try use acquire() link database
        // sqlite
        rb.link(MysqlDriver {}, "mysql://root:example@localhost:3306/test").await.unwrap();
        // mysql
        // rb.link(MysqlDriver{},"mysql://root:123456@localhost:3306/test").await.unwrap();
        // postgresql
        // rb.link(PgDriver{},"postgres://postgres:123456@localhost:5432/postgres").await.unwrap();
        // mssql/sqlserver
        // rb.link(MssqlDriver{},"jdbc:sqlserver://localhost:1433;User=SA;Password={TestPass!123456};Database=test").await.unwrap();
        rb
    }
}