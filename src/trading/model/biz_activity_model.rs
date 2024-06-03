use rbatis::RBatis;
use rbatis::rbdc::DateTime;
use serde_json::json;
use crate::trading::model::biz_activity::BizActivity;
use crate::trading::model::Db;

pub struct BizActivityModel {
    db: RBatis,
}

impl BizActivityModel {
    pub async fn new() -> Self {
        Self {
            db: Db::get_db_client().await,
        }
    }
    pub async fn add(&self) -> anyhow::Result<()> {
        let table = BizActivity {
            id: Some("2".into()),
            name: Some("2".into()),
            pc_link: Some("2".into()),
            h5_link: Some("2".into()),
            pc_banner_img: None,
            h5_banner_img: None,
            sort: Some("2".to_string()),
            status: Some(2),
            remark: Some("2".into()),
            create_time: Some(DateTime::now()),
            version: Some(1),
            delete_flag: Some(1),
        };
        let tables = [table.clone(), {
            let mut t3 = table.clone();
            t3.id = "3".to_string().into();
            t3
        }];

        let data = BizActivity::insert(&self.db, &table).await;
        println!("insert = {}", json!(data));

        let data = BizActivity::insert_batch(&self.db, &tables, 10).await;
        println!("insert_batch = {}", json!(data));
        Ok(())
    }
    pub async fn update(&self) -> anyhow::Result<()> {
        let table = BizActivity {
            id: Some("2".into()),
            name: Some("2".into()),
            pc_link: Some("2".into()),
            h5_link: Some("2".into()),
            pc_banner_img: None,
            h5_banner_img: None,
            sort: None,
            status: Some(2),
            remark: Some("2".into()),
            create_time: Some(DateTime::now()),
            version: Some(1),
            delete_flag: Some(1),
        };

        let data = BizActivity::update_by_column(&self.db, &table, "id").await;
        println!("update_by_column = {}", json!(data));

        let data = BizActivity::update_by_name(&self.db, &table, "2").await;
        println!("update_by_name = {}", json!(data));
        Ok(())
    }
}