// use std::env;
// use fast_log::Config;
// use rbatis::RBatis;
// use rbdc_mysql::MysqlDriver;
//
// pub struct Db {}
//
// impl Db {
//     pub async fn get_db_client() -> RBatis {
//
//         /// initialize rbatis. also you can call rb.clone(). this is  an Arc point
//         let rb = RBatis::new();
//         /// connect to database
//
//         //init() just set driver
//         //rb.init(rbdc_sqlite::driver::SqliteDriver {}, "sqlite://target/sqlite.db" ).unwrap();
//
//         // link() will set driver and try use acquire() link database
//         // sqlite
//         rb.link(MysqlDriver {}, &*env::var("DB_HOST").unwrap()).await.unwrap();
//         rb.get_pool().unwrap().set_max_open_conns(100).await;
//         // rb.link_opt("mysql://yourusername:yourpassword@localhost:3306/yourdatabase", DBPoolOptions::new().max_connections(10)).await?;
//         // mysql
//         // rb.link(MysqlDriver{},"mysql://root:123456@localhost:3306/tests").await.unwrap();
//         // postgresql
//         // rb.link(PgDriver{},"postgres://postgres:123456@localhost:5432/postgres").await.unwrap();
//         // mssql/sqlserver
//         // rb.link(MssqlDriver{},"jdbc:sqlserver://localhost:1433;User=SA;Password={TestPass!123456};Database=tests").await.unwrap();
//         rb
//     }
// }
