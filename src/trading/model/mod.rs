pub mod biz_activity;
pub mod biz_activity_model;
pub mod market;
pub(crate) mod asset;
pub(crate) mod strategy;

use std::{env, println};
use fast_log::Config;
use rbatis::RBatis;
use rbdc_mysql::MysqlDriver;
use rbdc_mysql::protocol::response;
use serde_json::json;
use crate::trading::model::market::candles::CandlesEntity;

pub struct Db {}

impl Db {
    pub async fn get_db_client() -> RBatis {
        /// enable log crate to show sql logs
        // fast_log::init(fast_log::Config::new().console()).expect("fast_log init error");
        // if let Err(e) = fast_log::init(Config::new().console()) {
        //     eprintln!("fast_log init error: {:?}", e);
        // }

        /// initialize rbatis. also you can call rb.clone(). this is  an Arc point
        let rb = RBatis::new();
        /// connect to database

        //init() just set driver
        //rb.init(rbdc_sqlite::driver::SqliteDriver {}, "sqlite://target/sqlite.db" ).unwrap();

        // link() will set driver and try use acquire() link database
        // sqlite
        rb.link(MysqlDriver {}, &*env::var("DB_HOST").unwrap()).await.unwrap();
        // rb.link_opt("mysql://yourusername:yourpassword@localhost:3306/yourdatabase", DBPoolOptions::new().max_connections(10)).await?;
        // mysql
        // rb.link(MysqlDriver{},"mysql://root:123456@localhost:3306/test").await.unwrap();
        // postgresql
        // rb.link(PgDriver{},"postgres://postgres:123456@localhost:5432/postgres").await.unwrap();
        // mssql/sqlserver
        // rb.link(MssqlDriver{},"jdbc:sqlserver://localhost:1433;User=SA;Password={TestPass!123456};Database=test").await.unwrap();
        rb
    }
}

struct Model {}

impl Model {
    pub async fn db(&self) -> RBatis {
        Db::get_db_client().await
    }
}